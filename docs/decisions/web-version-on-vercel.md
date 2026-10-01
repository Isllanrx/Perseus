# Web version on Vercel: a metadata-only server, audio straight from the CDN to the browser

**Status:** Active (2026-09-30). Complements the [Rust and Tauri rewrite](rust-tauri-rewrite.md) (the desktop app is
unchanged).

## Context
Goal: offer Perseus online (own domain `isllan.dev`, Vercel account) for people who would rather not install
anything, responsive on desktop, Android and iOS, without affecting the desktop app. Facts measured on 2026-09-30:
- `api-v2.soundcloud.com` sends no `Access-Control-Allow-Origin`: the browser cannot talk to the API.
- `cf-media.sndcdn.com`, `cf-hls-media.sndcdn.com`, `playback.media-streaming.soundcloud.cloud` and `i1.sndcdn.com`
  answer with `Access-Control-Allow-Origin: *` (progressive files, manifests, segments and cover art).
- Vercel functions have a 4.5 MB limit per non-streaming response, bill origin bandwidth and have a bounded
  duration.
- The Vercel Rust runtime (public beta, `vercel_runtime` 2.4) builds `api/*.rs` with `cargo build --bin <name>` at
  the workspace root; validated on `amazonlinux:2023` (the build base image): 1m16s, 9.6 MB, no cmake.

## Decision
- **A metadata-only server** (`crates/perseus-web`, Axum; the `api/perseus.rs` adapter with `VercelLayer`):
  `config`, `inspect`, `search`, `plan` and `stream`, all small JSON. It reuses the whole `perseus-core`: URL
  allowlist, short links, client_id, DRM-free stream selection, names, folders and numbering.
- **Disk-free planning in the core** (`features::download::remote::plan_remote`): `planning.rs` was split into a pure
  part (`folders`, `Selection`, `claim_unique_name`, `playlist_file_name`) and a disk part; `plan_download` and the
  engine use the same pure part, so desktop and web apply exactly the same rules.
- **Audio straight from the CDN in the browser** (`web/src/lib/web`): `/api/stream` returns the signed URL
  (progressive) or the HLS parts already validated by the core; the tab downloads, validates, writes tags (ID3v2.3
  for MP3, `ilst` for fragmented MP4) and delivers the file. Per-track and per-segment retries follow the same policy
  as the core.
- **The same contract as the IPC**: the web backend implements the `src-tauri` commands and events (`JobEvent`); the
  React UI is the same, selected at build time (`vite --mode web`). The unused branch is dropped from the bundle
  (verified: the desktop bundle contains no web code).
- **Destination**: a folder picked by the user through the File System Access API (desktop Chromium; atomic writes
  via `createWritable`) or the browser's downloads (a single file; playlists as an uncompressed `.zip` via
  `client-zip`), which works on Android and iOS.
- **No watch mode, library, sync or bandwidth limit on the web**: they need an always-on process or persistent disk.
  The web plan accepts up to 1,000 tracks (keeping the response under the function limit); above that, a translated
  error asks for a limit or points to the desktop app.
- **Responsive** with rules scoped to `:root[data-platform="web"]` (an attribute only the web build's HTML gets):
  iOS safe areas, `100dvh`, 44 px touch targets, no zoom when focusing inputs, no pull-to-refresh.

## Alternatives considered
- *The server downloads and returns the audio*: hits the 4.5 MB limit and the function duration, and pays for
  bandwidth on every download. Rejected.
- *Rewrite the backend in TypeScript (Node on Vercel)*: it would duplicate the allowlist, client_id discovery,
  stream selection and naming, with a risk of drifting from the desktop app. Rejected in favor of the Rust core.
- *Vercel standalone mode (an Axum server handling everything)*: the server would also serve the static files,
  taking them off the CDN. Rejected; the classic `api/*.rs` mode plus a rewrite keeps static files on the CDN.
- *lofty compiled to WASM in the browser*: exact tag parity, but it would require Rust + wasm-bindgen in the web
  build. The TS writers were checked against lofty: same fields as the file produced by the desktop app (the M4A
  from HLS AAC has identical tags; the web version also writes `LABEL`, which lofty does not map from `Publisher`
  for MP4).

## Consequences
- No audio byte passes through Vercel; the only cost is invocations returning small JSON.
- Every user shares the client_id and the function's outbound IP. Three layers of pacing: (1) in the browser, a
  `Pacer` shared by the tab spaces `plan`/`stream` with a random 400–900 ms gap (~92/min per visitor, with no
  robot-like bursts); (2) in the function, 150 req/min per IP and per instance (429 + `Retry-After`, which the
  engine honors); (3) a Vercel Firewall rate-limit rule with the same quota (the only free rule on Hobby; configured
  in the dashboard, since `vercel.json` does not accept rate limits). A fixed `SOUNDCLOUD_CLIENT_ID` avoids discovery
  on cold starts. SoundCloud blocking the Vercel IP is an accepted risk.
- **Hobby plan (free, no overage billing; an exhausted quota pauses the resource for up to 30 days)**:
  `maxDuration` of 30 s (the fixed 2 GB of provisioned memory is billed by run time), gzip at the origin (Fast
  Origin Transfer), `config` with `s-maxage=86400` and `inspect`/`search` with `s-maxage=120` (the CDN answers
  without invoking the function), a single default region, and no paid features (Blob, KV, Cron, Image
  Optimization).
- Ogg Opus has no tags on the web (it only shows up when it is the track's only format).
- iOS: the manifest uses `display: browser`; home-screen apps in standalone mode do not deliver downloads reliably.
