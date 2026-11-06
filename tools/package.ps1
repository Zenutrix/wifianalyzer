<#
.SYNOPSIS
  Baut den WiFi Analyzer (Release) und erzeugt das Download-Paket in dist\.

.DESCRIPTION
  Ergebnis:
    dist\WiFiAnalyzer-<version>-win64.zip   (EXE + README + LICENSE)
    dist\WiFiAnalyzer-Setup.exe             (Installer, benötigt Inno Setup 6.7+)
    dist\WiFiAnalyzer.exe                   (einzelne EXE, portable)
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

# Installer (Inno Setup) – optional, falls ISCC nicht gefunden wird
$iscc = @(
    (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source,
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
$setup = Join-Path $dist "WiFiAnalyzer-Setup.exe"
Get-ChildItem $dist -Filter "WiFiAnalyzer-Setup*.exe" -ErrorAction SilentlyContinue | Remove-Item -Force
if ($iscc) {
    & $iscc /Q "/DAppVersion=$version" "/DSourceExe=$exe" "/DOutputDir=$dist" (Join-Path $root "installer\WiFiAnalyzer.iss")
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup fehlgeschlagen" }
} else {
    Write-Warning "Inno Setup (ISCC.exe) nicht gefunden – Installer wird übersprungen. winget install JRSoftware.InnoSetup"
}

$sums = foreach ($f in @($zip, (Join-Path $dist "WiFiAnalyzer.exe"), $setup) | Where-Object { Test-Path $_ }) {
    "{0}  {1}" -f (Get-FileHash $f -Algorithm SHA256).Hash.ToLower(), (Split-Path $f -Leaf)
}
$sums | Set-Content -Encoding ascii (Join-Path $dist "SHA256SUMS.txt")
Remove-Item $stage -Recurse -Force

Write-Host "Fertig:"
Get-ChildItem $dist -File | ForEach-Object { "  {0}  ({1:N1} MB)" -f $_.Name, ($_.Length / 1MB) }
