# Third-Party Licenses

## Sovereign GE — Third-Party Component Licenses

**Last Updated:** February 22, 2026

This document lists all third-party dependencies included in or required by Sovereign GE, grouped by component. Each entry includes the project name, license, and upstream URL.

This file is maintained manually and updated with each release. If you discover a missing or incorrect entry, please open an issue.

---

## Core Runtime (Rust)

| Component | License | URL |
|---|---|---|
| gtk4-rs | MIT | https://github.com/gtk-rs/gtk4-rs |
| rust-skia (skia-safe) | MIT | https://github.com/aspect-rs/rust-skia |
| surrealdb | BSL 1.1 → Apache 2.0 (after change date) | https://github.com/surrealdb/surrealdb |
| pyo3 | Apache 2.0 / MIT | https://github.com/PyO3/pyo3 |
| tokio | MIT | https://github.com/tokio-rs/tokio |
| serde / serde_json | Apache 2.0 / MIT | https://github.com/serde-rs/serde |
| uuid | Apache 2.0 / MIT | https://github.com/uuid-rs/uuid |
| ring (cryptography) | ISC / OpenSSL | https://github.com/briansmith/ring |
| libp2p (rust-libp2p) | MIT | https://github.com/libp2p/rust-libp2p |
| quinn (QUIC) | Apache 2.0 / MIT | https://github.com/quinn-rs/quinn |
| clap | Apache 2.0 / MIT | https://github.com/clap-rs/clap |
| tracing | MIT | https://github.com/tokio-rs/tracing |
| sha2 / hkdf | Apache 2.0 / MIT | https://github.com/RustCrypto |
| vsss-rs (Shamir's Secret Sharing) | Apache 2.0 / MIT | https://github.com/ArtOfBlockchain/vsss-rs |

### Note on SurrealDB License

SurrealDB uses the Business Source License 1.1, which converts to Apache 2.0 after a specified change date. Sovereign GE uses SurrealDB in embedded mode (not as a hosted service). Verify compliance with the current BSL terms before each release. If the BSL terms become incompatible, the `sovereign-db` crate is designed behind a `GraphDB` trait that can be backed by SQLite + JSONB as a fallback.

---

## AI / Python Layer

| Component | License | URL |
|---|---|---|
| llama-cpp-python | MIT | https://github.com/abetlen/llama-cpp-python |
| llama.cpp | MIT | https://github.com/ggerganov/llama.cpp |
| openWakeWord | Apache 2.0 | https://github.com/dscripka/openWakeWord |
| faster-whisper | MIT | https://github.com/SYSTRAN/faster-whisper |
| piper-tts | MIT | https://github.com/rhasspy/piper |
| sentence-transformers | Apache 2.0 | https://github.com/UKPLab/sentence-transformers |
| transformers (Hugging Face) | Apache 2.0 | https://github.com/huggingface/transformers |
| trocr (via transformers) | MIT | https://github.com/microsoft/unilm/tree/master/trocr |
| numpy | BSD 3-Clause | https://github.com/numpy/numpy |
| torch (PyTorch) | BSD 3-Clause | https://github.com/pytorch/pytorch |

---

## AI Models (Weights)

AI model weights are distributed separately from the Sovereign GE source code. Each model has its own license governing use, modification, and redistribution.

| Model | License | URL |
|---|---|---|
| Phi-3-mini (router) | MIT | https://huggingface.co/microsoft/Phi-3-mini-4k-instruct |
| Llama 3.1-8B (reasoning) | Llama 3.1 Community License | https://huggingface.co/meta-llama/Llama-3.1-8B-Instruct |
| Whisper-small (STT) | MIT | https://huggingface.co/openai/whisper-small |
| Piper voices (TTS) | MIT | https://github.com/rhasspy/piper |
| TrOCR-base (OCR) | MIT | https://huggingface.co/microsoft/trocr-base-handwritten |
| all-MiniLM-L6-v2 (embeddings) | Apache 2.0 | https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2 |

### Note on Llama 3.1 License

The Llama 3.1 Community License permits commercial and research use but includes specific terms regarding acceptable use policies and attribution. It is **not** an OSI-approved open-source license. Users who redistribute Sovereign GE with Llama 3.1 weights must comply with Meta's license terms independently. The `sovereign-ai` model abstraction layer supports swapping Llama for a fully permissive alternative (e.g., Mistral-7B under Apache 2.0, Qwen2.5-7B under Apache 2.0) without code changes.

---

## System Dependencies

These are not bundled with Sovereign GE but are required at runtime. They are installed via the system package manager.

| Component | License | Purpose |
|---|---|---|
| GTK 4 | LGPL 2.1+ | UI toolkit |
| Skia | BSD 3-Clause | 2D rendering engine |
| GStreamer | LGPL 2.1+ | Audio pipeline (voice I/O) |
| PipeWire | MIT / LGPL 2.1+ | Audio server |
| pandoc | GPL 2+ | Document format conversion |
| typst | Apache 2.0 | PDF export engine |
| Python 3.11+ | PSF License | AI runtime |
| CUDA toolkit (optional) | NVIDIA EULA (proprietary) | GPU inference acceleration |
| ROCm (optional) | MIT | AMD GPU inference acceleration |

### Note on CUDA

NVIDIA CUDA is proprietary software. It is **not** bundled with Sovereign GE. Users who install CUDA for GPU acceleration do so under NVIDIA's terms. Sovereign GE functions without CUDA (CPU-only inference) and supports ROCm (MIT-licensed) as an open alternative for AMD GPUs.

---

## Build Tools

| Component | License | Purpose |
|---|---|---|
| Rust compiler (rustc) | Apache 2.0 / MIT | Compilation |
| Cargo | Apache 2.0 / MIT | Build system |
| maturin | Apache 2.0 / MIT | Python-Rust bridge build |
| Nix (optional) | LGPL 2.1 | Reproducible builds |

---

## License Compatibility Summary

| License | AGPL-3.0 Compatible | Notes |
|---|---|---|
| MIT | ✅ | Permissive, no issues |
| Apache 2.0 | ✅ | Compatible with GPL v3+ |
| BSD 3-Clause | ✅ | Permissive, no issues |
| ISC | ✅ | Permissive, no issues |
| LGPL 2.1+ | ✅ | Dynamic linking preserved |
| PSF License | ✅ | Permissive, no issues |
| GPL 2+ | ✅ | "Or later" clause enables v3 compatibility |
| BSL 1.1 (SurrealDB) | ⚠️ | Embedded use likely OK; verify per release |
| Llama 3.1 Community | ⚠️ | Separate distribution; not compiled into AGPL binary |
| NVIDIA CUDA EULA | ❌ | Not bundled; optional user-installed dependency |

---

## How to Verify

```bash
# Rust dependencies
cargo license --manifest-path Cargo.toml

# Python dependencies
pip-licenses --format=markdown

# System dependencies
dpkg -l | grep -E "gtk|skia|gstreamer|pipewire|pandoc|typst"
```

---

*If you believe any entry in this document is incorrect or incomplete, please open an issue or submit a pull request.*
