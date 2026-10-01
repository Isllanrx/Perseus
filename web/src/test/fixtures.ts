import type { Job, Release, Track } from "../types";

export function makeTrack(position: number, overrides: Partial<Track> = {}): Track {
  return {
    id: 1000 + position,
    position,
    title: `Faixa ${position}`,
    artist: "Perseus Ensemble",
    duration_ms: 120_000,
    artwork_url: null,
    permalink_url: null,
    available: true,
    reason: null,
    reason_code: null,
    group: null,
    ...overrides,
  };
}

export function makeRelease(overrides: Partial<Release> = {}): Release {
  const tracks = [makeTrack(1), makeTrack(2), makeTrack(3, { available: false, reason: "protegida por DRM", reason_code: "drm" })];
  return {
    kind: "playlist",
    url: "https://soundcloud.com/perseus/sets/argonautas",
    playlist_context: null,
    title: "Argonautas",
    artist: "Perseus Ensemble",
    artwork_url: null,
    permalink_url: null,
    total_tracks: tracks.length,
    tracks,
    ...overrides,
  };
}

export function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job1",
    url: "https://soundcloud.com/perseus/sets/argonautas",
    mode: "playlist",
    state: "running",
    title: "Argonautas",
    target_dir: "C:/music/Perseus Ensemble - Argonautas",
    created_at: 1_790_000_000,
    finished_at: null,
    error: null,
    error_code: null,
    counts: { downloaded: 0, reused: 0, failed: 0, unavailable: 0, selected: 3 },
    report: null,
    ...overrides,
  };
}
