@echo off
set "PATH=C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;%PATH%"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "CMAKE=C:\Program Files\CMake\bin\cmake.exe"

REM Kill stale cargo/rustc processes that may hold NAS file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

REM Clean sovereign artifacts to avoid stale fingerprints
if exist "Z:\cargo-target\debug\deps\libsovereign_*" (
    del /f /q "Z:\cargo-target\debug\deps\libsovereign_*" 2>nul
)
if exist "Z:\cargo-target\debug\.fingerprint" (
    for /d %%d in ("Z:\cargo-target\debug\.fingerprint\sovereign-*") do rd /s /q "%%d" 2>nul
)

cargo %*
