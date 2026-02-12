# Sovereign OS

Your data, your rules. A local-first personal operating system with AI assistance, end-to-end encryption, and peer-to-peer sync.

## Architecture

The project is a Rust workspace with 9 crates:

| Crate | Description |
|---|---|
| `sovereign-core` | Data models, config, security policy, interfaces |
| `sovereign-db` | SurrealDB graph storage (in-memory and RocksDB) |
| `sovereign-crypto` | Key hierarchy, XChaCha20-Poly1305, Shamir secret sharing |
| `sovereign-ai` | LLM orchestrator, intent classification, voice pipeline |
| `sovereign-p2p` | libp2p networking, device pairing, manifest-based sync |
| `sovereign-ui` | GTK4 application shell |
| `sovereign-canvas` | Skia-rendered infinite canvas with threads and documents |
| `sovereign-skills` | Pluggable skill system (editor, search, PDF export, etc.) |
| `sovereign-app` | Binary entry point — CLI and GUI |

## Prerequisites

- Rust (edition 2021)
- GTK4 development libraries
- CUDA toolkit (for GPU-accelerated inference; optional)
- Python 3 + `huggingface-hub` (for downloading models)

## Getting Started

### 1. Download AI models

The AI system uses quantized GGUF models via llama.cpp:

```bash
pip install huggingface-hub

# Router model — intent classification (~2 GB)
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  Qwen2.5-3B-Instruct-Q4_K_M.gguf --local-dir models/

# Reasoning model — complex queries (~5 GB)
huggingface-cli download Qwen/Qwen2.5-7B-Instruct-GGUF \
  Qwen2.5-7B-Instruct-Q4_K_M.gguf --local-dir models/
```

Filenames must match exactly what is in `config/default.toml`.

### 2. Configure

All settings live in `config/default.toml`:

```toml
[database]
mode = "persistent"
path = "data/sovereign.db"

[ui]
theme = "dark"
default_width = 1280
default_height = 720

[ai]
model_dir = "models"
router_model = "Qwen2.5-3B-Instruct-Q4_K_M.gguf"
reasoning_model = "Qwen2.5-7B-Instruct-Q4_K_M.gguf"
n_gpu_layers = 99   # 99 = all layers on GPU, 0 = CPU-only
n_ctx = 4096         # context window in tokens

[voice]
enabled = false
wake_word_model = "models/sovereign.rpw"
whisper_model = "models/ggml-large-v3-turbo.bin"
piper_binary = "piper"
piper_model = "models/en_US-lessac-medium.onnx"
piper_config = "models/en_US-lessac-medium.onnx.json"
```

You can override the config path at runtime:

```bash
sovereign --config /path/to/custom.toml run
```

### 3. Build

```bash
# Default build (CUDA enabled)
cargo build --release -j 4

# CPU-only (no CUDA required)
cargo build --release -j 4 --no-default-features

# With end-to-end encryption
cargo build --release -j 4 --features encryption

# With encryption + P2P sync
cargo build --release -j 4 --features p2p
```

Use `-j 4` on WSL2 with 16 GB RAM to avoid OOM. Drop to `-j 2` if builds still crash.

### 4. Run

```bash
# Launch the GUI
cargo run --release -j 4 -- run

# Or run the built binary directly
./target/release/sovereign run
```

On first launch with an empty database, sample data is seeded automatically.

## Feature Flags

| Flag | Crate | What it enables |
|---|---|---|
| `cuda` (default) | sovereign-ai | GPU-accelerated LLM inference |
| `wake-word` | sovereign-ai | Rustpotter-based wake word detection |
| `encryption` | sovereign-app | Document encryption, guardian recovery |
| `p2p` | sovereign-app | Device pairing, sync engine (implies `encryption`) |

## CLI Commands

Beyond `run`, the binary exposes data management commands:

```bash
sovereign create-doc --title "My Note" --thread-id "thread:abc"
sovereign get-doc --id "document:xyz"
sovereign list-docs [--thread-id "thread:abc"]
sovereign update-doc --id "document:xyz" --title "New Title"
sovereign delete-doc --id "document:xyz"

sovereign create-thread --name "Research" --description "..."
sovereign list-threads

sovereign add-relationship --from "document:a" --to "document:b" --relation-type "references"
sovereign list-relationships --doc-id "document:a"

sovereign commit --doc-id "document:xyz" --message "snapshot reason"
sovereign list-commits --doc-id "document:xyz"
```

With `--features encryption`:

```bash
sovereign encrypt-data
sovereign list-guardians
sovereign initiate-recovery
```

With `--features p2p`:

```bash
sovereign pair-device --peer-id <libp2p-peer-id>
sovereign list-devices
sovereign enroll-guardian --name "Alice" --peer-id <libp2p-peer-id>
```

## GPU Memory

| Configuration | VRAM Usage |
|---|---|
| Router only (3B Q4) | ~2 GB |
| Router + Reasoning (3B + 7B Q4) | ~6 GB |

The 3B router model loads at startup. The 7B reasoning model loads on demand when intent confidence is low. Both stay in VRAM simultaneously. Set `n_gpu_layers = 0` in config to fall back to CPU.

## Tests

```bash
cargo test -j 4
```

## License

All rights reserved.
