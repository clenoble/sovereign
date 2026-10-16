# 14 — Hardware

What Sovereign GE needs to run well, what it checks, and which on-device models it is built
around. Everything runs locally, so the user's machine is the whole budget: the models, the
graph and the encryption share it.

*Consolidates:* specification v1.3 §Hardware Requirements (Target Specification, Installation
Hardware Check, Model Candidates) and Open Question 1 (orchestrator model selection);
`config/default.toml`, `config/models.lock`; `README.md` (model downloads, CUDA);
`CLAUDE.md` (orchestrator, CUDA runtime); `RELEASE_NOTES_v0.0.3`, `v0.0.6`–`v0.0.8`;
Appendix C §2.3 (local inference).

---

## Target machine

**Status:** ◐ Partial — the target is unchanged from v1; no build verifies a machine against
it, and no shipped build uses the GPU (below).

The reference desktop is a mid-range gaming PC of 2024–2026:

| Component | Target |
|---|---|
| CPU | 6 cores (Intel i5-12400 / AMD Ryzen 5 5600 class) |
| RAM | 16 GB |
| GPU | NVIDIA RTX 3060 with 12 GB VRAM, or AMD equivalent |
| Storage | 512 GB NVMe SSD; about 100 GB for the application and models |
| Disk encryption | OS full-disk encryption on (BitLocker, FileVault, LUKS) — a deployment requirement of the at-rest model, ch. 07 |

What the target buys: both language models resident on the GPU, with voice and the vision
sidecar alongside.

### What runs where today

**Status:** ◐ Partial — no shipped build uses the GPU (code:
`crates/sovereign-shell/Cargo.toml`, `crates/sovereign-app/Cargo.toml`, `_release_build.bat`).

