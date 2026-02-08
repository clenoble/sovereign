# Sovereign OS — Implementation Plan

**Date:** February 6, 2026  
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

### Spike 1: Skia ↔ GTK4 Canvas

| Item | Detail |
|------|--------|
| **Deliverable** | Minimal GTK4 window with embedded Skia surface rendering rectangles, handling zoom/pan via mouse events |
| **Accept criteria** | Smooth 60fps pan/zoom on your hardware. No tearing, no input lag. |
| **Fallback** | If Skia integration takes >5 days, pivot to Cairo-only prototype. |
| **Claude generates** | Rust project scaffold, gtk4-rs window, rust-skia rendering code, mouse event handling |
| **You test** | Build, run, evaluate rendering quality and performance |

### Spike 2: SurrealDB Embedded Benchmark

| Item | Detail |
|------|--------|
| **Deliverable** | Rust program that creates 50K document nodes with relationships in embedded SurrealDB, runs benchmark queries |
| **Accept criteria** | Single document fetch < 5ms. Graph traversal (2 hops, 10 results) < 50ms. Bulk insert 50K docs < 30s. |
| **Fallback** | SQLite + JSONB with application-level graph traversal. |
| **Claude generates** | Benchmark harness, schema creation, test data generator, query suite |
| **You test** | Run benchmarks, report numbers |

### Spike 3: PyO3 + Model Loading

| Item | Detail |
|------|--------|
| **Deliverable** | Rust binary that loads a quantized Qwen2.5-3B via PyO3, sends a prompt, gets a response, unloads the model |
| **Accept criteria** | Model loads in < 10s. Inference latency < 500ms for simple classification. Memory returns to baseline after unload (±100MB). |
| **Fallback** | Subprocess model (Python as separate process, communicate via Unix socket instead of PyO3). |
| **Claude generates** | PyO3 bridge code, Python model wrapper, memory monitoring script |
| **You test** | Run on your GPU, measure latency and memory |

**Decision gate:** After week 2, review spike results. Adjust tech choices if any spike fails. Then commit to full build.

---

## Phase 1: Core Foundation (Weeks 3-6)

**Goal:** Rust core runtime + SurrealDB + basic GTK4 shell. No canvas yet, no AI.

### 1.1 Project Structure & Build System

```
sovereign-os/
├── Cargo.toml                  # Workspace root
├── crates/
│   ├── sovereign-core/         # Core runtime, lifecycle, config
│   ├── sovereign-db/           # SurrealDB abstraction layer
│   ├── sovereign-skills/       # Skill registry, IPC, lifecycle
│   ├── sovereign-canvas/       # Skia canvas widget
│   ├── sovereign-ui/           # GTK4 shell (taskbar, windows, search)
│   ├── sovereign-ai/           # PyO3 bridge + orchestrator interface
│   └── sovereign-app/          # Main binary, ties everything together
├── python/
│   ├── sovereign_ai/           # Python AI orchestrator
│   │   ├── models/             # Model abstraction layer
│   │   ├── intent/             # Intent classification
│   │   ├── voice/              # STT/TTS pipeline
│   │   └── profile/            # User profile & adaptive learning
│   └── sovereign_sdk/          # Python SDK for skills
├── skills/
│   ├── markdown-editor/
│   ├── image-viewer/
│   └── pdf-export/
├── tests/
├── docs/
└── config/
```

**Claude generates:** Full project scaffold, Cargo workspace config, CI pipeline (GitHub Actions), development Nix flake or Dockerfile for reproducible builds.

### 1.2 SurrealDB Abstraction Layer (`sovereign-db`)

| Deliverable | Detail |
|-------------|--------|
| Schema definition | Document nodes, relationships, threads, version history |
| CRUD operations | Create/read/update/delete documents |
| Graph queries | Traverse relationships (outbound, inbound, N-hop) |
| Version control | Commit, branch, diff, merge |
| Abstraction trait | `GraphDB` trait that could be backed by SurrealDB or SQLite |

### 1.3 Skill Registry & IPC (`sovereign-skills`)

| Deliverable | Detail |
|-------------|--------|
| Skill manifest parser | Parse `skill.json`, validate capabilities |
| Skill lifecycle | Install, load, unload, health check |
| IPC protocol | Unix socket server, JSON-RPC request/response |
| Skill SDK (Python) | `sovereign_sdk` package that skills use to interact with GraphDB |

### 1.4 GTK4 Shell (`sovereign-ui`)

| Deliverable | Detail |
|-------------|--------|
| Main window | Application window with layout regions |
| Taskbar widget | Bottom-anchored bar with pinned items, recent context, action buttons |
| Document window manager | Free-floating, draggable, resizable windows with sovereignty badges |
| Search overlay | Blurred overlay with search field, filters, results list |
| Theme | Dark theme matching wireframes (CSS variables for owned/external colors) |

**Phase 1 acceptance criteria:** Application launches, shows GTK4 shell with taskbar, can create/read documents in SurrealDB via CLI commands. No canvas, no AI, no skills running yet — just the skeleton.

---

## Phase 2: Canvas & Visual System (Weeks 7-10)

**Goal:** The spatial map is the core UX. Build the 2D infinite canvas with all navigation and visual elements.

### 2.1 Skia Canvas Widget (`sovereign-canvas`)

