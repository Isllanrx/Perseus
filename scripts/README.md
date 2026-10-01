# Scripts

Local and release automation. The scripts carry no comments (see the policy in
[`xtask/README.md`](../xtask/README.md)); what each one does is documented here.

| Script | Usage | What it does |
| --- | --- | --- |
| `test-all.ps1` | `pwsh scripts/test-all.ps1 [-Sanity] [-Full] [-Offline]` | Runs the Perseus checks and writes one log per stage to `target\test-reports` (summary in `summary.txt`). It does not stop at the first failure; it exits with 1 if any stage fails. |
| `smoke.ps1` | `pwsh scripts/smoke.ps1` | Smoke test against the real SoundCloud: release binaries, resolution, download, idempotency and exit codes. |
| `collect-release.ps1` | `pwsh scripts/collect-release.ps1 -OutDir dist` | Gathers the release artifacts (NSIS installer and zipped CLI) and generates `SHA256SUMS`. |
| `scan-secrets.mjs` | `node scripts/scan-secrets.mjs` | Scans tracked files for secrets: private keys, provider tokens and plain-text SoundCloud client ids. Exits with 1 if it finds anything. |

## `test-all.ps1`

- **Default:** secrets, comments (`cargo xtask comments`), formatting, clippy, Rust tests (unit, integration,
  harness, contract, regression, resilience, concurrency, persistence and properties), Rust coverage, cargo-deny,
  CLI tests, frontend types/lint/Vitest with coverage/build, npm audit, E2E on Chromium, WebKit and Firefox
  (accessibility, keyboard, i18n/RTL, responsiveness, offline privacy and visual regression), live tests and smoke.
- **`-Sanity`:** a quick check after a change: formatting, clippy, core Rust tests, Vitest and E2E on Chromium only.
- **`-Full`:** adds the heavy stages: fuzzing (20,000 cases per property), performance (load, scalability, spikes,
  soak, volume, memory), mutation (cargo-mutants and Stryker) and flaky-test detection (repeated runs).
- **`-Offline`:** skips the stages that reach the real SoundCloud (live tests and smoke).
- Sets `CARGO_INCREMENTAL=0`: test runs gain nothing from the incremental cache, which once grew to 13 GB in
  `target/`.
- Aborts before a stage if drive C: has less than 15 GB free.

## `smoke.ps1`

Requires network access. Uses `target\release\perseus-cli.exe` (`cargo build --release -p perseus-cli`) and, if
present, checks the NSIS installer. Exits with code 1 at the first failed check.

## `collect-release.ps1`

Run it after `npm run build` (installer) and `cargo build --release -p perseus-cli`. `SHA256SUMS` uses the
`sha256sum` format (`<hash>  <file>`) so CI can verify it with `sha256sum --check`.

## `scan-secrets.mjs`

The dummy identifiers used by the tests are kept in an allowlist inside the script.