| Build | Inference | GPU needed |
|---|---|---|
| `sovereign` — native shell, the desktop UI (shipped) | CPU only (llama.cpp, in-process) | No |
| Android app (`sovereign_app`, mobile feature set; shipped) | CPU — llama.cpp is compiled in; whether a model loads on a phone is open (*Mobile* below) | No |
| `sovereign-tauri` built with `cuda` (the old release script's feature set, ch. 02) — **frozen, not shipped** | NVIDIA GPU, all layers offloaded (`n_gpu_layers = 99`) | NVIDIA driver, plus the three CUDA 13 runtime DLLs beside the executable or on `PATH` |
| Default `cargo build` of either desktop binary | CPU only | No |

The current release ships no CUDA build: the v0.0.9 assets contain no CUDA executable and no
CUDA runtime (ch. 02). A GPU build of the native shell is open work (backlog).

◐ AMD GPUs: no ROCm or Vulkan inference build exists; "or AMD equivalent" in the target is not
backed by a build. Speech-to-text runs on the CPU by design, to avoid contending with the
language models for VRAM.

## Installation hardware check

**Status:** ○ Specified, not built — one clause in conflict (below).

On first start the system evaluates CPU cores, RAM, GPU VRAM and GPU compute support, and
adjusts rather than fails:

| Condition | Effect |
|---|---|
| GPU VRAM < 8 GB | Warn; disable image generation (and any feature that needs it). |
| RAM < 12 GB | Warn; the reasoning model is not loaded automatically. |
| No GPU compute | Warn; inference, voice and OCR run on the CPU. |

The result is shown in plain language before onboarding continues: what is adequate, what is
limited, which features will be unavailable, and how to change models later (the Models
window, ch. 08). The user can continue or configure models.

Nothing of this exists in code; there is no image generation feature to disable either.

**Decided 2026-10-10 (Céline):** the cloud fallback of the v1 hardware dialog is **abandoned**.
On limited hardware the dialog says what will be slow or unavailable and how to choose smaller
models; it never offers a cloud API.

## Model budget and lifecycle

**Status:** ✅ Built (code: `crates/sovereign-ai/src/intent/classifier.rs`,
`crates/sovereign-ai/src/llm/`, `config/default.toml`, `crates/sovereign-shell/src/app.rs`).

| Model | Role | Size on disk (Q4_K_M) | Residency |
|---|---|---|---|
| Router, 3–4B | Intent classification; the resident model | about 2 GB | Loaded on first use, then resident |
| Reasoning, 7B | Complex or ambiguous queries, the chat agent loop | about 5 GB | Loaded on demand; freed at the next request after 5 minutes idle |

The router is loaded when the orchestrator is built. The native shell builds the orchestrator
lazily, on the first chat or browser request, so the router loads then; the Tauri host
(mobile) builds it at start-up. The reasoning model's idle check runs when a request arrives,
not on a timer: after five idle minutes it is freed at the next classification, before the
router runs.

Default context window: 4,096 tokens. A model swap drops the old model before loading the new
one, so VRAM is never held twice. Models live in a user-chosen model directory; they are
redownloadable assets, not user data.

**Integrity.** Known models are pinned by SHA-256 in a manifest compiled into the binary; a
listed model whose bytes do not match is refused. Any other model is trusted on first use and
recorded (ch. 08).

## Model candidates

**Status:** ✅ Built — selection made, other families supported at runtime.

Selection criteria: open weights (can be quantised or fine-tuned); permissive licence;
strong instruction-following; English **and** French; GGUF available for llama.cpp. Appendix C
§2.3 bears on the choice: its risk assessment rests on the shipped models being small (1–8B)
and safety-filtered, with larger or uncensored models left to the user.

### Router (3–4B)

**Status:** ✅ Built — Qwen2.5-3B is the default (code: `config/default.toml`).

| Model | Licence | Notes |
|---|---|---|
| **Qwen2.5-3B-Instruct** — selected, default | Apache 2.0 | Best EN/FR, strong instruction-following, well-supported GGUF. Pinned in the integrity manifest. |
| Qwen 3.x 4B | Apache 2.0 | Supported; thinking mode suppressed in prompts and stripped from output; own sampling settings. [unverified] Whether the supported model is Qwen3-4B or a Qwen 3.5 release — the README names both. |
| Phi-4-mini (3.8B) | MIT | Candidate; strong reasoning, English-centric. Not tested. |
| Llama 3.2-3B | Llama 3 licence | Candidate; weaker French. The Llama 3 prompt format is supported. |
| Ministral 3B | [unverified] licence | Found by the fuzzy model resolver; Mistral prompt format supported. |

### Reasoning (7–8B)

**Status:** ✅ Built — Qwen2.5-7B is the default (code: `config/default.toml`).

| Model | Licence | Notes |
|---|---|---|
| **Qwen2.5-7B-Instruct** — selected, default | Apache 2.0 | Excellent multilingual, same tokenizer family as the router, strong at code. Pinned in the integrity manifest. |
| Llama 3.x 8B | Llama 3 licence | Candidate; strong English and tool use. |
| Mistral-7B-v0.3 | Apache 2.0 | Candidate; good European-language support. |

Any GGUF model can be hot-swapped at runtime; the prompt format is detected from the
filename (ChatML, ChatML with Qwen 3 thinking suppression, Mistral, Llama 3). Fine-tuning is
not needed for the current scope: a heuristic parser handles most intents and the router is
the fallback for ambiguous input.

*Replaces v1.x:* "Router (1–3B)" — the supported router range is 3–4B. v1 gave the reasoning
range as 7–13B; every built and listed candidate is 7–8B. [unverified] Whether a model above
8B is still intended as a default; Appendix C §2.3's assessment assumes 8B or less.

### Other on-device models

**Status:** ◐ Partial — present behind optional features or sidecars; none in a shipped
build (the native shell has no voice or vision, the mobile feature set has no speech-to-text,
and the Tauri desktop build that carried them is frozen).

| Model | Purpose | Where | Footprint |
|---|---|---|---|
| Whisper large-v3-turbo (whisper.cpp) | Speech-to-text | `voice-stt` feature | CPU |
| Piper (`en_US-lessac-medium`) | Text-to-speech | Voice pipeline, external `piper` binary | CPU |
| Rustpotter wake-word model | Wake word | `wake-word` feature | CPU, always listening when enabled |
| MediaPipe gesture tasks | Gesture detection | `jiminy-vision` sidecar | Every frame, light |
| SmolVLM2 (about 2.2B) | Windowed scene description | `jiminy-vision` sidecar, off by default, lazy-loaded | Fits in about 5.6 GB VRAM (verified on a GTX 1660) |

Voice and the wake word are off by default. The vision sidecar has its own Python environment
and GPU budget (ch. 02).

## Mobile

**Status:** ○ Specified, not built — no hardware requirement is set for Android.

**Decided 2026-10-10 (Céline): no AI on mobile for now.** llama.cpp is to be removed from the
Android build (it is compiled in today although no model ships).

The Android app (ch. 02) runs the same backend on SurrealKV with p2p sync. llama.cpp is
compiled into the mobile library (through `sovereign-ai`), and the app builds the orchestrator
at start-up (code: `crates/sovereign-app/Cargo.toml`, `src/lib.rs`). [unverified] Whether a
model is present and loads on a phone in practice, and on what class of phone; if the router
cannot load, the app runs without the orchestrator and chat requests return "AI orchestrator
not available". Pairing and sync are reliable on Wi-Fi; over cellular data they are unstable
(v0.0.8 known issue, ch. 05).
