@echo off
REM Release build of the SHIPPED Windows desktop artifact: the native shell
REM (crate sovereign-shell, binary `sovereign.exe`). See doc\RELEASE.md.
REM
REM Until 2026-10-10 this script built the Tauri desktop binary with CUDA. That
REM binary is frozen and not shipped since 2026-10-09; its recipe now lives in
REM _release_tauri_frozen.bat. The v0.0.9 sovereign.exe was already the shell.
REM
REM CPU-only. The shell has NO `cuda` feature today: it pulls sovereign-ai with
REM the workspace pin `default-features = false`, so llama.cpp builds for the CPU
REM and the exe needs no CUDA runtime DLLs. A CUDA shell build would need a new
REM feature in crates\sovereign-shell\Cargo.toml first.
REM
REM Built standalone (`-p sovereign-shell`), so the shell resolves its OWN
REM features, exactly as it ships -- no feature unification with sovereign-app.
REM `--locked`: the binary is built from the committed Cargo.lock, the same one
REM `cargo audit` checks in _prepublish.bat.
REM
REM Env vars: see _env.bat. Plus:
REM   SOVEREIGN_DIST_DIR   where the artifact and its .sha256 are staged
REM                        (default: %SOVEREIGN_TARGET_DIR%\dist, outside git)
REM
REM Output: <dist>\sovereign.exe and <dist>\sovereign.exe.sha256
REM (sha256sum format; concatenate the .sha256 files of all assets into SHA256SUMS).
REM
REM Unlike _build.bat, this does NOT kill running cargo/rustc processes: another
REM build (another worktree, the IDE) may legitimately be running.

setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"
if not defined SOVEREIGN_DIST_DIR set "SOVEREIGN_DIST_DIR=%CARGO_TARGET_DIR%\dist"

echo.
echo === cargo build -p sovereign-shell --release --locked
cargo build -p sovereign-shell --release --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

if not exist "%SOVEREIGN_DIST_DIR%" mkdir "%SOVEREIGN_DIST_DIR%"
copy /y "%CARGO_TARGET_DIR%\release\sovereign.exe" "%SOVEREIGN_DIST_DIR%\sovereign.exe" >nul
if errorlevel 1 goto :fail

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\write-sha256.ps1" "%SOVEREIGN_DIST_DIR%\sovereign.exe"
if errorlevel 1 goto :fail

echo.
echo RELEASE BUILD OK: %SOVEREIGN_DIST_DIR%\sovereign.exe
echo EXIT=0
endlocal & exit /b 0

:fail
set "RC=%ERRORLEVEL%"
if "%RC%"=="0" set "RC=1"
echo RELEASE BUILD FAILED
echo EXIT=%RC%
endlocal & exit /b %RC%
