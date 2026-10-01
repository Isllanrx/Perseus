# Architecture

## Stack
Rust 1.90+ (edition 2024) in a Cargo workspace: tokio, reqwest 0.13 (rustls/aws-lc, HTTP/2), serde, lofty 0.25
(tags and validation), m3u8-rs 6, tracing (with a JSON subscriber and a rolling appender), clap 4, directories.
Desktop app: Tauri 2.12 (WebView2) with the opener, dialog and single-instance plugins. Frontend: React 19,
TypeScript 6, Vite 8, `@tauri-apps/api` 2. Installer: NSIS through the Tauri bundler. Decision:
[Rust and Tauri rewrite](../decisions/rust-tauri-rewrite.md).

## Workspace
- `crates/perseus-core` — the domain, with no Tauri dependency. Vertical slices:
  - `features/inspect.rs`, `features/download/` (engine, audio, hls, planning, naming, validation, tagging,
    library [per-folder archive + global library], models), `features/watch.rs` (built on top of download through
    its public `run_download` API).
  - `shared/` — config, error, http, retry, filesystem, format, `soundcloud/` (auth, client, models, transcoding,
    urls).
  - `events.rs` — the typed `Event` plus the `Reporter` (structured log + adapter sink).
- `crates/perseus-cli` — the `perseus-cli` binary (clap). No business rules.
- `src-tauri` — the `perseus` binary (the window). `commands.rs` (IPC), `jobs.rs` (JobManager), `dto.rs` (contract).
- `crates/perseus-web` — HTTP backend for the online version (Axum), metadata only; `api/` — the Vercel function
  adapter (`vercel_runtime`). Decision: [web version on Vercel](../decisions/web-version-on-vercel.md).
- `xtask` — automation (`cargo xtask comments`: the [no-comments policy](../decisions/no-comments-in-code.md)).
  `std` only; it never ships in the app, the installer or the Vercel function.
- `web/` — React; `dist/` is embedded by `tauri::generate_context!` at compile time. `vite --mode web` builds the
  online version (same UI, with the in-browser backend in `src/lib/web`).

## Download flow
1. The adapter canonicalizes the URL (`SoundCloudClient::canonicalize`: allowlist plus hop-by-hop short-link
   expansion).
2. `run_download(client, request, cancel, reporter)`:
   - `resolve` → `plan_download` (numbering by position in the full playlist, folder, `ensure_within`).
   - Stubs are hydrated through `fetch_tracks` (batches of 50, up to 4 in parallel, order preserved).
   - `build_job` per track (`choose_transcoding`; DRM, previews and region locks become `unavailable`).
   - Fully hydrated tracks become tasks immediately; stub batches are fetched in parallel (`buffer_unordered(4)`)
     and each batch schedules its tracks as it arrives (parity with the v0.2 Scrapy spider). A failed batch only
     marks the tracks in that batch; a track missing from the response becomes `not_returned`.
   - `Scheduler::claim_unique_name`: file names are unique per folder (seeded from the archive); a template without
     `{id}` that collides, or a name truncated at 200 characters, gets ` [id]` appended.
   - Each job becomes a tokio task in a `JoinSet`, bounded by `Semaphore(workers)`; a panic in a task becomes a
     track failure (via a `task::Id -> track_id` map); cancellation goes through a `CancellationToken` (the future
     is dropped and the `PartFile` guard removes the `.part` file).
   - `DownloadReport.stats` (`DownloadStats`): selected, scheduled, attempts, track retries, transport retries (a
     per-job `task_local` counter in `http::count_transport_retries`, re-applied in every `JoinSet` task), library
     copies, moves to `Removed/`, bytes/s.
