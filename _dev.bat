@echo off
REM Tauri dev wrapper — launches the Tauri CLI from the project root so it
REM finds crates/sovereign-app/tauri.conf.json (in a subfolder), which
REM in turn:
REM   1. runs `npm run dev --prefix ../../frontend` (Vite on :5173, HMR)
REM   2. builds + runs sovereign.exe pointed at the dev server
REM
REM Honors the same env vars as _build.bat / _run.bat.

if not defined SOVEREIGN_LLVM_DIR  set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"
set "RUST_LOG=info"

REM Kill stale cargo/rustc processes that may hold target-dir file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

if defined SOVEREIGN_TARGET_DIR set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"

REM Use the Tauri CLI bundled in frontend/node_modules. Cwd is the project
REM root so the CLI's subfolder scan finds crates/sovereign-app/tauri.conf.json.
call "frontend\node_modules\.bin\tauri.cmd" dev %*
echo EXIT_CODE=%ERRORLEVEL%
