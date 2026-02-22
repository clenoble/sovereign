# Sovereign OS — Implementation Plan

**Date:** February 6, 2026
**Updated:** February 8, 2026
**Target:** MVP Alpha in ~4-6 months
**Team:** You (testing, integration, hardware debugging, design decisions) + Claude (code generation, architecture, documentation)

---

## Development Model

**How this works in practice:**

1. Each phase below has concrete deliverables.
2. For each deliverable, you open a conversation (or use Claude Code / agent teams) and I generate the code.
3. You test on real hardware, report issues, I iterate.
4. We move to the next deliverable when the current one passes your acceptance criteria.

**Bottleneck is not code generation — it's your testing/integration time.** Plan accordingly: batch code generation sessions, then spend days testing and feeding back.

---

## Phase 0: Validation Spikes (Weeks 1-2)

**Goal:** Validate the three highest-risk integrations before committing to the full build.

### Spike 1: Skia ↔ GTK4 Canvas — ✅ PASSED

| Item | Detail |
|------|--------|
| **Deliverable** | Minimal GTK4 window with embedded Skia surface rendering rectangles, handling zoom/pan via mouse events |
| **Accept criteria** | Smooth 60fps pan/zoom on your hardware. No tearing, no input lag. |
| **Result** | GPU-accelerated rendering via GLArea + Skia DirectContext. Pan/zoom/hit-testing all functional. |

### Spike 2: SurrealDB Embedded Benchmark — ✅ PASSED

| Item | Detail |
|------|--------|
| **Deliverable** | Rust program that creates 50K document nodes with relationships in embedded SurrealDB, runs benchmark queries |
| **Accept criteria** | Single document fetch < 5ms. Graph traversal (2 hops, 10 results) < 50ms. Bulk insert 50K docs < 30s. Search by title < 100ms. Thread query < 50ms. |
| **Result** | All 5 benchmarks pass after adding `idx_thread_id` index. |

**Benchmark results (in-memory, SurrealDB 2.6):**

| Benchmark | Result | Target |
|-----------|--------|--------|
| Single document fetch | 0.10 ms | < 5 ms |
| Graph traversal (2 hops) | 0.46 ms | < 50 ms |
| Bulk insert 50K docs | 9.20 s | < 30 s |
| Search by title | 3.96 ms | < 100 ms |
| Thread documents query | 0.12 ms | < 50 ms |

**Lesson learned:** SurrealDB requires explicit indexes for field-based queries. The thread query was 688ms without an index, 0.12ms with one. All query-filtered fields must be indexed in the production schema.

### Spike 3: llama.cpp Direct FFI + Model Loading — 🔲 NEXT

| Item | Detail |
|------|--------|
| **Deliverable** | Rust binary that loads a quantized Qwen2.5-3B GGUF via `llama-cpp-2` crate, sends a prompt, gets a response, unloads the model |
| **Accept criteria** | Model loads in < 10s. Inference latency < 500ms for simple classification. Memory returns to baseline after unload (±100MB). |
| **Fallback** | PyO3 + llama-cpp-python (adds Python runtime dependency). |
| **Claude generates** | Rust `llama-cpp-2` integration code, `ModelBackend` trait, memory monitoring, benchmark harness |
| **You test** | Run on your GPU, measure latency and memory |

**Design decision:** Direct llama.cpp FFI via Rust instead of PyO3. This eliminates the Python runtime dependency for core inference, removes one FFI boundary, and simplifies deployment. PyO3 can be added later as a second `ModelBackend` implementation if specialist Python-only models are needed.

**Decision gate:** After spike 3, review all results. Adjust tech choices if any spike fails. Then commit to full build.

---

## Phase 1a: Data Layer (Weeks 3-4)

**Goal:** Rust core runtime + SurrealDB abstraction with CLI harness. Test data layer independently before building UI.

### 1a.1 Project Structure & Build System

```
sovereign-os/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── sovereign-core/         # Core runtime, lifecycle, config
│   ├── sovereign-db/           # SurrealDB abstraction layer
│   ├── sovereign-skills/       # Skill registry, traits, lifecycle
│   ├── sovereign-canvas/       # Skia canvas widget
│   ├── sovereign-ui/           # GTK4 shell (taskbar, windows, search)
│   ├── sovereign-ai/           # llama.cpp bridge + orchestrator interface
│   └── sovereign-app/          # Main binary, ties everything together
├── models/                     # GGUF model files (gitignored)
├── skills/
│   ├── markdown-editor/
│   ├── image-viewer/
│   └── pdf-export/
├── tests/
├── docs/
└── config/
```

