# Roadmap

## Vision
Perseus is the most robust open-source downloader for **public** SoundCloud content, with no user account: correct
by default (never DRM, never a preview disguised as a full track), resilient to API changes, observable, and easy to
use both for non-programmers (installer + app) and for automation (CLI + JSON).

## Principles behind the choices
- Legitimacy over coverage: open streams only; unavailable tracks are explained, not worked around.
- Idempotency: running again never re-downloads what is already intact.
- Explainable failure: every error reaches the user with a cause and a suggested action; every event has a
  `run_id`.
- Trustworthy distribution: verifiable artifacts (checksums, provenance) and, whenever possible, signed ones.

## Horizons
### Shipped (v0.2)
Vertical slice refactor, React UI, tests, Windows exe (Python; replaced in v1.0).

### Now (v1.0 — Rust + Tauri)
Async Rust core, parallel HLS, Tauri 2 app over IPC, native CLI, NSIS installer, 11 languages; the full catalog
without login (profiles, likes, albums, related tracks, search), Best quality, templates, archive, library,
non-destructive sync, `.m3u8`, Range resume; a category-based test suite with CI on 3 engines; the online version on
Vercel. (The artist's original file was dropped: it requires login.)

### Next (v1.1 — what SoundCloud offers in 2026 that nobody uses; backlog 15–26)
- Stations and trending (`/discover/sets/...`), liked playlists, a chart browser by region.
- Preservation (library tracks that disappeared from SoundCloud), ISRC deduplication, an artist radar.
- Advanced search (Creative Commons, facets), completing an album from a single.

### Later (v1.2 — distribution)
- Authenticode signing and automatic updates (`tauri-plugin-updater`).
- Optional remuxing with ffmpeg (fragmented MP4, format conversion).
- macOS/Linux builds of the app.

### Future
- i18n for the CLI (the UI already has 11 languages).
- Optional persistence of job history (in memory today).
- `perseus-core` as a public crate for programmatic use.
