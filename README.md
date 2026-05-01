# Sovereign GE

**Your data, your rules.**

An experimental local-first graphical environment with on-device AI, end-to-end encryption, and peer-to-peer sync.

Sovereign explores what personal computing looks like when nothing leaves your machine — no cloud accounts, no telemetry, no external servers. AI runs locally via quantized Qwen models (2.5 and 3.5) through llama.cpp. Documents are encrypted at rest with per-document keys. Devices sync directly over libp2p.

This is a prototype. Built in Rust. 8 crates plus a Svelte 5 + Tauri 2 frontend (the only supported UI as of [v0.0.3](RELEASE_NOTES_v0.0.3.md)). Co-developed with [Claude](https://claude.ai) by Anthropic.

## What it explores

- **On-device AI** — A 3B router classifies intent; a 7B model handles complex queries. Multi-turn chat with tool calling, trust tracking, and prompt injection detection. Supports Qwen 2.5 and 3.5 (with thinking-mode suppression), Mistral, and Llama3. No API keys, no subscriptions.
- **Spatial canvas** — Documents live on an infinite 2D canvas. Time runs left to right, thread lanes top to bottom. Adaptive level-of-detail: full cards at close zoom, density heatmap at extreme zoom-out. Minimap, sticky lane labels, cascade stacking for same-date cards.
- **Embedded browser** — Browse the web from within Sovereign. An LLM-powered reliability assessment scores external content on domain-specific rubrics (factual integrity, logical coherence, rhetorical style). Save pages to your workspace with provenance and reliability metadata.
- **Memory consolidation** — Background AI process discovers semantic links between documents. Suggests relationships (supports, references, contradicts, continues, derived-from) with strength scores and rationale. Accept or dismiss — dismissed pairs are never re-suggested.
- **Action gravity** — Friction scales with irreversibility. Reading is instant. Deleting requires confirmation and a 30-day undo window. Security enforced by code architecture, not prompts.
- **Encryption & social recovery** — XChaCha20-Poly1305 with per-document keys. Zero plaintext on disk. Shamir secret sharing splits your recovery key across trusted guardians — 3 of 5 can reconstruct it.
- **Peer-to-peer sync** — Device pairing over libp2p. Encrypted manifests ensure even the network can't see your data.
- **Unified communications** — Email, Signal, WhatsApp — organized by person, not by app. Conversations stay local.
- **Content skills** — Composable tools instead of monolithic apps. ~30 built-in skills as of v0.0.3: markdown editor, PDF/HTML/plaintext export, search, find-replace, image handling, file import, outline extractor, link checker, PII detector, redactor, table of contents, JSON/YAML formatter, CSV → markdown, sort lists, case converter, backlink map, orphan finder, daily journal, thread summary, plus 20 community spec-as-seed skills. Third-party WASM skill plugins via the Component Model.
- **Voice pipeline** — Wake word, Whisper speech-to-text, Piper TTS (optional).

## Architecture

Rust workspace with 8 crates:

| Crate | Role |
|---|---|
| `sovereign-core` | Shared types, config, interfaces, user profile, security primitives |
| `sovereign-db` | SurrealDB graph storage (in-memory and RocksDB persistent) |
| `sovereign-crypto` | XChaCha20-Poly1305, key hierarchy, Shamir secret sharing, guardian recovery |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tool calling, trust, voice, reliability assessment, memory consolidation |
| `sovereign-skills` | Skill registry — ~30 built-in skills covering read-only, read+write, and cross-document operations (markdown editor, exports, find-replace, outline / link / PII / readability scanners, redactor, ToC, formatters, backlink map, orphan finder, daily journal, thread summary, community seeds) |
| `sovereign-p2p` | libp2p networking, device pairing, encrypted sync |
| `sovereign-comms` | Unified communications — email (IMAP/SMTP), Signal, WhatsApp |
| `sovereign-app` | Binary entry point — CLI dispatch, Tauri bootstrap, embedded browser, Tauri commands |

The sole UI is a **Tauri 2.10 + Svelte 5 frontend** (`frontend/`), built with SvelteKit 2.50 and Vite 7.3 over Tauri IPC. Includes timeline canvas, AI chat panel, embedded browser, suggestion panel, onboarding wizard, settings, and trust dashboard. The previous Iced-based `sovereign-ui` and `sovereign-canvas` crates were retired in v0.0.3.

## Getting started

### Prerequisites

**All platforms:** Rust (edition 2021), [Node.js](https://nodejs.org/) 20+ (for frontend), Python 3 + `huggingface-hub` (for model downloads)

**Windows additionally:** Visual Studio Build Tools 2022 (C++ workload), [CMake](https://cmake.org/), [LLVM](https://llvm.org/) (for `libclang.dll`)

**Optional:** CUDA toolkit for GPU-accelerated inference. **Note for CUDA 13:** the runtime DLLs (`cudart64_13.dll`, `cublas64_13.dll`, `cublasLt64_13.dll`) live in `<CUDA_PATH>\bin\x64\` rather than `\bin\`, and the installer doesn't add the `\bin\x64` directory to `PATH`. Either copy the three DLLs next to `sovereign.exe` or prepend `\bin\x64` to `PATH` before launch.

### 1. Download models

```bash
pip install huggingface-hub

# Router — intent classification (~2 GB)
# Qwen 2.5:
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  qwen2.5-3b-instruct-q4_k_m.gguf --local-dir models/
# Or Qwen 3.5 (thinking-mode auto-suppressed):
huggingface-cli download Qwen/Qwen3-4B-GGUF \
  qwen3-4b-q4_k_m.gguf --local-dir models/

# Reasoning — complex queries (~5 GB)
huggingface-cli download Qwen/Qwen2.5-7B-Instruct-GGUF \
  qwen2.5-7b-instruct-q4_k_m.gguf --local-dir models/
```

Filenames must match `config/default.toml`. Qwen 3.5 models are auto-detected from the GGUF filename and use optimized sampling parameters with `/no_think` thinking-mode suppression.

### 2. Build & run

```bash
# Install frontend dependencies
cd frontend && npm install && cd ..

# Build frontend + Rust backend together
cd frontend && npm run build && cd ..
cargo build -p sovereign-app --features encrypted-log -j 4

# Or use Tauri CLI for dev mode (hot-reload)
cd frontend && npm run tauri dev
```

On Windows, set `LIBCLANG_PATH` (defaults to `$env:ProgramFiles\LLVM\bin`) before building if you installed LLVM elsewhere. The `_build.bat` and `_release_build.bat` wrappers in the repo root configure the MSVC + LLVM + CUDA environment for you:

```cmd
:: Debug build with default features
_build.bat build -p sovereign-app -j 4

:: Full-feature CUDA release build
:: (cuda + encryption + p2p + comms-email + web-browse)
_release_build.bat
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
| `web-browse` | Embedded browser with LLM reliability assessment |
| `encrypted-log` | Per-entry encrypted session log (on by default) |

## Tests

```bash
# Backend
cargo test -j 4

# sovereign-ai without CUDA
cargo test -p sovereign-ai --no-default-features -j 4

# Frontend (Vitest + happy-dom; covers stores and a growing set of components)
cd frontend && npm test
```

## Status

This is an experimental prototype. Try it, break it, contribute. See [open issues](https://github.com/clenoble/sovereign/issues) for good starting points.

**Latest release:** [v0.0.3](RELEASE_NOTES_v0.0.3.md) — skills system, embedded browser with Qwen-driven reliability assessment, Qwen 3.5 support, AI-suggested document links, full-feature CUDA release build, and the Iced UI retirement.

**On the `pii-management-dashboard` branch (targeted for v0.0.4):** PII detection pipeline, vault, signup capture, autofill, cookies tab, share ledger, and a three-column dashboard panel.

Ideas we haven't built yet: federation, plugin marketplace, mobile companion, collaborative editing, rich document format (WYSIWYG), semantic search via embeddings.

## License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
