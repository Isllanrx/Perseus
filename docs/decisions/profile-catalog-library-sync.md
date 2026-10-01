# Full profile catalog, local library and non-destructive sync

**Status:** Active (2026-09-30)

## Context
A comparison with scdl, yt-dlp, DownCloud, imthaghost/scdl, SoundCloudExplode and NotTobi/soundcloud-dl revealed
three gaps: (1) profiles, likes, reposts, popular tracks, albums and related tracks were not supported;
(2) idempotency depended on the file name, so changing the template or the order downloaded tracks again; (3) there
was no sync, no search and no quality choice. None of those tools reuses tracks across folders, resumes through
verified Range requests, checks disk space or syncs without deleting.

## Decision
- Profile tabs become a `PlaylistSource` (Likes, Uploads, PopularTracks, Reposts, Related), and `/albums` and
  `/sets` become `Resource::Collection`, with one folder per playlist. Generic pagination through `next_href`,
  accepted only if it starts with `api_base` (no arbitrary hosts are followed).
- A per-folder archive (`.perseus-archive.json`, keyed by track id, with file and quality) and a global library
  (`library.json` in `data_local_dir`). The library only copies valid files from disk and rewrites their tags;
  entries whose file has disappeared are dropped on save.
- Opt-in sync moves tracks that left the playlist into `Removed/`. It never deletes.
- `Compatible` quality (MP3, the default) or `Best` (the highest bitrate estimated from `preset`), still excluding
  encrypted streams and previews.
- HTTP Range resume only for progressive streams, accepting a `206` with a consistent `Content-Range`; any other
  response starts over from zero.
- Downloading the original file stays out: the `/tracks/{id}/download` endpoint returns 401 without login (the
  anonymous rule; see [no DRM circumvention](no-drm-circumvention.md)).

## Alternatives considered
- A single global archive (like yt-dlp's `--download-archive`): rejected because it does not allow the same track
  in two playlists with different numbering; the library solves that by copying.
- A sync that deletes (scdl `--sync`): rejected for being destructive and irreversible.
- Hard links instead of copies: rejected because they fail across volumes and each copy needs its own tags.

## Consequences
- More on-disk state (two JSON files) with atomic writes; corruption falls back to "empty" and only costs a
  re-download.
- `DownloadRequest` gained `options` and `library_path`; Tauri's `JobIn` accepts the new fields with safe defaults.
