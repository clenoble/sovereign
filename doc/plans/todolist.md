# Sovereign GE — Todo List

## Resolved Issues

<details>
<summary>Click to expand (3 resolved)</summary>

### 1. ~~Voice pipeline crashes on Windows (ggml symbol conflict)~~ — RESOLVED

**Status:** Fixed — whisper gated behind `voice-stt` feature flag (off by default)
**Resolution:** Option 5 implemented. `whisper-rs`, `cpal`, and `ringbuf` are now optional dependencies in `sovereign-ai`, gated behind the `voice-stt` feature. Default builds exclude whisper entirely, eliminating the ggml symbol collision. Enable with `--features voice-stt` on Linux/WSL where it works.

### 2. ~~NAS target directory intermittent write failures~~ — MITIGATED

**Status:** Mitigated — all batch scripts now kill stale processes before building
**Resolution:** `_build.bat`, `_check.bat`, and `_run.bat` now kill orphaned `cargo.exe`/`rustc.exe` processes and clean stale sovereign artifacts before invoking cargo. This addresses the most common cause (stale SMB file locks). Windows Defender exclusion is still recommended for further reliability.

### 3. ~~C: drive nearly full (3.5 GB free)~~ — RESOLVED

**Status:** Resolved — C: drive freed up, local builds now use `C:/cargo-target`
**Resolution:** Disk cleaned up. Debug builds work locally (~15 min). NAS (`Z:\cargo-target`) available as fallback.

</details>

---

## Post-MVP Roadmap

### Priority 1 — UX Principle Enforcement (spec gaps in working code)

These are implemented in code but not yet wired into the user-facing flow. High ROI, mostly small changes.