3. `AudioFetcher::download`, under a lock per final file (a global map of `Weak<Mutex>` keyed in lowercase): two
   jobs in the same process never write the same `.part`. Order: file already in the folder (by archive, then by
   name) → copy from the global library → network with retries (3x, equal-jitter backoff) on a fresh
   `stream_url` → `.part` → validation → `rename` → tags (best effort) → `TrackDownloaded`.
   - Progressive: chunked streaming with a Content-Length check and a 1 GiB cap; a mid-file drop keeps the `.part`
     and the next attempt resumes with `Range` (only a `206` with a consistent `Content-Range` is accepted).
   - HLS: `hls::segment_urls` (best variant, refuses DRM and byte ranges, init map) → parallel segments
     (`buffered(6)`, written in order), each with its own retry.

## HTTP call map (one shared `reqwest::Client`)
| Caller | Endpoint | Retry | Auth |
|---|---|---|---|
| `ClientIdProvider::discover` | `GET soundcloud.com` + trusted JS bundles | transport (connection/timeout/408/429/5xx, `Retry-After`) | — |
| `ClientIdProvider::verify` | `GET api-v2/search/tracks?limit=1` | transport | candidate |
| `SoundCloudClient::expand_shortlink` | `GET on.soundcloud.com/*` without automatic redirects, max 5 hops | transport | — |
| `resolve` / `fetch_tracks` / `stream_url` | `api-v2/resolve`, `api-v2/tracks?ids=`, `<transcoding.url>` | transport + 1 refresh on 401/403 | client_id |
| `AudioFetcher` | progressive CDN, HLS manifest/segments, artwork | transport + track retry (fresh stream) + per-segment retry | signed URL |

