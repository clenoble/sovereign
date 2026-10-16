#!/usr/bin/env bash
# Build the Android APK (arm64 / aarch64 only). See doc/RELEASE.md, "Android".
#
# Usage: scripts/build-android.sh --debug | --release
#
#   --debug    debug build, signed with the Android SDK debug key. This is what
#              the published APKs have been so far.
#   --release  release build. The Gradle scaffold (crates/sovereign-app/gen/android)
#              declares NO signing config, so the output is UNSIGNED: it must be
#              signed (apksigner) with a release key held outside the repository
#              before it can be installed, and hashed only AFTER signing.
#
# Linux or WSL2 only: the repo's patched Gradle BuildTask calls `cargo tauri` on
# Linux/macOS; its Windows branch is not maintained.
#
# Required env: JAVA_HOME (JDK 17), ANDROID_HOME (SDK), NDK_HOME (NDK).
# ANDROID_NDK_ROOT / ANDROID_NDK / NDK_ROOT default to NDK_HOME (llama-cpp-sys-2's
# build script reads ANDROID_NDK_ROOT). Also needed: `cargo tauri` (tauri-cli 2.x),
# the Rust target aarch64-linux-android, and Node.js for the frontend build.
#
# Optional env: SOVEREIGN_DIST_DIR (default: $CARGO_TARGET_DIR/dist or ./target/dist),
#               SOVEREIGN_APK_NAME (default: sovereign-android-aarch64-<mode>.apk).
set -euo pipefail

MODE="${1:-}"
case "$MODE" in
    --debug)   MODE=debug ;;
    --release) MODE=release ;;
    *) echo "usage: $0 --debug | --release" >&2; exit 2 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
for v in JAVA_HOME ANDROID_HOME NDK_HOME; do
    if [ -z "${!v:-}" ]; then echo "$v is not set (see doc/RELEASE.md, Android)" >&2; exit 2; fi
done
export ANDROID_NDK_ROOT="${ANDROID_NDK_ROOT:-$NDK_HOME}"
export ANDROID_NDK="${ANDROID_NDK:-$NDK_HOME}"
export NDK_ROOT="${NDK_ROOT:-$NDK_HOME}"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"

cargo tauri --version >/dev/null 2>&1 || {
    echo "cargo tauri not found: cargo install tauri-cli --version '^2' --locked" >&2; exit 2; }
rustup target list --installed 2>/dev/null | grep -qx aarch64-linux-android || {
    echo "Rust target missing: rustup target add aarch64-linux-android" >&2; exit 2; }
if [ ! -d "$ROOT/frontend/node_modules" ]; then
    echo "frontend/node_modules missing, running npm ci"
    (cd "$ROOT/frontend" && npm ci)
fi

# `--features mobile` + `-- --no-default-features`: tauri-cli has no flag for
# --no-default-features, the `--` passes it to cargo. Without it the desktop
# default `rocksdb` is pulled into the Android build and fails in bindgen.
DEBUG_ARG=""
if [ "$MODE" = debug ]; then DEBUG_ARG="--debug"; fi
cd "$ROOT/crates/sovereign-app"
echo "=== cargo tauri android build $DEBUG_ARG --target aarch64 --features mobile -- --no-default-features"
# shellcheck disable=SC2086  # DEBUG_ARG is empty or one word
cargo tauri android build $DEBUG_ARG --target aarch64 --features mobile -- --no-default-features

OUT="$ROOT/crates/sovereign-app/gen/android/app/build/outputs/apk/universal/$MODE"
APK="$(ls "$OUT"/*.apk 2>/dev/null | head -n1 || true)"
if [ -z "$APK" ]; then echo "no APK found in $OUT" >&2; exit 1; fi

DIST="${SOVEREIGN_DIST_DIR:-${CARGO_TARGET_DIR:-$ROOT/target}/dist}"
NAME="${SOVEREIGN_APK_NAME:-sovereign-android-aarch64-$MODE.apk}"
mkdir -p "$DIST"
cp -f "$APK" "$DIST/$NAME"
echo "APK: $APK -> $DIST/$NAME"
if [ "$MODE" = debug ]; then
    ( cd "$DIST" && sha256sum "$NAME" > "$NAME.sha256" && cat "$NAME.sha256" )
else
    echo "UNSIGNED release APK: sign it (apksigner) before installing or publishing,"
    echo "then hash the SIGNED file: sha256sum <signed.apk> > <signed.apk>.sha256"
fi
