# Contributing to Sovereign GE

Welcome! This guide will help you understand the codebase and start contributing.

## Quick Start

### Prerequisites

- **Rust** (stable, edition 2021) — `rustup default stable`
- **Node.js** 20+ and **npm** (for the Tauri/Svelte frontend)
- **CMake** and **LLVM** (for llama-cpp-2 bindgen)
- **Windows additionally:** Visual Studio 2022 Build Tools with C++ workload

### Build

```bash
# Clone
git clone https://github.com/clenoble/sovereign.git
cd sovereign

# Download a GGUF model for the router (optional — app runs without it, AI features won't work)
pip install huggingface-hub
huggingface-cli download Qwen/Qwen2.5-3B-Instruct-GGUF \
  qwen2.5-3b-instruct-q4_k_m.gguf --local-dir models/

# Install frontend dependencies
cd frontend && npm install && cd ..

# Build frontend
cd frontend && npm run build && cd ..

# Build with Tauri UI (recommended)
cargo build -p sovereign-app --no-default-features --features tauri-ui,encrypted-log -j 4

# Run
./target/debug/sovereign run

# Run tests
cargo test -j 4                                          # all crates except sovereign-ai
cargo test -p sovereign-ai --no-default-features -j 4    # sovereign-ai (skip CUDA)
```

On Windows from Git Bash, see [CLAUDE.md](CLAUDE.md) for the full MSVC environment setup.

---

## Architecture Overview

Sovereign GE is a 10-crate Rust workspace plus a Svelte frontend. Here's how the pieces relate:

```
    ┌─────────────────┐
    │  frontend/      │  Svelte 5 + SvelteKit 2 + Tauri 2.0
    │  (web UI)       │  Canvas, chat, onboarding, settings
    └────────┬────────┘
             │ Tauri IPC (invoke / events)
    ┌────────▼────────┐
    │ sovereign-app   │  Binary: CLI + Tauri host + GUI bootstrap
    └────────┬────────┘
             │ depends on all crates below
   ┌─────────┼─────────────┐
   │         │             │
   │  ┌──────▼──────┐ ┌───▼──────────┐
   │  │sovereign-ai │ │sovereign-ui  │  (legacy Iced GUI)
   │  │ Orchestrator│ │sovereign-    │  (legacy Iced canvas)
   │  │ LLM, intent │ │  canvas      │
   │  │ tools, trust│ └──────────────┘
   │  └──────┬──────┘
   │         │
   ├─────────┼─────────────┐
   │         │             │
   ┌▼────────▼──┐ ┌───────▼─────┐
   │sovereign-db│ │sovereign-   │
   │  SurrealDB │ │  crypto     │  Also: sovereign-skills,
   │  graph     │ │  XChaCha20  │  sovereign-p2p, sovereign-comms
   └──────┬─────┘ └──────┬──────┘
          └───────┬──────┘
           ┌──────▼──────┐
           │sovereign-   │  Shared types, config,
           │  core       │  interfaces, events
           └─────────────┘
```

### Key Data Flow

1. **User types in search bar or chat panel** (Svelte frontend)
2. Frontend calls Tauri `invoke()` → Rust command handler
3. Both go through the same path: `handle_query()` → `IntentClassifier.classify()` → action gate → `execute_action()`
4. The classifier uses a local 3B GGUF model (Qwen2.5) to determine intent (search, open, create_thread, chat, etc.)
5. For "chat" intent, the agent loop runs: build prompt → generate → parse tool calls → execute tools → feed results back → repeat (up to 5 rounds)
6. Results emit `OrchestratorEvent`s via Tauri `emit()` → frontend event listener updates stores → reactive UI updates

### Tauri Frontend Architecture

The active UI is a Svelte 5 + SvelteKit 2 app bundled via Tauri 2.0:

- **Stores** (`lib/stores/*.svelte.ts`): Svelte 5 rune modules using `$state()`, `$derived()`, `$effect()`. Must use `.svelte.ts` extension — Svelte 4 `writable` stores don't propagate reactivity when updated from async Tauri IPC.
- **Commands** (`lib/api/commands.ts`): Typed wrappers around `@tauri-apps/api/core.invoke()` for all backend operations (chat, documents, threads, contacts, settings, auth).
- **Events** (`lib/api/events.ts`): Listens for `OrchestratorEvent` from the Rust backend and dispatches to stores.
- **Canvas** (`Canvas.svelte` + `CanvasCard.svelte`): HTML5 Canvas for background (grid, lanes, date ticks, heatmap) with DOM-overlaid cards. Camera with pan/zoom, 4 LOD tiers (full → title → dot → heatmap).
- **Timeline layout**: X-axis = document `modified_at`, Y-axis = thread lanes. "Now" line, adaptive date tick spacing (day → week → month → year), density heatmap at extreme zoom-out.