| Deliverable | Detail |
|-------------|--------|
| Custom GTK4 widget | Embeds Skia rendering surface |
| Coordinate system | World coordinates (infinite 2D) ↔ screen coordinates (viewport) |
| Camera | Pan (drag), zoom (scroll), animate transitions |
| Hit testing | Click on card → identify which document node |

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

**Phase 2 acceptance criteria:** Canvas renders real documents from SurrealDB. Zoom/pan is smooth. Cards show sovereignty distinction. Progressive density works. Minimap works.

---

## Phase 3: AI Orchestrator (Weeks 8-12, parallel with Phase 2)

**Goal:** Local AI that classifies intent, routes to skills, and converses with the user.

### 3.1 PyO3 Bridge (`sovereign-ai`)

| Deliverable | Detail |
|-------------|--------|
| Python interpreter lifecycle | Init, run, shutdown within Rust process |
| Model registry | Load/unload models, query status, swap models |
| Async inference | Non-blocking calls from Rust → Python → model → Rust |
| Memory watchdog | Monitor Python heap, alert on leaks |

### 3.2 Model Abstraction Layer (`python/sovereign_ai/models/`)

| Deliverable | Detail |
|-------------|--------|
| `ModelInterface` base class | `classify_intent()`, `generate()`, `embed()` |
| llama.cpp backend | Load GGUF models via llama-cpp-python |
| Config loader | Read `models.toml`, instantiate correct backend |
| Model swap | Hot-swap models without restarting process |

### 3.3 Intent Classifier (`python/sovereign_ai/intent/`)

| Deliverable | Detail |
|-------------|--------|
| Intent taxonomy | navigation, edit_command, search, voice_dictation, skill_invoke, clarification |
| Classification pipeline | Input → Router model → Intent + confidence + entities |
| Context manager | Active thread, active document, recent actions → context window |
| Disambiguation | If confidence < 0.7, generate clarification question |

### 3.4 Voice Pipeline (`python/sovereign_ai/voice/`)

| Deliverable | Detail |
|-------------|--------|
| Audio capture | System mic → rolling buffer |
| Wake word detection | openWakeWord (or Porcupine) |
| STT | Whisper-small, streaming chunks |
| TTS | Piper, async playback |
| Mode detection | Command vs dictation based on context |

### 3.5 User Profile (`python/sovereign_ai/profile/`)

| Deliverable | Detail |
|-------------|--------|
| Profile schema | Interaction patterns, skill preferences, learned workflows |
| Feedback loop | Track suggestion acceptance rate, adjust thresholds |
| Persistence | JSON file, loaded on startup |

**Phase 3 acceptance criteria:** User can type or speak a command ("open my research notes"), orchestrator classifies intent, routes to correct action (open document), responds via TTS. Disambiguation works for ambiguous queries.

---

## Phase 4: MVP Skills (Weeks 10-14)

**Goal:** Three working skills that demonstrate the Skill architecture end-to-end.

### 4.1 Markdown Editor (`skills/markdown-editor/`)

| Deliverable | Detail |
|-------------|--------|
| Editor widget | Text editing with markdown syntax highlighting |
| Live preview | Side-by-side or toggle rendered view |
| GraphDB integration | Read/write document content via sovereign_sdk |
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
| GraphDB integration | Read image file reference from document node |

### 4.3 PDF Export (`skills/pdf-export/`)

| Deliverable | Detail |
|-------------|--------|
| Export single document | Markdown → PDF with styling |
| Export thread | Multiple documents → combined PDF with table of contents |
| Layout options | Page size, margins, font, header/footer |
| Engine | typst (Rust-native, fast) or weasyprint (Python, HTML→PDF) |

**Phase 4 acceptance criteria:** User can create a markdown document, edit it, view images, and export to PDF — all through the skill system with proper IPC and GraphDB integration.

---

## Phase 5: Integration & Polish (Weeks 14-18)

**Goal:** Wire everything together. The full loop works end-to-end.

### 5.1 Full Loop Integration

- User speaks "create a new research note in Project Alpha"
- Orchestrator classifies intent → create document + assign to thread
- Document appears on canvas in correct thread
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
- Semantic search via sentence-transformers embeddings
- Search overlay shows results with sovereignty shapes
- Timebox jump filters by date range

### 5.5 Testing

| Type | Tool | Coverage Target |
|------|------|----------------|
| Rust unit tests | cargo test | Core: 80%+, DB: 70%+, Skills: 60%+ |
| Python unit tests | pytest | Orchestrator: 70%+, SDK: 80%+ |
| Integration tests | Custom harness | Full loop: create → edit → export |
| Manual testing | You | UX, performance, edge cases |

### 5.6 Documentation

- User guide (basic usage, keyboard shortcuts)
- Skill developer guide (how to build a new skill)
- Architecture overview (for contributors)

**Phase 5 acceptance criteria:** Complete end-to-end flow works. A non-developer could install and use the basic system. No critical bugs.

---

## Timeline Summary

```
Week  1-2   ██ Phase 0: Validation Spikes
Week  3-6   ████ Phase 1: Core Foundation
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

Ready to start **Phase 0, Spike 1: Skia ↔ GTK4 Canvas prototype**?

I can generate the full Rust project with:
- Cargo.toml with gtk4-rs + rust-skia dependencies
- Main window with embedded Skia surface
- Rectangle rendering + mouse-driven pan/zoom
- Basic hit testing

You build and run it, tell me how it performs.
