# Rust backend rewrite and Tauri 2 desktop app

**Status:** Active (2026-09-30). Supersedes the v0.2 decisions ([history](python-v0.2-history.md)).

## Context
The Python executable produced slow, failure-prone downloads. Causes identified in v0.2:
- The Scrapy engine ran in a `spawn` subprocess: in the PyInstaller `.exe`, every job relaunched the whole binary
  and re-imported Scrapy/Twisted; `auto` mode could redo everything on the direct engine after a crash.
- HLS segments were downloaded one after another on a single thread; a failed segment restarted the whole track.
- AutoThrottle, the GIL and synchronous `requests` limited real parallelism.
- UI progress travelled log → `QueueListener` → SSE → local HTTP, with a token, CSRF and DNS rebinding to defend
  against, all inside a 52.5 MB zip with a bundled Python runtime.

## Decision
- **Rust core** (`crates/perseus-core`), async on tokio + reqwest (rustls, HTTP/2, one shared pool), keeping the
  `inspect`/`download`/`watch` vertical slices and the `shared` kernel.
- **A single engine**: tracks as tokio tasks (`JoinSet` + a `workers` semaphore), HLS segments in parallel
  (`buffered`, written in order) with per-segment retries. No subprocess and no failover (there is no longer an
  engine that can "go down" for infrastructure reasons).
- **Metadata and validation with lofty** (replacing mutagen); **m3u8-rs** for HLS.
- **Tauri 2 desktop app** (`src-tauri`): IPC commands plus `perseus://job` events; no HTTP server, port or token.
  React stays in `web/`, swapping `fetch`/`EventSource` for `invoke`/`listen`.
- A separate **CLI** (`crates/perseus-cli`, clap) with the same options and exit codes (minus `--engine`/`--gui`).
- A per-user **NSIS installer** (Tauri bundler) with the WebView2 bootstrapper.

## Alternatives considered
- *Optimize the Python version* (parallel HLS, drop Scrapy): it would fix part of the slowness but keep the 50 MB
  executable, the startup cost and the local HTTP surface.
- *Electron*: a ~100 MB bundle and another JS runtime; Tauri reuses the system WebView2.
- *Tauri 3*: still alpha as of 2026-09; Tauri 2 is the stable line (Dependabot ignores Tauri major versions).

## Consequences
- A 5.2 MB installer; an 11-track / 50 MB playlist in 3.4 s in the app (real measurement).
- `perseus-core` is testable without a network: `wiremock` + `HostPolicy::Loopback`, enabled only under
  `cfg(test)` or the `test-util` feature (a dev-dependency of the app; never in release builds).
- Playwright E2E now runs the real interface against a mocked IPC backend (`web/src/e2e/mockBackend.ts`, dropped
  from the production build); the real backend is covered by `cargo test`.
- The HTTP API load test disappeared together with the HTTP API.
- Requires Microsoft WebView2 (the installer installs it). pywebview's "open in browser" fallback was replaced by a
  native error dialog pointing to the log folder.
