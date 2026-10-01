import type { CoreEvent } from "./events";
import { TransferError, fetchArtwork, fetchHls, fetchProgressive } from "./fetch-audio";
import { HttpError } from "./http";
import { TRACK_POLICY, backoffDelay, isAbort, sleep } from "./retry";
import type { Artifact, Sink } from "./sink";
import { type Bytes, type TagInput, coverMime, looksLikeAudio, tagAudio } from "./tags";
import type { Container, PlanFolder, PlanIn, PlanOut, ReadyItem, StreamOut } from "./types";

export interface EngineDeps {
  plan: (body: PlanIn, signal: AbortSignal) => Promise<PlanOut>;
  stream: (item: ReadyItem, signal: AbortSignal) => Promise<StreamOut>;
  sink: Sink;
  emit: (event: CoreEvent) => void;
  signal: AbortSignal;
  now?: () => number;
  random?: () => number;
}

export interface EngineReport {
  title: string | null;
  downloaded: number;
  reused: number;
  failed: number;
  unavailable: number;
  bytes: number;
  elapsedS: number;
  cancelled: boolean;
  fatal: { code: string; message: string } | null;
  artifact: Artifact | null;
}

interface Entry {
  file: string;
  artist: string;
  title: string;
  durationMs: number | null;
}

class WriteError extends Error {
  override name = "WriteError";
}

const MIME: Record<Container, string> = { mp3: "audio/mpeg", mp4: "audio/mp4", ogg: "audio/ogg" };
const PROGRESS_INTERVAL_MS = 250;

