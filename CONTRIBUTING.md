# Contributing to Perseus

Thanks for wanting to help. This guide covers setting up your environment, validating a change and submitting it.

## Environment

You need [Rust 1.90+](https://rustup.rs/) (MSVC toolchain on Windows, with `rustfmt` and `clippy`) and
[Node.js 24+](https://nodejs.org/).

```powershell
npm ci                  # Tauri CLI
npm ci --prefix web     # interface
npm run dev             # app with hot reload
```

## Validation

Every change has to pass the same steps CI runs:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets     # pedantic; CI treats warnings as errors
cargo test --workspace
cargo deny check                           # advisories, licenses and sources

cd web
npm run check          # types, ESLint, Vitest with coverage, and build
npm run test:e2e       # Playwright: the real interface against a mocked IPC backend
cd ..
```

To run everything at once, with one log per stage in `target\test-reports\` and a summary in `summary.txt`:

```powershell
pwsh scripts/test-all.ps1 -Sanity    # quick pass after a change: fmt, clippy, core, Vitest, E2E on Chromium
pwsh scripts/test-all.ps1            # default: everything below except the heavy stages
pwsh scripts/test-all.ps1 -Full      # adds fuzzing, performance, mutation and flaky-test detection
pwsh scripts/test-all.ps1 -Offline   # skips the stages that reach the real SoundCloud
```

The orchestrator disables Rust incremental compilation, cleans instrumented coverage builds when it finishes and
aborts if drive C: has less than 15 GB free. The full category matrix (including the categories that do not apply
and why) lives in the [testing strategy](docs/decisions/testing-strategy.md).

| Suite | Where | What it proves |
| --- | --- | --- |
| Unit, integration and harness | `cargo test --workspace` | Domain rules and the engine against a simulated SoundCloud (wiremock and a chaotic TCP server in `src/test_support.rs`) |
| Resilience and persistence | `features/download/resilience_tests.rs` | Network drops mid-file with `Range` resume, corrupted `.part` files, 429 with `Retry-After`, 5xx chaos, cancellation, concurrent jobs in the same folder, lost/legacy/future archives, atomic writes |
| Properties and fuzzing | `src/property_tests.rs` (`PROPTEST_CASES=20000` in fuzz mode) | URLs, file names (Windows, bidi, traversal), templates and the JSON/archive/audio parsers never panic |
| Rust ↔ frontend contract | `contracts/` + `jobs.rs` + `web/src/contracts.test.ts` | Every log event has a translation in all 11 languages; the payload the frontend sends is accepted by Rust. Regenerate with `UPDATE_CONTRACTS=1 cargo test -p perseus -- contract` |
| CLI | `crates/perseus-cli/tests/cli.rs` | Golden `--help` (`UPDATE_GOLDEN=1`), hostile inputs exit with code 2, JSON logs carry `run_id` and never the `client_id` |
| Configuration and security | `src-tauri/tests/config.rs`, `scripts/scan-secrets.mjs` | CSP without `unsafe-*`, minimal IPC, per-user installer, versions in sync, no committed secrets |
| Performance | `cargo test -p perseus-core --release -- --ignored --test-threads=1 load_ perf_` | Load, 1→16 worker scalability, failure spikes, soak with memory measurement, a 5,000-track volume test |
| Rust coverage | `cargo llvm-cov --workspace --all-features --summary-only` | At least 80% line coverage |
| Mutation | `cargo mutants` (see `test-all.ps1`) and `npm run test:mutation` | The tests catch bugs introduced on purpose |
| Live | `cargo test -p perseus-core -- --ignored --skip load_ --skip perf_` | An HLS download against the real SoundCloud |
| Frontend | `npm run check` (in `web/`) | Types, ESLint, Vitest with coverage, and build |
| E2E | `npm run test:e2e` (in `web/`) | Chromium, WebKit and Firefox: flows, keyboard, forms, navigation, offline privacy, XSS, axe, 6 viewports, RTL and visual regression (`--update-snapshots` to accept a change) |
| Smoke | `pwsh scripts/smoke.ps1` | The release CLI against real links: exit codes, likes, `.m3u8`, archive, idempotency, library, filters, installer |

Changes to the download flow must also be seen working with a real SoundCloud link (smoke test or the app).
Describe what you tested in the pull request and attach the log when it is relevant.

### Reference performance

Performance tests against a local simulated server (release build; they measure the engine, not your
connection):

| Scenario | Result |
| --- | --- |
| 500 tracks with 40 injected 503 errors | 5.3 s, no failures; a second run downloads nothing |
| Scalability (150 ms latency per track) | 1 simultaneous download: 4.9 tracks/s; 16 simultaneous: 70.8 tracks/s (14.6x) |
| Burst of 120 503 errors with 16 downloads | 200 of 200 tracks completed |
| 60 consecutive runs | stable timing and stable live memory (2.2 MB → 2.7 MB) |
| 5,000-track playlist | metadata filled in 100 batches in 0.37 s |

## CI/CD

| Workflow | When it runs | What it does |
| --- | --- | --- |
| [`ci.yml`](.github/workflows/ci.yml) | Pushes to `main` and `dev`, and pull requests | Interface types, ESLint, Vitest with coverage and build; rustfmt, pedantic clippy and Rust tests on Windows; Playwright E2E on Chromium, WebKit and Firefox; secret scanning, cargo-deny and npm audit; workflow linting; installer build |
| [`comments.yml`](.github/workflows/comments.yml) | Pushes to `main` and `dev`, and pull requests | `xtask` tests and the no-comments check |
| [`quality.yml`](.github/workflows/quality.yml) | Weekly and on demand | Fuzzing with 20,000 cases, performance tests and mutation testing (cargo-mutants and Stryker) |
| [`security.yml`](.github/workflows/security.yml) | Pull requests, pushes to `main` and `dev`, and weekly | CodeQL for Rust and TypeScript, full-history secret scanning with gitleaks, and dependency review |
| [`scorecard.yml`](.github/workflows/scorecard.yml) | `main` and weekly | OpenSSF Scorecard |
| [`release.yml`](.github/workflows/release.yml) | Pushes to `main` with a new version, `v*` tags, or on demand | Builds the installer and CLI from scratch, generates `SHA256SUMS`, attests build provenance and publishes a **pre-release** |
| [`dependabot.yml`](.github/dependabot.yml) | Weekly | Proposes updates for actions, crates and npm packages |

Every job starts with read-only permissions; only the publishing job is granted write access.

### Publishing a release

1. Bump the version in `Cargo.toml` (workspace), `src-tauri/tauri.conf.json`, `package.json` and
   `web/package.json` in a pull request.
2. The flow is `dev` → pull request → `main`. On merge, `release.yml` reads the version from `Cargo.toml` and, if
   the `v<version>` tag does not exist yet, builds everything, creates the tag on the merge commit and publishes the
   pre-release. A merge without a version change does not produce a release. Pushing a `v*` tag by hand also
   publishes one.
3. Test the installer with real links, then mark the release as final on GitHub.

`pwsh scripts/collect-release.ps1` gathers the installer, the zipped CLI and `SHA256SUMS` locally into `dist\`.

## Standards

- Follow the vertical slice architecture: a new use case goes in `crates/perseus-core/src/features/`; only
  cross-cutting infrastructure goes in `crates/perseus-core/src/shared/`. The CLI and the app (`src-tauri`) hold no
  business rules.
- No `unwrap()` in production code and no `unsafe` in the core or the CLI. TypeScript runs in `strict` mode, with no
  `any`.
- Expected errors are variants of `perseus_core::Error`; nothing is silently swallowed.
- No `println!` outside the CLI. Progress the interface cares about is an `Event` emitted through the `Reporter`
  (it becomes a structured log line and an IPC event at the same time).
- The IPC contract lives in `src-tauri/src/dto.rs` and is mirrored in `web/src/types.ts`; change both together.
- Interface text is never hard-coded in a component: every key starts in `web/src/i18n/messages/pt-BR.ts` and must
  exist in the other 10 languages (`tsc` fails if one is missing, and the dictionary test rejects unknown
  placeholders). Backend messages shown to the user carry a stable code (`code`, `reason_code` or the typed event in
  `detail`) so they can be translated.
- **No comments in code** (Rust, TypeScript/TSX, JavaScript, CSS, HTML, TOML, YAML and PowerShell). Names, types and
  tests carry the meaning; explanations, decisions and processes go in Markdown (`README.md`, `scripts/README.md`,
  `docs/context/`, decisions in `docs/decisions/`). The only exception is a **single-line** tool directive
  (`// eslint-...`, `// @ts-...`, the version after a SHA-pinned `uses:`, `#Requires`...). `cargo xtask comments`
  fails CI if any comment is found; `cargo xtask comments --strip` removes them with proof that the code did not
  change. In `clap` structs, descriptions go in `#[arg(help = "...")]`, never in doc comments. Details in
  [`xtask/README.md`](xtask/README.md).
- Never circumvent DRM or any other protection. Pull requests that do will not be accepted.

## Commits and pull requests

- Commit messages explain **why** a change exists, not just what changed.
- Do not use `--no-verify` or force-push to `main`.
- One pull request per topic, with tests covering the new behavior or the fixed bug.

## Security

Security issues do not belong in public issues. See [SECURITY.md](SECURITY.md).