**Note:** No `python/` directory in MVP. AI inference runs entirely through llama.cpp FFI. Python integration (PyO3) is a post-MVP option if specialist models require it.

**Claude generates:** Full project scaffold, Cargo workspace config, CI pipeline (GitHub Actions), development Nix flake or Dockerfile for reproducible builds.

### 1a.2 SurrealDB Abstraction Layer (`sovereign-db`)

| Deliverable | Detail |
|-------------|--------|
| Schema definition | Document nodes, relationships, threads, version history |
| Index strategy | Indexes on all query-filtered fields (thread_id, doc_type, title, created_at) |
| CRUD operations | Create/read/update/delete documents |
| Graph queries | Traverse relationships (outbound, inbound, N-hop) |
| Batch operations | Bulk relationship creation in transactions (not individual RELATE queries) |
| Version control | Commit, branch, diff, merge |
| Abstraction trait | `GraphDB` trait that could be backed by SurrealDB or SQLite |

### 1a.3 Core Runtime (`sovereign-core`)

| Deliverable | Detail |
|-------------|--------|
| Config system | Load `config.toml`, validate settings |
| Lifecycle manager | Startup, shutdown, signal handling |
| CLI harness | Command-line interface for CRUD operations (test data layer without UI) |

**Phase 1a acceptance criteria:** `sovereign-db` CRUD and graph queries work via CLI. Batch relationship insert performs within 2x of single-insert benchmarks. Schema includes all indexes. Version control (commit/branch) works.

---

## Phase 1b: UI Shell (Weeks 5-6)

**Goal:** GTK4 shell with static mock data. Skill manifest parsing. No data binding yet.

### 1b.1 Skill Registry (`sovereign-skills`)

| Deliverable | Detail |
|-------------|--------|
| Skill manifest parser | Parse `skill.json`, validate capabilities |
| Skill lifecycle | Install, load, unload, health check |
| Core skill trait | `CoreSkill` trait for in-process skills (direct Rust calls, no IPC overhead) |
| Community skill IPC | Unix socket server, JSON-RPC request/response (for sandboxed community/sideloaded skills only) |

**Design decision:** Core skills (markdown-editor, image-viewer, pdf-export) use direct Rust trait calls — no IPC serialization overhead, no Unix socket round-trip. IPC is reserved for community and sideloaded skills where the sandbox boundary requires process isolation.

### 1b.2 GTK4 Shell (`sovereign-ui`)

| Deliverable | Detail |
|-------------|--------|
| Main window | Application window with layout regions |
| Taskbar widget | Bottom-anchored bar with pinned items, recent context, action buttons |
| Document window manager | Free-floating, draggable, resizable windows with sovereignty badges |
| Search overlay | Blurred overlay with search field, filters, results list |
| Theme | Dark theme matching wireframes (CSS variables for owned/external colors) |

**Phase 1b acceptance criteria:** Application launches, shows GTK4 shell with taskbar and mock documents. Skill manifests parse correctly. Core skill trait compiles and can be called in-process. No real data binding yet — just the visual skeleton.

---

## Week 7 Checkpoint: Cross-Phase Interface Definitions

**Goal:** Before Phase 2 and 3 run in parallel, define the shared interfaces they'll both build against. This prevents integration pain in Phase 5.

| Interface | Definition |
|-----------|------------|
| `CanvasController` trait | `navigate_to_document(doc_id)`, `highlight_card(doc_id)`, `zoom_to_thread(thread_id)`, `get_viewport()` |
| `OrchestratorEvent` enum | Events the AI sends to the UI: `DocumentOpened`, `SearchResults`, `ActionProposed`, `ActionExecuted` |
| `UserIntent` struct | Parsed intent from the AI: action type, target document/thread, confidence, entities |
| `ModelBackend` trait | `classify_intent()`, `generate()`, `embed()` — already designed in spike 3 |

Both Phase 2 and Phase 3 build to these interfaces. Integration in Phase 5 becomes wiring, not redesign.

---

## Phase 2: Canvas & Visual System (Weeks 7-10)