### Directory Layout

```
frontend/                        # Tauri + Svelte 5 web UI
├── package.json                 # Svelte 5.51, SvelteKit 2.50, Tauri 2.10, Vite 7.3
├── src/
│   ├── routes/
│   │   ├── +layout.svelte       # Root layout: auth check, profile load, event listener
│   │   └── +page.svelte         # Main page: canvas, bubble, chat, taskbar, panels
│   └── lib/
│       ├── api/
│       │   ├── commands.ts      # Tauri invoke() wrappers for all backend commands
│       │   └── events.ts        # Tauri event listeners (orchestrator events → store updates)
│       ├── stores/              # Svelte 5 rune stores ($state, $derived, $effect)
│       │   ├── app.svelte.ts    # Auth state, bubble state, pending actions
│       │   ├── canvas.svelte.ts # Camera, documents, threads, timeline layout, heatmap
│       │   ├── chat.svelte.ts   # Chat messages, visibility, generating state
│       │   ├── documents.svelte.ts
│       │   ├── contacts.svelte.ts
│       │   └── theme.svelte.ts
│       ├── components/
│       │   ├── Canvas.svelte    # 2D canvas: thread lanes, date ticks, heatmap, "Now" line
│       │   ├── CanvasCard.svelte# LOD document cards (full → title → dot → heatmap)
│       │   ├── Bubble.svelte    # AI bubble (animated border by state)
│       │   ├── BubblePreview.svelte # SVG bubble face variants
│       │   ├── Chat.svelte      # Chat panel with markdown, approval buttons
│       │   ├── Minimap.svelte   # Canvas minimap with viewport indicator
│       │   ├── Taskbar.svelte   # Bottom taskbar
│       │   ├── Search.svelte    # Search overlay
│       │   ├── OnboardingWizard.svelte
│       │   ├── LoginScreen.svelte
│       │   ├── SettingsPanel.svelte
│       │   └── ...              # DocumentPanel, ContactPanel, ModelPanel, etc.
│       ├── theme/colors.ts      # CSS variable definitions
│       └── utils/markdown.ts    # Markdown rendering (marked + DOMPurify)

crates/
├── sovereign-core/src/
│   ├── config.rs        # AppConfig (TOML)
│   ├── interfaces.rs    # OrchestratorEvent (30+ variants), CanvasController trait
│   └── security.rs      # ActionLevel (5 levels), Plane, BubbleVisualState
├── sovereign-db/src/
│   ├── traits.rs        # GraphDB trait (~50 async methods)
│   ├── schema.rs        # Document, Thread, Contact, Commit, Message, etc.
│   └── surreal.rs       # SurrealDB implementation
├── sovereign-ai/src/
│   ├── orchestrator.rs  # Central hub: query handling, action dispatch, agent loop
│   ├── intent/
│   │   ├── classifier.rs # 3B router + 7B reasoning escalation
│   │   └── parser.rs     # Heuristic + JSON intent parsing
│   ├── llm/
│   │   ├── backend.rs    # llama-cpp-2 inference (global OnceLock backend)
│   │   ├── async_backend.rs # Async wrapper (spawn_blocking)
│   │   ├── prompt.rs     # System prompts, few-shot examples
│   │   ├── context.rs    # Multi-turn prompt assembly
│   │   └── format.rs     # PromptFormatter: ChatML, Mistral, Llama3
│   ├── tools.rs          # 10 tools (6 read-only, 4 write)
│   ├── action_gate.rs    # Authorization: action levels, plane checking
│   ├── trust.rs          # Per-workflow trust tracking
│   ├── injection.rs      # Prompt injection detection
│   ├── session_log.rs    # Append-only JSONL session history
│   └── autocommit.rs     # Auto-commit engine for document versioning
├── sovereign-ui/src/
│   ├── app.rs            # Main Iced app: Message enum, update(), view()
│   ├── chat.rs           # Chat widget
│   ├── search.rs         # Search overlay
│   ├── taskbar.rs        # Bottom taskbar
│   ├── theme.rs          # Dark/light theme, palette functions
│   └── panels/           # Document, model, inbox, contact, camera panels
├── sovereign-canvas/src/
│   ├── state.rs          # CanvasState (camera, cards, layout)
│   ├── renderer.rs       # Iced shader program
│   └── layout.rs         # Thread lanes, card positioning
├── sovereign-crypto/src/
│   ├── aead.rs           # XChaCha20-Poly1305 encrypt/decrypt
│   ├── master_key.rs     # Passphrase → master key (Argon2)
│   └── guardian.rs       # Shamir secret sharing (feature-gated)
├── sovereign-skills/src/
│   ├── registry.rs       # Skill discovery and lookup
│   └── traits.rs         # CoreSkill trait
├── sovereign-comms/src/
│   ├── channel.rs        # CommunicationChannel trait
│   └── channels/         # email.rs, signal.rs, whatsapp.rs (stub)
├── sovereign-p2p/src/
│   └── node.rs           # libp2p node, sync commands/events
└── sovereign-app/src/
    ├── main.rs           # Entry point
    ├── cli.rs            # Clap CLI definition
    ├── commands.rs       # CLI command handlers
    ├── setup.rs          # DB + orchestrator wiring
    └── seed.rs           # First-launch sample data
```

