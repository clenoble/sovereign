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

# 0: every locked dependency in the registry cache first — the editor's dependency-graph
# tests run `cargo metadata --offline` on purpose (they must not reach the network).
run "0/8 cargo fetch --locked" cargo fetch --locked
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
# 5b: guarantees that exist only in release builds (dev switches, the guardian's
# auto-approve and the release-delay override compiled out, P2P debug logging off
# by default) — a debug test run only checks their debug half.
run "5b/8 cargo test --release: backup host release delay" \
    cargo test --release -p sovereign-p2p --lib release_delay_override_exists_only_in_debug_builds --locked -j "$JOBS"
run "5b/8 cargo test --release: guardian" \
    cargo test --release -p sovereign-guardian --locked -j "$JOBS"
run "5b/8 cargo test --release: shell dev switches and log filter" \
    cargo test --release -p sovereign-shell dev:: --locked -j "$JOBS"
run "5b/8 cargo test --release: shell document editor (EDITOR-default E1-E9)" \
    cargo test --release -p sovereign-shell editor_default_tests --locked -j "$JOBS"
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
