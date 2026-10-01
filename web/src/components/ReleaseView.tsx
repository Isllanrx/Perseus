import { ArrowDownToLine, Check, CircleSlash, Copy, Download, FolderOpen, LoaderCircle, RefreshCw, RotateCcw, X } from "lucide-react";
import { type CSSProperties, Fragment, type ReactElement, type ReactNode } from "react";

import { useI18n } from "../i18n/context";
import { reasonText } from "../i18n/describe";
import type { MessageKey } from "../i18n/messages/pt-BR";
import { formatDuration, formatTotalDuration } from "../lib/format";
import { IS_WEB } from "../lib/platform";
import type { Job, Release, TrackProgress, TrackStatus } from "../types";

const STATUS: Record<TrackStatus, { label: MessageKey | null; icon: ReactNode }> = {
  idle: { label: null, icon: null },
  queued: { label: "status.queued", icon: <LoaderCircle size={14} className="spin" aria-hidden /> },
  downloading: { label: "status.downloading", icon: <ArrowDownToLine size={14} aria-hidden /> },
  retrying: { label: "status.retrying", icon: <RotateCcw size={14} aria-hidden /> },
  done: { label: "status.done", icon: <Check size={14} aria-hidden /> },
  reused: { label: "status.reused", icon: <RefreshCw size={14} aria-hidden /> },
  copied: { label: "status.copied", icon: <Copy size={14} aria-hidden /> },
  unavailable: { label: "status.unavailable", icon: <CircleSlash size={14} aria-hidden /> },
  failed: { label: "status.failed", icon: <X size={14} aria-hidden /> },
};

const KIND_LABEL: Record<Release["kind"], MessageKey> = {
  playlist: "release.playlist",
  track: "release.track",
  likes: "release.likes",
  uploads: "release.uploads",
  popular: "release.popular",
  reposts: "release.reposts",
  related: "release.related",
  albums: "release.albums",
  playlists: "release.playlists",
};

const VIRTUAL_TITLES: ReadonlySet<Release["kind"]> = new Set(["likes", "uploads", "popular", "reposts", "albums", "playlists"]);

const FINISHED: ReadonlySet<TrackStatus> = new Set(["done", "reused", "copied", "unavailable", "failed"]);

interface ReleaseViewProps {
  release: Release;
  progress: Record<number, TrackProgress>;
  job: Job | null;
  onOpenFolder: () => void;
}

export function ReleaseView({ release, progress, job, onOpenFolder }: ReleaseViewProps): ReactElement {
  const i18n = useI18n();
  const { t, formatNumber } = i18n;
  const totalMs = release.tracks.reduce((sum, track) => sum + (track.duration_ms ?? 0), 0);
  const available = release.tracks.filter((track) => track.available).length;
  const inScope = release.tracks.filter((track) => progress[track.id] && progress[track.id]?.status !== "idle");
  const finished = inScope.filter((track) => FINISHED.has(progress[track.id]?.status ?? "idle")).length;
  const ratio = inScope.length ? finished / inScope.length : 0;
  const running = job?.state === "running";
  const hiddenTracks = release.total_tracks - release.tracks.length;

  function statusLabel(state: TrackStatus, label: MessageKey, reason: string | null, fraction: number | null): string {
    const base = t(label);
    if (state === "downloading" && fraction !== null) return `${base} ${formatNumber(Math.round(fraction * 100))}%`;
    return reason && state !== "done" ? `${base}: ${reason}` : base;
  }

  return (
    <section className="release" aria-labelledby="release-title">
      <header className="release-head">
        <div className="cover">
          {release.artwork_url ? (
            <img src={release.artwork_url} alt="" width={220} height={220} referrerPolicy="no-referrer" />
          ) : (
            <div className="cover-empty" aria-hidden />
          )}
        </div>
        <div className="release-info">
          <p className="release-kind">{t(KIND_LABEL[release.kind])}</p>
          <h1 id="release-title" className="release-title">
            {VIRTUAL_TITLES.has(release.kind) ? t(KIND_LABEL[release.kind]) : release.title}
          </h1>
          <p className="release-artist">{release.artist}</p>
          <dl className="release-facts">
            <div>
              <dt>{t("release.tracks")}</dt>
              <dd>{formatNumber(release.total_tracks)}</dd>
            </div>
            <div>
              <dt>{t("release.duration")}</dt>
              <dd>{totalMs ? formatTotalDuration(totalMs, i18n.locale) : "--"}</dd>
            </div>
            <div>
              <dt>{t("release.available")}</dt>
              <dd>
                {available === release.tracks.length
                  ? t("release.allAvailable")
                  : t("release.availableCount", { available, total: release.tracks.length })}
              </dd>
            </div>
          </dl>
          {job?.target_dir && (
            <button type="button" className="ghost-button" onClick={onOpenFolder}>
              {IS_WEB ? <Download size={16} aria-hidden /> : <FolderOpen size={16} aria-hidden />}
              {IS_WEB ? t("release.save") : t("release.openFolder")}
            </button>
          )}
        </div>
      </header>

      {inScope.length > 0 && (
        <div className="progress" data-running={running || undefined}>
          <div className="progress-text" aria-live="polite">
            <span>{t("release.progress", { finished, count: inScope.length })}</span>
            {job && (
              <span className="progress-counts">
                {job.counts.downloaded > 0 && <span>{t("release.countDownloaded", { count: job.counts.downloaded })}</span>}
                {job.counts.reused > 0 && <span>{t("release.countReused", { count: job.counts.reused })}</span>}
                {job.counts.unavailable > 0 && (
                  <span>{t("release.countUnavailable", { count: job.counts.unavailable })}</span>
                )}
                {job.counts.failed > 0 && (
                  <span className="is-failed">{t("release.countFailed", { count: job.counts.failed })}</span>
                )}
              </span>
            )}
          </div>
          <div
            className="blade"
            style={{ "--ratio": ratio } as CSSProperties}
            role="progressbar"
            aria-label={t("release.progressLabel")}
            aria-valuemin={0}
            aria-valuemax={inScope.length}
            aria-valuenow={finished}
          >
            <div className="blade-fill" style={{ transform: `scaleX(${ratio})` }} />
          </div>
        </div>
      )}

      <ol className="ledger">
        {release.tracks.map((track, index) => {
          const entry = progress[track.id];
          const groupHeader =
            track.group && track.group !== release.tracks[index - 1]?.group ? (
              <li className="ledger-group">
                {track.group}
              </li>
            ) : null;
          const state = entry?.status ?? (track.available ? "idle" : "unavailable");
          const reason = entry
            ? reasonText(i18n, entry.reason, entry.reason_code)
            : track.available
              ? null
              : reasonText(i18n, track.reason, track.reason_code);
          const status = STATUS[state];
          return (
            <Fragment key={`${track.group ?? ""}:${String(track.id)}:${String(index)}`}>
              {groupHeader}
              <li className="ledger-row" data-status={state}>
                <span className="ledger-pos">{formatNumber(track.position)}</span>
                <span className="ledger-title">
                  <span className="ledger-name">{track.title}</span>
                  <span className="ledger-artist">{track.artist}</span>
                </span>
                <span className="ledger-duration">{formatDuration(track.duration_ms)}</span>
                <span className="ledger-status" title={reason ?? undefined}>
                  {status.icon}
                  {status.label && <span>{statusLabel(state, status.label, reason, entry?.progress ?? null)}</span>}
                </span>
              </li>
            </Fragment>
          );
        })}
      </ol>
      {hiddenTracks > 0 && <p className="ledger-more">{t("release.more", { count: hiddenTracks })}</p>}
    </section>
  );
}