**Goal:** The spatial map is the core UX. Build the 2D infinite canvas with all navigation and visual elements.

### 2.1 Skia Canvas Widget (`sovereign-canvas`)

| Deliverable | Detail |
|-------------|--------|
| Custom GTK4 widget | Embeds Skia rendering surface (proven in spike 1) |
| Coordinate system | World coordinates (infinite 2D) ↔ screen coordinates (viewport) |
| Camera | Pan (drag), zoom (scroll), animate transitions |
| Hit testing | Click on card → identify which document node |
| `CanvasController` impl | Implement the trait defined at week 7 checkpoint |

### 2.2 Canvas Rendering

| Element | Rendering |
|---------|-----------|
| Owned document cards | Rounded rectangles, owned-color fill/border, title + type label |
| External document cards | Parallelograms (skewed), external-color fill/border |
| Density blobs | Radial gradient circles at high zoom-out |
| Timeline ruler | Horizontal axis with month/year ticks, NOW marker |
| Milestone markers | Vertical lines with labels |
| Soft links | Dashed lines between related thread rows |
| Thread labels | Left-aligned labels per thread row |
| Minimap | Small overview rectangle with viewport indicator |

### 2.3 Progressive Density

| Zoom Level | Cards Visible | Rendering |
|------------|--------------|-----------|
| < 50 cards in viewport | All | Full card preview with labels |
| 50-200 cards | Some | Smaller cards, truncated labels |
| > 200 cards | None | Heat-map blobs only |

### 2.4 Canvas ↔ Data Binding

- Canvas reads document positions from SurrealDB (spatial_position per thread)
- AI auto-positions new documents (semantic clustering on X, chronological on Y)
- User can drag to override positions
- Position changes saved back to DB

### 2.5 Canvas Performance Benchmarks

| Benchmark | Target | Description |
|-----------|--------|-------------|
| 50K cards at full zoom-out | ≥ 30 fps | Density blob rendering mode |
| 200 cards in viewport | ≥ 60 fps | Mixed card + truncated label mode |
| 50 cards at full zoom-in | ≥ 60 fps | Full card preview with labels |
| Pan/zoom latency | < 16ms | No perceptible lag on input |

**Phase 2 acceptance criteria:** Canvas renders real documents from SurrealDB. Zoom/pan is smooth. Cards show sovereignty distinction. Progressive density works. Minimap works. Performance benchmarks pass.

---

## Phase 3: AI Orchestrator (Weeks 8-12, parallel with Phase 2)

**Goal:** Local AI that classifies intent, routes to skills, and converses with the user.

### 3.1 llama.cpp Bridge (`sovereign-ai`)

| Deliverable | Detail |
|-------------|--------|
| `ModelBackend` trait impl | llama.cpp backend via `llama-cpp-2` crate (proven in spike 3) |
| Model registry | Load/unload GGUF models, query status, swap models |
| Async inference | Non-blocking calls from UI thread → inference thread → callback |
| Memory watchdog | Monitor RSS, alert on leaks, enforce unload after inactivity timeout |
| Config loader | Read `models.toml`, instantiate correct backend + quantization level |

### 3.2 Intent Classifier (`sovereign-ai`)

| Deliverable | Detail |
|-------------|--------|
| Intent taxonomy | navigation, edit_command, search, voice_dictation, skill_invoke, clarification |
| Classification pipeline | Input → Qwen2.5-3B router → `UserIntent` + confidence + entities |
| Context manager | Active thread, active document, recent actions → context window |
| Disambiguation | If confidence < 0.7, generate clarification question via Qwen2.5-7B |

### 3.3 Voice Pipeline (`sovereign-ai`)

| Deliverable | Detail |
|-------------|--------|
| Audio capture | System mic → rolling buffer |
| Wake word detection | openWakeWord (or Porcupine) |
| STT | whisper.cpp large-v3-turbo (native C++, no Python) |
| TTS | Piper (standalone C++ binary, invoked as subprocess) |
| Mode detection | Command vs dictation based on context |

### 3.4 User Profile (`sovereign-ai`)

| Deliverable | Detail |
|-------------|--------|
| Profile schema | Interaction patterns, skill preferences, learned workflows |
| Feedback loop | Track suggestion acceptance rate, adjust thresholds |
| Persistence | JSON file, loaded on startup |

