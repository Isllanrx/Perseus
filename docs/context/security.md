# Security

## Invariants
- Input URLs: only `soundcloud.com` / `m.` / `www.` and `on.soundcloud.com` short links; no credentials and no
  port; the path is restricted to `[A-Za-z0-9_-]`; only `in` and `secret_token` survive, and both are validated by
  regex. Parsing is manual: the WHATWG parser would normalize `..` and percent-encoding before validation.
- Bytes are only downloaded over `https://` from `*.sndcdn.com`, `*.soundcloud.com` and `*.soundcloud.cloud`
  (`Http::ensure_trusted`) — this applies to the stream, every HLS segment, the init map, the artwork and every
  redirect (custom redirect policy; the client is `https_only`).
- Never use `*-encrypted-hls` streams or HLS with an `#EXT-X-KEY` other than `NONE` (anti-circumvention). `snipped`
  previews are refused. HLS with byte ranges is refused (it would produce a corrupted file).
- Pagination: `next_href` is only followed if it starts with `api_base`; there are limits on pages, playlists per
  collection, related tracks and search results. Search queries are 1..=200 chars; result URLs go through
  `normalize_url`.
- Range resume: only a `206` whose `Content-Range` starts at the requested offset is accepted; anything else starts
  over from zero.
- Per-folder archive: entries whose `file` is not a plain name (separator, `:`, `..`, NUL) are dropped on load,
  otherwise reuse or sync could delete or move files outside the folder. The global library (`%LOCALAPPDATA%`) is
  read-only: the source is revalidated as audio of the same format before copying.
- Limits: audio 1 GiB, artwork 10 MiB, manifest 5 MiB, 10k segments, API responses 32 MiB, JS page/bundle 16 MiB,
  URLs 2048 chars, `Retry-After` capped at 60 s.
- File names: sanitization (separators, control characters, reserved Windows names, `..`, NFC) plus
  `ensure_within(base, target)` (lexical normalization, plus `canonicalize` when the path exists, against
  symlinks).
- Atomic writes: `.part` + `rename`, removed by a guard (`PartFile`) on failure or cancellation; the client_id cache
  goes through `tempfile` + `persist` in `%LOCALAPPDATA%\Perseus\cache`, outside the repository.
- The client_id is masked in logs; network errors never carry the URL (CDN signature/client_id). No cookie store.
- Desktop app: no HTTP server and no port. IPC is restricted to the `main` window through capabilities (only events
  are exposed besides the app's commands); CSP in `tauri.conf.json`; `open_folder` only opens the `target_dir` the
  backend registered for the job; `JobIn` uses `deny_unknown_fields` and limits validated in Rust.
- `HostPolicy::Loopback` (accepts `http://127.0.0.1`) only exists under `cfg(test)` / the `test-util` feature.
- Web version: the function never carries audio; `/api/stream` only accepts a transcoding URL (path regex, no query,
  trusted host) and a `track_authorization` matching `[0-9A-Za-z._-]{1,4096}`; `/api/plan` uses
  `deny_unknown_fields` and accepts up to 1,000 tracks; bodies up to 16 KiB; a per-IP limit (150 req/min per
  instance, 429 + `Retry-After`, the same quota as the Vercel Firewall rule); the frontend spaces calls with a
  random gap (`pacer.ts`); no CORS (same origin only); `plan`/`stream`/errors are `no-store`. In the browser,
  `isTrustedMediaUrl` repeats the media allowlist and the `vercel.json` CSP restricts `connect-src`/`img-src` to
  SoundCloud hosts. HSTS without `preload` (it would apply to the whole `isllan.dev` domain).
- Release: actions pinned by SHA, `cargo deny` (RustSec, licenses, sources), `npm audit`, `SHA256SUMS` and build
  provenance attestation.

## History
- v0.1: short links were expanded with `"on.soundcloud.com" in url` (which accepted
  `evil.com/?on.soundcloud.com`); `?in=` was injected without validation; the `transcodings[0]` fallback could pick
  an encrypted stream; the cache was written inside the package; the full client_id was logged at INFO. All fixed in
  v0.2.
- v0.2 (Python): the local HTTP surface was defended with a per-session token, TrustedHost, Origin checks and CSP.
  Removed in v1.0 (Rust rewrite): there is no HTTP API anymore.
- v1.0: CDN redirects are now validated against the allowlist (previously `requests` followed any host).
- v1.0 (catalog and library): a tampered archive with `..\` could direct deletions or moves outside the folder;
  fixed before release with validation on load and a test.
- v1.0 (property tests): file names accepted C1 control characters (U+0080–U+009F) and bidirectional formatting
  (U+202E) from titles; a title could display a fake extension in Explorer. `INVALID_CHARS` now covers `\p{Cc}` and
  bidi characters.
