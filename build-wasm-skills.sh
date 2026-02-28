#!/usr/bin/env bash
# Build all WASM skill plugins.
# Prerequisites: rustup target add wasm32-wasip1 && cargo install wasm-tools
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SKILLS_DIR="$SCRIPT_DIR/skills"

# Ensure target and tools are available
if ! rustup target list --installed | grep -q wasm32-wasip1; then
    echo "Installing wasm32-wasip1 target..."
    rustup target add wasm32-wasip1
fi

if ! command -v wasm-tools &>/dev/null; then
    echo "Installing wasm-tools..."
    cargo install wasm-tools
fi

BUILT=0
FAILED=0

for skill_dir in "$SKILLS_DIR"/*/; do
    if [ ! -f "$skill_dir/Cargo.toml" ]; then
        continue
    fi

    name=$(basename "$skill_dir")
    echo "Building WASM skill: $name"

    # Build the cdylib targeting wasm32-wasip1
    if (cd "$skill_dir" && cargo build --target wasm32-wasip1 --release 2>&1); then
        # Find the built .wasm file (cdylib produces a .wasm with underscored name)
        wasm_name="${name//-/_}"
        core_wasm="$skill_dir/target/wasm32-wasip1/release/${wasm_name}.wasm"

        if [ ! -f "$core_wasm" ]; then
            echo "  ERROR: Expected $core_wasm not found"
            FAILED=$((FAILED + 1))
            continue
        fi

        # Convert core module to component
        component_wasm="$skill_dir/${name}.component.wasm"
        if wasm-tools component new "$core_wasm" -o "$component_wasm" 2>&1; then
            size=$(du -h "$component_wasm" | cut -f1)
            echo "  OK: $component_wasm ($size)"
            BUILT=$((BUILT + 1))
        else
            echo "  ERROR: wasm-tools component new failed"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "  ERROR: cargo build failed"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo "Done: $BUILT built, $FAILED failed"
