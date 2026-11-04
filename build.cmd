@echo off
rem Baut den WiFi Analyzer und erzeugt das Paket in dist\ (siehe tools\package.ps1).
rem Mit der GNU-Toolchain wird zusaetzlich MinGW-w64 (WinLibs) im PATH benoetigt.
set "MINGW=%LOCALAPPDATA%\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
if exist "%MINGW%" set "PATH=%MINGW%;%PATH%"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\package.ps1" || exit /b 1
