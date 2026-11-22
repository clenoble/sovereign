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
ROOT="$(pwd)"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
JOBS="${SOVEREIGN_JOBS:-4}"
DIST="${SOVEREIGN_DIST_DIR:-$TARGET_DIR/dist}"
NAME="sovereign-linux-x86_64"
TRIPLE="x86_64-unknown-linux-gnu"

# The builder's paths stay out of the binary (v0.0.13 scope, the builder paths;
# Celine, 2026-11-17). Panic locations and debug paths would otherwise name the
# builder's home folder, through the cargo registry, the git checkouts, the
# repository and the target folder.
#
# Rust: --remap-path-prefix, given through --config so that cargo ADDS it to the
# linker option .cargo/config.toml sets for this target. A RUSTFLAGS variable
# would REPLACE that option. When several prefixes match, the last one given
# wins, so the most specific folders come last.
# C and C++ (the sources build scripts compile): the same map through
# -ffile-prefix-map, which covers __FILE__ in their asserts.
TARGET_ABS="$(realpath -m "$TARGET_DIR")"
CARGO_DIR="$(realpath -m "${CARGO_HOME:-$HOME/.cargo}")"
MAPS=(
    "$ROOT=sovereign"
    "$TARGET_ABS=target"
    "$CARGO_DIR/registry/src=cargo/registry"
    "$CARGO_DIR/git/checkouts=cargo/git"
)
RUST_MAPS=""
C_MAPS=""
for m in "${MAPS[@]}"; do
    RUST_MAPS="$RUST_MAPS${RUST_MAPS:+, }\"--remap-path-prefix=$m\""
    C_MAPS="$C_MAPS -ffile-prefix-map=$m"
done
export CFLAGS="${CFLAGS:-}$C_MAPS"
export CXXFLAGS="${CXXFLAGS:-}$C_MAPS"

echo "=== cargo build -p sovereign-shell --release --locked (target: $TARGET_DIR)"
cargo build -p sovereign-shell --release --locked -j "$JOBS" \
    --config "target.$TRIPLE.rustflags=[$RUST_MAPS]"

# The measure: the builder's home folder, as text in the binary, must be absent.
BIN="$TARGET_DIR/release/sovereign"
LEFT="$(strings -a "$BIN" | grep -c -F "$HOME" || true)"
echo "=== strings naming the builder's home ($HOME): $LEFT"
if [ "$LEFT" -ne 0 ]; then
    strings -a "$BIN" | grep -F "$HOME" | cut -c1-160 | sort | uniq -c | sort -rn | head -20
    echo "RELEASE BUILD FAILED: the binary names the builder's home folder"
    exit 1
fi

mkdir -p "$DIST"
cp -f "$BIN" "$DIST/$NAME"
( cd "$DIST" && sha256sum "$NAME" > "$NAME.sha256" && cat "$NAME.sha256" )
echo "RELEASE BUILD OK: $DIST/$NAME"
