# v0.2 history (Python)

**Status:** Superseded by the [Rust and Tauri rewrite](rust-tauri-rewrite.md) (2026-09-30). Kept as a record of why
v0.2 worked the way it did and of what v1.0 no longer needs.

## Scrapy engine in a subprocess with failover to a direct engine

### Context
The project started out on Scrapy. The Twisted reactor cannot restart within the same process (watch mode runs
several cycles), the GUI needs to genuinely cancel downloads, and the v0.1 pipeline made blocking downloads inside
the reactor, serializing everything. Failover was required in case the main engine went down.

### Decision
- Scrapy runs in a `multiprocessing` subprocess with the `spawn` context; logs return to the parent through
  `QueueHandler`/`QueueListener` and the report through a `Queue`. Cancelling = `terminate()`.
- Audio downloads leave the reactor: a dedicated Twisted `ThreadPool` via `deferToThreadPool`.
- A direct engine (`requests` + `ThreadPoolExecutor`) shares planning, the fetcher and validation.
- `--engine auto`: if Scrapy fails for **infrastructure** reasons (`engine_failure`: crash, import, reactor), the
  direct engine takes over. A domain error (404, unsupported resource) does not trigger failover — it would just
  fail again.

### Alternatives considered
- **Crawl4AI as a second engine**: a Chromium-based page crawler; the data comes from a JSON API, so it would add
  ~300 MB of browser and attack surface for no gain. Rejected.
- **The direct engine only**: simpler, but it would lose Scrapy's AutoThrottle and stats.
- **CrawlerRunner in the same process**: no real cancellation, and exposed to `ReactorNotRestartable`.

### Consequences
- A spawn cost (~1–2 s) per run; acceptable.
- In the exe, `freeze_support()` has to be the launcher's first call.
- Failover is idempotent because intact files are reused.

## Local web interface (FastAPI + React) instead of Tkinter

### Context
The Tkinter GUI had structural problems (widgets modified off the main thread, a "Stop" button that stopped
nothing, invisible subprocess logs), and the goal was a modern, responsive React interface with the Perseus visual
identity.

### Decision
- A FastAPI/uvicorn server **bound to 127.0.0.1 only**, on a random port, with a per-session token injected into
  `index.html`.
- Defenses: `TrustedHostMiddleware` (DNS rebinding), `Origin` checks (CSRF), a strict CSP, no OpenAPI.
- Progress over FastAPI's native SSE, generated from the structured logs (`JobEventHandler`), with replay via
  `Last-Event-ID`.
- A React 19 + Vite frontend with locally bundled fonts (no Google Fonts). Tkinter was removed so as not to maintain
  two GUIs.

### Alternatives considered
- **Electron/Tauri**: a native window, but an extra runtime (Electron) or a Rust toolchain (Tauri), and more complex
  packaging alongside the Python backend.
- **pywebview**: depends on WebView2 and complicates E2E testing.
- **Fix Tkinter**: it would not deliver a React UI.

### Consequences
- The UI is testable with Playwright against the real server (with a simulated SoundCloud).
- It depends on an installed browser; the exe opens the local URL automatically.
- The React build lives outside git and has to be present in the wheel/exe (CI ensures it).

## PyInstaller onedir executable with a visible console

### Context
The `.exe` could not trigger false positives in Windows Defender/SmartScreen. PyInstaller executables are
frequently flagged by heuristics, especially in onefile mode.

### Decision
- **onedir** (a folder with `Perseus.exe` + `_internal\`): onefile self-extracts into `%TEMP%` and runs from there,
  the classic dropper pattern heuristics flag.
- **No UPX**: a compressed/packed executable is a strong malware signal.
- **Version metadata** (company, product, copyright, version) and a custom icon: a binary without metadata hurts its
  reputation.
- **A visible console**: a local server running hidden and opening a port is exactly what antivirus software flags;
  with a console, the user sees the URL and quits by closing the window.
- An `asInvoker` manifest (no administrator prompt).
- Distribution with `SHA256SUMS` and a build provenance attestation in the release.

### Alternatives considered
- **onefile**: easier to distribute, but with a higher false-positive rate.
- **Nuitka**: compiles to C, also suffers from false positives and requires a compiler at build time.
- **A locally compiled PyInstaller bootloader**: reduces known signatures, but requires MSVC at build time.

### Consequences
- The definitive mitigation is **Authenticode signing** with a code-signing certificate (backlog #9).
- Residual false positives: submit for analysis at https://www.microsoft.com/en-us/wdsi/filesubmission.
