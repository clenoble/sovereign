@echo off
REM FROZEN -- NOT SHIPPED. Release build of the Tauri DESKTOP binary
REM (`sovereign-tauri.exe`). Tauri on desktop was frozen on 2026-10-09: the
REM native shell is the only desktop UI and the only shipped desktop artifact
REM (_release_build.bat). Keep this for a developer who still needs the Tauri
REM desktop binary (e.g. for its developer CLI subcommands). Tauri remains the
REM Android host; for the APK see doc\RELEASE.md, not this script.
REM
REM Usage:  _release_tauri_frozen.bat [--cuda]
REM
REM Feature set: the one the old _release_build.bat used, `p2p,comms-email,
REM web-browse` on top of the defaults; `cuda` only with --cuda (needs the CUDA
REM toolkit, CUDA_PATH set, and the three CUDA runtime DLLs shipped next to the
REM exe -- see CLAUDE.md, "CUDA 13 runtime DLL gotcha").
REM
REM Built through the Tauri CLI (`tauri build --no-bundle`), not plain
REM `cargo build --release`: a plain-cargo Tauri binary loads the devUrl
REM (localhost:1420) at runtime and shows "can't reach this page" without a dev
REM server. The CLI runs beforeBuildCommand (`npm run build`) and produces a
REM binary that serves the embedded frontend. Needs `npm ci` in frontend\ first.
REM
REM Env vars: see _env.bat. Output: %SOVEREIGN_TARGET_DIR%\release\sovereign-tauri.exe

setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"

set "FEATURES=p2p,comms-email,web-browse"
if /i "%~1"=="--cuda" goto :cuda
goto :build

:cuda
if not defined CUDA_PATH (
    echo CUDA_PATH is not set: install the CUDA toolkit and open a new shell.
    endlocal & exit /b 1
)
REM MSBuild's CUDA targets resolve the toolkit from CudaToolkitDir (or a
REM versioned CUDA_PATH_Vxx_y); set it so shells that predate the CUDA install
REM still build. CUDA 13 keeps its runtime DLLs in bin\x64.
set "CudaToolkitDir=%CUDA_PATH%\"
set "PATH=%CUDA_PATH%\bin\x64;%CUDA_PATH%\bin;%PATH%"
set "FEATURES=cuda,%FEATURES%"

:build
if not exist "frontend\node_modules\.bin\tauri.cmd" (
    echo frontend\node_modules is missing: run `npm ci` in frontend\ first.
    endlocal & exit /b 1
)
echo.
echo === FROZEN, NOT SHIPPED: tauri build --no-bundle --features %FEATURES%
call "frontend\node_modules\.bin\tauri.cmd" build --no-bundle --features %FEATURES%
set "RC=%ERRORLEVEL%"
echo EXIT=%RC%
endlocal & exit /b %RC%
