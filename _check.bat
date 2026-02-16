@echo off
set "PATH=C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;%PATH%"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "CMAKE=C:\Program Files\CMake\bin\cmake.exe"

REM Kill stale cargo/rustc processes that may hold NAS file locks
powershell -NoProfile -Command "Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force" 2>nul

echo PATH includes CMake: %PATH:~0,40%
where cmake
cargo check -p sovereign-app --target-dir "Z:\cargo-target" -j 4
echo EXITCODE=%ERRORLEVEL%
