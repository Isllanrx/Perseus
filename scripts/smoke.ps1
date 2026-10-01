[CmdletBinding()]
param(
    [string]$Playlist = 'https://soundcloud.com/forss/sets/soulhack',
    [string]$Likes = 'https://soundcloud.com/kitty-326047409/likes',
    [string]$Track = 'https://soundcloud.com/forss/flickermood'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cli = Join-Path $root 'target\release\perseus-cli.exe'
$work = Join-Path ([IO.Path]::GetTempPath()) "perseus-smoke-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
$failures = 0

function Check([string]$Name, [scriptblock]$Body) {
    $watch = [Diagnostics.Stopwatch]::StartNew()
    try {
        & $Body
        Write-Host ("PASS  {0,-58} {1,6:N1}s" -f $Name, $watch.Elapsed.TotalSeconds)
    } catch {
        $script:failures++
        Write-Host ("FAIL  {0,-58} {1}" -f $Name, $_.Exception.Message)
    }
}

function Invoke-Cli([string[]]$Arguments) {
    $output = & $cli @Arguments 2>&1 | Out-String
    return @{ Code = $LASTEXITCODE; Output = $output }
}

function Read-Report([string]$Path) { Get-Content $Path -Raw | ConvertFrom-Json }

if (-not (Test-Path $cli)) { throw "CLI de release ausente: $cli (rode cargo build --release -p perseus-cli)" }
New-Item -ItemType Directory -Force $work | Out-Null

try {
    Check 'versao da CLI' {
        $r = Invoke-Cli @('--version')
        if ($r.Code -ne 0 -or $r.Output -notmatch 'perseus') { throw "saida: $($r.Output)" }
    }
    Check 'link fora do SoundCloud sai com codigo 2' {
        $r = Invoke-Cli @('--info', 'https://evil.example/a/b')
        if ($r.Code -ne 2) { throw "codigo $($r.Code)" }
    }
    Check 'template sem {title}/{id} sai com codigo 2' {
        $r = Invoke-Cli @($Track, '--name-template', '{artist}', '-o', $work)
        if ($r.Code -ne 2) { throw "codigo $($r.Code)" }
    }
    Check '--info de playlist lista faixas' {
        $r = Invoke-Cli @('--info', $Playlist)
        if ($r.Code -ne 0 -or $r.Output -notmatch 'Soulhack') { throw "codigo $($r.Code)" }
    }
    Check 'faixa avulsa baixa e valida' {
        $report = Join-Path $work 'track.json'
        $r = Invoke-Cli @($Track, '-o', $work, '--report-json', $report)
        $json = Read-Report $report
        if ($r.Code -ne 0 -or -not $json.summary.ok -or $json.summary.downloaded -lt 1) { throw "codigo $($r.Code)" }
    }
    Check 'curtidas (3 primeiras, melhor qualidade) + m3u8 + archive' {
        $report = Join-Path $work 'likes.json'
        $r = Invoke-Cli @($Likes, '-l', '3', '--quality', 'best', '-o', $work, '--report-json', $report)
        $json = Read-Report $report
        if ($r.Code -ne 0 -or -not $json.summary.ok -or ($json.summary.downloaded + $json.summary.reused) -ne 3) { throw "codigo $($r.Code)" }
        $folder = Get-ChildItem $work -Directory | Where-Object Name -like '*Likes' | Select-Object -First 1
        if (-not $folder) { throw 'pasta de curtidas ausente' }
        foreach ($name in @('Likes.m3u8', '.perseus-archive.json')) {
            if (-not (Test-Path (Join-Path $folder.FullName $name))) { throw "$name ausente" }
        }
        if (Get-ChildItem $folder.FullName -Filter '*.part') { throw 'sobrou arquivo .part' }
    }
    Check 'segunda execucao nao baixa nada (idempotencia)' {
        $report = Join-Path $work 'likes2.json'
        $r = Invoke-Cli @($Likes, '-l', '3', '--quality', 'best', '-o', $work, '--report-json', $report)
        $json = Read-Report $report
        if ($r.Code -ne 0 -or $json.summary.downloaded -ne 0 -or $json.summary.reused -ne 3) { throw "baixadas $($json.summary.downloaded)" }
    }
    Check 'biblioteca copia faixa para outra pasta sem rede' {
        $other = Join-Path $work 'outra'
        $report = Join-Path $work 'copy.json'
        $r = Invoke-Cli @($Track, '-o', $other, '--report-json', $report)
        if ($r.Code -ne 0 -or $r.Output -notmatch 'biblioteca local\s+1') { throw "codigo $($r.Code)" }
    }
    Check 'filtro de duracao marca faixas como indisponiveis' {
        $report = Join-Path $work 'filter.json'
        $r = Invoke-Cli @($Playlist, '--max-duration', '1', '-o', (Join-Path $work 'filtro'), '--report-json', $report)
        $json = Read-Report $report
        if ($json.summary.downloaded -ne 0 -or $json.summary.unavailable -lt 1) { throw "codigo $($r.Code)" }
    }
    $installer = Get-ChildItem (Join-Path $root 'target\release\bundle\nsis') -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($installer) {
        Check "instalador $($installer.Name) (< 15 MB, PE valido)" {
            if ($installer.Length -gt 15MB) { throw "$($installer.Length) bytes" }
            $head = [IO.File]::ReadAllBytes($installer.FullName)[0..1]
            if ([Text.Encoding]::ASCII.GetString($head) -ne 'MZ') { throw 'nao e executavel PE' }
        }
    }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}

if ($failures -gt 0) { Write-Host "smoke: $failures falha(s)"; exit 1 }
Write-Host 'smoke: tudo OK'
