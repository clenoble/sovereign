@echo off
REM Run the Tauri desktop UI (now an option; the native shell is the default —
REM see _run.bat). Binary: `sovereign-tauri`. For Tauri with frontend HMR use
REM _dev.bat instead. Honors the same env vars as _build.bat / _run.bat.

if not defined SOVEREIGN_LLVM_DIR  set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"
set "RUST_LOG=info"

REM Kill stale cargo/rustc processes that may hold target-dir file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

if defined SOVEREIGN_TARGET_DIR set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"

cargo run -p sovereign-app -j 2 -- run 2>&1
echo EXIT_CODE=%ERRORLEVEL%