**Phase 3 acceptance criteria:** User can type or speak a command ("open my research notes"), orchestrator classifies intent via Qwen2.5-3B, routes to correct action (open document), responds via Piper TTS. Disambiguation works for ambiguous queries using Qwen2.5-7B.

---

## Phase 4: MVP Skills (Weeks 10-14)

**Goal:** Three working skills that demonstrate the Skill architecture end-to-end.

### 4.1 Markdown Editor (`skills/markdown-editor/`)

| Deliverable | Detail |
|-------------|--------|
| Editor widget | Text editing with markdown syntax highlighting |
| Live preview | Side-by-side or toggle rendered view |
| GraphDB integration | Read/write document content via `CoreSkill` trait (direct Rust calls) |
| Auto-commit | Commit changes per version control policy |
| Keyboard shortcuts | Standard editing shortcuts |

**Implementation options:**
- Wrap an existing Rust/C markdown editor (e.g., sourceview with markdown mode)
- Build minimal custom editor with GtkSourceView
- Use a webview-based editor (milkdown, codemirror) via WebKitGTK

### 4.2 Image Viewer (`skills/image-viewer/`)

| Deliverable | Detail |
|-------------|--------|
| Viewer widget | Display PNG, JPEG, SVG, WebP |
| Controls | Zoom, pan, fit-to-window, actual-size |
| Metadata | Show EXIF/file metadata panel |
| GraphDB integration | Read image file reference from document node via `CoreSkill` trait |

### 4.3 PDF Export (`skills/pdf-export/`)

| Deliverable | Detail |
|-------------|--------|
| Export single document | Markdown → PDF with styling |
| Export thread | Multiple documents → combined PDF with table of contents |
| Layout options | Page size, margins, font, header/footer |
| Engine | typst (Rust-native, fast) or weasyprint (Python, HTML→PDF) |

**Phase 4 acceptance criteria:** User can create a markdown document, edit it, view images, and export to PDF — all through the skill system with `CoreSkill` trait calls and GraphDB integration.

---

## Phase 5: Integration & Polish (Weeks 14-18)

**Goal:** Wire everything together. The full loop works end-to-end.

### 5.1 Full Loop Integration

- User speaks "create a new research note in Project Alpha"
- Orchestrator classifies intent → create document + assign to thread
- Document appears on canvas in correct thread (via `CanvasController`)
- Markdown editor opens in floating window
- User edits, auto-commits
- User says "export this as PDF"
- PDF export skill generates file
- Version history tracks all changes

### 5.2 File Import

| Format | Method |
|--------|--------|
| .md | Direct import (native) |
| .txt | Import as plain text document |
| .pdf | Extract text (pdftotext), import as document + original as media attachment |
| .docx | Extract via pandoc, import as markdown |
| .html | Extract via readability algorithm, import as markdown |
| .png/.jpg | Import as media node |

### 5.3 Sovereignty Halo Polish

- Ingest animation (parallelogram → rectangle morph)
- Provenance trail creation on import
- Visual indicators in document windows and canvas

### 5.4 Search & Semantic Index

- Full-text search via SurrealDB
- Semantic search via GGUF embedding model (loaded through `ModelBackend` trait)
- Search overlay shows results with sovereignty shapes
- Timebox jump filters by date range

### 5.5 Testing

| Type | Tool | Coverage Target |
|------|------|----------------|
| Rust unit tests | cargo test | Core: 80%+, DB: 70%+, Skills: 60%+ |
| Integration tests | Custom harness | Full loop: create → edit → export |
| Canvas benchmarks | Built-in profiler | All Phase 2.5 targets pass |
| Manual testing | You | UX, performance, edge cases |

### 5.6 Documentation

- User guide (basic usage, keyboard shortcuts)
- Skill developer guide (how to build a new skill)
- Architecture overview (for contributors)

**Phase 5 acceptance criteria:** Complete end-to-end flow works. A non-developer could install and use the basic system. No critical bugs.

---

## Post-MVP: Deferred Features

The following are fully specified but intentionally excluded from MVP to keep scope manageable:

