import { Eye } from "lucide-react";
import { type ReactElement, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { ActivityLog } from "./components/ActivityLog";
import { CommandBar } from "./components/CommandBar";
import { ReleaseView } from "./components/ReleaseView";
import { SearchResults } from "./components/SearchResults";
import { Settings } from "./components/Settings";
import { Sidebar } from "./components/Sidebar";
import type { CommandError } from "./components/CommandBar";
import { type I18n, useI18n } from "./i18n/context";
import { describeEvent, specificErrorText } from "./i18n/describe";
import { ApiError, api } from "./lib/api";
import { canPickFolder } from "./lib/platform";
import { useJobEvents, type WatchStatus } from "./lib/useJobEvents";
import type {
  AppConfig,
  Job,
  JobOptions,
  LogLine,
  Mode,
  Release,
  SearchHit,
  TrackEvent,
  TrackProgress,
} from "./types";

const OPTIONS_KEY = "perseus.options.v1";
const MAX_LOG_LINES = 1500;
const DEFAULT_OPTIONS: JobOptions = {
  outputDir: "",
  workers: 4,
  limit: null,
  interval: 60,
  askFolder: true,
  quality: "compatible",
  nameTemplate: "",
  minDurationS: null,
  maxDurationS: null,
  maxKbps: null,
  writePlaylistFile: true,
  originalArtwork: false,
  syncRemoved: false,
  useLibrary: true,
};

const isNumber = (value: unknown): boolean => typeof value === "number" && Number.isFinite(value);
const isOptionalNumber = (value: unknown): boolean => value === null || isNumber(value);
const isBoolean = (value: unknown): boolean => typeof value === "boolean";
const isString = (value: unknown): boolean => typeof value === "string";

export function looksLikeUrl(text: string): boolean {
  const value = text.trim();
  return /^https?:\/\//i.test(value) || /(^|\.)soundcloud\.com(\/|$)/i.test(value) || value.includes("/");
}

interface JobView {
  release: Release | null;
  progress: Record<number, TrackProgress>;
  watchStatus: WatchStatus | null;
}

function loadOptions(): JobOptions {
  try {
    const raw = localStorage.getItem(OPTIONS_KEY);
    if (!raw) return DEFAULT_OPTIONS;
    const stored = JSON.parse(raw) as Record<string, unknown>;
    const pick = <K extends keyof JobOptions>(key: K, valid: (value: unknown) => boolean): JobOptions[K] =>
      valid(stored[key]) ? (stored[key] as JobOptions[K]) : DEFAULT_OPTIONS[key];
    return {
      outputDir: pick("outputDir", isString),
      workers: pick("workers", isNumber),
      limit: pick("limit", isOptionalNumber),
      interval: pick("interval", isNumber),
      askFolder: pick("askFolder", isBoolean),
      quality: pick("quality", (value) => value === "compatible" || value === "best"),
      nameTemplate: pick("nameTemplate", isString),
      minDurationS: pick("minDurationS", isOptionalNumber),
      maxDurationS: pick("maxDurationS", isOptionalNumber),
      maxKbps: pick("maxKbps", isOptionalNumber),
      writePlaylistFile: pick("writePlaylistFile", isBoolean),
      originalArtwork: pick("originalArtwork", isBoolean),
      syncRemoved: pick("syncRemoved", isBoolean),
      useLibrary: pick("useLibrary", isBoolean),
    };
  } catch {
    return DEFAULT_OPTIONS;
  }
}

function saveOptions(options: JobOptions): void {
  try {
    localStorage.setItem(OPTIONS_KEY, JSON.stringify(options));
  } catch {
  }
}

function inspectKey(url: string, mode: Mode): string {
  return `${url.trim()}|${mode === "track" ? "track" : "playlist"}`;
}

interface ErrorState {
  code: string | null;
  message: string | null;
}

function errorOf(error: unknown): ErrorState {
  return error instanceof ApiError ? { code: error.code, message: error.message } : { code: null, message: null };
}

function presentError(i18n: I18n, error: ErrorState): CommandError {
  const fallback = i18n.locale.startsWith("pt") && error.message ? error.message : i18n.t("error.generic");
  return { text: specificErrorText(i18n, error.code) ?? fallback, detail: error.message };
}

function initialProgress(release: Release, limit: number | null): Record<number, TrackProgress> {
  const progress: Record<number, TrackProgress> = {};
  for (const track of release.tracks) {
    if (!track.available) progress[track.id] = { status: "unavailable", reason: track.reason, reason_code: track.reason_code };
    else if (limit === null || track.position <= limit) progress[track.id] = { status: "queued" };
  }
  return progress;
}

const PENDING: ReadonlySet<TrackProgress["status"]> = new Set(["queued", "retrying", "downloading"]);

export function App(): ReactElement {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [url, setUrl] = useState("");
  const [mode, setMode] = useState<Mode>("playlist");
  const [options, setOptions] = useState<JobOptions>(loadOptions);
  const [release, setRelease] = useState<Release | null>(null);
  const [inspectedKey, setInspectedKey] = useState("");
  const [inspecting, setInspecting] = useState(false);
  const [error, setError] = useState<ErrorState | null>(null);
  const [search, setSearch] = useState<{ query: string; hits: SearchHit[] } | null>(null);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [selectedJobId, setSelectedJobId] = useState<string | null>(null);
  const [views, setViews] = useState<Record<string, JobView>>({});
  const [logs, setLogs] = useState<Record<string, LogLine[]>>({});
  const inspectRequest = useRef(0);
  const i18n = useI18n();
  const { t } = i18n;

  useEffect(() => {
    api.config().then(setConfig, (err: unknown) => setError(errorOf(err)));
    api.jobs().then(setJobs, () => undefined);
  }, []);

  const updateOptions = useCallback((next: JobOptions) => {
    setOptions(next);
    saveOptions(next);
  }, []);

  const runInspect = useCallback(
    async (target: string, key: string, requestId: number): Promise<Release | null> => {
      try {
        const result = await api.inspect(target.trim(), mode);
        if (requestId !== inspectRequest.current) return null;
        setRelease(result);
        setInspectedKey(key);
        setSelectedJobId(null);
        return result;
      } catch (err) {
        if (requestId === inspectRequest.current) {
          setError(errorOf(err));
          setRelease(null);
          setInspectedKey("");
        }
        return null;
      } finally {
        if (requestId === inspectRequest.current) setInspecting(false);
      }
    },
    [mode],
  );

  const inspectUrl = useCallback(
    async (target: string): Promise<Release | null> => {
      const key = inspectKey(target, mode);
      if (!target.trim()) return null;
      if (key === inspectedKey && release) return release;
      const requestId = ++inspectRequest.current;
      setInspecting(true);
      setError(null);
      if (!looksLikeUrl(target)) {
        try {
          const hits = await api.search(target.trim());
          if (requestId === inspectRequest.current) {
            setSearch({ query: target.trim(), hits });
            setRelease(null);
            setInspectedKey("");
            setSelectedJobId(null);
          }
        } catch (err) {
          if (requestId === inspectRequest.current) setError(errorOf(err));
        } finally {
          if (requestId === inspectRequest.current) setInspecting(false);
        }
        return null;
      }
      setSearch(null);
      return runInspect(target, key, requestId);
    },
    [mode, inspectedKey, release, runInspect],
  );

  const inspect = useCallback(() => inspectUrl(url), [inspectUrl, url]);

  const pickSearchHit = (hit: SearchHit): void => {
    setUrl(hit.url);
    setSearch(null);
    void inspectUrl(hit.url);
  };

  const start = useCallback(async () => {
    const current = await inspect();
    if (!current) return;
    setError(null);
    try {
      let outputDir = options.outputDir.trim() || null;
      if (options.askFolder && canPickFolder()) {
        const chosen = await api.pickOutputDir(t("settings.chooseFolderTitle"), outputDir ?? config?.default_output_dir ?? null);
        if (!chosen) return;
        outputDir = chosen;
        updateOptions({ ...options, outputDir: chosen });
      }
      const job = await api.createJob({
        url: current.url,
        mode,
        output_dir: outputDir,
        workers: options.workers,
        limit: mode === "watch" ? null : options.limit,
        interval: options.interval,
        quality: options.quality,
        name_template: options.nameTemplate.trim() || null,
        min_duration_s: options.minDurationS,
        max_duration_s: options.maxDurationS,
        max_kbps: options.maxKbps,
        write_playlist_file: options.writePlaylistFile,
        original_artwork: options.originalArtwork,
        sync_removed: options.syncRemoved,
        use_library: options.useLibrary,
      });
      setViews((prev) => ({
        ...prev,
        [job.id]: {
          release: current,
          progress: initialProgress(current, mode === "watch" ? null : options.limit),
          watchStatus: null,
        },
      }));
      setJobs((prev) => [job, ...prev.filter((item) => item.id !== job.id)]);
      setSelectedJobId(job.id);
    } catch (err) {
      setError(errorOf(err));
    }
  }, [inspect, mode, options, config, t, updateOptions]);

  const selectedJob = jobs.find((job) => job.id === selectedJobId) ?? null;
  const selectedView = selectedJobId ? views[selectedJobId] : undefined;
  const shownRelease = selectedJobId ? (selectedView?.release ?? null) : release;
  const running = selectedJob?.state === "running";

  const cancel = useCallback(() => {
    if (!selectedJobId) return;
    api.cancelJob(selectedJobId).catch((err: unknown) => setError(errorOf(err)));
  }, [selectedJobId]);

  const openFolder = useCallback(() => {
    if (!selectedJobId) return;
    api.openFolder(selectedJobId).catch((err: unknown) => setError(errorOf(err)));
  }, [selectedJobId]);

  const pickOutputDir = useCallback(() => {
    api.pickOutputDir(t("settings.chooseFolderTitle"), options.outputDir.trim() || (config?.default_output_dir ?? null)).then(
      (dir) => {
        if (dir) updateOptions({ ...options, outputDir: dir });
      },
      (err: unknown) => setError(errorOf(err)),
    );
  }, [options, updateOptions, config, t]);

  const patchView = useCallback((jobId: string, patch: (view: JobView) => JobView) => {
    setViews((prev) => {
      const view = prev[jobId] ?? { release: null, progress: {}, watchStatus: null };
      return { ...prev, [jobId]: patch(view) };
    });
  }, []);

  const jobIdsKey = jobs.map((job) => job.id).join(",");
  const jobIds = useMemo(() => (jobIdsKey ? jobIdsKey.split(",") : []), [jobIdsKey]);

  useJobEvents(jobIds, {
    onLog: (jobId, line) =>
      setLogs((prev) => {
        const lines = [...(prev[jobId] ?? []), line];
        if ((lines.at(-2)?.id ?? 0) > line.id) lines.sort((a, b) => a.id - b.id);
        return { ...prev, [jobId]: lines.slice(-MAX_LOG_LINES) };
      }),
    onTrack: (jobId, event: TrackEvent) => {
      if (event.track_id === null) return;
      const trackId = event.track_id;
      patchView(jobId, (view) => ({
        ...view,
        progress: {
          ...view.progress,
          [trackId]: { status: event.status, reason: event.reason, progress: event.progress },
        },
      }));
    },
    onState: (job) => {
      setJobs((prev) => (prev.some((item) => item.id === job.id) ? prev.map((item) => (item.id === job.id ? job : item)) : prev));
      if (job.state !== "running") {
        patchView(job.id, (view) => ({
          ...view,
          progress: Object.fromEntries(
            Object.entries(view.progress).map(([id, value]) => [id, PENDING.has(value.status) ? { status: "idle" } : value]),
          ),
        }));
      }
    },
    onWatch: (jobId, status) => patchView(jobId, (view) => ({ ...view, watchStatus: status })),
    onEnd: (jobId) => {
      if (jobId === selectedJobId) setError(null);
    },
  });

  const selectJob = (jobId: string): void => {
    setSelectedJobId(jobId);
    const view = views[jobId];
    if (view?.release) setUrl(view.release.url);
  };

  const empty = !shownRelease && !selectedJob && !search;
  const jobError: ErrorState | null =
    selectedJob?.state === "failed" ? { code: selectedJob.error_code, message: selectedJob.error } : null;
  const shownError = error ?? jobError;
  const watchStatus = selectedView?.watchStatus;

  return (
    <div className="app">
      <Sidebar version={config?.version} jobs={jobs} selectedJobId={selectedJobId} onSelect={selectJob} />

      <main className="main" data-empty={empty || undefined}>
        <div className="stage" aria-hidden />
        <div className="main-inner">
          {empty && (
            <header className="hero">
              <h1 className="hero-title">{t("app.heroTitle")}</h1>
              <p className="hero-text">{t("app.heroText")}</p>
            </header>
          )}

          <CommandBar
            url={url}
            mode={mode}
            isQuery={Boolean(url.trim()) && !looksLikeUrl(url)}
            inspecting={inspecting}
            running={running}
            error={shownError ? presentError(i18n, shownError) : null}
            onUrlChange={(value) => {
              setUrl(value);
              if (selectedJobId && value !== selectedView?.release?.url) setSelectedJobId(null);
            }}
            onModeChange={setMode}
            onInspect={() => void inspect()}
            onStart={() => void start()}
            onCancel={cancel}
          />
          <Settings config={config} options={options} mode={mode} onChange={updateOptions} onPickFolder={pickOutputDir} />

          {selectedJob?.mode === "watch" && selectedJob.state === "running" && (
            <p className="watch-status" aria-live="polite">
              <Eye size={16} aria-hidden />
              {watchStatus ? describeEvent(i18n, watchStatus.detail, watchStatus.message) : t("app.watchSyncing")}
            </p>
          )}

          {search && !shownRelease && !selectedJob && (
            <SearchResults query={search.query} hits={search.hits} onPick={pickSearchHit} />
          )}

          {shownRelease && (
            <ReleaseView
              release={shownRelease}
              progress={selectedView?.progress ?? {}}
              job={selectedJob}
              onOpenFolder={openFolder}
            />
          )}
          {!shownRelease && selectedJob && (
            <section className="release release-orphan">
              <h1 className="release-title">{selectedJob.title ?? t("app.orphanTitle")}</h1>
              <p className="release-artist">
                {t("app.orphanSummary", { downloaded: selectedJob.counts.downloaded, failed: selectedJob.counts.failed })}
              </p>
            </section>
          )}
        </div>

        <ActivityLog lines={selectedJobId ? (logs[selectedJobId] ?? []) : []} />
      </main>
    </div>
  );
}
