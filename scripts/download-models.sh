#!/usr/bin/env bash
#
# Download GGUF models for Sovereign GE.
#
# Usage:
#   ./scripts/download-models.sh          # download all models
#   ./scripts/download-models.sh router   # download router (3B) only
#   ./scripts/download-models.sh reason   # download reasoning (7B) only
#
# Models are stored in ./models/ relative to the project root.
# Edit config/default.toml to point at different models.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
MODEL_DIR="$PROJECT_ROOT/models"

# ── Model definitions ────────────────────────────────────────────────
# MODELTRUST: each model carries an EXPECTED SHA-256. Get the official value
# from the Hugging Face file's git-LFS OID (the "sha256:" on the file's page,
# or `GET .../resolve/main/<file>` LFS pointer). If set, the download is
# verified against it and a mismatch aborts. Leave empty to fetch unverified —
# the script then PRINTS the computed hash so you can paste it here and into
# config/models.lock (the binary's embedded integrity manifest), then rebuild.
ROUTER_URL="https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf"
ROUTER_FILE="qwen2.5-3b-instruct-q4_k_m.gguf"   # lowercase dest to match config/default.toml
ROUTER_SIZE="1.93 GB"
ROUTER_SHA256="9c9f56a391a3abbd5b89d0245bf6106081bcc3173119d4229235dd9d23253f94"   # bartowski Q4_K_M, HF LFS oid

REASONING_URL="https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
REASONING_FILE="qwen2.5-7b-instruct-q4_k_m.gguf"   # lowercase, single file (not split) to match config/default.toml
REASONING_SIZE="4.68 GB"
REASONING_SHA256="65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423"   # bartowski Q4_K_M, HF LFS oid

# Collects "filename: hash" lines to print a ready-to-paste models.lock at the end.
MANIFEST_LINES=""

# ── Helpers ──────────────────────────────────────────────────────────
# Portable SHA-256 (Git Bash / Linux have sha256sum; macOS has shasum).
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# MODELTRUST: verify a downloaded file against its expected hash (if set) and
# record the computed hash for the models.lock snippet. Aborts on mismatch.
verify_and_record() {
    local dest="$1"
    local expected="$2"
    local file
    file="$(basename "$dest")"
    echo "  Hashing $file (SHA-256)..."
    local got
    got="$(sha256_of "$dest")"

    if [ -n "$expected" ]; then
        if [ "$got" != "$expected" ]; then
            echo "  !! INTEGRITY FAILURE: $file"
            echo "       expected $expected"
            echo "       got      $got"
            echo "  Refusing a model that doesn't match its pinned hash. Deleting."
            rm -f "$dest"
            exit 1
        fi
        echo "  ✓ verified against pinned hash"
    else
        echo "  ⚠ no pinned hash set — computed $got (verify against upstream before trusting)"
    fi
    MANIFEST_LINES="${MANIFEST_LINES}    \"$file\": { \"sha256\": \"$got\" },"$'\n'
}

download() {
    local url="$1"
    local dest="$2"
    local label="$3"
    local size="$4"
    local expected="$5"

    if [ -f "$dest" ]; then
        echo "  Already exists: $(basename "$dest") — verifying"
        verify_and_record "$dest" "$expected"
        return 0
    fi

    echo "  Downloading $label ($size)..."
    echo "  → $(basename "$dest")"

    # Use curl with resume support, progress bar, and redirect following
    curl -L --progress-bar --retry 3 --retry-delay 5 \
         -C - -o "$dest.part" "$url"

    # Rename on success (atomic-ish)
    mv "$dest.part" "$dest"
    echo "  Done: $(basename "$dest")"
    verify_and_record "$dest" "$expected"
}

# ── Main ─────────────────────────────────────────────────────────────
mkdir -p "$MODEL_DIR"

target="${1:-all}"

echo "Sovereign GE — Model Downloader"
echo "================================"
echo "Model directory: $MODEL_DIR"
echo ""

case "$target" in
    router)
        echo "[1/1] Router model (3B)"
        download "$ROUTER_URL" "$MODEL_DIR/$ROUTER_FILE" "Router (3B)" "$ROUTER_SIZE" "$ROUTER_SHA256"
        ;;
    reason|reasoning)
        echo "[1/1] Reasoning model (7B)"
        download "$REASONING_URL" "$MODEL_DIR/$REASONING_FILE" "Reasoning (7B)" "$REASONING_SIZE" "$REASONING_SHA256"
        ;;
    all)
        echo "[1/2] Router model (3B)"
        download "$ROUTER_URL" "$MODEL_DIR/$ROUTER_FILE" "Router (3B)" "$ROUTER_SIZE" "$ROUTER_SHA256"
        echo ""
        echo "[2/2] Reasoning model (7B)"
        download "$REASONING_URL" "$MODEL_DIR/$REASONING_FILE" "Reasoning (7B)" "$REASONING_SIZE" "$REASONING_SHA256"
        ;;
    *)
        echo "Usage: $0 [router|reason|all]"
        exit 1
        ;;
esac

echo ""
echo "Models ready in: $MODEL_DIR"
ls -lh "$MODEL_DIR"/*.gguf 2>/dev/null || true

# MODELTRUST: print the integrity-manifest snippet for config/models.lock.
if [ -n "$MANIFEST_LINES" ]; then
    echo ""
    echo "── config/models.lock snippet ──────────────────────────────────────"
    echo "Paste these into the \"models\" object of config/models.lock, then"
    echo "rebuild so they're embedded as the binary's load-time trust anchor:"
    echo ""
    echo "  \"models\": {"
    printf '%s' "$MANIFEST_LINES" | sed '$ s/,$//'
    echo "  }"
    echo "────────────────────────────────────────────────────────────────────"
fi
