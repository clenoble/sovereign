@echo off
REM Portable build wrapper for Sovereign GE on Windows.
REM
REM Honors these environment variables (override per-machine via "set X=...")
REM   SOVEREIGN_LLVM_DIR    LLVM bin directory containing libclang.dll
REM                         (default: C:\Program Files\LLVM\bin)
REM   SOVEREIGN_CMAKE_DIR   CMake bin directory
REM                         (default: C:\Program Files\CMake\bin)
REM   SOVEREIGN_TARGET_DIR  Cargo target directory
REM                         (default: leave unset, cargo uses ./target)
REM
REM On most machines the defaults Just Work after `winget install LLVM.LLVM`
REM and `winget install Kitware.CMake`. Set the env vars in your shell profile
REM if your toolchain lives elsewhere or you want a non-default target dir.

if not defined SOVEREIGN_LLVM_DIR  set "SOVEREIGN_LLVM_DIR=C:\Program Files\LLVM\bin"
if not defined SOVEREIGN_CMAKE_DIR set "SOVEREIGN_CMAKE_DIR=C:\Program Files\CMake\bin"

set "PATH=%SOVEREIGN_CMAKE_DIR%;%SOVEREIGN_LLVM_DIR%;%PATH%"
set "LIBCLANG_PATH=%SOVEREIGN_LLVM_DIR%"
set "CMAKE=%SOVEREIGN_CMAKE_DIR%\cmake.exe"

REM Kill stale cargo/rustc processes that may hold target-dir file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

REM Clean sovereign artifacts to avoid stale fingerprints (only if SOVEREIGN_TARGET_DIR is set)
if defined SOVEREIGN_TARGET_DIR (
    if exist "%SOVEREIGN_TARGET_DIR%\debug\deps\libsovereign_*" (
        del /f /q "%SOVEREIGN_TARGET_DIR%\debug\deps\libsovereign_*" 2>nul
    )
    if exist "%SOVEREIGN_TARGET_DIR%\debug\.fingerprint" (
        for /d %%d in ("%SOVEREIGN_TARGET_DIR%\debug\.fingerprint\sovereign-*") do rd /s /q "%%d" 2>nul
    )
    set "CARGO_TARGET_DIR=%SOVEREIGN_TARGET_DIR%"
)

cargo %*
