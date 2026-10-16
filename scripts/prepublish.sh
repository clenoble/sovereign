#!/usr/bin/env bash
# Local pre-publish gate, Linux/WSL counterpart of _prepublish.bat (same steps,
# same order, stops at the first failure). See doc/RELEASE.md.
#
# An open `cargo audit` advisory fails the gate on purpose: fix it, or accept
# it with a written justification in .cargo/audit.toml.
#
# Env: CARGO_TARGET_DIR (default: ./target), SOVEREIGN_JOBS (default 4).
# If rocksdb's bindgen cannot find stddef.h, export BINDGEN_EXTRA_CLANG_ARGS
# with clang's resource include dir, e.g.
#   export BINDGEN_EXTRA_CLANG_ARGS="-I$(clang -print-resource-dir)/include"
set -euo pipefail

cd "$(dirname "$0")/.."
JOBS="${SOVEREIGN_JOBS:-4}"
STEP="none"
trap 'rc=$?; if [ "$rc" -ne 0 ]; then echo; echo "PRE-PUBLISH GATE FAILED at step $STEP"; echo "EXIT=$rc"; fi' EXIT

run() { STEP="$1"; shift; echo; echo "=== $STEP"; "$@"; }

run "1/8 cargo check --workspace --exclude sovereign-ai --all-targets" \
    cargo check --workspace --exclude sovereign-ai --all-targets --locked -j "$JOBS"
run "2/8 cargo check -p sovereign-shell (standalone features)" \
    cargo check -p sovereign-shell --locked -j "$JOBS"
run "3/8 cargo check -p sovereign-app --no-default-features --features mobile" \
    cargo check -p sovereign-app --no-default-features --features mobile --locked -j "$JOBS"
run "4/8 cargo test --workspace --exclude sovereign-ai" \
    cargo test --workspace --exclude sovereign-ai --locked -j "$JOBS"
run "4b/8 cargo test -p sovereign-db --features encryption" \
    cargo test -p sovereign-db --features encryption --locked -j "$JOBS"
run "5/8 cargo test -p sovereign-ai --no-default-features --features encrypted-log" \
    cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j "$JOBS"
run "6/8 cargo audit" cargo audit

cd frontend
if [ ! -d node_modules ]; then
    STEP="7/8 frontend: npm ci"
    echo "node_modules missing, running npm ci"
    npm ci
fi
run "7/8 frontend: npx vitest run" npx vitest run
run "8/8 frontend: npm audit --omit=dev --audit-level=high" \
    npm audit --omit=dev --audit-level=high

echo
echo "PRE-PUBLISH GATE PASSED (8/8)"
