# Sovereign OS — Todo List

## Open Issues

### 1. ~~Voice pipeline crashes on Windows (ggml symbol conflict)~~ — RESOLVED

**Status:** Fixed — whisper gated behind `voice-stt` feature flag (off by default)
**Resolution:** Option 5 implemented. `whisper-rs`, `cpal`, and `ringbuf` are now optional dependencies in `sovereign-ai`, gated behind the `voice-stt` feature. Default builds exclude whisper entirely, eliminating the ggml symbol collision. Enable with `--features voice-stt` on Linux/WSL where it works.

---

### 2. ~~NAS target directory intermittent write failures~~ — MITIGATED

**Status:** Mitigated — all batch scripts now kill stale processes before building
**Resolution:** `_build.bat`, `_check.bat`, and `_run.bat` now kill orphaned `cargo.exe`/`rustc.exe` processes and clean stale sovereign artifacts before invoking cargo. This addresses the most common cause (stale SMB file locks). Windows Defender exclusion is still recommended for further reliability.

---

### 3. C: drive nearly full (3.5 GB free)

**Status:** Ongoing constraint
**Severity:** Medium — prevents local debug builds

**Problem:**
Debug build artifacts require ~17 GB. The C: drive only has ~3.5 GB free, forcing all builds to the NAS (`Z:\cargo-target`), which is slower (~16 min) and prone to intermittent write failures (see issue #2).

**Suggested solutions:**
1. Use `--release` builds which produce smaller artifacts
2. Clean up C: drive (temp files, old build artifacts, Windows Update cache)
3. Move the project to a larger drive
4. Use `cargo clean` regularly on local target directories

---

## Feature Roadmap

### Canvas & Documents

- [x] **Document links on canvas** — Visualize relationships between documents directly on the canvas (lines/arrows between cards based on `related_to` edges)
- [ ] **Version tracking on canvas** — Show commit history per document card (expand to see snapshots, diff between versions)

### Communications

- [ ] **Seed contacts & messaging data** — Extend `seed.rs` to populate sample contacts, conversations, and messages on first launch (similar to existing thread/document seeding)
- [ ] **Include calls and emails in intent threads** — Route incoming emails and call logs into the thread/document model so the orchestrator can reason about them alongside documents
- [ ] **Pinned contact in taskbar** — Add a pinned-contacts section to the taskbar for quick access to frequently contacted people
- [ ] **Contact panel** — New side panel (like document/chat/search panels) for viewing contact details, conversation history, and linked documents

### Advanced Features

- [ ] **Rich document format** — Support formatted text (headings, bold, lists, embedded images) beyond plain-text content, with a WYSIWYG or markdown editor
- [ ] **Video** — Video playback and annotation support (embedded player, video document type, thumbnail previews on canvas)
- [ ] **Light theme** — Add a light theme option alongside the current dark theme, with a toggle in settings
- [ ] **Onboarding flow** — First-launch guided experience: passphrase setup, device naming, theme selection, sample data walkthrough
- [ ] **Model management GUI** — Settings panel for AI models: list installed models with size/format/role, assign default model per feature (router, reasoning, STT, TTS), download new GGUF models from Hugging Face (search, pick quantization, progress bar), delete unused models to free disk space

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
