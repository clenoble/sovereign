@echo off
REM Local pre-publish gate. Run before cutting a release; it stops at the FIRST
REM failing step and exits with its code. See doc\RELEASE.md.
REM
REM   0  cargo fetch --locked                  (the editor's dependency-graph tests run
REM      `cargo metadata --offline` on purpose: every locked dependency must be cached first)
REM   1  cargo check --workspace --exclude sovereign-ai --all-targets
REM   2  cargo check -p sovereign-shell        (standalone: the shell's own features, as it ships)
REM   3  cargo check -p sovereign-app --no-default-features --features mobile   (the APK feature set, host target)
REM   4  cargo test --workspace --exclude sovereign-ai
REM   5  cargo test -p sovereign-ai --no-default-features --features encrypted-log
REM      (without encrypted-log the session-log tests are silently skipped)
REM   5b cargo test --release, for the guarantees that exist only in release builds
REM      (dev switches, the guardian's auto-approve and the release-delay override
REM      compiled out, P2P debug logging off by default); a debug test run only
REM      checks their debug half
REM   6  cargo audit                          (ignores: .cargo\audit.toml, each one justified there)
REM   7  frontend: npx vitest run
REM   8  frontend: npm audit --omit=dev --audit-level=high
REM
REM Every cargo step runs with --locked: what is tested and audited is the
REM committed Cargo.lock, the one the release build uses.
REM
REM An open `cargo audit` advisory FAILS the gate. That is deliberate: either fix
REM it, or accept it with a written justification in .cargo\audit.toml (a
REM maintainer decision, never a silent ignore).
REM
REM CI on the public mirror runs a similar set, but only after a push. This gate
REM is what runs BEFORE anything is published.
REM
REM Env vars: see _env.bat (SOVEREIGN_TARGET_DIR defaults to C:\cargo-target;
REM a cold run builds the whole workspace, 10+ minutes). If frontend\node_modules
REM is missing, `npm ci` is run first.
REM
REM Not covered here: the Python sidecars (jiminy-bridge, jiminy-vision) are in
REM no shipped build; run their pytest by hand when touching them.

setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"
set "STEP=none"

set "STEP=0/8 cargo fetch --locked"
echo. & echo === %STEP%
cargo fetch --locked
if errorlevel 1 goto :fail

set "STEP=1/8 cargo check --workspace --exclude sovereign-ai --all-targets"
echo. & echo === %STEP%
cargo check --workspace --exclude sovereign-ai --all-targets --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=2/8 cargo check -p sovereign-shell (standalone features)"
echo. & echo === %STEP%
cargo check -p sovereign-shell --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=3/8 cargo check -p sovereign-app --no-default-features --features mobile"
echo. & echo === %STEP%
cargo check -p sovereign-app --no-default-features --features mobile --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=4/8 cargo test --workspace --exclude sovereign-ai"
echo. & echo === %STEP%
cargo test --workspace --exclude sovereign-ai --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail
REM The encrypted layer's tests compile only with the encryption feature.
cargo test -p sovereign-db --features encryption --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=5/8 cargo test -p sovereign-ai --no-default-features --features encrypted-log"
echo. & echo === %STEP%
cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=5b/8 cargo test --release (release-only guarantees)"
echo. & echo === %STEP%
cargo test --release -p sovereign-p2p --lib release_delay_override_exists_only_in_debug_builds --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail
cargo test --release -p sovereign-guardian --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail
cargo test --release -p sovereign-shell dev:: --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail
cargo test --release -p sovereign-shell editor_default_tests --locked -j %SOVEREIGN_JOBS%
if errorlevel 1 goto :fail

set "STEP=6/8 cargo audit"
echo. & echo === %STEP%
cargo audit
if errorlevel 1 goto :fail

set "STEP=7/8 frontend: npx vitest run"
echo. & echo === %STEP%
pushd frontend
if not exist "node_modules" (
    echo node_modules missing, running npm ci
    call npm ci
    if errorlevel 1 goto :fail_popd
)
call npx vitest run
if errorlevel 1 goto :fail_popd

set "STEP=8/8 frontend: npm audit --omit=dev --audit-level=high"
echo. & echo === %STEP%
call npm audit --omit=dev --audit-level=high
if errorlevel 1 goto :fail_popd
popd

echo.
echo PRE-PUBLISH GATE PASSED (8/8)
echo EXIT=0
endlocal & exit /b 0

:fail_popd
REM Capture the failing code BEFORE popd, which may reset ERRORLEVEL.
set "RC=%ERRORLEVEL%"
popd
goto :report

:fail
set "RC=%ERRORLEVEL%"

:report
if "%RC%"=="0" set "RC=1"
echo.
echo PRE-PUBLISH GATE FAILED at step %STEP%
echo EXIT=%RC%
endlocal & exit /b %RC%
