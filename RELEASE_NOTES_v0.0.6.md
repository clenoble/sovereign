# v0.0.6 — Encryption at rest + Reachy Mini (Jiminy) embodiment

This release delivers two independent headline features. First, **full
at-rest encryption of the graph database**: message bodies, message subjects,
and the metadata of threads, conversations, contacts, share records, and
documents are now encrypted on disk under per-entity AEAD keys, with
**token blind-indexes** so encrypted fields are still searchable. The
encrypted store is swapped in transparently after login via a
`LayeredGraphDB`, and Android gains **persistent SurrealKV storage** (replacing
the v0.0.5 process-lifetime `kv-mem`). Second, **Reachy Mini support** through
the **Jiminy** bridge — Sovereign GE can now embody itself in a physical (or
simulated) Reachy Mini robot, with bidirectional voice, camera-based vision and
scene understanding, and gesture interaction.

The two feature sets are orthogonal: encryption lives in `sovereign-db` /
`sovereign-crypto`; Jiminy lives in `sovereign-ai` + Python sidecars and is
entirely `#[cfg(feature = "jiminy" | "vision")]`-gated. Desktop default
behavior is unchanged except that the local graph is now encrypted at rest.

> **Build status.** The encryption work is verified on the non-CUDA matrix
> (`sovereign-db` 105 tests under both `surrealkv` and `rocksdb`) **and
> on-device on a Pixel 8 Pro** (Android 16, debug aarch64 APK: fresh
> install → login → encrypted write → force-stop → relaunch → decrypt).
> The Jiminy stack is validated **live on a physical Reachy Mini** plus the
> sidecar/vision test suites. Encryption + all of `main`'s Jiminy work were
> merged on `v0.0.6/wire-encryption` (clean auto-merge), and the
> **merged-tree integration `cargo check` is green** — both the default
> workspace and the combined `--features vision,jiminy` config (where the
> encrypted `LayeredGraphDB` swap and the Jiminy `run_tauri` wiring coexist)
> compile with no errors. Windows builds must use a **warm cargo target** —
> Smart App Control blocks freshly compiled build-scripts, so never
> `cargo clean` on the Windows host (see *Build / packaging*).

## Highlights — Encryption at rest

- **Message body & subject encryption with searchable blind-index.**
  `Message.body` and `Message.subject` are encrypted at rest; a
  **token blind-index** (`search_messages_by_token_hashes`) preserves keyword
  search over ciphertext without revealing plaintext to the store. (Phase 2a)

- **Metadata encryption across the graph.** Thread names, conversation titles,
  contact names + notes, document titles, and share-record URLs are encrypted,
  each with a matching token-hash search path
  (`search_documents_by_title_token_hashes`, `find_thread_by_name_token_hashes`,
  …). Share records gain a `via_url_nonce`. (Phase 2b)