The main client only follows redirects to allowlisted hosts (`redirect::Policy::custom`). `SoundCloudClient` caps
concurrent API requests at 8 (`Semaphore`, the equivalent of Scrapy's `CONCURRENT_REQUESTS`); the permit covers the
request and its body and is released before a client_id refresh.

## Concurrency
- `ClientIdProvider`: `tokio::sync::Mutex` (single flight for discovery and renewal).
- Tracks: tokio tasks (true multi-core parallelism); synchronous CPU work (lofty) runs in `spawn_blocking`.
- App: each job is a task on `tauri::async_runtime`; at most 3 active, 50 retained and 5,000 events per job.

## Observability
- Every `Event` is logged through `tracing` with `run_id` and `event` (stable names: `download_planned`,
  `track_downloaded`, `track_reused`, `track_copied`, `track_moved`, `playlist_file_written`, `download_retry`,
  `track_unavailable`, `track_failed`, `download_finished`, `download_cancelled`, `watch_*`); the UI receives the
  serialized `Event` (tagged `event`, snake_case), pinned in `contracts/log-events.json` and delivered to the
  adapter sink. In the app, `run_id` is the job id.
- App: a daily JSON log in `%LOCALAPPDATA%\Perseus\data\logs` (7 files kept), filtered with `PERSEUS_LOG`.
- CLI: text/JSON on stderr, JSON with `--log-file`, `--report-json` (`DownloadReport::to_json` with a `summary`).

## Desktop app (src-tauri)
- IPC: `get_config`, `inspect`, `search`, `list_jobs`, `create_job`, `cancel_job`, `job_events` (replay after a
  reload), `open_folder` (only the job's known folder), `pick_output_dir(title, initial)` (native dialog in Rust).
- Events: the `perseus://job` channel carries `{job_id, id, event: log|track|state|watch|end, data}`; `id` is
  sequential per job and generated under the same lock as the state change. The frontend deduplicates by id and
  never applies a stale `state`.
- `Job` receives an `EventEmitter` (closure) instead of the `AppHandle`, so it is testable without the Tauri
  runtime.
- Minimal capabilities (`core:event:allow-listen/unlisten`); plugins are only used from the Rust side.
- CSP in `tauri.conf.json` (images only from `*.sndcdn.com`, `connect-src ipc:`), `freezePrototype`.
- Single instance: a second launch focuses the existing window. Closing the window cancels every job.
- `JobIn::validated` requires an absolute destination folder (the app's working directory is its install folder).

## Online version (Vercel)
- Routes `GET /api/config|inspect|search` and `POST /api/plan|stream`; `vercel.json` rewrites `/api/<route>` to the
  single `api/perseus.rs` function with `?route=` (the router accepts both the original and the rewritten path).
- `plan` uses `perseus_core::features::download::plan_remote` (the same rules as `plan_download`, without disk
  access); `stream` only accepts transcoding URLs
  (`/media/soundcloud:tracks:<id>/<uuid>/stream/(progressive|hls)`) and returns the signed URL or the HLS parts.
- Browser (`web/src/lib/web`): `backend.ts` implements the IPC commands and events; `engine.ts` downloads from the
  CDN with `workers` (max 6), retries, validation, tags (`tags/`), `.m3u8` and the destination (`sink.ts`: folder or
  `.zip`). `pacer.ts` keeps one pace per tab for `plan`/`stream` (a random 400–900 ms gap), below the 150/min per
  IP enforced by the function and the Firewall, which fits the Hobby plan (see
  [web version on Vercel](../decisions/web-version-on-vercel.md)).
- `lib/backend.ts` picks Tauri or the browser based on the build mode; `lib/platform.ts` exposes `IS_WEB`.
- Dev: `cargo run -p perseus-vercel` (port 3000) + `npm --prefix web run dev:web` (5173, proxies `/api`).

## Internationalization (web/src/i18n)
- 11 locales (en, es, zh-CN, hi, fr, pt-BR, pt-PT, ar, bn, ru, id); pt-BR is the source dictionary and defines
  `MessageKey`; the others are `Record<MessageKey, Message>` (a missing key breaks `tsc`). Plurals use
  `Intl.PluralRules`.
- A dependency-free runtime: `I18nProvider` (detects `navigator.languages`, persists to `perseus.locale.v1`, sets
  `lang`/`dir` on `<html>`), `useI18n`, and numbers/times/units through `Intl`.
- The sidebar selector shows the code (EN, PT-BR...); the native name goes in `aria-label`/`title`.
- The backend keeps its logs in ASCII pt-BR but sends stable data for the UI to translate: `CommandError.code`,
  `JobOut.error_code`, `reason_code` on tracks and `track` events, and `detail` (the serialized `Event`) on
  `log`/`watch` events. `i18n/describe.ts` builds the sentences and falls back to the original text when there is
  no translation.
- RTL (Arabic): logical CSS properties, with the active-job marker and the progress bar mirrored.

## Tests
Full matrix in the [testing strategy](../decisions/testing-strategy.md).
- `crates/perseus-core/src/test_support.rs` (`cfg(test)`): wiremock helpers, `RawServer` (raw-TCP HTTP that cuts the
  body mid-stream, honors `Range` and measures concurrency) and the `LIVE_BYTES` counting allocator
  (`#[global_allocator]`).
- `features/download/resilience_tests.rs`: network, concurrency, persistence and performance (`perf_*` ignored by
  default).
- `src/property_tests.rs`: proptest; the same suite becomes fuzzing with `PROPTEST_CASES=20000`.
- `contracts/` at the root: `log-events.json` (generated by Rust, read by Vitest) and `job-in.json` (read by both).
- `crates/perseus-cli/tests/cli.rs` (the real binary, golden files in `tests/golden/`); `src-tauri/tests/config.rs`.
- `web/e2e/`: `perseus.spec.ts`, `interaction.spec.ts`, `visual.spec.ts` (baselines in `visual.spec.ts-snapshots/`).
- `web/e2e-web/` (`playwright.web.config.ts`): the online version on desktop Chromium, Pixel 7 and iPhone 14
  (WebKit), with mocked `/api` and CDN; phones from 320 to 414 px with no horizontal scroll and touch targets of at
  least 44 px.
- `scripts/`: `test-all.ps1` (orchestrator), `smoke.ps1` (real network), `scan-secrets.mjs`, `collect-release.ps1`.
- CI: `ci.yml` (every push; E2E across a chromium/webkit/firefox matrix) and `quality.yml` (weekly: fuzzing,
  performance, mutation).
