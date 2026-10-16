#!/usr/bin/env bash
# Release build of the shipped Linux desktop artifact: the native shell
# (crate sovereign-shell), staged as `sovereign-linux-x86_64` with its .sha256.
# Linux counterpart of _release_build.bat. See doc/RELEASE.md.
#
# CPU-only: the shell has no `cuda` feature (sovereign-ai is pulled with
# default-features = false). Built standalone (-p sovereign-shell) so the shell
# resolves its own features, as it ships; --locked builds the committed
# Cargo.lock, the one `cargo audit` checks.
#
# System packages (Debian/Ubuntu names) are listed in doc/RELEASE.md.
# On WSL with the sources on a network mount, copy the tree to the WSL
# filesystem first (CLAUDE.md, "WSL2 / Linux").
#
# Env: CARGO_TARGET_DIR (default: ./target), SOVEREIGN_JOBS (default 4),
#      SOVEREIGN_DIST_DIR (default: $CARGO_TARGET_DIR/dist).
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
JOBS="${SOVEREIGN_JOBS:-4}"
DIST="${SOVEREIGN_DIST_DIR:-$TARGET_DIR/dist}"
NAME="sovereign-linux-x86_64"

echo "=== cargo build -p sovereign-shell --release --locked (target: $TARGET_DIR)"
cargo build -p sovereign-shell --release --locked -j "$JOBS"

mkdir -p "$DIST"
cp -f "$TARGET_DIR/release/sovereign" "$DIST/$NAME"
( cd "$DIST" && sha256sum "$NAME" > "$NAME.sha256" && cat "$NAME.sha256" )
echo "RELEASE BUILD OK: $DIST/$NAME"
