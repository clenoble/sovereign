@echo off
REM Portable `cargo check` wrapper. See _build.bat for the env vars honored.

if not defined SOVEREIGN_LLVM_DIR  set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"

REM Kill stale cargo/rustc processes that may hold target-dir file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

if defined SOVEREIGN_TARGET_DIR set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"

echo PATH includes CMake: %PATH:~0,40%
where cmake
cargo check -p sovereign-app -j 4
echo EXITCODE=%ERRORLEVEL%
