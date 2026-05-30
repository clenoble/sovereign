@echo off
REM Portable `cargo test` wrapper for Sovereign GE on Windows.
REM
REM Same env vars as _build.bat plus a forced CARGO_TARGET_DIR fallback to
REM C:\cargo-target. Reason: Windows Smart App Control sometimes blocks
REM execution of test binaries built into the workspace `target/` dir
REM (especially under WSL or NAS-mounted source trees), surfacing as
REM "An Application Control policy has blocked this file" (os error 4551)
REM partway through linking. A target dir on a local drive avoids the
REM block.
REM
REM Pass any cargo test args after the wrapper, e.g.
REM   _test.bat -p sovereign-p2p
REM   _test.bat -p sovereign-app account_key_migration
REM   _test.bat --workspace --exclude sovereign-ai

if not defined SOVEREIGN_LLVM_DIR  set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"
if not defined SOVEREIGN_TARGET_DIR set "SOVEREIGN_TARGET_DIR=C:\cargo-target"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"
set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"

REM Kill stale cargo/rustc processes that may hold target-dir file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

cargo test %*
