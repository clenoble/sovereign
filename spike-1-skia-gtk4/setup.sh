#!/usr/bin/env bash
# Spike 1 — WSL2 Environment Setup
# Run this inside your WSL2 Ubuntu instance (24.04 recommended).
#
# Prerequisites:
#   - WSL2 with Ubuntu 24.04
#   - NVIDIA GPU driver on the Windows host (version 525+ for WSLg/OpenGL support)
#   - WSLg enabled (default on Windows 11 / recent Windows 10)
#
# Usage:
#   chmod +x setup.sh && ./setup.sh

set -euo pipefail

echo "=== Spike 1: WSL2 Environment Setup ==="
echo ""

# ── Check WSL2 GPU access ────────────────────────────────────────────────────
echo "[1/4] Checking GPU access..."
if [ -e /dev/dxg ]; then
    echo "  OK: /dev/dxg found (GPU passthrough available)"
else
    echo "  WARN: /dev/dxg not found — GPU acceleration may not work."
    echo "  Make sure you have the latest NVIDIA driver on Windows with WSL support."
fi

if command -v glxinfo &>/dev/null; then
    GL_RENDERER=$(glxinfo 2>/dev/null | grep "OpenGL renderer" || true)
    echo "  $GL_RENDERER"
else
    echo "  (glxinfo not installed yet — will verify after setup)"
fi

# ── System packages ──────────────────────────────────────────────────────────
echo ""
echo "[2/4] Installing system dependencies..."
sudo apt update
sudo apt install -y \
    build-essential \
    pkg-config \
    clang \
    python3 \
    ninja-build \
    curl \
    git \
    libgtk-4-dev \
    libglib2.0-dev \
    libepoxy-dev \
    libgl-dev \
    libegl-dev \
    libfontconfig-dev \
    libfreetype-dev \
    mesa-utils

# ── Rust toolchain ───────────────────────────────────────────────────────────
echo ""
echo "[3/4] Installing Rust..."
if command -v rustc &>/dev/null; then
    echo "  Rust already installed: $(rustc --version)"
    echo "  Updating..."
    rustup update stable
else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    echo "  Installed: $(rustc --version)"
fi

# ── Verify ───────────────────────────────────────────────────────────────────
echo ""
echo "[4/4] Verifying setup..."
echo "  Rust:    $(rustc --version)"
echo "  Cargo:   $(cargo --version)"
echo "  GTK4:    $(pkg-config --modversion gtk4)"
echo "  Epoxy:   $(pkg-config --modversion epoxy)"
echo "  Clang:   $(clang --version | head -1)"

echo ""
GL_RENDERER=$(glxinfo 2>/dev/null | grep "OpenGL renderer" || echo "  (could not query GL renderer)")
echo "  GL:      $GL_RENDERER"

echo ""
echo "=== Setup complete ==="
echo ""
echo "Next steps:"
echo "  1. Copy the project to your WSL2 home directory for faster builds:"
echo "     cp -r /mnt/z/03\\ -\\ user-centered\\ OS/spike-1-skia-gtk4 ~/spike-1-skia-gtk4"
echo "  2. Build (first build will compile Skia from source — expect 15-30 min):"
echo "     cd ~/spike-1-skia-gtk4 && cargo build --release"
echo "  3. Run:"
echo "     cargo run --release"
