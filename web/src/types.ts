export type Mode = "playlist" | "track" | "watch";
export type JobState = "running" | "completed" | "partial" | "failed" | "cancelled";
export type TrackStatus =
  | "idle"
  | "queued"
  | "downloading"
  | "retrying"
  | "done"
  | "reused"
  | "copied"
  | "unavailable"
  | "failed";
export type Quality = "compatible" | "best";
export type ReleaseKind =
  | "track"
  | "playlist"
  | "likes"
  | "uploads"
  | "popular"
  | "reposts"
  | "related"
  | "albums"
  | "playlists";

export interface AppConfig {
  version: string;
  default_output_dir: string;
  max_workers: number;
  default_workers: number;
  interval_min: number;
  interval_max: number;
}

export interface Track {
  id: number;
  position: number;
  title: string;
  artist: string;
  duration_ms: number | null;
  artwork_url: string | null;
  permalink_url: string | null;
  available: boolean;
  reason: string | null;
  reason_code: string | null;
  group: string | null;
}

export interface Release {
  kind: ReleaseKind;
  url: string;
  playlist_context: string | null;
  title: string;
  artist: string;
  artwork_url: string | null;
  permalink_url: string | null;
  total_tracks: number;
  tracks: Track[];
}

export interface JobCounts {
  downloaded: number;
  reused: number;
  failed: number;
  unavailable: number;
  selected: number | null;
}

export interface Job {
  id: string;
  url: string;
  mode: Mode;
  state: JobState;
  title: string | null;
  target_dir: string | null;
  created_at: number;
  finished_at: number | null;
  error: string | null;
  error_code: string | null;
  counts: JobCounts;
  report: Record<string, unknown> | null;
}

export interface JobOptions {
  outputDir: string;
  workers: number;
  limit: number | null;
  interval: number;
  askFolder: boolean;
  quality: Quality;
  nameTemplate: string;
  minDurationS: number | null;
  maxDurationS: number | null;
  maxKbps: number | null;
  writePlaylistFile: boolean;
  originalArtwork: boolean;
  syncRemoved: boolean;
  useLibrary: boolean;
}

export type SearchHitKind = "track" | "playlist" | "album" | "user";

export interface SearchHit {
  kind: SearchHitKind;
  title: string;
  subtitle: string;
  url: string;
  artwork_url: string | null;
  duration_ms: number | null;
  track_count: number | null;
}

export interface TrackProgress {
  status: TrackStatus;
  reason?: string | null;
  reason_code?: string | null;
  progress?: number | null;
}

export interface LogLine {
  id: number;
  ts: number;
  level: string;
  message: string;
  detail?: Record<string, unknown> | null;
}

export interface TrackEvent {
  track_id: number | null;
  status: TrackStatus;
  reason: string | null;
  reason_code: string | null;
  bytes: number | null;
  progress: number | null;
}

export type JobEventKind = "log" | "track" | "state" | "watch" | "end";

export interface JobEvent {
  job_id: string;
  id: number;
  event: JobEventKind;
  data: unknown;
}
