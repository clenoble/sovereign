# Spike 3: llama.cpp FFI — Local LLM Inference

Validates that Rust can load, run inference on, and cleanly unload GGUF models via llama.cpp direct FFI — without a Python runtime. Tests hot-swap workflow on a GTX 1660 (6GB VRAM).

## Acceptance Criteria

| Benchmark | Target | Description |
|-----------|--------|-------------|
| Model load time (3B) | < 10s | Load Qwen2.5-3B Q4 to GPU |
| Model load time (7B) | < 10s | Load Qwen2.5-7B Q4 to GPU |
| Inference latency (3B) | < 500ms | Simple classification task |
| Inference latency (7B) | < 2s | Short generation task |
| Memory reclaimed after unload | ±100MB | VRAM returns to baseline |
| Hot-swap: unload 3B → load 7B | No OOM | Sequential model swap |

## Prerequisites

- cmake, C/C++ compiler (`sudo apt install cmake build-essential`)
- CUDA toolkit (`sudo apt install nvidia-cuda-toolkit`)
- GGUF model files downloaded (see below)

## Model Download

```bash
# Install huggingface CLI if needed
pip install huggingface-hub

# Router model (~2GB)
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  qwen2.5-3b-instruct-q4_k_m.gguf --local-dir models/

# Reasoning model (~5GB)
huggingface-cli download Qwen/Qwen2.5-7B-Instruct-GGUF \
  qwen2.5-7b-instruct-q4_k_m.gguf --local-dir models/
```

## Usage

```bash
# Default (models in ./models/)
cargo run --release

# Custom model directory
cargo run --release -- --model-dir /path/to/models

# Adjust GPU layers (0 = CPU only, 99 = all on GPU)
cargo run --release -- --n-gpu-layers 99

# Adjust context size
cargo run --release -- --n-ctx 2048
```

## What This Tests

1. **Model loading** — `llama-cpp-2` crate with CUDA GPU offload
2. **Inference** — Tokenize, decode loop, sampling (temp + top_p), detokenize
3. **Memory management** — RAII-based unload, VRAM reclamation verification
4. **Hot-swap** — Unload one model, load another without OOM or restart

## Architecture

- `backend.rs` — `ModelBackend` trait (carries forward to `sovereign-ai`)
- `llm.rs` — `LlamaCppBackend` implementing the trait via `llama-cpp-2`
- `benchmark.rs` — RSS/VRAM measurement, timing, result reporting
- `main.rs` — CLI orchestration: 3 phases (3B, 7B, hot-swap)

## Decision Gates

- **All pass**: Proceed with llama.cpp FFI for Phase 3 (AI Orchestrator)
- **Failures**: Evaluate alternatives (llama.cpp C API direct, or PyO3 fallback)
