import type { CreateJobBody } from "../api";
import type { AppConfig, Job, JobCounts, JobEvent, JobEventKind, JobState, Release, SearchHit } from "../../types";
import { type EngineReport, runEngine } from "./engine";
import { type CoreEvent, levelOf, messageOf } from "./events";
import { getJson, postJson } from "./http";
import { Pacer, type PacingPolicy } from "./pacer";
import { isAbort } from "./retry";
import { type Artifact, DirectorySink, DownloadSink, type Sink, saveArtifact } from "./sink";
import type { PlanIn, PlanOut, ReadyItem, StreamOut } from "./types";

const MAX_ACTIVE_JOBS = 3;
const MAX_RETAINED_JOBS = 20;
const MAX_EVENTS_PER_JOB = 5000;
const MAX_WORKERS = 6;

type Listener = (event: JobEvent) => void;

function commandError(kind: string, code: string, message: string): Error & { kind: string; code: string } {
  return Object.assign(new Error(message), { kind, code });
}

function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function nowSeconds(): number {
  return Date.now() / 1000;
}

class WebJob {
  readonly id = crypto.randomUUID();
  readonly createdAt = nowSeconds();
  readonly controller = new AbortController();
  state: JobState = "running";
  title: string | null = null;
  targetDir: string | null = null;
  finishedAt: number | null = null;
  error: string | null = null;
  errorCode: string | null = null;
  report: Record<string, unknown> | null = null;
  counts: JobCounts = { downloaded: 0, reused: 0, failed: 0, unavailable: 0, selected: null };
  artifact: Artifact | null = null;
  private seq = 0;
  readonly events: JobEvent[] = [];

  constructor(
    readonly spec: CreateJobBody,
    private readonly deliver: (event: JobEvent) => void,
  ) {}

  snapshot(): Job {
    return {
      id: this.id,
      url: this.spec.url,
      mode: this.spec.mode,
      state: this.state,
      title: this.title,
      target_dir: this.targetDir,
      created_at: this.createdAt,
      finished_at: this.finishedAt,
      error: this.error,
      error_code: this.errorCode,
      counts: { ...this.counts },
      report: this.report,
    };
  }

  private publish(items: [JobEventKind, unknown][]): void {
    for (const [event, data] of items) {
      this.seq += 1;
      const item: JobEvent = { job_id: this.id, id: this.seq, event, data };
      if (this.events.length === MAX_EVENTS_PER_JOB) this.events.shift();
      this.events.push(item);
      this.deliver(item);
    }
  }

  private stateEvent(): [JobEventKind, unknown] {
    return ["state", this.snapshot()];
  }

  private static track(
    trackId: number,
    status: string,
    reason: string | null,
    reasonCode: string | null,
    bytes: number | null,
  ): [JobEventKind, unknown] {
    return ["track", { track_id: trackId, status, reason, reason_code: reasonCode, bytes, progress: null }];
  }

  onEvent(event: CoreEvent): void {
    const out: [JobEventKind, unknown][] = [];
    if (event.event !== "track_progress") {
      out.push(["log", { ts: nowSeconds(), level: levelOf(event), message: messageOf(event), detail: event }]);
    }
    switch (event.event) {
      case "planned":
        this.title = event.album;
        this.counts.selected = (this.counts.selected ?? 0) + event.selected;
        out.push(this.stateEvent());
        break;
      case "track_progress":
        out.push([
          "track",
          { track_id: event.track_id, status: "downloading", reason: null, reason_code: null, bytes: event.bytes, progress: event.fraction },
        ]);
        break;
      case "track_downloaded":
        this.counts.downloaded += 1;
        out.push(WebJob.track(event.track_id, "done", null, null, event.bytes), this.stateEvent());
        break;
      case "track_reused":
        this.counts.reused += 1;
        out.push(WebJob.track(event.track_id, "reused", null, null, null), this.stateEvent());
        break;
      case "track_retry":
        out.push(WebJob.track(event.track_id, "retrying", event.reason, null, null));
        break;
      case "track_unavailable":
        this.counts.unavailable += 1;
        out.push(WebJob.track(event.track_id, "unavailable", event.reason, event.reason_code, null), this.stateEvent());
        break;
      case "track_failed":
        this.counts.failed += 1;
        out.push(WebJob.track(event.track_id, "failed", event.reason, event.reason_code, null), this.stateEvent());
        break;
      default:
        break;
    }
    this.publish(out);
  }

