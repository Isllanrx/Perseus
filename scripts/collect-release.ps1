[CmdletBinding()]
param(
  [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root "src-tauri/tauri.conf.json") -Raw | ConvertFrom-Json).version
$out = Join-Path $root $OutDir

if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null

$installer = Get-ChildItem (Join-Path $root "target/release/bundle/nsis") -Filter "Perseus_${version}_x64-setup.exe"
if (-not $installer) { throw "Instalador Perseus_${version}_x64-setup.exe nao encontrado. Rode 'npm run build'." }
Copy-Item $installer.FullName (Join-Path $out $installer.Name)

$cli = Join-Path $root "target/release/perseus-cli.exe"
if (-not (Test-Path $cli)) { throw "perseus-cli.exe nao encontrado. Rode 'cargo build --release -p perseus-cli'." }
Compress-Archive -Path $cli, (Join-Path $root "LICENSE") -DestinationPath (Join-Path $out "perseus-cli-${version}-win-x64.zip")

$sums = Get-ChildItem $out -File | Sort-Object Name | ForEach-Object {
  "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
}
[System.IO.File]::WriteAllText((Join-Path $out "SHA256SUMS"), (($sums -join "`n") + "`n"))

Get-ChildItem $out | Format-Table Name, Length
