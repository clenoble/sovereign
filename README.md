# Sovereign GE

**Your data, your rules.**

An experimental local-first graphical environment with on-device AI, end-to-end encryption, and peer-to-peer sync.

Sovereign explores what personal computing looks like when nothing leaves your machine — no cloud accounts, no telemetry, no external servers. AI runs locally via quantized Qwen models (2.5 and 3.5) through llama.cpp. Documents are encrypted at rest with per-document keys. Devices sync directly over libp2p.

This is a prototype. Built in Rust. 13 crates. The **default desktop UI is a native Rust shell** (Vello + winit + wgpu) that calls the backend in-process — no web stack, no IPC. The Svelte 5 + Tauri 2 frontend is kept as a build option and is the **mobile** UI. Co-developed with [Claude](https://claude.ai) by Anthropic.

## What it explores

- **On-device AI** — A 3B router classifies intent; a 7B model handles complex queries. Multi-turn chat with tool calling, trust tracking, and prompt injection detection. Supports Qwen 2.5 and 3.5 (with thinking-mode suppression), Mistral, and Llama3. No API keys, no subscriptions.
- **Spatial canvas** — Documents live on a 2D canvas, rendered natively with Vello (no web stack). Time runs left to right on a **calibrated, zoom-adaptive axis** (zoom in for hours/minutes, out for months/years) with a live "now" line; thread lanes run top to bottom. Adaptive level-of-detail, minimap, sticky lane labels, and fit-to-data vs now-centered framing.
- **Embedded browser** — Browse the web from within Sovereign. An LLM-powered reliability assessment scores external content on domain-specific rubrics (factual integrity, logical coherence, rhetorical style). Save pages to your workspace with provenance and reliability metadata.
- **Memory consolidation** — Background AI process discovers semantic links between documents. Suggests relationships (supports, references, contradicts, continues, derived-from) with strength scores and rationale. Accept or dismiss — dismissed pairs are never re-suggested.
- **Action gravity** — Friction scales with irreversibility. Reading is instant. Deleting requires confirmation and a 30-day undo window. Security enforced by code architecture, not prompts.
- **Encryption & social recovery** — XChaCha20-Poly1305 field-level encryption with per-entity keys: document titles + bodies, message bodies, contact PII, and vault secrets are ciphertext at rest. Structural metadata (record ids, timestamps, graph edges, blind-index search tokens) stays queryable in the clear and is covered by OS full-disk encryption (BitLocker / FileVault / LUKS) — a deployment requirement, not an app guarantee. Shamir secret sharing splits your recovery key across trusted guardians — 3 of 5 can reconstruct it.
- **Peer-to-peer sync** — Interactive device pairing over libp2p: a QR + PIN handshake exchanges sealed keys over a live connection (the account key never travels in the QR), and the new device auto-syncs the workspace right after pairing. Encrypted manifests ensure even the network can't see your data.
- **Unified communications** — Email, Signal, WhatsApp — organized by person, not by app. Conversations stay local.
- **Content skills** — Composable tools instead of monolithic apps. ~30 built-in skills: markdown editor, PDF/HTML/plaintext export, search, find-replace, image handling, file import, outline extractor, link checker, PII detector, redactor, table of contents, JSON/YAML formatter, CSV → markdown, sort lists, case converter, backlink map, orphan finder, daily journal, thread summary, plus 20 community spec-as-seed skills. Third-party WASM skill plugins via the Component Model.
- **Voice pipeline** — Wake word, Whisper speech-to-text, Piper TTS (optional).

## Architecture

Rust workspace with 13 crates:

| Crate | Role |
|---|---|
| `sovereign-core` | Shared types, config, interfaces, user profile, security primitives |
| `sovereign-db` | SurrealDB graph storage (in-memory and RocksDB persistent) |
| `sovereign-crypto` | XChaCha20-Poly1305, key hierarchy, Shamir secret sharing, guardian recovery |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tool calling, trust, voice, reliability assessment, memory consolidation |
| `sovereign-skills` | Skill registry — 24 built-in skills covering read-only, read+write, and cross-document operations (markdown editor, exports, find-replace, outline / link / PII / readability scanners, redactor, ToC, formatters, backlink map, orphan finder, daily journal, thread summary, community seeds) |
| `sovereign-p2p` | libp2p networking, device pairing, encrypted sync |
| `sovereign-comms` | Unified communications — email (IMAP/SMTP), Signal, WhatsApp |
| `sovereign-shell` | **Native desktop UI** (default) — Rust/Vello/winit/wgpu, calls the backend crates in-process (no IPC). Builds as `sovereign` |
| `sovereign-app` | Tauri/CLI binary — Tauri bootstrap, dev CLI, embedded browser, Tauri commands. Builds as `sovereign-tauri` (desktop) + the `sovereign_app` cdylib (mobile) |
| `sovereign-import` | Bulk import funnel — stub-first migration of a document tree into the workspace (text / extract / stub tiers, folder→lane, timestamps preserved) |
| `sovereign-relay` | Public store-and-forward relay / seed node — capability-gated mailbox for backup & recovery messages |
| `sovereign-guardian` | Guardian enrollment + Recovery-Key shard custody for social recovery |
| `sovereign-guardian-app` | Companion guardian app — holds a recovery shard and approves recovery requests |

**Two desktop frontends, one mobile.** The default desktop UI is the **native shell** (`sovereign-shell`, binary `sovereign`): a roll-our-own widget layer on Vello 0.9 + winit 0.30 + wgpu + parley, rendering the spatial timeline canvas, floating windows (documents, contact-first inbox, chat, tabbed settings, embedded browser via `wry`, devices & sync), and the orchestrator bubble — all calling the backend in-process. The **Tauri 2.10 + Svelte 5 frontend** (`frontend/`, SvelteKit 2.50 + Vite 7.3) remains a build option on desktop (`sovereign-tauri`) and is the **mobile** UI (`cargo tauri android`). The previous Iced-based `sovereign-ui` and `sovereign-canvas` crates were retired in v0.0.3.

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
# Default desktop UI — the native shell (binary `sovereign`). No Node needed.
cargo build -p sovereign-shell -j 4
./target/debug/sovereign            # (or _run.bat on Windows)

# Tauri UI (kept as an option; also the mobile UI). Needs the frontend built:
cd frontend && npm install && npm run build && cd ..
cargo build -p sovereign-app --features encrypted-log -j 4
./target/debug/sovereign-tauri run  # (or _run-tauri.bat / _dev.bat for hot-reload)
```

On Windows, set `LIBCLANG_PATH` (defaults to `$env:ProgramFiles\LLVM\bin`) before building if you installed LLVM elsewhere. The `_build.bat` and `_release_build.bat` wrappers in the repo root configure the MSVC + LLVM + CUDA environment for you:

```cmd
:: Debug build with default features
_build.bat build -p sovereign-app -j 4

:: Full-feature CUDA release build
:: (cuda + encryption + p2p + comms-email + web-browse)
_release_build.bat
```

On first launch, the onboarding wizard offers to seed a sample workspace (enabled by default); a device that onboards by **pairing** skips seeding and instead receives its workspace from the paired peer over sync.

**Mobile pairing (Android).** Pairing a phone is driven from its onboarding wizard — choose *pair with an existing device* and scan the desktop's QR (the account key never travels in the QR; only a sealed handshake does). Both devices must be on the **same Wi-Fi/LAN**: discovery is mDNS and sync is QUIC-over-UDP, which the Android emulator's NAT blocks, so pairing must be tested on a **physical device**. One sharp edge: if the phone has **mobile data or Wi-Fi calling on, Android often keeps the cellular (IMS) network as the system default**, and libp2p binds its sync socket to whichever network is default *when the P2P node starts*. That socket then can't reach the desktop's LAN address — the workspace stays empty and the logs show `sendmsg … Operation not permitted` / `Outbound request … DialFailure`. The fix is to **turn off mobile data (and Wi-Fi calling) so Wi-Fi is the default, then restart the app** so the P2P node rebinds to Wi-Fi (Android won't move a live socket onto a different network). Binding P2P sockets to the Wi-Fi network explicitly is a known follow-up.

### 3. Configure

Settings live in `config/default.toml`. The dev CLI lives on the Tauri binary — override at runtime with `sovereign-tauri --config path/to/custom.toml run`.

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

This is an experimental prototype. Try it, break it, contribute. The most approachable contribution path is writing a new skill — see [doc/writing-skills.md](doc/writing-skills.md) for the WASM Component Model guide, sandbox model, and a worked example.

**Latest release:** [v0.0.9](RELEASE_NOTES_v0.0.9.md) — bulk document import (stub-first migration: text/extract/stub tiers, folder→lane, timestamps preserved), guardian-based social recovery (enroll guardians, threshold recovery, in-progress shares sealed at rest), and a broad security-hardening pass. Release notes for every version are in `RELEASE_NOTES_v0.0.*.md`.

Ideas we haven't built yet: federation, plugin marketplace, collaborative editing, rich document format (WYSIWYG), semantic search via embeddings.

## License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.html)
