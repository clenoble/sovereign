@echo off
REM Local pre-publish gate. Run before cutting a release; it stops at the FIRST
REM failing step and exits with its code. See doc\RELEASE.md.
REM
REM   _prepublish.bat RELEASE_NOTES_vX.Y.Z.md [RELEASE_NOTES_vX.Y.Z.public.md]
REM       the notes step 9 checks (without them the gate fails, exit 2), and the
REM       public notes, if any: step 9 checks their claims too, and that every
REM       marker in them is one of the full notes' (claims verifier, V10)
REM
REM   0  cargo fetch --locked                  (the editor's dependency-graph tests run
REM      `cargo metadata --offline` on purpose: every locked dependency must be cached first)
REM   1  cargo check --workspace --exclude sovereign-ai --all-targets
REM   2  cargo check -p sovereign-shell        (standalone: the shell's own features, as it ships)
REM   3  cargo check -p sovereign-app --no-default-features --features mobile   (the APK feature set, host target)
REM   4  cargo test --workspace --exclude sovereign-ai
REM      (and sovereign-db with encryption, and sovereign-app with p2p: tests that
REM      compile only with a feature the workspace run does not turn on)
REM   5  cargo test -p sovereign-ai --no-default-features --features encrypted-log
REM      (without encrypted-log the session-log tests are silently skipped)
REM   5b cargo test --release, for the guarantees that exist only in release builds
REM      (dev switches, the guardian's auto-approve and the release-delay override
REM      compiled out, P2P debug logging off by default); a debug test run only
REM      checks their debug half
REM   6  cargo audit                          (ignores: .cargo\audit.toml, each one justified there)
REM   7  frontend: npx vitest run
REM   7b frontend: npx svelte-check --threshold error (0 errors since v0.0.12 F-a11y, F3;
REM      kept at 0 here, F4)
REM   8  frontend: npm audit --omit=dev --audit-level=high
REM   9  release-note claims: every claim of the notes names a test this gate ran,
REM      or a manual check (scripts\check-release-claims.py, doc\plans\CLAIMS-verifier.md).
REM      Steps 4-7 write the list of what they ran to %TEMP%\sovereign-gate-lists.
REM      A green step 9 is not a proof: the second position reads its table.
REM
REM Every check reads `%errorlevel% neq 0`, never `errorlevel 1`, which misses a
REM negative code (a step killed from outside exits -1; v0.0.12 item 17).
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
REM a cold run builds the whole workspace, 10+ minutes). Step 7 always runs
REM `npm ci` first, so the frontend is tested against the committed lock.
REM
REM Not covered here: the Python sidecars (jiminy-bridge, jiminy-vision) are in
REM no shipped build; run their pytest by hand when touching them.

setlocal
cd /d "%~dp0"
call "%~dp0_env.bat"
set "STEP=none"
REM Every test step also lists what it ran: step 9 refuses a claim whose test
REM this gate never ran (doc\plans\CLAIMS-verifier.md, V4).
set "LISTDIR=%TEMP%\sovereign-gate-lists"
if exist "%LISTDIR%" rmdir /s /q "%LISTDIR%"
mkdir "%LISTDIR%"
if %errorlevel% neq 0 goto :fail
REM The run's temporary files go to a folder of its own under the target
REM directory, so the test stores the harnesses leave stop piling up in %TEMP%.
REM At the end, scripts\gate-tmp.py prints what was left there, grouped by name,
REM and what appeared in the system folder meanwhile; a gate that passed removes
REM its folder, a gate that failed keeps it until the next run. Set after
REM LISTDIR, which stays in the system folder as before.
set "SYS_TEMP=%TEMP%"
set "GATE_TMP=%SOVEREIGN_TARGET_DIR%\gate-tmp"
python -I scripts\gate-tmp.py start "%GATE_TMP%" "%SYS_TEMP%"
if %errorlevel% neq 0 goto :fail
set "TMP=%GATE_TMP%"
set "TEMP=%GATE_TMP%"
set "TMPDIR=%GATE_TMP%"

set "STEP=0/9 cargo fetch --locked"
echo. & echo === %STEP%
cargo fetch --locked
if %errorlevel% neq 0 goto :fail

set "STEP=1/9 cargo check --workspace --exclude sovereign-ai --all-targets"
echo. & echo === %STEP%
cargo check --workspace --exclude sovereign-ai --all-targets --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail

set "STEP=2/9 cargo check -p sovereign-shell (standalone features)"
echo. & echo === %STEP%
cargo check -p sovereign-shell --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail

