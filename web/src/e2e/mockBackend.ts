import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import type { AppConfig, Job, JobEvent, JobEventKind, JobState, Mode, Release, Track } from "../types";

const PLAYLIST_URL = "https://soundcloud.com/perseus/sets/argonautas";
const SLOW_PLAYLIST_URL = "https://soundcloud.com/perseus/sets/lenta";
const DRM_TRACK_ID = 1012;
const DRM_REASON = "protegida por DRM (somente streams criptografados)";

interface CreateJobSpec {
  url: string;
  mode: Mode;
  output_dir: string | null;
  workers: number;
  limit: number | null;
  interval: number;
}

interface MockJob {
  job: Job;
  cancelled: boolean;
  events: JobEvent[];
}

function tracks(size: number): Track[] {
  return Array.from({ length: size }, (_, index) => {
    const id = 1001 + index;
    const drm = id === DRM_TRACK_ID;
    return {
      id,
      position: index + 1,
      title: `Faixa ${String(index + 1).padStart(2, "0")}`,
      artist: "Perseus Ensemble",
      duration_ms: 120_000 + id,
      artwork_url: null,
      permalink_url: null,
      available: !drm,
      reason: drm ? DRM_REASON : null,
      reason_code: drm ? "drm" : null,
      group: null,
    };
  });
}

const RELEASES: Record<string, Release> = {
  [PLAYLIST_URL]: release(PLAYLIST_URL, "Argonautas", 12),
  [SLOW_PLAYLIST_URL]: release(SLOW_PLAYLIST_URL, "Lenta", 8),
};

function release(url: string, title: string, size: number): Release {
  const list = tracks(size);
  return {
    kind: "playlist",
    url,
    playlist_context: null,
    title,
    artist: "Perseus Ensemble",
    artwork_url: null,
    permalink_url: url,
    total_tracks: list.length,
    tracks: list,
  };
}

function fail(kind: string, code: string, message: string): never {
  // eslint-disable-next-line @typescript-eslint/only-throw-error -- mesmo formato serializado pelo CommandError do Rust
  throw { kind, code, message };
}

function resolve(raw: string): Release {
  let parsed: URL;
  try {
    parsed = new URL(raw.includes("://") ? raw : `https://${raw}`);
  } catch {
    fail("invalid_input", "invalid_url", "URL malformada.");
  }
  const host = parsed.hostname.toLowerCase();
  if (!["soundcloud.com", "www.soundcloud.com", "m.soundcloud.com"].includes(host)) {
    fail("invalid_input", "invalid_url", `Dominio nao suportado: ${host}. Use um link de soundcloud.com.`);
  }
  const found = RELEASES[`https://soundcloud.com${parsed.pathname.replace(/\/$/, "")}`];
  if (!found) fail("not_found", "not_found", "Recurso nao encontrado: removido, privado sem link secreto ou indisponivel na sua regiao.");
  return found;
}

const sleep = (ms: number): Promise<void> => new Promise((done) => setTimeout(done, ms));

