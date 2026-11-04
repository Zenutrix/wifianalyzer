<#
.SYNOPSIS
  Baut den WiFi Analyzer (Release) und erzeugt das Download-Paket in dist\.

.DESCRIPTION
  Ergebnis:
    dist\WiFiAnalyzer-<version>-win64.zip   (EXE + README + LICENSE)
    dist\WiFiAnalyzer.exe                   (einzelne EXE)
    dist\SHA256SUMS.txt                     (Prüfsummen)

.PARAMETER NoBuild
  Vorhandenen Build in target\release verwenden.
#>
param([switch]$NoBuild)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "WiFi Analyzer $version"

if (-not $NoBuild) {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build fehlgeschlagen" }
}

$exe = Join-Path $root "target\release\wifi-analyzer.exe"
if (-not (Test-Path $exe)) { throw "$exe nicht gefunden" }

$dist = Join-Path $root "dist"
$name = "WiFiAnalyzer-$version-win64"
$stage = Join-Path $dist $name
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $exe (Join-Path $stage "WiFiAnalyzer.exe")
Copy-Item (Join-Path $root "assets\README.txt") (Join-Path $stage "README.txt")
Copy-Item (Join-Path $root "LICENSE") (Join-Path $stage "LICENSE.txt")

$zip = Join-Path $dist "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
Copy-Item $exe (Join-Path $dist "WiFiAnalyzer.exe") -Force

$sums = foreach ($f in @($zip, (Join-Path $dist "WiFiAnalyzer.exe"))) {
    "{0}  {1}" -f (Get-FileHash $f -Algorithm SHA256).Hash.ToLower(), (Split-Path $f -Leaf)
}
$sums | Set-Content -Encoding ascii (Join-Path $dist "SHA256SUMS.txt")
Remove-Item $stage -Recurse -Force

Write-Host "Fertig:"
Get-ChildItem $dist -File | ForEach-Object { "  {0}  ({1:N1} MB)" -f $_.Name, ($_.Length / 1MB) }