  finish(report: EngineReport): void {
    const out: [JobEventKind, unknown][] = [];
    if (report.fatal) {
      const detail = { event: "job_failed", code: report.fatal.code, reason: report.fatal.message };
      out.push(["log", { ts: nowSeconds(), level: "ERROR", message: report.fatal.message, detail }]);
    }
    this.state = report.cancelled
      ? "cancelled"
      : report.fatal
        ? "failed"
        : report.failed === 0
          ? "completed"
          : "partial";
    this.error = report.fatal?.message ?? null;
    this.errorCode = report.fatal?.code ?? null;
    this.finishedAt = nowSeconds();
    this.title = this.title ?? report.title;
    this.counts = {
      downloaded: report.downloaded,
      reused: report.reused,
      failed: report.failed,
      unavailable: report.unavailable,
      selected: this.counts.selected,
    };
    this.artifact = report.artifact;
    this.targetDir = report.artifact?.name ?? null;
    this.report = {
      summary: {
        downloaded: report.downloaded,
        reused: report.reused,
        failed: report.failed,
        unavailable: report.unavailable,
        bytes: report.bytes,
        elapsed_s: report.elapsedS,
      },
      artifact: report.artifact ? { name: report.artifact.name, bytes: report.artifact.blob.size } : null,
    };
    out.push(this.stateEvent(), ["end", {}]);
    this.publish(out);
  }
}

export interface WebBackendOptions {
  autoSave?: boolean;
  pacing?: PacingPolicy;
}

export class WebBackend {
  private readonly jobs = new Map<string, WebJob>();
  private readonly listeners = new Set<Listener>();
  private pendingRoot: FileSystemDirectoryHandle | null = null;
  private readonly autoSave: boolean;
  private readonly pacer: Pacer;

  constructor(options: WebBackendOptions = {}) {
    this.autoSave = options.autoSave ?? true;
    this.pacer = new Pacer(options.pacing);
    if (typeof window !== "undefined") {
      window.addEventListener("beforeunload", (event) => {
        if ([...this.jobs.values()].some((job) => job.state === "running")) event.preventDefault();
      });
    }
  }