export function installMockBackend(): void {
  const jobs = new Map<string, MockJob>();
  let counter = 0;

  const config: AppConfig = {
    version: "0.3.0-e2e",
    default_output_dir: "C:\\Users\\e2e\\Music\\Perseus",
    max_workers: 16,
    default_workers: 4,
    interval_min: 10,
    interval_max: 3600,
  };

  function publish(entry: MockJob, event: JobEventKind, data: unknown): void {
    const item: JobEvent = { job_id: entry.job.id, id: entry.events.length + 1, event, data };
    entry.events.push(item);
    void emit("perseus://job", item);
  }

  function log(entry: MockJob, level: string, message: string, detail: Record<string, unknown>): void {
    publish(entry, "log", { ts: Date.now() / 1000, level, message, detail });
  }

  function finish(entry: MockJob, state: JobState): void {
    entry.job = { ...entry.job, state, finished_at: Date.now() / 1000 };
    publish(entry, "state", entry.job);
    publish(entry, "end", {});
  }

  async function run(entry: MockJob, spec: CreateJobSpec, playlist: Release): Promise<void> {
    const delay = playlist.url === SLOW_PLAYLIST_URL ? 600 : 50;
    const selected = playlist.tracks.slice(0, spec.limit ?? undefined);
    await sleep(20);
    entry.job = {
      ...entry.job,
      title: playlist.title,
      target_dir: `${config.default_output_dir}\\Perseus Ensemble - ${playlist.title}`,
      counts: { ...entry.job.counts, selected: selected.length },
    };
    log(entry, "INFO", `'${playlist.title}' por 'Perseus Ensemble': ${selected.length} de ${playlist.total_tracks} faixa(s)`, {
      event: "planned",
      album: playlist.title,
      artist: "Perseus Ensemble",
      selected: selected.length,
      total: playlist.total_tracks,
      target_dir: entry.job.target_dir,
    });
    publish(entry, "state", entry.job);

    for (const track of selected) {
      await sleep(delay);
      if (entry.cancelled) {
        finish(entry, "cancelled");
        return;
      }
      const counts = { ...entry.job.counts };
      if (track.id === DRM_TRACK_ID) {
        counts.unavailable += 1;
        log(entry, "WARNING", `Faixa ${track.id} indisponivel: ${DRM_REASON}`, {
          event: "track_unavailable",
          track_id: track.id,
          reason: DRM_REASON,
          reason_code: "drm",
        });
        publish(entry, "track", {
          track_id: track.id,
          status: "unavailable",
          reason: DRM_REASON,
          reason_code: "drm",
          bytes: null,
          progress: null,
        });
      } else {
        publish(entry, "track", {
          track_id: track.id,
          status: "downloading",
          reason: null,
          reason_code: null,
          bytes: 2048,
          progress: 0.5,
        });
        counts.downloaded += 1;
        const file = `${String(track.position).padStart(2, "0")}. Perseus Ensemble - ${track.title}.mp3`;
        log(entry, "INFO", `Concluido: ${file}`, { event: "track_downloaded", track_id: track.id, file, bytes: 4096 });
        publish(entry, "track", {
          track_id: track.id,
          status: "done",
          reason: null,
          reason_code: null,
          bytes: 4096,
          progress: null,
        });
      }
      entry.job = { ...entry.job, counts };
      publish(entry, "state", entry.job);
    }
    finish(entry, "completed");
  }

  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "get_config":
          return config;
        case "inspect":
          return resolve(String(payload.url));
        case "search":
          return Object.values(RELEASES)
            .filter((item) => item.title.toLowerCase().includes(String(payload.query).toLowerCase()))
            .map((item) => ({
              kind: "playlist",
              title: item.title,
              subtitle: item.artist,
              url: item.url,
              artwork_url: null,
              duration_ms: null,
              track_count: item.total_tracks,
            }));
        case "list_jobs":
          return [...jobs.values()].map((entry) => entry.job).reverse();
        case "job_events": {
          const entry = jobs.get(String(payload.id));
          if (!entry) fail("job_not_found", "job_not_found", "Tarefa nao encontrada.");
          return entry.events.filter((item) => item.id > Number(payload.after));
        }
        case "create_job": {
          const spec = payload.spec as CreateJobSpec;
          const playlist = resolve(spec.url);
          if ([...jobs.values()].filter((entry) => entry.job.state === "running").length >= 3) {
            fail("too_many_jobs", "too_many_jobs", "Limite de 3 tarefas simultaneas atingido.");
          }
          counter += 1;
          const entry: MockJob = {
            cancelled: false,
            events: [],
            job: {
              id: `e2e${String(counter)}`,
              url: spec.url,
              mode: spec.mode,
              state: "running",
              title: null,
              target_dir: null,
              created_at: Date.now() / 1000,
              finished_at: null,
              error: null,
              error_code: null,
              counts: { downloaded: 0, reused: 0, failed: 0, unavailable: 0, selected: null },
              report: null,
            },
          };
          jobs.set(entry.job.id, entry);
          void run(entry, spec, playlist);
          return entry.job;
        }
        case "cancel_job": {
          const entry = jobs.get(String(payload.id));
          if (!entry) fail("job_not_found", "job_not_found", "Tarefa nao encontrada.");
          entry.cancelled = true;
          return entry.job;
        }
        case "open_folder":
          return null;
        case "pick_output_dir":
          return `${config.default_output_dir}\\Escolhida`;
        default:
          fail("internal", "internal", `Comando desconhecido: ${cmd}`);
      }
    },
    { shouldMockEvents: true },
  );
}
