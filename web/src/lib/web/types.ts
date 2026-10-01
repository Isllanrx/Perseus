import type { Mode, Quality } from "../../types";

export type Container = "mp3" | "mp4" | "ogg";

export interface PlanIn {
  url: string;
  mode: Exclude<Mode, "watch">;
  limit: number | null;
  quality: Quality;
  name_template: string | null;
  min_duration_s: number | null;
  max_duration_s: number | null;
  write_playlist_file: boolean;
  original_artwork: boolean;
}

export interface Tags {
  artist: string | null;
  genre: string | null;
  isrc: string | null;
  label: string | null;
  composer: string | null;
  copyright: string | null;
  album_artist: string | null;
  year: number | null;
}

export interface ReadyItem {
  status: "ready";
  track_id: number;
  file_name: string;
  title: string;
  artist: string;
  album: string;
  track_number: number;
  total_tracks: number;
  duration_ms: number | null;
  permalink_url: string | null;
  artwork_url: string | null;
  artwork_fallback_url: string | null;
  transcoding_url: string;
  track_authorization: string | null;
  protocol: string;
  container: Container;
  estimated_kbps: number;
  tags: Tags;
}

export interface SkippedItem {
  status: "unavailable" | "failed";
  track_id: number;
  reason: string;
  reason_code: string | null;
}

export type PlanItem = ReadyItem | SkippedItem;

export interface PlanFolder {
  folder: string;
  album: string;
  owner: string;
  single: boolean;
  total_tracks: number;
  playlist_file: string | null;
  items: PlanItem[];
}

export interface PlanOut {
  url: string;
  title: string;
  folders: PlanFolder[];
}

export type StreamOut = { protocol: "progressive"; url: string } | { protocol: "hls"; parts: string[] };