  listen(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private deliver = (event: JobEvent): void => {
    for (const listener of this.listeners) listener(event);
  };

  private job(id: unknown): WebJob {
    const job = typeof id === "string" ? this.jobs.get(id) : undefined;
    if (!job) throw commandError("job_not_found", "job_not_found", "Tarefa nao encontrada.");
    return job;
  }

  async invoke(command: string, args: Record<string, unknown> = {}): Promise<unknown> {
    switch (command) {
      case "get_config":
        return getJson<AppConfig>("/api/config", {});
      case "inspect":
        return getJson<Release>("/api/inspect", { url: text(args.url), mode: text(args.mode, "playlist") });
      case "search":
        return getJson<SearchHit[]>("/api/search", { query: text(args.query) });
      case "list_jobs":
        return [...this.jobs.values()].map((job) => job.snapshot()).sort((a, b) => b.created_at - a.created_at);
      case "job_events": {
        const after = Number(args.after ?? 0);
        return this.job(args.id).events.filter((event) => event.id > after);
      }
      case "create_job":
        return this.create(args.spec as CreateJobBody);
      case "cancel_job": {
        const job = this.job(args.id);
        job.controller.abort();
        return job.snapshot();
      }
      case "open_folder": {
        const job = this.job(args.id);
        if (!job.artifact) {
          throw commandError("conflict", "download_not_ready", "O arquivo desta tarefa ainda nao esta pronto.");
        }
        saveArtifact(job.artifact);
        return null;
      }
      case "pick_output_dir":
        return this.pickDirectory();
      default:
        throw commandError("internal", "internal", `Comando desconhecido: ${command}`);
    }
  }

  private async pickDirectory(): Promise<string | null> {
    const picker = window.showDirectoryPicker;
    if (!picker) return null;
    try {
      const handle = await picker.call(window, { id: "perseus", mode: "readwrite", startIn: "music" });
      this.pendingRoot = handle;
      return handle.name;
    } catch (error) {
      if (isAbort(error)) return null;
      if (error instanceof DOMException && (error.name === "SecurityError" || error.name === "NotAllowedError")) {
        throw commandError("conflict", "picker_needs_gesture", "O navegador bloqueou o seletor de pasta.");
      }
      throw commandError("internal", "internal", error instanceof Error ? error.message : String(error));
    }
  }

  private create(spec: CreateJobBody): Job {
    if (spec.mode === "watch") {
      throw commandError("invalid_input", "watch_unsupported", "O monitoramento so existe no app desktop.");
    }
    if ([...this.jobs.values()].filter((job) => job.state === "running").length >= MAX_ACTIVE_JOBS) {
      throw commandError("too_many_jobs", "too_many_jobs", `Limite de ${String(MAX_ACTIVE_JOBS)} tarefas simultaneas atingido.`);
    }
    const root = spec.output_dir !== null && this.pendingRoot?.name === spec.output_dir ? this.pendingRoot : null;
    this.pendingRoot = null;
    const sink: Sink = root ? new DirectorySink(root) : new DownloadSink();
    const job = new WebJob(spec, this.deliver);
    this.jobs.set(job.id, job);
    this.evictFinished();
    void this.run(job, sink);
    return job.snapshot();
  }

  private async run(job: WebJob, sink: Sink): Promise<void> {
    const { spec } = job;
    const body: PlanIn & { workers: number } = {
      url: spec.url,
      mode: spec.mode === "track" ? "track" : "playlist",
      limit: spec.limit,
      quality: spec.quality,
      name_template: spec.name_template,
      min_duration_s: spec.min_duration_s,
      max_duration_s: spec.max_duration_s,
      write_playlist_file: spec.write_playlist_file,
      original_artwork: spec.original_artwork,
      workers: Math.min(Math.max(1, spec.workers), MAX_WORKERS),
    };
    let report: EngineReport;
    try {
      report = await this.engine(job, sink, body);
    } catch (error) {
      report = {
        title: null,
        downloaded: 0,
        reused: 0,
        failed: 0,
        unavailable: 0,
        bytes: 0,
        elapsedS: 0,
        cancelled: job.controller.signal.aborted,
        fatal: { code: "internal", message: error instanceof Error ? error.message : String(error) },
        artifact: null,
      };
    }
    job.finish(report);
    if (this.autoSave && report.artifact && !report.cancelled) saveArtifact(report.artifact);
  }

  private engine(job: WebJob, sink: Sink, body: PlanIn & { workers: number }): Promise<EngineReport> {
    return runEngine(body, {
      plan: async (input, signal) => {
        await this.pacer.wait(signal);
        return postJson<PlanOut>("/api/plan", input, signal);
      },
      stream: async (item: ReadyItem, signal) => {
        await this.pacer.wait(signal);
        return postJson<StreamOut>(
          "/api/stream",
          { transcoding_url: item.transcoding_url, track_authorization: item.track_authorization },
          signal,
        );
      },
      sink,
      emit: (event) => job.onEvent(event),
      signal: job.controller.signal,
    });
  }

  private evictFinished(): void {
    const finished = [...this.jobs.values()]
      .filter((job) => job.state !== "running")
      .sort((a, b) => b.createdAt - a.createdAt);
    for (const job of finished.slice(MAX_RETAINED_JOBS)) this.jobs.delete(job.id);
  }
}
