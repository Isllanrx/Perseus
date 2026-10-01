---
updated: 2026-09-30
sprint: v1.0 — Rust rewrite + Tauri app
---

# Current sprint: v1.0 — Rust rewrite + Tauri app

| Item | Status | Notes |
|---|---|---|
| `perseus-core`: URLs, client_id, API, models, transcoding (parity with v0.2) | Done | tests ported from the pytest cases |
| Single async engine (tokio), HLS with parallel segments and per-segment retry | Done | validated live (progressive and HLS AAC) |
| Tags and validation with lofty (ID3v2.3, MP4, Opus + cover art) | Done | |
| Watch mode with bounded per-track retries | Done | settle logic tested; long live run still pending |
| `perseus-cli` with the same options and exit codes | Done | without `--engine`/`--gui` |
| Tauri 2 app: IPC, events, replay, single instance, folder dialog, startup error | Done | validated by driving the real app over CDP |
| React moved to `invoke`/`listen`, per-track % progress | Done | 97.5% line coverage |
| Playwright E2E with a mocked IPC backend | Done | flows, a11y, 6 viewports, languages |
| NSIS installer (5.2 MB) + zipped CLI + SHA256SUMS | Done | `scripts/collect-release.ps1` |
| CI/CD rewritten (Rust on Windows, cargo-deny, CodeQL for Rust) | Done | first GitHub runs observed after the v1.0 merge |
| Python removal (code, tests, PyInstaller, uv) | Done | v0.2 decisions kept in `docs/decisions/python-v0.2-history.md` |
| Parity with the Scrapy crawler: streaming hydration, per-batch failure isolation, panic → track failure, 8-request API cap, `stats` | Done | audit on 2026-09-30 |
| Absolute destination folder required in the app; average speed and retries in the CLI summary | Done | |
| Interface in 11 languages with a code-based selector, system detection and RTL | Done | 70 Vitest tests, 13 E2E (including Arabic + axe) |
| Full catalog without login: profile tabs, per-folder collections, related tracks, search | Done | live: 77 liked tracks, albums, sets, popular, related, search |
| Best quality, name templates, length filters, bandwidth limit, original cover art | Done | |
| Per-folder archive, global library (copy without network), sync to `Removed/`, `.m3u8`, Range resume, disk space check | Done | 88 Rust tests, 77 Vitest (93.6% lines), 14 E2E |
| Category-based test suite + CI on 3 browsers + weekly quality run | Done | 15/15 stages; Rust 141, frontend 113, E2E 70; Rust mutation only equivalents, frontend 93.4% |
| Disk protection (lightweight debug profile and a disk space check in the orchestrator) | Done | target/debug 26.8 GB → 1.5 GB |
| Online version on Vercel: `perseus-web` + `api/`, `plan_remote` in the core, in-browser backend, ID3/MP4 tags in TS, ZIP/folder, responsive UI (Android/iOS) | Done | core Rust 129 + web 13; Vitest 163; web E2E 27 (3 profiles); Linux build validated on amazonlinux:2023; real download under the production CSP |
| Randomized pacing in the browser + limits aligned with the Hobby plan (150/min, 30 s `maxDuration`, gzip, CDN cache) | Done | Firewall rule documented in `docs/online-version.md` |
| Desktop app validation after the web version | Done | `test-all.ps1` (desktop E2E on 3 browsers + visual, live, smoke) and a regenerated NSIS installer |
| Vercel deployment + `perseus.isllan.dev` domain | Done | API and UI verified in production |
| No comments in code: `xtask` with lexers, 679 removed with proof, `comments.yml` workflow | Done | the `--help` golden test caught clap doc comments (converted to `help =`); empty `catch` allowed in ESLint |
| Cleanup: docs in `docs/`, unused dependencies removed, allowlist-based `.vercelignore`, `yoke-derive` 0.8.4 | Done | `cargo deny` passes again; 174 Rust tests, 163 Vitest |
| v1.0.0 pre-release published automatically on merge to `main` | Done | installer, zipped CLI, `SHA256SUMS` and provenance |
| Vercel Firewall rate-limit rule (150 req/min per IP on `/api/`) | Pending | steps in `docs/online-version.md` |
| `on.soundcloud.com` short link validated live | Pending | |
| Authenticode signing of the installer | Pending | depends on a certificate |

## Sprint risks
- `devUrl`: building the `perseus` crate outside the Tauri CLI produces a development binary (see
  coding-standards).
- Dependabot opened major-version bumps for several actions; review them before merging.