set "STEP=3/9 cargo check -p sovereign-app --no-default-features --features mobile"
echo. & echo === %STEP%
cargo check -p sovereign-app --no-default-features --features mobile --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail

set "STEP=4/9 cargo test --workspace --exclude sovereign-ai"
echo. & echo === %STEP%
cargo test --workspace --exclude sovereign-ai --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test --workspace --exclude sovereign-ai --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\04-workspace.txt"
if %errorlevel% neq 0 goto :fail
REM The encrypted layer's tests compile only with the encryption feature.
cargo test -p sovereign-db --features encryption --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test -p sovereign-db --features encryption --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\04-db-encryption.txt"
if %errorlevel% neq 0 goto :fail
REM The app's tests that compile only with p2p (29 at v0.0.13 rc1, among them a duress
REM session running no node): `cargo test --workspace` never turns p2p on for the app
REM (Céline, 2026-11-20, from the rc1 gates' step 9).
cargo test -p sovereign-app --features p2p --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test -p sovereign-app --features p2p --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\04-app-p2p.txt"
if %errorlevel% neq 0 goto :fail

set "STEP=5/9 cargo test -p sovereign-ai --no-default-features --features encrypted-log"
echo. & echo === %STEP%
cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\05-ai-encrypted-log.txt"
if %errorlevel% neq 0 goto :fail

set "STEP=5b/9 cargo test --release (release-only guarantees)"
echo. & echo === %STEP%
cargo test --release -p sovereign-p2p --lib release_delay_override_exists_only_in_debug_builds --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-p2p --lib release_delay_override_exists_only_in_debug_builds --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\05b-p2p-release.txt"
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-guardian --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-guardian --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\05b-guardian-release.txt"
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-shell dev:: --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-shell dev:: --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\05b-shell-dev-release.txt"
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-shell editor_default_tests --locked -j %SOVEREIGN_JOBS%
if %errorlevel% neq 0 goto :fail
cargo test --release -p sovereign-shell editor_default_tests --locked -j %SOVEREIGN_JOBS% -- --list > "%LISTDIR%\05b-shell-editor-release.txt"
if %errorlevel% neq 0 goto :fail

set "STEP=6/9 cargo audit"
echo. & echo === %STEP%
cargo audit
if %errorlevel% neq 0 goto :fail

set "STEP=7/9 frontend: npx vitest run"
echo. & echo === %STEP%
pushd frontend
REM Always npm ci: the gate tests the committed lock, never a node_modules
REM left by an earlier install (WSL 0024; Celine, 2026-10-28).
call npm ci
if %errorlevel% neq 0 goto :fail_popd
call npx vitest run
if %errorlevel% neq 0 goto :fail_popd
call npx vitest list > "%LISTDIR%\07-vitest.txt"
if %errorlevel% neq 0 goto :fail_popd

set "STEP=7b/9 frontend: npx svelte-check --threshold error"
echo. & echo === %STEP%
call npx svelte-check --threshold error
if %errorlevel% neq 0 goto :fail_popd

set "STEP=8/9 frontend: npm audit --omit=dev --audit-level=high"
echo. & echo === %STEP%
call npm audit --omit=dev --audit-level=high
if %errorlevel% neq 0 goto :fail_popd
popd

set "STEP=9/9 release-note claims (scripts\check-release-claims.py)"
echo. & echo === %STEP%
if "%~1"=="" goto :no_notes
set "LISTARGS="
for %%f in ("%LISTDIR%\*.txt") do call set "LISTARGS=%%LISTARGS%% --list "%%~f""
if not "%~2"=="" set "LISTARGS=%LISTARGS% --public "%~2""
python -I scripts\check-release-claims.py "%~1" %LISTARGS%
if %errorlevel% neq 0 goto :fail

python -I scripts\gate-tmp.py report "%GATE_TMP%" "%SYS_TEMP%" --remove
echo.
echo PRE-PUBLISH GATE PASSED (9/9)
echo EXIT=0
endlocal & exit /b 0

:no_notes
echo Name the release notes this gate checks: _prepublish.bat RELEASE_NOTES_vX.Y.Z.md
set "RC=2"
goto :report

:fail_popd
REM Capture the failing code BEFORE popd, which may reset ERRORLEVEL.
set "RC=%ERRORLEVEL%"
popd
goto :report

:fail
set "RC=%ERRORLEVEL%"

:report
if defined GATE_TMP python -I scripts\gate-tmp.py report "%GATE_TMP%" "%SYS_TEMP%"
if "%RC%"=="0" set "RC=1"
echo.
echo PRE-PUBLISH GATE FAILED at step %STEP%
echo EXIT=%RC%
endlocal & exit /b %RC%
