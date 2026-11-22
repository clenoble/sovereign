#!/usr/bin/env bash
# Local pre-publish gate, Linux/WSL counterpart of _prepublish.bat (same steps,
# same order, stops at the first failure). See doc/RELEASE.md.
#
#   scripts/prepublish.sh RELEASE_NOTES_vX.Y.Z.md [RELEASE_NOTES_vX.Y.Z.public.md]
#       the notes step 9 checks (without them the gate fails at step 9, exit 2),
#       and the public notes, if any: step 9 checks their claims too, and that
#       every marker in them is one of the full notes' (claims verifier, V10)
#
# Every test step also lists what it ran, in $TMPDIR/sovereign-gate-lists (the
# system folder, before the gate's own one is set): step 9 refuses a claim whose
# test this gate never ran (scripts/check-release-claims.py). A green step 9 is
# not a proof: the second position reads its table.
#
# An open `cargo audit` advisory fails the gate on purpose: fix it, or accept
# it with a written justification in .cargo/audit.toml.
#
# Env: CARGO_TARGET_DIR (default: ./target), SOVEREIGN_JOBS (default 4).
# If rocksdb's bindgen cannot find stddef.h, export BINDGEN_EXTRA_CLANG_ARGS
# with clang's resource include dir, e.g.
#   export BINDGEN_EXTRA_CLANG_ARGS="-I$(clang -print-resource-dir)/include"
set -euo pipefail

# The notes are named relative to where the gate is called from.
NOTES=""
[ -n "${1:-}" ] && NOTES="$(realpath -m "$1")"
PUBLIC=""
[ -n "${2:-}" ] && PUBLIC="$(realpath -m "$2")"
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
JOBS="${SOVEREIGN_JOBS:-4}"
STEP="none"

# The run's temporary files go to a folder of its own under the target
# directory, so the test stores the harnesses leave stop piling up in /tmp.
# At the end, scripts/gate-tmp.py prints what was left there, grouped by name,
# and what appeared in the system folder meanwhile; a gate that passed removes
# its folder, a gate that failed keeps it until the next run.
SYS_TMP="${TMPDIR:-/tmp}"
LISTDIR="$SYS_TMP/sovereign-gate-lists"
rm -rf "${LISTDIR:?}"
mkdir -p "$LISTDIR"
GATE_TMP="$(realpath -m "${CARGO_TARGET_DIR:-target}")/gate-tmp"
python3 -I "$ROOT/scripts/gate-tmp.py" start "$GATE_TMP" "$SYS_TMP"
export TMPDIR="$GATE_TMP"

on_exit() {
    local rc=$?
    local remove=""
    [ "$rc" -eq 0 ] && remove="--remove"
    python3 -I "$ROOT/scripts/gate-tmp.py" report "$GATE_TMP" "$SYS_TMP" $remove || true
    if [ "$rc" -ne 0 ]; then echo; echo "PRE-PUBLISH GATE FAILED at step $STEP"; echo "EXIT=$rc"; fi
}
trap on_exit EXIT

run() { STEP="$1"; shift; echo; echo "=== $STEP"; "$@"; }
# A test command, then the same command listing what it ran into $LISTDIR/<file>.
tested() { local list="$1"; shift; "$@"; "$@" -- --list > "$LISTDIR/$list"; }

# 0: every locked dependency in the registry cache first — the editor's dependency-graph
# tests run `cargo metadata --offline` on purpose (they must not reach the network).
run "0/9 cargo fetch --locked" cargo fetch --locked
run "1/9 cargo check --workspace --exclude sovereign-ai --all-targets" \
    cargo check --workspace --exclude sovereign-ai --all-targets --locked -j "$JOBS"
run "2/9 cargo check -p sovereign-shell (standalone features)" \
    cargo check -p sovereign-shell --locked -j "$JOBS"
run "3/9 cargo check -p sovereign-app --no-default-features --features mobile" \
    cargo check -p sovereign-app --no-default-features --features mobile --locked -j "$JOBS"
run "4/9 cargo test --workspace --exclude sovereign-ai" tested 04-workspace.txt \
    cargo test --workspace --exclude sovereign-ai --locked -j "$JOBS"
# The encrypted layer's tests compile only with the encryption feature.
run "4b/9 cargo test -p sovereign-db --features encryption" tested 04-db-encryption.txt \
    cargo test -p sovereign-db --features encryption --locked -j "$JOBS"
# The app's tests that compile only with p2p (29 at v0.0.13 rc1, among them a duress
# session running no node): `cargo test --workspace` never turns p2p on for the app
# (Céline, 2026-11-20, from the rc1 gates' step 9).
run "4c/9 cargo test -p sovereign-app --features p2p" tested 04-app-p2p.txt \
    cargo test -p sovereign-app --features p2p --locked -j "$JOBS"
run "5/9 cargo test -p sovereign-ai --no-default-features --features encrypted-log" tested 05-ai-encrypted-log.txt \
    cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j "$JOBS"
# 5b: guarantees that exist only in release builds (dev switches, the guardian's
# auto-approve and the release-delay override compiled out, P2P debug logging off
# by default) — a debug test run only checks their debug half.
run "5b/9 cargo test --release: backup host release delay" tested 05b-p2p-release.txt \
    cargo test --release -p sovereign-p2p --lib release_delay_override_exists_only_in_debug_builds --locked -j "$JOBS"
run "5b/9 cargo test --release: guardian" tested 05b-guardian-release.txt \
    cargo test --release -p sovereign-guardian --locked -j "$JOBS"
run "5b/9 cargo test --release: shell dev switches and log filter" tested 05b-shell-dev-release.txt \
    cargo test --release -p sovereign-shell dev:: --locked -j "$JOBS"
run "5b/9 cargo test --release: shell document editor (EDITOR-default E1-E9)" tested 05b-shell-editor-release.txt \
    cargo test --release -p sovereign-shell editor_default_tests --locked -j "$JOBS"
run "6/9 cargo audit" cargo audit

cd frontend
# Always npm ci: the gate tests the committed lock, never a node_modules left by
# an earlier install (WSL, 2026-10-28; Celine).
run "7/9 frontend: npm ci" npm ci
run "7/9 frontend: npx vitest run" npx vitest run
npx vitest list > "$LISTDIR/07-vitest.txt"
run "7b/9 frontend: npx svelte-check --threshold error" npx svelte-check --threshold error
run "8/9 frontend: npm audit --omit=dev --audit-level=high" \
    npm audit --omit=dev --audit-level=high
cd "$ROOT"

STEP="9/9 release-note claims (scripts/check-release-claims.py)"
echo; echo "=== $STEP"
if [ -z "$NOTES" ]; then
    echo "Name the release notes this gate checks: scripts/prepublish.sh RELEASE_NOTES_vX.Y.Z.md"
    exit 2
fi
LISTARGS=()
for f in "$LISTDIR"/*.txt; do LISTARGS+=(--list "$f"); done
[ -n "$PUBLIC" ] && LISTARGS+=(--public "$PUBLIC")
python3 -I scripts/check-release-claims.py "$NOTES" "${LISTARGS[@]}"

echo
echo "PRE-PUBLISH GATE PASSED (9/9)"
echo "EXIT=0"
