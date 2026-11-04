@echo off
rem Baut den WiFi Analyzer (Release). Benoetigt Rust (GNU-Toolchain) und MinGW-w64 (WinLibs).
set "MINGW=%LOCALAPPDATA%\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin"
set "PATH=%USERPROFILE%\.cargo\bin;%MINGW%;%PATH%"
cargo build --release || exit /b 1
echo.
echo Fertig: target\release\wifi-analyzer.exe