| Feature | Specification Status | Why Deferred |
|---------|---------------------|--------------|
| **P2P Device Sync** | Fully specified (libp2p, QUIC, encrypted fragments) | Requires multi-device testing infrastructure. Single-device MVP is sufficient for validation. |
| **Guardian Social Recovery** | Fully specified (Shamir 3-of-5, 72-hour waiting period) | Depends on P2P layer. Complex UX requiring multiple test participants. |
| **Identity Firewall** | Fully specified (synthetic identities, kernel proxy) | Requires browser/network integration. Not needed for document-centric MVP. |
| **Image Generation** | Model candidates evaluated (FLUX.1-schnell, SDXL) | Not core to document workflow. Saves ~3-4GB VRAM for router + reasoning models. Add as optional skill post-MVP. |
| **PyO3 Python Bridge** | Architecture ready (`ModelBackend` trait) | llama.cpp direct FFI covers MVP inference needs. Add if specialist Python-only models are required. |
| **Community Skill Registry** | Fully specified (registry.sovereign.org, PGP signatures) | No third-party skills exist yet. Core skills use in-process trait calls. IPC + sandbox for community skills when the ecosystem grows. |

---

## Default Model Suite (MVP)

### Always-Running Models (loaded on boot)

| Model | Size | Purpose | Hardware | Runtime |
|-------|------|---------|----------|---------|
| **Qwen2.5-3B-Instruct** | 3B, ~2GB GGUF Q4 | Router: intent classification, quick generation | CPU, 2GB RAM | llama.cpp |
| **whisper.cpp large-v3-turbo** | ~1.5GB | Voice → text (real-time) | CPU/GPU | whisper.cpp (native C++) |
| **Piper TTS** | 10-50MB | Text → voice | CPU | Standalone binary |
| **sentence-transformers** | ~110MB GGUF | Semantic search embeddings | CPU/GPU | llama.cpp (BERT GGUF) |

### On-Demand Models (loaded when needed)

| Model | Size | When Loaded | Runtime |
|-------|------|-------------|---------|
| **Qwen2.5-7B-Instruct** | 7B, ~5GB GGUF Q4 | Complex multi-step tasks, disambiguation, reasoning | llama.cpp |

**Total baseline VRAM:** ~4-5GB (without reasoning model). ~9-10GB with reasoning model loaded.

**Unload policy:** On-demand models unload after 5 minutes of inactivity.

---

## Timeline Summary

```
Week  1-2   ██ Phase 0: Validation Spikes [Spike 1 ✅, Spike 2 ✅, Spike 3 next]
Week  3-4   ██ Phase 1a: Data Layer (sovereign-core + sovereign-db + CLI)
Week  5-6   ██ Phase 1b: UI Shell (sovereign-ui + sovereign-skills)
Week  7     █ Checkpoint: Cross-phase interface definitions
Week  7-10  ████ Phase 2: Canvas & Visual System
Week  8-12  █████ Phase 3: AI Orchestrator (parallel)
Week 10-14  █████ Phase 4: MVP Skills (parallel)
Week 14-18  █████ Phase 5: Integration & Polish
            ─────────────────────────────────
            ~18 weeks (~4.5 months)
```

**Buffer:** Add 2-4 weeks for unexpected issues, sick days, motivation dips. **Realistic: 5-6 months.**

---

## How to Use Claude / Agent Teams Effectively

### Per-Phase Workflow

1. **Start of phase:** Share the phase spec with me. I generate the full code scaffold.
2. **Implementation:** I write the code in large chunks. You review and test.
3. **Iteration:** You report test results, I fix issues. Tight feedback loops.
4. **End of phase:** We verify acceptance criteria together.

### When to Use Agent Teams

Agent teams (Claude Code or multi-agent setups) are most useful for:

- **Parallel code generation:** Generate multiple crates/modules simultaneously
- **Large refactors:** When a design decision changes mid-build, regenerate affected code across multiple files
- **Test generation:** Agents can write comprehensive test suites faster than interactive chat
- **Documentation:** Generate docs from code + specs in one pass

### Session Strategy

- **Architecture sessions:** Interactive chat (like now). Discuss tradeoffs, make decisions.
- **Code generation sessions:** Provide spec + context, I generate large code blocks or full files. Use file creation tools.
- **Debug sessions:** You paste error output, I diagnose and generate fixes.
- **Review sessions:** You share working code, I review for bugs, performance issues, or architectural drift.

---

## Next Step

**Spike 3: llama.cpp Direct FFI** — Load Qwen2.5-3B-Instruct GGUF from Rust via `llama-cpp-2` crate. Benchmark load time, inference latency, and memory reclamation on unload.
