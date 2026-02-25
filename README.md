# Sovereign GE

**Your data, your rules.**

An experimental local-first graphical environment with on-device AI, end-to-end encryption, and peer-to-peer sync.

Sovereign explores what personal computing looks like when nothing leaves your machine — no cloud accounts, no telemetry, no external servers. AI runs locally via quantized Qwen 2.5 models through llama.cpp. Documents are encrypted at rest with per-document keys. Devices sync directly over libp2p.

This is a prototype. Built in Rust. 10 crates. ~16K lines. Co-developed with [Claude](https://claude.ai) by Anthropic.

## What it explores

- **On-device AI** — A 3B router classifies intent; a 7B model handles complex queries. Multi-turn chat with tool calling, trust tracking, and prompt injection detection. No API keys, no subscriptions.
- **Spatial canvas** — Documents live on an infinite 2D canvas. Time runs left to right, projects top to bottom. Thread lanes, relationship arrows, minimap.
- **Action gravity** — Friction scales with irreversibility. Reading is instant. Deleting requires confirmation and a 30-day undo window. Security enforced by code architecture, not prompts.
- **Encryption & social recovery** — XChaCha20-Poly1305 with per-document keys. Zero plaintext on disk. Shamir secret sharing splits your recovery key across trusted guardians — 3 of 5 can reconstruct it.
- **Peer-to-peer sync** — Device pairing over libp2p. Encrypted manifests ensure even the network can't see your data.
- **Unified communications** — Email, Signal, WhatsApp — organized by person, not by app. Conversations stay local.
- **Content skills** — Composable tools instead of monolithic apps. Markdown editor, PDF export, search, image handling, file import.
- **Voice pipeline** — Wake word, Whisper speech-to-text, Piper TTS (optional).

## Architecture

Rust workspace with 10 crates:

| Crate | Role |
|---|---|
| `sovereign-core` | Shared types, config, interfaces, user profile, security primitives |
| `sovereign-db` | SurrealDB graph storage (in-memory and RocksDB persistent) |
| `sovereign-crypto` | Key hierarchy, XChaCha20-Poly1305, Shamir secret sharing, guardian recovery |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tool calling, trust, voice |
| `sovereign-ui` | Iced 0.14 GUI — taskbar, panels, chat, search, theming |
| `sovereign-canvas` | Infinite canvas — thread lanes, document cards, relationship arrows, minimap |
| `sovereign-skills` | Skill registry — markdown editor, search, image, PDF export, file import |
| `sovereign-p2p` | libp2p networking, device pairing, encrypted sync |
| `sovereign-comms` | Unified communications — email (IMAP/SMTP), Signal, WhatsApp |
| `sovereign-app` | Binary entry point — CLI dispatch and GUI bootstrap |

## Getting started

### Prerequisites

**All platforms:** Rust (edition 2021), Python 3 + `huggingface-hub` (for model downloads)

**Windows additionally:** Visual Studio Build Tools 2022 (C++ workload), [CMake](https://cmake.org/), [LLVM](https://llvm.org/) (for `libclang.dll`)

**Optional:** CUDA toolkit for GPU-accelerated inference

### 1. Download models

```bash
pip install huggingface-hub

# Router — intent classification (~2 GB)
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  qwen2.5-3b-instruct-q4_k_m.gguf --local-dir models/

# Reasoning — complex queries (~5 GB)
huggingface-cli download Qwen/Qwen2.5-7B-Instruct-GGUF \
  qwen2.5-7b-instruct-q4_k_m.gguf --local-dir models/
```

Filenames must match `config/default.toml`.

### 2. Build & run

```bash
# Linux / WSL2
cargo build --release -j 4
cargo run --release -- run

# Windows (PowerShell)
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
cargo build --release -p sovereign-app
.\target\release\sovereign.exe run
```

On first launch, sample data is seeded automatically.

### 3. Configure

Settings live in `config/default.toml`. Override at runtime with `sovereign --config path/to/custom.toml run`.

## Feature flags

| Flag | What it enables |
|---|---|
| `cuda` | GPU-accelerated LLM inference |
| `voice-stt` | Wake word detection + Whisper STT |
| `encryption` | Document encryption, guardian recovery |
| `p2p` | Device pairing and sync (implies `encryption`) |
| `comms-email` | Email channel (IMAP/SMTP) |
| `comms-signal` | Signal channel |
| `comms-whatsapp` | WhatsApp Business API channel |

## Tests

```bash
cargo test -j 4

# sovereign-ai without CUDA
cargo test -p sovereign-ai --no-default-features -j 4
```

## Status

This is an experimental prototype. Try it, break it, contribute.

Ideas we haven't built yet: federation, plugin marketplace, mobile companion, collaborative editing. See [Issues](https://github.com/clenoble/sovereign/issues).

## License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