---

## Feature Flags

Most heavy dependencies are opt-in:

| Flag | Default | What it gates |
|------|---------|---------------|
| `iced-ui` | ON | Legacy Iced 0.14 native GUI |
| `tauri-ui` | off | Tauri 2.0 + Svelte 5 web UI (active frontend) |
| `cuda` | ON (sovereign-ai only) | GPU-accelerated LLM inference |
| `voice-stt` | off | Whisper speech-to-text |
| `wake-word` | off | Wake word detection (requires voice-stt) |
| `encryption` | off | Document encryption, key management |
| `p2p` | off | Device pairing and sync |
| `comms-email` | off | Email (IMAP/SMTP) |
| `comms-signal` | off | Signal messenger |
| `comms-whatsapp` | off | WhatsApp (stub) |

To build with the Tauri UI (most common for contributors):
```bash
cd frontend && npm install && npm run build && cd ..
cargo build -p sovereign-app --no-default-features --features tauri-ui,encrypted-log
cargo test -p sovereign-ai --no-default-features
```

---

## UX Principles

The project follows 8 UX principles (see `doc/spec/sovereign_os_ux_principles.md`). The most relevant for contributors:

1. **Action Gravity** — Friction scales with irreversibility. Reading is instant (Level 1). Creating/editing requires confirmation (Level 3). Deleting is soft-delete with 30-day undo (Level 5). Enforced in `action_gate.rs`.

2. **Hard Barriers** — Security constraints are enforced by code architecture, never by prompts. The LLM can ask for anything — the execution layer decides what's allowed.

3. **Sovereignty Halo** — Content from the user's own data is visually distinct from external/imported content. Tool results include `(owned)` or `(external)` markers.

---

## How to Pick an Issue

Check [GitHub Issues](https://github.com/clenoble/sovereign/issues) for `good first issue` and `help wanted` labels.

**Best starting points:**
- Issues touching a single crate (e.g., just `sovereign-ai` or just `sovereign-db`)
- Issues with clear acceptance criteria and file paths listed
- Issues tagged `good first issue` are scoped to be completable in a few hours

**Before starting:**
1. Comment on the issue to claim it
2. Read the files listed in the issue description
3. Run the existing tests to make sure your setup works
4. Create a branch from `main`

---

## Making Changes

### Running Tests

```bash
# Full workspace (excludes sovereign-ai CUDA)
cargo test -j 4

# Specific crate
cargo test -p sovereign-db -j 4

# sovereign-ai (must skip CUDA on most machines)
cargo test -p sovereign-ai --no-default-features -j 4

# Integration test (builds the binary and runs it as a subprocess)
cargo test -p sovereign-app --test cli_integration -j 4
```

### Code Style

- Rust edition 2021
- Prefer safe code, minimize `unsafe` blocks
- Comments only where logic isn't self-evident
- No over-engineering — solve the current problem, not hypothetical future ones
- Follow existing patterns in the crate you're modifying

### Pull Requests

- Keep PRs focused on a single issue
- Include tests for new functionality
- Make sure `cargo test` passes before submitting
- Reference the issue number in the PR description

---

## Documentation Map

| Document | Purpose |
|----------|---------|
| [CLAUDE.md](CLAUDE.md) | Build instructions, architecture reference, dev environment setup |
| [doc/spec/sovereign_os_specification.md](doc/spec/sovereign_os_specification.md) | Technical specification (v1.2) |
| [doc/spec/sovereign_os_ux_principles.md](doc/spec/sovereign_os_ux_principles.md) | 8 UX principles guiding the design |
| [doc/design/design_decisions.md](doc/design/design_decisions.md) | Canvas, navigation, visual design decisions |
| [doc/plans/sovereign_os_implementation_plan.md](doc/plans/sovereign_os_implementation_plan.md) | Phase roadmap (all phases complete) |
| [doc/plans/todolist.md](doc/plans/todolist.md) | Open issues and feature roadmap |
| [doc/legal/sovereign_os_ethics.md](doc/legal/sovereign_os_ethics.md) | Ethical analysis and binding design constraints |
| [doc/writing-skills.md](doc/writing-skills.md) | Guide for writing third-party WASM skill plugins |

---

## Questions?

Open an issue on [GitHub](https://github.com/clenoble/sovereign/issues) or comment on an existing one.
