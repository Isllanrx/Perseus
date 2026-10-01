# Coding standards and pitfalls

## Comments
Decision: [no comments in code](../decisions/no-comments-in-code.md).
- Code has no comments; only single-line tool directives. Explanations go in Markdown. `cargo xtask comments` runs in
  CI (`.github/workflows/comments.yml`) and in `scripts/test-all.ps1`.
- Pitfall: `///` on a struct/enum deriving from `clap` is the `--help` text. Use `#[arg(help = "...")]`,
  `#[value(help = "...")]` or `#[command(about = "...")]`; the xtask refuses doc comments in those files.
- After `cargo xtask comments --strip`, run `cargo fmt --all` (enums and lists may fit on one line again).
- An empty `catch {}` means intentional best effort (ESLint `no-empty` with `allowEmptyCatch`); any other empty
  block is an error.

## Rust
- Workspace lints: clippy `pedantic` (CI runs with `-D warnings`), `unwrap_used` warns; `unsafe` is forbidden in the
  core and the CLI (`#![forbid(unsafe_code)]`). `expect` only with a message, for invariants (static regexes, tests).
- Expected errors are variants of `perseus_core::Error`; `is_retryable()` decides retries and `kind()` feeds the UI
  and the exit codes. `reqwest::Error` becomes `Error::Transfer` without the URL (it may contain the client_id or a
  signature).
- Futures sent to another thread (Tauri commands, `JoinSet::spawn`) must be `Send`: do not use closures in
  `stream::iter(..).map(..).buffered(..)` that borrow from the enclosing scope — collect the futures into a `Vec`
  first (see `fetch_tracks`). `#[tokio::test]` is single-threaded and does not catch this; only the app build does.
- No heavy synchronous I/O on the runtime: lofty (validation/tags) and `read_dir` run in `spawn_blocking`.
- On Windows, renaming the `.part` fails while its handle is open: close (`drop`) the file before validating or
  renaming it.
- `directories::ProjectDirs` on Windows uses `%LOCALAPPDATA%\Perseus\{cache,data}`; logs live in `data\logs`.
- `tracing-appender` with `max_log_files` fails if the folder does not exist: create it first (first launch).
- lofty writes ID3v2.3 with `WriteOptions::use_id3v23(true)` and warns (through `log`) that it swaps UTF-8 for
  UTF-16: silenced with `lofty=error` in the filter.
- A plain `cargo build` of the `perseus` crate produces a binary that loads `devUrl`; production builds only go
  through `npm run build` / `npx tauri build` (which enables `custom-protocol`).
- Test constructors (`SoundCloudClient::for_tests`, `HostPolicy::Loopback`) only exist under `cfg(test)` or the
  `test-util` feature (a dev-dependency of the app). Never enable them in release builds.
- The pt-BR `link.exe` prints "Criando biblioteca..." for every binary: the `linker_messages` lint is turned off.
- `Path::new(".mp3").extension()` is `None` (to Rust it is a hidden file without an extension). Compare extensions
  with `ext.eq_ignore_ascii_case(format.extension.trim_start_matches('.'))`. This bug made the archive useless.
- `tokio::task_local!` does not cross `JoinSet::spawn`: each task has to be wrapped in the scope again
  (`count_transport_retries`).
- `#![forbid(unsafe_code)]` does not accept a local `allow`; the crate uses `forbid` outside tests and `deny` in
  tests, where only the counting allocator (memory-leak detection) has an `allow`.
- `--all-features` plus full PDBs on Windows once pushed `target/` to 34 GB: the `dev` profile uses
  `debug = "line-tables-only"` and dependencies without symbols. To inspect variables in a debugger:
  `CARGO_PROFILE_DEV_DEBUG=full`.
- `wiremock` tests: the first registered mock that matches wins; "fail first, then succeed" responses use
  `up_to_n_times` mounted before the success mock. Remember that `run_download` resolves the URL again.
- The final file has an ID3 tag at the start: comparing audio with the original requires stripping the header
  (`audio_payload`).

## Frontend
- The IPC contract in `src-tauri/src/dto.rs` is mirrored in `web/src/types.ts` (snake_case); change both together.
  The contract tests (`contracts/`) break if only one side changes.
- Stryker uses the `command` runner (the Vitest runner did not execute tests in a sandbox whose path contains a
  space and reported a false 14%). Tests that read files outside `web/` are excluded from the mutation command.
- Vitest: `new URL("../x", import.meta.url)` becomes a Vite asset import and is blocked outside the root; read
  shared files with `resolve(process.cwd(), "..", ...)`.
- IPC errors arrive as `{kind, code, message}` (the `CommandError` serialization); `api.ts` turns them into
  `ApiError`.
- `web/src/e2e/mockBackend.ts` is only imported under `vite --mode e2e`; the `import.meta.env.MODE === "e2e"`
  condition is constant in the production build and the module is dropped (check with `grep mockIPC web/dist`).
- The events `mockIPC` warns "Couldn't find callback id" under `StrictMode`; it is an artifact of the mock, not the
  app.
- TypeScript is pinned to **6.0** (TS 7 does not expose the JS API typescript-eslint needs); ESLint is pinned to
  **9** (jsx-a11y does not support ESLint 10 yet). Do not use `--legacy-peer-deps`.
- The CSP forbids `data:` for fonts and scripts: Vite runs with `assetsInlineLimit: 0`.
- No hard-coded interface text in components: a key in `i18n/messages/pt-BR.ts` plus the 10 translations. Backend
  messages shown to the user need a stable code (never translate by comparing pt-BR strings).
- Error/status state stores a code plus details, never the rendered text: the text is built at render time so a
  language switch updates the whole screen.
- Playwright's `getByText` matches substrings: with the activity log translated, use `{ exact: true }`.
- CSS: use logical properties (`inline-end`, `text-align: start`) because of Arabic (RTL); `letter-spacing` is reset
  for ar/hi/bn/zh (it breaks Arabic letter joining).
- Backend messages stay in ASCII pt-BR (logs and fallback); the UI translates them by code.
- The Windows working tree may be in CRLF (`.gitattributes` normalizes to LF on commit): scripts that process text
  must tolerate `\r\n`.
