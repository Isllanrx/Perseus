---
updated: 2026-09-30
scope: v1.0 (Rust + Tauri), after the full catalog and the test suite
---

# Architecture review

## What is solid
- **Vertical slices** (`inspect`, `download`, `watch`) over a `shared` kernel; the CLI and the app are thin adapters
  over `run_download` and typed events. No business rules in the adapters.
- **Explicit trust boundaries**: URLs checked against an allowlist with manual parsing, bytes only from allowed hosts
  (redirects included), `ensure_within` on every path, sanitized names (Windows, C0/C1, bidi), archives validated
  on load. Covered by property tests and hostile inputs.
- **Layered idempotency**: a per-folder archive (id → file), a name-based fallback, a global library; unique names
  per folder; atomic writes.
- **Measured resilience**: transport and track retries kept separate and counted, `Range` resume, immediate
  cancellation, failure isolation per batch and per task (a panic becomes a track failure).
- **A verified Rust ↔ frontend contract** through shared files; the frontend has no network access (CSP
  `connect-src` limited to IPC) and minimal IPC.

## Risks and debt
| Risk | Impact | Current mitigation | Next step |
|---|---|---|---|
| Unofficial SoundCloud API (client_id from the bundle, `api-v2` endpoints) | High | discovery + a single renewal, live tests and smoke tests | backlog: detect breakage in the weekly CI with a live test |
| File lock only within a process | Medium | app jobs are serialized | backlog 27: lock the `.part` through `fs4` |
| Core tested on Windows only | Medium | frontend E2E on 3 engines | backlog 28–29 |
| App state in memory (jobs vanish on close) | Low | archive/library persist what matters | backlog 12 |
| `library.json` grows without bound | Low | entries for missing files are dropped on save | measure with large libraries before optimizing |
| Unsigned installer | Medium (SmartScreen) | SHA256SUMS + provenance | Authenticode certificate |

## Related decisions
[Vertical slices](../decisions/vertical-slice-architecture.md), [no DRM](../decisions/no-drm-circumvention.md),
[Rust + Tauri](../decisions/rust-tauri-rewrite.md),
[catalog, library and sync](../decisions/profile-catalog-library-sync.md),
[testing strategy](../decisions/testing-strategy.md).