- **Transparent `LayeredGraphDB` swap.** Boot opens the raw `SurrealGraphDB`
  (the key-encryption key isn't available pre-login). On login,
  `install_session()` swaps the inner DB to an `EncryptedGraphDB` behind an
  `arc-swap`, so every consumer (orchestrator, skills, Tauri commands) keeps a
  single stable `Arc` while reads/writes start flowing through encryption.
  (Phase 2c)

- **Per-entity AEAD keys.** `EncryptedGraphDB` mints a fresh per-entity key into
  a `KeyDatabase`, each wrapped at rest under the device key
  (`keys.<entity>.db`). This keeps the blast radius of any single key small.

- **Persistent storage on Android (SurrealKV).** Mobile now uses a pure-Rust
  **SurrealKV** backend that cross-compiles cleanly to Android (RocksDB does
  not), replacing v0.0.5's `kv-mem`. Encrypted rows now survive an app restart
  on-device. Desktop keeps RocksDB; `sovereign-db` prefers RocksDB at runtime
  when both are compiled in. (Phase 1)

## Highlights — Reachy Mini (Jiminy)

- **Physical AI embodiment bridge.** A `jiminy-bridge` Python sidecar
  (FastAPI on `:9100`) wraps the `reachy_mini` SDK for emotion playback, poses,
  dances, and speech. The Rust `sovereign-ai/jiminy.rs` `JiminyBridge` maps
  `OrchestratorEvent`s to robot commands over HTTP, with graceful degradation
  when the robot/sidecar is absent. A fan-out thread forwards every
  orchestrator event to **both** the UI and Jiminy.

- **Bidirectional voice.** **TTS** via a bundled, auto-detected **Piper** voice
  (real `/speak` audio, resampled to the robot speaker); Markdown is stripped
  before speaking. **STT** runs **out-of-process** in the sidecar
  (`faster-whisper`) — deliberately not in-process, because `whisper-rs` and
  `llama-cpp-2` both embed `ggml` and clash at runtime. The voice pipeline
  supports a dual source: `cpal` (PC mic) or `jiminy` (robot ReSpeaker array).

- **Camera vision + scene understanding.** A separate `jiminy-vision` service
  (own venv, `:9101`) does always-on **MediaPipe** gesture detection (shush,
  open-palm/stop, point, fist, thumbs up/down, victory) and a **windowed
  SmolVLM2 (~2.2B)** scene captioner (lazy-loaded, off until a window is opened,
  default 300s). Frame source is the PC webcam (dev) or the robot camera.

- **Vision-aware chat.** The latest scene caption is injected into the
  orchestrator's system prompt (`format_vision_context()`), so the AI's replies
  can reference what Jiminy currently sees. One `SharedVision` store is written
  by the vision poller and read by the orchestrator.

- **Gesture-triggered voice input.** The `talking_hand` gesture opens the robot
  mic (`POST /listen` → attentive cue → record → `faster-whisper` → orchestrator
  → spoken reply). `voice-event`s light up the mic button and surface the
  transcript in the chat window.

- **Barge-in & lifecycle cues.** A `shush` gesture (or `/stop`) interrupts
  Jiminy mid-speech; on app exit Jiminy plays a **goodnight/sleep** animation
  (`POST /sleep` over a raw blocking TCP request, since the async runtime is
  already tearing down).

- **Vision UI & simulator.** A camera tile + `VisionPanel`, vision-window
  controls and duration setting in Settings, new `vision`/`voice` Svelte stores,
  and a gold-themed **MuJoCo** scene for the Reachy Mini simulator (`--sim`).

## Under the hood

- **`KeyDatabase` persistence fix (P0, on-device-only).** `encrypt_with` minted
  per-entity keys into an in-memory `KeyDatabase` but never `save()`d them, so
  rows written in a session became permanently unreadable after a force-stop —
  invisible to tests that never exercised close-and-reopen. `EncryptedGraphDB`
  now holds an `Arc<DeviceKey>` and persists the key DB on each mint, with two
  regression tests (`mint_persists_key_db_to_disk`,
  `restart_simulation_recovers_encrypted_thread`) locking in the contract.
  Trade-off: a synchronous `fs::write` per mint (bounded; batching on a debounce
  is a future optimisation).

- **New `GraphDB` trait surface is purely additive.** Ten new methods
  (`set_*_encryption`, `search_*_by_token_hashes`) were added — no existing
  signatures changed — so `main`'s Jiminy code holding `Arc<dyn GraphDB>`
  compiles unchanged against the encrypted layer.

- **Isolated vision venv.** `jiminy-vision` keeps its own dependency set (pins
  `transformers` 4.x — 5.x dropped `SmolVLMImageProcessor` — and `torchvision`)
  so the heavy VLM stack never disturbs the `reachy_mini` sidecar.
  Verified on a GTX 1660 (caption fits ~5.6 GB VRAM).

- **Endpoints / config.** `JIMINY_URL` (default `http://127.0.0.1:9100`) and
  `JIMINY_VISION_URL` (default `http://127.0.0.1:9101`); vision-window duration
  is a `sovereign-core` config field surfaced in Settings.

## Tests

- **Frontend (vitest):** 249 passed across 13 files (adds `vision`/`voice`
  store suites).
- **sovereign-crypto:** 128 · **sovereign-db:** 105 (green under both
  `encryption,surrealkv` and `encryption,rocksdb`) · **sovereign-skills:** 167 ·
  **sovereign-p2p:** 49 (incl. `e2e_sync`) · **sovereign-core:** 46 ·
  **sovereign-app:** 38 + `cli_integration` · **sovereign-comms:** 11.
  Workspace total (excluding CUDA-gated `sovereign-ai`): **546 passed, 0 failed.**
- **Jiminy:** `jiminy-bridge` and `jiminy-vision` suites pass; full stack
  validated live on the Reachy Mini.
- `sovereign-ai` (default CUDA feature) is validated separately; it is untouched
  by the encryption work.

## Known limitations / notes

- **Per-mint `fs::write`.** A burst of inbound encrypted messages serialises
  their per-message-key persists; bounded today, debounced batching is planned.
- **STT stays out-of-process.** The in-process voice pipeline remains disabled
  on Jiminy builds due to the `ggml` double-embed clash; LLM-on-CPU replies take
  ~1–2 min, so a **CUDA build is the latency fix** for live robot conversation.
- **Robot mic.** Capture uses the robot camera mic; the AEC speakerphone
  endpoint returns silence via PortAudio.
- **Android key storage.** Keys still live in the app data dir (`crypto/`);
  Android Keystore integration is not yet wired.
- Carried over from v0.0.5: `comms-signal` is still not in mobile builds
  (presage `!Send` stream); document branch-ancestry (`is_ancestor`) remains a
  conservative stub.

## Build / packaging

- **Version bump pending.** Workspace `version` is still `0.0.5` — bump to
  `0.0.6` before tagging.
- **Desktop (Windows):** build on a **warm cargo target** — Smart App Control
  enforce-mode blocks freshly compiled, unsigned build-scripts (`os error
  4551`), so a clean/wiped target fails non-deterministically. Never
  `cargo clean` on the Windows host; incremental builds reuse cached
  build-scripts and pass. CUDA 13 runtime DLLs (`cudart64_13.dll`, …) still come
  from `%CUDA_PATH%\bin\x64` (not `\bin`).
- **Jiminy is feature-gated and off by default.** Enable with
  `--features jiminy` (robot bridge), `vision` (camera + scene), `wake-word`.
  Each sidecar runs separately: `jiminy-bridge` (`:9100`, needs the
  `reachy_mini` SDK + Piper + faster-whisper) and `jiminy-vision` (`:9101`, own
  venv with MediaPipe + SmolVLM2). Use `--sim` for the MuJoCo simulator.
- **Android (WSL2/Linux):** unchanged build path from v0.0.5; now ships the
  SurrealKV persistent backend (`mobile` feature pulls `surrealkv`).
