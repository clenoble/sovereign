@echo off
set "PATH=C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;%PATH%"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "CMAKE=C:\Program Files\CMake\bin\cmake.exe"
set "CARGO_TARGET_DIR=Z:\cargo-target"
set "RUST_LOG=info"

REM Kill stale cargo/rustc processes that may hold NAS file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

cargo run -p sovereign-app -j 2 -- run 2>&1
echo EXIT_CODE=%ERRORLEVEL%
