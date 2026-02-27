# Sovereign GE — Todo List

## Open Issues

### 1. ~~Voice pipeline crashes on Windows (ggml symbol conflict)~~ — RESOLVED

**Status:** Fixed — whisper gated behind `voice-stt` feature flag (off by default)
**Resolution:** Option 5 implemented. `whisper-rs`, `cpal`, and `ringbuf` are now optional dependencies in `sovereign-ai`, gated behind the `voice-stt` feature. Default builds exclude whisper entirely, eliminating the ggml symbol collision. Enable with `--features voice-stt` on Linux/WSL where it works.

---

### 2. ~~NAS target directory intermittent write failures~~ — MITIGATED

**Status:** Mitigated — all batch scripts now kill stale processes before building
**Resolution:** `_build.bat`, `_check.bat`, and `_run.bat` now kill orphaned `cargo.exe`/`rustc.exe` processes and clean stale sovereign artifacts before invoking cargo. This addresses the most common cause (stale SMB file locks). Windows Defender exclusion is still recommended for further reliability.

---

### 3. ~~C: drive nearly full (3.5 GB free)~~ — RESOLVED

**Status:** Resolved — C: drive freed up, local builds now use `C:/cargo-target`
**Resolution:** Disk cleaned up. Debug builds work locally (~15 min). NAS (`Z:\cargo-target`) available as fallback.

---

## Feature Roadmap

### Canvas & Documents

- [x] **Document links on canvas** — Visualize relationships between documents directly on the canvas (lines/arrows between cards based on `related_to` edges)
- [x] **Version tracking on canvas** — History button in FloatingPanel toolbar toggles between editor and scrollable commit list with snapshot previews

### Communications

- [x] **Seed contacts & messaging data** — 5 contacts, 4 conversations (Email/Signal/WhatsApp/SMS), 15 messages with mixed read/unread status, 2 conversations linked to threads
- [x] **Include calls and emails in intent threads** — Conversations linked to threads via `linked_thread_id`, `list_contacts` and `view_messages` intents routed through orchestrator
- [x] **Pinned contact in taskbar** — Top 3 non-owned contacts auto-pinned with initial + name, click opens contact panel
- [x] **Contact panel** — Floating panel showing contact info, addresses, conversation list, and message history with back navigation

### Advanced Features

- [ ] **Rich document format** — Support formatted text (headings, bold, lists, embedded images) beyond plain-text content, with a WYSIWYG or markdown editor
- [ ] **Video** — Video playback and annotation support (embedded player, video document type, thumbnail previews on canvas)
- [x] **Light theme** — Dark/light toggle via atomic `ThemeMode` in theme module; all color constants replaced with palette functions (`pick(dark, light)`); taskbar toggle button switches instantly
- [x] **Onboarding flow** — 4-step wizard (Welcome, Device Name, Theme Select, Sample Data) with `~/.sovereign/onboarding_done` marker; full-screen overlay on first launch
- [x] **Model management GUI** — Settings panel listing installed GGUF models with size, role assignment (Router/Reasoning), refresh and delete; taskbar "Models" button; scans model_dir from config

---

## Completed

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

---

## Post-MVP — Open Issues

See [GitHub Issues](https://github.com/clenoble/sovereign/issues) for contributor-friendly tasks:

- [ ] [#1](https://github.com/clenoble/sovereign/issues/1) Wire injection scanner into orchestrator (`good first issue`)
- [ ] [#2](https://github.com/clenoble/sovereign/issues/2) Add soft-delete (`deleted_at`) for documents (`good first issue`)
- [ ] [#3](https://github.com/clenoble/sovereign/issues/3) Add provenance styling to chat bubbles (`good first issue`)
- [ ] [#4](https://github.com/clenoble/sovereign/issues/4) Add tracing for reasoning model load/unload lifecycle (`good first issue`)
- [ ] [#5](https://github.com/clenoble/sovereign/issues/5) Implement conversational confirmation flow
- [ ] Trust dashboard read-only view (Settings panel)
- [ ] Session log encryption (AES-256-GCM at rest)
- [ ] Progressive canvas density (cards → heatmap blobs at zoom-out)
- [ ] Rich document format (WYSIWYG / markdown editor)
- [ ] WhatsApp channel (currently stub)
- [ ] Skill sandbox / confinement (Landlock on Linux, AppContainer on Windows)
- [ ] Wake word detection (always-on VAD streaming)
- [ ] P2P CRDT-based conflict resolution
- [ ] Guardian recovery UI flow