function round(value: number, digits: number): number {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

function reasonOf(error: unknown): string {
  if (error instanceof HttpError || error instanceof TransferError || error instanceof WriteError) return error.message;
  if (error instanceof Error && error.message) return error.message;
  return "erro inesperado";
}

function tagInput(item: ReadyItem, cover: Bytes | null): TagInput {
  return {
    title: item.title,
    artist: item.tags.artist ?? item.artist,
    album: item.album,
    trackNumber: item.track_number,
    totalTracks: item.total_tracks,
    year: item.tags.year,
    comment: item.permalink_url,
    genre: item.tags.genre,
    isrc: item.tags.isrc,
    label: item.tags.label,
    composer: item.tags.composer,
    copyright: item.tags.copyright,
    albumArtist: item.tags.album_artist,
    cover: cover ? { data: cover, mime: coverMime(cover) } : null,
  };
}

export function playlistText(album: string, folder: PlanFolder, entries: ReadonlyMap<number, Entry>): string | null {
  const lines = ["#EXTM3U", `#PLAYLIST:${album}`];
  let tracks = 0;
  for (const item of folder.items) {
    const entry = entries.get(item.track_id);
    if (!entry) continue;
    const seconds = entry.durationMs === null ? -1 : Math.trunc(entry.durationMs / 1000);
    lines.push(`#EXTINF:${String(seconds)},${entry.artist} - ${entry.title}`, entry.file);
    tracks += 1;
  }
  return tracks ? `${lines.join("\n")}\n` : null;
}

export async function runEngine(spec: PlanIn & { workers: number }, deps: EngineDeps): Promise<EngineReport> {
  const { sink, emit, signal } = deps;
  const now = deps.now ?? (() => performance.now());
  const random = deps.random ?? Math.random;
  const started = now();
  const report: EngineReport = {
    title: null,
    downloaded: 0,
    reused: 0,
    failed: 0,
    unavailable: 0,
    bytes: 0,
    elapsedS: 0,
    cancelled: false,
    fatal: null,
    artifact: null,
  };
  const finish = (): EngineReport => {
    report.elapsedS = round((now() - started) / 1000, 3);
    report.cancelled = signal.aborted;
    if (report.cancelled) emit({ event: "cancelled" });
    else if (!report.fatal) {
      emit({
        event: "finished",
        downloaded: report.downloaded,
        reused: report.reused,
        failed: report.failed,
        unavailable: report.unavailable,
        bytes: report.bytes,
        elapsed_s: report.elapsedS,
      });
    }
    return report;
  };

  const { workers, ...body } = spec;
  let plan: PlanOut;
  try {
    plan = await deps.plan(body, signal);
  } catch (error) {
    if (isAbort(error) || signal.aborted) return finish();
    const code = error instanceof HttpError ? error.code : "api";
    emit({ event: "resolve_failed", url: spec.url, reason: reasonOf(error) });
    report.fatal = { code, message: reasonOf(error) };
    return finish();
  }
  report.title = plan.title;

  const queue: { folder: PlanFolder; item: ReadyItem }[] = [];
  for (const folder of plan.folders) {
    emit({
      event: "planned",
      album: folder.album,
      artist: folder.owner,
      selected: folder.items.length,
      total: folder.total_tracks,
      target_dir: folder.folder,
    });
    for (const item of folder.items) {
      if (item.status === "ready") queue.push({ folder, item });
      else if (item.status === "unavailable") {
        report.unavailable += 1;
        emit({ event: "track_unavailable", track_id: item.track_id, reason: item.reason, reason_code: item.reason_code });
      } else {
        report.failed += 1;
        emit({ event: "track_failed", track_id: item.track_id, reason: item.reason, reason_code: item.reason_code });
      }
    }
  }

  const artwork = new Map<string, Promise<Bytes | null>>();
  const coverFor = (item: ReadyItem): Promise<Bytes | null> => {
    const key = `${item.artwork_url ?? ""}|${item.artwork_fallback_url ?? ""}`;
    let cover = artwork.get(key);
    if (!cover) {
      cover = fetchArtwork([item.artwork_url, item.artwork_fallback_url], signal).catch(() => null);
      artwork.set(key, cover);
    }
    return cover;
  };
  const entries = new Map<string, Map<number, Entry>>();
  const record = (folder: PlanFolder, item: ReadyItem): void => {
    let folderEntries = entries.get(folder.folder);
    if (!folderEntries) {
      folderEntries = new Map();
      entries.set(folder.folder, folderEntries);
    }
    folderEntries.set(item.track_id, {
      file: item.file_name,
      artist: item.artist,
      title: item.title,
      durationMs: item.duration_ms,
    });
  };

  const transfer = async (item: ReadyItem): Promise<Bytes[]> => {
    const stream = await deps.stream(item, signal);
    let lastProgress = -Infinity;
    const onProgress = (bytes: number, fraction: number | null): void => {
      const at = now();
      if (at - lastProgress < PROGRESS_INTERVAL_MS && fraction !== 1) return;
      lastProgress = at;
      emit({ event: "track_progress", track_id: item.track_id, bytes, fraction });
    };
    return stream.protocol === "hls"
      ? fetchHls(stream.parts, signal, onProgress)
      : fetchProgressive(stream.url, signal, onProgress);
  };

  const download = async (folder: PlanFolder, item: ReadyItem): Promise<void> => {
    if ((await sink.existing(folder.folder, item.file_name)) !== null) {
      report.reused += 1;
      record(folder, item);
      emit({ event: "track_reused", track_id: item.track_id, file: item.file_name });
      return;
    }
    const itemStarted = now();
    for (let attempt = 1; ; attempt += 1) {
      try {
        const parts = await transfer(item);
        if (!looksLikeAudio(parts, item.container)) throw new TransferError("arquivo de audio invalido");
        const tagged = tagAudio(parts, item.container, tagInput(item, await coverFor(item)));
        const blob = new Blob(tagged, { type: MIME[item.container] });
        await sink.write(folder.folder, item.file_name, blob).catch((error: unknown) => {
          throw isAbort(error) ? error : new WriteError(`nao foi possivel gravar: ${reasonOf(error)}`);
        });
        report.downloaded += 1;
        report.bytes += blob.size;
        record(folder, item);
        emit({
          event: "track_downloaded",
          track_id: item.track_id,
          file: item.file_name,
          bytes: blob.size,
          attempts: attempt,
          elapsed_s: round((now() - itemStarted) / 1000, 3),
          protocol: item.protocol,
        });
        return;
      } catch (error) {
        if (isAbort(error) || signal.aborted) return;
        if (error instanceof HttpError && error.kind === "unavailable") {
          report.unavailable += 1;
          emit({ event: "track_unavailable", track_id: item.track_id, reason: error.message, reason_code: null });
          return;
        }
        if (error instanceof WriteError || attempt >= TRACK_POLICY.attempts) {
          report.failed += 1;
          emit({ event: "track_failed", track_id: item.track_id, reason: reasonOf(error), reason_code: null });
          return;
        }
        const retryAfter = error instanceof HttpError && error.retryAfterS ? error.retryAfterS * 1000 : 0;
        const delay = Math.max(backoffDelay(attempt, TRACK_POLICY, random), retryAfter);
        emit({
          event: "track_retry",
          track_id: item.track_id,
          title: item.title,
          attempt,
          max: TRACK_POLICY.attempts,
          reason: reasonOf(error),
          delay_s: round(delay / 1000, 1),
        });
        try {
          await sleep(delay, signal);
        } catch {
          return;
        }
      }
    }
  };

  let next = 0;
  const worker = async (): Promise<void> => {
    while (next < queue.length && !signal.aborted) {
      const job = queue[next];
      next += 1;
      if (job) await download(job.folder, job.item);
    }
  };
  await Promise.all(Array.from({ length: Math.max(1, Math.min(workers, queue.length)) }, worker));

  if (!signal.aborted) {
    for (const folder of plan.folders) {
      const folderEntries = entries.get(folder.folder);
      const text = folder.playlist_file && folderEntries ? playlistText(folder.album, folder, folderEntries) : null;
      if (!folder.playlist_file || !text) continue;
      try {
        await sink.write(folder.folder, folder.playlist_file, new Blob([text], { type: "audio/x-mpegurl" }));
        emit({ event: "playlist_file_written", file: folder.playlist_file, tracks: folderEntries?.size ?? 0 });
      } catch {
      }
    }
  }
  try {
    report.artifact = await sink.finish(plan.title);
  } catch {
    report.artifact = null;
  }
  if (report.artifact) emit({ event: "archive_ready", file: report.artifact.name, bytes: report.artifact.blob.size });
  return finish();
}
