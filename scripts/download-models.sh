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
ROUTER_URL="https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf"
ROUTER_FILE="Qwen2.5-3B-Instruct-Q4_K_M.gguf"
ROUTER_SIZE="1.93 GB"

REASONING_URL="https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
REASONING_FILE="Qwen2.5-7B-Instruct-Q4_K_M.gguf"
REASONING_SIZE="4.68 GB"

# ── Helpers ──────────────────────────────────────────────────────────
download() {
    local url="$1"
    local dest="$2"
    local label="$3"
    local size="$4"

    if [ -f "$dest" ]; then
        echo "  Already exists: $(basename "$dest") — skipping"
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
        download "$ROUTER_URL" "$MODEL_DIR/$ROUTER_FILE" "Router (3B)" "$ROUTER_SIZE"
        ;;
    reason|reasoning)
        echo "[1/1] Reasoning model (7B)"
        download "$REASONING_URL" "$MODEL_DIR/$REASONING_FILE" "Reasoning (7B)" "$REASONING_SIZE"
        ;;
    all)
        echo "[1/2] Router model (3B)"
        download "$ROUTER_URL" "$MODEL_DIR/$ROUTER_FILE" "Router (3B)" "$ROUTER_SIZE"
        echo ""
        echo "[2/2] Reasoning model (7B)"
        download "$REASONING_URL" "$MODEL_DIR/$REASONING_FILE" "Reasoning (7B)" "$REASONING_SIZE"
        ;;
    *)
        echo "Usage: $0 [router|reason|all]"
        exit 1
        ;;
esac

echo ""
echo "Models ready in: $MODEL_DIR"
ls -lh "$MODEL_DIR"/*.gguf 2>/dev/null || true
