[CmdletBinding()]
param([switch]$Sanity, [switch]$Full, [switch]$Offline)
$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $PSScriptRoot
$web = Join-Path $root 'web'
$reports = Join-Path $root 'target\test-reports'
New-Item -ItemType Directory -Force $reports | Out-Null
$results = [Collections.Generic.List[object]]::new()
$env:CARGO_INCREMENTAL = '0'
$minFreeGb = 15

function Assert-Disk([string]$Before) {
    $free = [math]::Floor((Get-PSDrive C).Free / 1GB)
    if ($free -lt $minFreeGb) {
        Write-Host "ABORTADO antes de '$Before': C: com $free GB livres (minimo $minFreeGb GB)." -ForegroundColor Red
        $results | Format-Table -AutoSize | Out-String | Tee-Object -FilePath (Join-Path $reports 'summary.txt')
        exit 2
    }
}

function Stage([string]$Name, [string]$Dir, [string]$Command) {
    Assert-Disk $Name
    $log = Join-Path $reports "$Name.log"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    Write-Host "==> $Name"
    Push-Location $Dir
    try {
        & pwsh -NoProfile -Command $Command *> $log
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $status = if ($code -eq 0) { 'PASS' } else { 'FAIL' }
    $results.Add([pscustomobject]@{ Etapa = $Name; Status = $status; Segundos = [math]::Round($watch.Elapsed.TotalSeconds, 1); Log = $log })
    Write-Host ("    {0} em {1:N1}s" -f $status, $watch.Elapsed.TotalSeconds)
}

if ($Sanity) {
    Stage 'comments' $root 'cargo xtask comments'
    Stage 'rust-fmt' $root 'cargo fmt --all -- --check'
    Stage 'rust-clippy' $root 'cargo clippy --workspace --all-targets --all-features -- -D warnings'
    Stage 'rust-core' $root 'cargo test -p perseus-core --all-features --lib'
    Stage 'web-unit' $web 'npx vitest run'
    Stage 'web-e2e-chromium' $web 'npx playwright test --project=chromium'
} else {
    Stage 'secrets' $root 'node scripts/scan-secrets.mjs'
    Stage 'comments' $root 'cargo xtask comments'
    Stage 'rust-fmt' $root 'cargo fmt --all -- --check'
    Stage 'rust-clippy' $root 'cargo clippy --workspace --all-targets --all-features -- -D warnings'
    Stage 'rust-tests' $root 'cargo test --workspace --all-features'
    Stage 'rust-coverage' $root 'cargo llvm-cov --workspace --all-features --summary-only --fail-under-lines 80; $c = $LASTEXITCODE; cargo llvm-cov clean --workspace | Out-Null; exit $c'
    Stage 'cargo-deny' $root 'cargo deny check'
    Stage 'web-typecheck' $web 'npm run typecheck'
    Stage 'web-lint' $web 'npm run lint'
    Stage 'web-unit-coverage' $web 'npm run test:coverage'
    Stage 'web-build' $web 'npm run build'
    Stage 'npm-audit' $web 'npm audit --audit-level=high --omit=dev'
    Stage 'web-e2e-3-browsers' $web 'npx playwright test'

    if ($Full) {
        Stage 'rust-fuzz' $root '$env:PROPTEST_CASES = "20000"; cargo test -p perseus-core --release --all-features -- prop_'
        Stage 'rust-performance' $root 'cargo test -p perseus-core --release --all-features -- --ignored --nocapture --test-threads=1 load_ perf_'
        Stage 'rust-mutation' $root 'cargo mutants --in-place --no-shuffle -p perseus-core --output target/mutants -f crates/perseus-core/src/shared/soundcloud/urls.rs -f crates/perseus-core/src/features/download/naming.rs -f crates/perseus-core/src/features/download/library.rs -f crates/perseus-core/src/shared/soundcloud/transcoding.rs -f crates/perseus-core/src/shared/filesystem.rs -f crates/perseus-core/src/shared/retry.rs -f crates/perseus-core/src/shared/format.rs -- --all-features --lib'
        Stage 'web-mutation' $web 'npm run test:mutation'
        Stage 'flaky-rust' $root 'for ($i = 1; $i -le 3; $i++) { cargo test --workspace --all-features; if ($LASTEXITCODE) { exit 1 } }'
        Stage 'flaky-web' $web 'for ($i = 1; $i -le 3; $i++) { npx vitest run; if ($LASTEXITCODE) { exit 1 } }; npx playwright test --project=chromium --repeat-each=3 --retries=0'
    }

    if (-not $Offline) {
        Stage 'rust-live' $root 'cargo test -p perseus-core --all-features -- --ignored --skip load_ --skip perf_'
        Stage 'cli-release-build' $root 'cargo build --release -p perseus-cli'
        Stage 'smoke-real' $root "& '$PSScriptRoot\smoke.ps1'"
    }
}

$results | Format-Table -AutoSize | Out-String | Tee-Object -FilePath (Join-Path $reports 'summary.txt')
if ($results.Status -contains 'FAIL') { exit 1 }
