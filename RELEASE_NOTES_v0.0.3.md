# v0.0.3 — Skills system, embedded browser, full-feature CUDA build

This release ships the skill registry, an embedded browser with on-device reliability assessment, Qwen 3.5 support, AI-suggested document links, and a packaged release-build path that actually offloads to the GPU. The Iced-based UI has been retired — Svelte 5 + Tauri is now the sole frontend.

## Highlights

- **Embedded browser with reliability assessment.** Open web pages inside the canvas; local Qwen models classify content (Factual / Opinion / Fiction) and score it on a 0–5 rubric. No network egress beyond the URL the user opens.
- **Skills system, four phases.** A registry plus ~30 built-in skills covering read-only, read+write, cross-document, and community-spec-as-seed skills (markdown editor, PDF/HTML/plaintext export, find-replace, outline extractor, link checker, PII detector, redactor, table of contents, JSON/YAML formatter, CSV → markdown, sort lists, case converter, backlink map, orphan finder, daily journal, thread summary, plus 20 community spec seeds). Skills can call the LLM and the graph DB.
- **Qwen 3.5 support.** Router escalates to a 7B reasoning model on low confidence; thinking-mode suppression strips `<think>` blocks defensively; per-model sampling profiles (temperature, top-k, top-p, presence penalty) so each family runs at its preferred settings.
- **Memory consolidation.** Background process finds semantic links between documents during idle (60s cooldown, 30s poll). Candidate pairs scored by the 3B router; suggestions ≥ 0.4 surface in the UI as accept/dismiss.
- **Iced UI retired.** `sovereign-ui` and `sovereign-canvas` crates are gone. The Svelte 5 + SvelteKit 2 + Tauri 2.10 frontend is now the only supported UI. Stack: Svelte 5.51, SvelteKit 2.50, Vite 7.3.

## UI / canvas

- Viewport-based lazy loading on the timeline canvas — only mounts DOM cards for visible documents.
- Timeline zoom fix: tick label font size stays fixed at 10px screen size regardless of zoom; new sub-daily intervals (10-minute / hourly / 6-hourly) when zoomed in; viewport-clipped tick rendering.
- Tauri 2.10 with devtools enabled in debug builds.

## Database

- `list_relationships` split into `list_outgoing_relationships` / `list_incoming_relationships` with proper graph traversal up to N hops.
- `SuggestedLink` schema for AI-proposed edges (separate from user-confirmed `RelatedTo`).
- `update_document_reliability` for storing the browser's classification + score on a doc.
- Fix: SurrealDB RELATE binds `Thing` records, not plain strings.
- `EncryptedGraphDB` decorator now matches the full `GraphDB` trait (8 methods that had drifted are restored).

## AI orchestrator

- Per-model sampling profiles (`SamplingConfig`) — Qwen 3.5 uses aggressive sampling (temp=1.0, top_p=0.95, presence_penalty=1.5).
- `ChatMLQwen3` prompt format with `/no_think` injection.
- Fuzzy model resolution + alias expansion (e.g. "mistral" finds "Ministral-3B-...").
- Format auto-detection from GGUF filename.

## P2P

- Avoid duplicate multiaddr parse in `node.listen()`.

## Build / packaging

- New `_release_build.bat` helper that sets the full env (CUDA paths, MSBuild integration vars `CUDA_PATH_V13_2` and `CudaToolkitDir`, `SOVEREIGN_TARGET_DIR`) and builds with `--features cuda,encryption,p2p,comms-email,web-browse`.
- CUDA 13 runtime DLL gotcha documented in `CLAUDE.md`: `cudart64_13.dll`, `cublas64_13.dll`, `cublasLt64_13.dll` live in `<CUDA_PATH>\bin\x64\`, not `\bin\` like CUDA 12. The CUDA 13 installer adds `\bin` to system PATH but **not** `\bin\x64`.
- Default `n_gpu_layers` raised from `0` → `99`. Previously a CUDA-feature build still ran on CPU because the config didn't offload any layers; this is now corrected.
- Repo audit: scrubbed hardcoded NAS paths and personal directories from committed files. Per-machine setup moved to gitignored `CLAUDE.local.md`.

## Quality

- Code review pass: 14 high/medium-priority fixes plus a performance pass (#18–23 in the issue tracker).
- Frontend testing: Vitest foundation + Tauri IPC mock layer (`mockTauriCommand`); first store test suites for `canvas`, `chat`, `browser`, `app`; first component test (`Bubble`).
- Docs: PII management & dashboard plan, skills implementation roadmap, third-party skill developer guide.

## Running v0.0.3

The release binary is compiled with `cuda,encryption,p2p,comms-email,web-browse`. To run it:

1. Install **NVIDIA driver** (CUDA toolkit not required if you ship the runtime DLLs alongside).
2. Either copy `cudart64_13.dll`, `cublas64_13.dll`, `cublasLt64_13.dll` from `<CUDA install>\bin\x64\` next to `sovereign.exe`, or add that path to `PATH`.
3. Place GGUF model files in the configured `model_dir` (default `models/` relative to the binary). Defaults expect `qwen2.5-3b-instruct-q4_k_m.gguf` (router) and `qwen2.5-7b-instruct-q4_k_m-{00001,00002}-of-00002.gguf` (reasoning), downloadable from `Qwen/Qwen2.5-{3B,7B}-Instruct-GGUF` on Hugging Face.

## Not in this release

- **PII management dashboard.** Active on the `pii-management-dashboard` branch — targeted for v0.0.4. Includes signup capture, vault, autofill, cookies tab, share ledger, and dashboard panel.
- **`comms-signal` feature.** Deferred from the release build due to a known build issue; track in the build script comment.
- **Voice pipeline.** Experimental — wake-word + Whisper STT + Piper TTS is wired but not enabled in default builds.

## Upgrading from v0.0.2

- The `EncryptedGraphDB` trait additions are backward-compatible at the data layer; no migration needed.
- If you maintained a local `config/default.toml` override, your `n_gpu_layers` value is preserved. New installs default to 99.
- The Iced UI is removed. If you were using `sovereign-ui` or `sovereign-canvas` crates, switch to the Svelte 5 frontend.
