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

REM --workspace, NOT a named-crate list. This gate used to be
REM `-p sovereign-app`, and sovereign-shell -- the DEFAULT desktop binary --
REM sat outside it. It stopped compiling when the F1 backend added three
REM P2pEvent variants and nobody noticed for an entire feature plus a live
REM e2e, because every build went through the Tauri owner. A named list has
REM the same failure mode one crate later; --workspace fails SAFE (a new crate
REM is checked by default) where a list fails SILENT.
REM
REM Only sovereign-ai is excluded, and only as a *selected package*: its own
REM `default = ["cuda", ...]` would force a CUDA toolchain on every checkout.
REM Its lib is still checked -- every workspace member pulls it via the
REM workspace pin `default-features = false`, so the CPU paths compile here;
REM only the cuda-gated code is out of scope. Check that explicitly when
REM touching it: `cargo check -p sovereign-ai --features cuda`.
REM
REM Warm cost: ~2s (vs ~1s for the old -p sovereign-app). Cold: ~30s.
cargo check --workspace --exclude sovereign-ai -j 4
set "WS_RC=%ERRORLEVEL%"

REM --workspace unifies features across all members, so sovereign-app pulling
REM `sovereign-ai/encrypted-log` turns it on for the shell too -- masking the
REM fact that the shell ships as a STANDALONE `-p sovereign-shell` build with
REM its OWN feature resolution. That blind spot hid SESSIONLOG-010: the shell
REM never enabled encrypted-log, so `set_session_log_key` didn't exist standalone
REM even though --workspace compiled fine. Check the shell the way it actually
REM ships (its own features, no unification) so a feature the shell forgets to
REM name can't pass here and fail at release.
cargo check -p sovereign-shell -j 4
set "SHELL_RC=%ERRORLEVEL%"

echo WORKSPACE_RC=%WS_RC%  SHELL_STANDALONE_RC=%SHELL_RC%
if not "%WS_RC%"=="0" ( echo EXITCODE=%WS_RC% & exit /b %WS_RC% )
echo EXITCODE=%SHELL_RC%
exit /b %SHELL_RC%