| # | Feature | Principle | Effort | GitHub | Status |
|---|---------|-----------|--------|--------|--------|
| 1 | **Wire injection scanner** — call `injection::scan()` on tool results before LLM sees them; surface warnings in chat | Injection Surfacing (P7) | Small | [#1](https://github.com/clenoble/sovereign/issues/1) | `good first issue` |
| 2 | **Provenance styling on chat bubbles** — visual distinction for owned vs external content (border color, icon) | Sovereignty Halo (P3) | Small | [#3](https://github.com/clenoble/sovereign/issues/3) | `good first issue` |
| 3 | **Reasoning model lifecycle tracing** — log load/unload timing, idle timer resets | Observability | Small | [#4](https://github.com/clenoble/sovereign/issues/4) | `good first issue` |
| 4 | **Trust dashboard read-only view** — Settings panel showing per-workflow trust levels | Trust Calibration (P5) | Medium | — | Needs review |
| 5 | **Conversational confirmation flow** — AI proposes → user confirms ("yes"/"no") → execute or cancel | Conversational Confirm (P2) | Medium | [#5](https://github.com/clenoble/sovereign/issues/5) | Needs design |
| 6 | **Plan visibility UI** — structured plan widget for multi-step tasks (checkboxes, reorder, cancel) | Plan Visibility (P4) | Medium | — | Needs design |

### Priority 2 — Architecture Decisions Required

These require design choices before implementation. Best done together.

| # | Feature | Key Decision | Affected Crates | Effort |
|---|---------|-------------|-----------------|--------|
| 7 | **Rich document format (WYSIWYG)** | Markdown-only vs full rich text? Iced text_editor vs embedded webview (milkdown/ProseMirror)? How to store: raw markdown, HTML, or custom AST? | sovereign-skills, sovereign-ui, sovereign-db | Large |
| 8 | **Progressive canvas density** | How to cluster semantically? Transition thresholds (card count per viewport)? Heatmap rendering (shader-based vs CPU rasterize)? | sovereign-canvas | Large |
| 9 | **Skill sandbox / confinement** | Landlock (Linux) vs AppContainer (Windows) vs WASM? IPC protocol (JSON-RPC over Unix socket vs shared memory)? How to handle skill crashes? | sovereign-skills, sovereign-core | Large |
| 10 | **P2P CRDT-based conflict resolution** | Which CRDT library (yrs/automerge-rs)? Per-document or per-field CRDTs? How to merge thread structure? Conflict UI for non-mergeable changes? | sovereign-p2p, sovereign-db | Large |
| 11 | **Guardian recovery UI flow** | Full-screen wizard vs panel? How to discover guardians (QR, libp2p, manual ID)? Progress feedback during 3-of-5 shard collection? Timeout/retry UX? | sovereign-ui, sovereign-p2p, sovereign-crypto | Large |
| 12 | ~~**Session log encryption**~~ | ~~Encrypt per-entry or whole file?~~ **DONE** — Per-entry XChaCha20-Poly1305 encryption with SHA-256 hash chain for tamper detection. Feature: `encrypted-log` (on by default). | sovereign-ai, sovereign-crypto | ~~Medium~~ |
| 13 | **Wake word + streaming VAD** | Always-on audio capture: battery/CPU impact? Wake word engine (rustpotter vs custom)? How to handle false positives? Privacy indicator in UI? | sovereign-ai (voice/), sovereign-ui | Large |

### Priority 3 — New Features (well-scoped, no major design needed)

| # | Feature | Description | Affected Crates | Effort |
|---|---------|-------------|-----------------|--------|
| 14 | **Soft-delete for documents** — `deleted_at` field, filter in queries, trash view | [#2](https://github.com/clenoble/sovereign/issues/2) | sovereign-db | Small |
| 15 | **30-day purge job** — background task to permanently delete items past retention window | sovereign-db, sovereign-app | Small |
| 16 | **Video playback skill** — embedded player, video document type, thumbnail on canvas | sovereign-skills, sovereign-ui | Medium |
| 17 | **File import pipeline** — .md/.txt direct, .pdf via pdf-extract, .docx via pandoc, .html via readability | sovereign-skills, sovereign-db | Medium |
| 18 | **Semantic search via embeddings** — GGUF embedding model, vector index in SurrealDB, search overlay integration | sovereign-ai, sovereign-db | Medium |
| 19 | **Minimap toggle/hover-reveal** — minimap hidden by default, show on hover or keyboard shortcut | sovereign-canvas | Small |
| 20 | **Timebox instant-jump** — timeline navigation widget (jump to year/month/week) | sovereign-canvas, sovereign-ui | Medium |
| 21 | **WhatsApp channel** — replace stub with whatsapp-web bridge or Business API integration | sovereign-comms | Large |
| 22 | **Hardware-contextual skill suggestions** — detect connected hardware, suggest relevant skills | sovereign-skills, sovereign-core | Medium |

### Priority 4 — Future / Exploratory

| # | Feature | Notes |
|---|---------|-------|
| 23 | **Cognitive sovereignty features** | Entropy metric, blind spot detection, modal clarity — from archived `sovereign_os_model_user_system_instructions.md`. Revisit when core UX is stable. |
| 24 | **Federation / multi-user** | Cross-user document sharing with access control. Depends on P2P maturity. |
| 25 | **Plugin marketplace** | Community skill registry with PGP signatures. Depends on skill sandbox (#9). |
| 26 | **Mobile companion** | Read-only or limited mobile client. Depends on P2P sync maturity. |
| 27 | **Collaborative editing** | Real-time multi-cursor editing. Depends on CRDTs (#10) and P2P. |
| 28 | **Identity Firewall** | Tracking prevention layer (cookies, fingerprints). Deferred — not core to document workflow. |
| 29 | **Image generation skill** | On-device Stable Diffusion / FLUX. Saves ~3-4GB VRAM for router+reasoning. |

---

## Items to Implement Together (Design Sessions)

These are the features where we should discuss architecture before coding. Ranked by impact and dependency order:

1. **Conversational confirmation flow (#5)** — Enables the entire Level 2+ action system. Without this, the action gate is enforced but the UX is broken (user can't approve proposed actions inline). Unlocks: all write actions, plan visibility, trust escalation.

2. **Rich document format (#7)** — The document panel currently handles plain text only. This is the most visible gap for end users. Decision: markdown-first (simpler, fits existing stack) vs rich text (more ambitious, needs editor widget choice).

3. **Progressive canvas density (#8)** — Core to the spatial UX promise. The spec describes cards transitioning to heatmap blobs at zoom-out. Decision: rendering approach (shader LOD vs CPU), clustering algorithm, transition breakpoints.

4. ~~**Session log encryption (#12)**~~ — **DONE**. Per-entry XChaCha20-Poly1305 with SHA-256 hash chain.

5. **Skill sandbox (#9)** — Prerequisite for community skills. No urgency until third-party skills exist, but architecture should be decided early to avoid retrofitting.

---

## Completed

<details>
<summary>Click to expand (29 completed items)</summary>

- [x] Fix cli_integration test on Windows (TOML backslash escaping) — commit 617ffc5
- [x] Fix ggml flash-attention crash — commit 85fdf05
- [x] Cross-platform Windows build support — commit c0a3579
- [x] Documentation alignment (README, spec, CLAUDE.md) — commit 617ffc5
- [x] Phase A-D code review and robustness improvements — commit 617ffc5
- [x] Gate voice/whisper behind `voice-stt` feature flag (ggml symbol conflict fix)
- [x] Document links on canvas — relationship edges as colored curved arrows
- [x] NAS pre-build cleanup in batch scripts (kill stale processes, clean artifacts)
- [x] Version tracking in FloatingPanel — commit history list with timestamp, message, and snapshot preview
- [x] Communications: seed contacts, messaging data, intent routing, pinned contacts in taskbar, contact panel
- [x] Light theme: ThemeMode enum, palette functions, taskbar toggle button
- [x] Onboarding flow: 4-step wizard, first-launch detection, marker file
- [x] Model management GUI: GGUF model list, role assignment, delete, taskbar button
- [x] Unified input path: search bar and chat panel both go through classify → gate → dispatch
- [x] Model-agnostic inference: hot-swap between Qwen, Mistral, Llama3 at runtime; fuzzy model resolution with alias expansion
- [x] Global LlamaBackend (`OnceLock`): fix concurrent model init crash, support router + reasoning coexistence
- [x] Multi-format prompts: ChatML, Mistral, Llama3 via `PromptFormatter` trait; automatic format detection on model swap
- [x] Phase 5 thread CRUD: create/rename/delete threads, move documents via AI orchestrator
- [x] Per-document version tracking: commit chains, auto-commit engine, restore from history
- [x] Session log: append-only JSONL at `~/.sovereign/orchestrator/session_log.jsonl`
- [x] Soft-delete: documents and threads use `deleted_at` field (purge deferred)
- [x] Write tools: `create_document`, `create_thread`, `rename_thread`, `move_document` (Level 3, require confirmation)
- [x] Rich chat agent loop: multi-turn with 10 tools, workspace context, few-shot examples
- [x] Trust tracking: per-workflow approval history with persistent JSON storage
- [x] Milestones: create/list/delete milestones on threads
- [x] Docs update: CONTRIBUTING.md, archive superseded specs, update implementation plan — commit ab6b6ab
- [x] Skill sandbox Phase 1: typed Capability enum, SkillContext, SkillDbAccess trait, execute_skill() gating — commit 3e5d903
- [x] Skill sandbox Phase 2: WASM plugin runtime with wasmtime Component Model, WIT contracts, word-count-wasm example — commit 1606097
- [x] Session log encryption: per-entry XChaCha20-Poly1305 with SHA-256 hash chain for tamper detection (`encrypted-log` feature, on by default)

</details>
