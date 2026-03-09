# Sovereign GE — Project Instructions

## Library Version Rule
Before writing or modifying any code that uses an external library (crate, pip package, npm module, etc.):
1. Check the **current latest version** on crates.io / PyPI / npm (use WebSearch or WebFetch)
2. Fetch the **latest API documentation** for that version — do NOT rely on memorized APIs from training data
3. Verify method signatures, constructors, and imports against the actual docs before writing code
4. If docs.rs fails to build for a crate, check the project's own hosted docs or GitHub source

This is critical — APIs change between versions and stale knowledge causes cascading build failures.

## Workspace Architecture

10-crate Rust workspace (~16k lines):

| Crate | Role |
|-------|------|
| `sovereign-core` | Shared types, config, interfaces, user profile, security primitives |
| `sovereign-db` | SurrealDB graph database — documents, threads, relationships, contacts, conversations |
| `sovereign-crypto` | Encryption (XChaCha20-Poly1305), key management, content signing |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tools, trust, voice pipeline |
| `sovereign-ui` | Iced 0.14 GUI — panels, chat, search bar, theming (legacy) |
| `sovereign-canvas` | Spatial canvas — document cards, thread zones, drag-and-drop (legacy) |
| `sovereign-skills` | Skill registry and built-in skills (markdown editor, PDF export, video) |
| `sovereign-p2p` | libp2p peer-to-peer sync (experimental) |
| `sovereign-comms` | Communications — email (IMAP/SMTP), Signal (via presage) |
| `sovereign-app` | Binary crate — CLI, GUI bootstrap, setup, seeding |

### AI Orchestrator Architecture (`sovereign-ai`)

The orchestrator uses local Qwen models (2.5 and 3.5) via llama-cpp-2 (no cloud, no external servers):
- **Router (3B–4B)**: Fast intent classification — classifies user input into ~20 action types. Supports Qwen 2.5-3B and Qwen 3.5-4B (with `/no_think` thinking-mode suppression).
- **Reasoning (7B)**: Escalation model for complex/ambiguous queries (loaded on demand, unloaded after 5min idle)
- **Chat agent loop**: Multi-turn with tool calling — loads session history, gathers workspace context, iterates up to 5 rounds of generate → tool call → execute → feed back
- **6 read-only tools**: `search_documents`, `list_threads`, `get_document`, `list_documents`, `search_messages`, `list_contacts` — all Observe level (Level 0), no confirmation needed
- **4 write tools**: `create_document`, `create_thread`, `rename_thread`, `move_document` — Modify level (Level 3), require confirmation
- **Prompt format**: ChatML (`<|im_start|>role\n...\n<|im_end|>`), ChatMLQwen3 (adds `/no_think` suppression), Mistral, and Llama3 formats via `PromptFormatter` trait
- **Per-model sampling**: `SamplingConfig` allows each model family to use optimized temperature, top_k, top_p, and presence_penalty. Qwen 3.5 uses aggressive sampling (temp=1.0, top_p=0.95, presence_penalty=1.5).
- **Tool call format**: `<tool_call>{"name":"...","arguments":{...}}</tool_call>` — learned via few-shot
- **Thinking-mode suppression**: Qwen 3.5 models inject `/no_think` in system prompts; `strip_think_blocks()` defensively removes any leaked `<think>...</think>` tags from output.

- **Unified input path**: Both search bar and chat panel go through classify → gate → dispatch. `handle_chat()` delegates to `handle_query()`, avoiding duplicate routing logic.
- **Model-agnostic**: Supports hot-swapping between Qwen 2.5, Qwen 3.5, Mistral, Llama3 and other GGUF models at runtime. Fuzzy model resolution with alias expansion (e.g. "mistral" finds "Ministral-3B-..."). Format auto-detected from GGUF filename.
- **Memory consolidation**: Background process discovers semantic links between documents when idle (60s cooldown, 30s poll). Scores candidate pairs via 3B router, suggests relationships with strength ≥ 0.4. See `consolidation.rs`.
- **Content reliability assessment**: LLM-powered scoring of external web content. Two-step: classify (factual/opinion/fiction) → score on domain-specific rubric (2–3 criteria, 0–5 each). See `reliability.rs`.

Key modules: `intent/` (classifier + parser), `llm/` (backend, async_backend, prompts, context, format), `orchestrator.rs`, `tools.rs`, `action_gate.rs`, `trust.rs`, `injection.rs`, `session_log.rs`, `autocommit.rs`, `consolidation.rs`, `reliability.rs`, `voice/`

### UX Principles (from `sovereign_os_ux_principles.md`)

These 8 principles are implemented across the codebase — respect them when modifying AI behavior:

1. **Action Gravity** — Friction scales with irreversibility (5 levels: Observe → Destruct). Enforced in `action_gate.rs`.
2. **Conversational Confirmation** — AI proposes with specifics, user confirms naturally. Encoded in chat system prompt.
3. **Sovereignty Halo / Provenance** — Label content as "(owned)" or "(external)". Tool results include provenance markers.
4. **Plan Visibility** — Multi-step plans shown before execution. Encoded in chat system prompt.
5. **Trust Calibration** — Per-workflow trust, never global. Implemented in `trust.rs` with persistent tracking.
6. **Hard Barriers** — Critical constraints enforced by code, not prompts. Chat tools are read-only regardless of model output.
7. **Injection Surfacing** — Detected attacks shown to user. Implemented in `injection.rs`.
8. **Error & Uncertainty** — Rank matches, explain failures, suggest alternatives. Encoded in chat system prompt.

### Tauri Frontend (`frontend/`)

The active UI is a **Svelte 5 + SvelteKit 2 + Tauri 2.0** web app (replaces the legacy Iced GUI). Stack: Svelte 5.51, SvelteKit 2.50, Tauri 2.10, Vite 7.3.

**Key patterns:**
- **Stores use `.svelte.ts` rune modules** — export `$state({})` objects + named functions. Components import and read properties directly (no `$` prefix). Svelte 4 `writable` stores fail with async Tauri IPC.
- **Tauri IPC**: `@tauri-apps/api/core.invoke()` for commands, `@tauri-apps/api/event.listen()` for events. CSP must include `connect-src ipc: http://ipc.localhost`.
- **Timeline canvas**: X = time (`modified_at`), Y = thread lanes. 4 LOD tiers: full card (zoom >= 0.6), title (>= 0.3), dot (>= 0.15), density heatmap (< 0.15). HTML5 Canvas background + DOM-overlaid cards.
- **Markdown rendering**: `marked` + `DOMPurify` (sanitizes HTML tags in AI responses).

**Directory layout:**
```
frontend/src/
├── routes/
│   ├── +layout.svelte         # Auth gate, profile load, Tauri event listener
│   └── +page.svelte           # Main: canvas, bubble, chat, taskbar, panels
└── lib/
    ├── api/commands.ts         # invoke() wrappers for all Tauri commands
    ├── api/events.ts           # OrchestratorEvent listener → store updates
    ├── stores/
    │   ├── app.svelte.ts       # Auth, bubble state, pending actions
    │   ├── canvas.svelte.ts    # Camera, docs, threads, timeline layout, heatmap
    │   ├── chat.svelte.ts      # Messages, visibility, generating state
    │   ├── browser.svelte.ts   # Embedded browser: URL, title, reliability, bounds
    │   ├── suggestions.svelte.ts # AI-suggested document links (pending list)
    │   ├── documents.svelte.ts
    │   ├── contacts.svelte.ts
    │   └── theme.svelte.ts
    ├── components/
    │   ├── Canvas.svelte       # Background: lanes, ticks, heatmap, "Now" line
    │   ├── CanvasCard.svelte   # LOD cards with cascade stacking + z-index
    │   ├── Bubble.svelte       # AI bubble with animated state ring + suggestion badge
    │   ├── Chat.svelte         # Chat panel: markdown, approve/reject, provenance
    │   ├── Minimap.svelte      # Viewport indicator + "Now" line
    │   ├── BrowserPanel.svelte # Embedded browser with reliability assessment
    │   ├── SuggestionPanel.svelte # AI-suggested document links (accept/dismiss)
    │   ├── OnboardingWizard.svelte
    │   ├── SettingsPanel.svelte
    │   └── ...                 # Search, Taskbar, LoginScreen, panels
    ├── theme/colors.ts         # CSS variable definitions
    └── utils/markdown.ts       # marked + DOMPurify pipeline
```

**Build the frontend:**
```bash
cd frontend && npm install && npm run build    # produces frontend/build/
```

**Feature flag:** `--features tauri-ui` on `sovereign-app` (mutually exclusive with `iced-ui`).

## Build & Development

### Toolchain Prerequisites (Windows)

Install via `winget` if missing:
- **Visual Studio 2022 Build Tools** with C++ workload: `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
- **LLVM** (for `libclang.dll` needed by bindgen): `winget install LLVM.LLVM`
- **CMake**: `winget install Kitware.CMake`
- **Rust** (stable MSVC): `winget install Rustlang.Rustup`
- **Node.js** 20+ (for frontend): `winget install OpenJS.NodeJS.LTS`

### WSL2 / Linux
- Source lives on NAS mount (`/mnt/nas/Current/Projets/03 - user-centered OS/`)
- Copy to WSL native filesystem (`~/`) before building for performance
- Always `rm -rf` target directory before `cp -r` (cp into existing dir nests instead of overwriting)
- Rust linker is rust-lld — be aware of `--as-needed` link ordering issues
- Limit parallel compilation to avoid OOM-crashing WSL: use `-j 4` (confirmed stable with 16 GB `.wslconfig`) or `-j 2` as fallback

### Windows (MSVC target)

#### Build scripts (from PowerShell or cmd.exe)
Three batch wrappers in the project root set `LIBCLANG_PATH`, `CMAKE`, and `PATH` automatically:
- `_build.bat` — runs `cargo <args>` (pass any cargo subcommand + flags)
- `_check.bat` — runs `cargo check -p sovereign-app` with `-j 4`
- `_run.bat` — runs the app

**From a native Windows shell (PowerShell / cmd):**
```powershell
# Build a specific crate
_build.bat build -p sovereign-canvas --target-dir "Z:\cargo-target" -j 2

# Build the whole app
_build.bat build -p sovereign-app --target-dir "Z:\cargo-target" -j 2
```

#### From bash (Claude Code / Git Bash)

**Critical: Git Bash `link.exe` conflict.** Git Bash ships `/usr/bin/link.exe` (GNU hard-link utility) which shadows the MSVC `link.exe` (linker). You MUST prepend the MSVC bin directory to PATH, otherwise linking fails with `link: extra operand`.

Full environment setup for bash:
```bash
# MSVC toolchain paths (adjust version numbers if MSVC updates)
export PATH="/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/bin/Hostx64/x64:/c/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64:/c/Program Files/CMake/bin:$PATH:/c/Users/celin/.cargo/bin"
export LIBCLANG_PATH="C:/Program Files/LLVM/bin"
export CMAKE="C:/Program Files/CMake/bin/cmake.exe"
export LIB="C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/lib/x64;C:/Program Files (x86)/Windows Kits/10/Lib/10.0.26100.0/um/x64;C:/Program Files (x86)/Windows Kits/10/Lib/10.0.26100.0/ucrt/x64"
export INCLUDE="C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207/include;C:/Program Files (x86)/Windows Kits/10/Include/10.0.26100.0/ucrt;C:/Program Files (x86)/Windows Kits/10/Include/10.0.26100.0/um;C:/Program Files (x86)/Windows Kits/10/Include/10.0.26100.0/shared"

# Then run cargo as usual
cargo.exe check -p sovereign-ai --no-default-features --target-dir "C:/cargo-target" -j 2
cargo.exe build -p sovereign-app --target-dir "C:/cargo-target" -j 2
```

#### Key notes
- `sovereign-ai` default feature is `cuda` — disable on machines without CUDA toolkit: `--no-default-features`
- Use `--target-dir` to control where build artifacts go. Use `"Z:/cargo-target"` if Z: drive (NAS) is available, otherwise `"C:/cargo-target"`. Forward slashes in bash, backslashes in cmd/PowerShell.
- Windows needs `/FORCE:MULTIPLE` linker flag (MSVC) because `llama-cpp-sys-2` and `whisper-rs-sys` both embed ggml — this is set in `.cargo/config.toml`
- Before rebuilding after errors, kill stale processes and clean sovereign artifacts:
  ```powershell
  Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force
  Remove-Item '<target-dir>\debug\deps\libsovereign_*' -Force -ErrorAction SilentlyContinue
  Remove-Item '<target-dir>\debug\.fingerprint\sovereign-*' -Recurse -Force -ErrorAction SilentlyContinue
  ```

#### Running the Tauri app from bash
When launching `sovereign.exe` in the background (e.g. `./sovereign.exe &`), the shell reports exit code 0 almost immediately. **The app is still running** — the Tauri window runs in a separate GUI thread. The exit code 0 from the shell only indicates that the initial process setup completed. Use `tasklist | grep sovereign` or Task Manager to confirm the app is still running. Kill with `taskkill //F //IM sovereign.exe`.

## Testing

### WSL2 / Linux
```bash
cargo test -j 4
```

### Windows (from PowerShell / cmd)
```powershell
# All crates except sovereign-ai (which defaults to CUDA)
_build.bat test --target-dir "Z:\cargo-target" -j 2

# sovereign-ai specifically (skip CUDA)
_build.bat test -p sovereign-ai --no-default-features --target-dir "Z:\cargo-target" -j 2

# Integration test (builds + runs the binary as subprocess)
_build.bat test -p sovereign-app --test cli_integration --target-dir "Z:\cargo-target" -j 2
```

### Windows (from bash — Claude Code / Git Bash)
Set the full environment from the bash section above, then:
```bash
# sovereign-ai (skip CUDA)
cargo.exe test -p sovereign-ai --no-default-features --target-dir "C:/cargo-target" -j 2

# Single crate
cargo.exe test -p sovereign-canvas --target-dir "C:/cargo-target" -j 2

# All crates except sovereign-ai
cargo.exe test --target-dir "C:/cargo-target" -j 2
```

### Key gotchas
- **In-memory SurrealDB instances are isolated.** Each `create_db()` with memory mode creates a fresh DB. Tests requiring state across function calls must share a single DB instance or use persistent mode.
- **TOML backslash escaping on Windows.** When writing Windows paths into TOML strings, replace `\` with `/` — otherwise `\U`, `\t`, etc. are misinterpreted as escape sequences and config silently falls back to defaults.
- **Integration tests use persistent temp DBs** since each subprocess gets its own in-memory DB.

## User Confirmation Required
- When a problem can be solved either by installing a missing system package or by changing the code, **ask the user** which approach they prefer before proceeding
- Never run `sudo` commands to install packages without explicit user approval

## Git & NAS Push/Merge Workflow

**VSCode `git.untrackedChanges` is set to `hidden`** because SurrealDB `.db` files (10k+) flood source control despite being in `.gitignore`. This means new files won't appear in the Source Control panel — you must `git add <file>` explicitly.

The repo on the NAS is at `\\nas\home\Current\Projets\03 - user-centered OS` (bare repo).

### Windows (from bash — Claude Code / Git Bash)

Remote URL uses UNC path (already configured):
```bash
# Remote should be set to:
git remote set-url origin '//nas/home/Current/Projets/03 - user-centered OS'

# Safe directory exceptions (one-time, already configured):
git config --global --add safe.directory '//nas/home/Current/Projets/03 - user-centered OS'
git config --global --add safe.directory '//nas/home/Current/Projets/03 - user-centered OS/.git'

# Push / pull
git push origin main
git pull origin main
```

### GitHub

Public repo: `https://github.com/clenoble/sovereign.git` (remote name: `github`)
```bash
git push github main
git push github --tags
```

### WSL2 / Linux

WSL cannot use UNC paths — mount the NAS first:
```bash
# 1. Mount the NAS (if not already mounted — requires sudo, ask user)
sudo mount -t drvfs 'Z:' /mnt/nas

# 2. Ensure git trusts the NAS path (one-time)
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS'
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS/.git'

# 3. Set remote to WSL-accessible path
git remote set-url origin '/mnt/nas/03 - user-centered OS'

# 4. Push
git push origin main
```

## Code Style
- Rust: edition 2021, prefer safe code, minimize unsafe blocks
- Keep spike code simple and focused — no over-engineering
- Comments only where logic isn't self-evident

## sovereign-app Module Structure
The binary crate (`sovereign-app`) is split into focused modules:
- `cli.rs` — Clap CLI struct and Commands enum. **Subcommand is optional** — running `sovereign.exe` with no args defaults to `run` (launches GUI).
- `commands.rs` — Async CLI handler functions (create/get/list/update/delete for docs, threads, relationships, commits, contacts, conversations)
- `tauri_commands.rs` — 40+ Tauri `invoke()` command handlers (chat, documents, threads, contacts, settings, browser, suggestions, reliability)
- `tauri_events.rs` — `OrchestratorEvent` → Tauri `emit()` bridge with typed payloads
- `browser.rs` — Embedded Tauri webview lifecycle: create, navigate, back/forward/refresh, set bounds, destroy
- `web.rs` — Web content fetching via `reqwest` + `readability` text extraction (8KB truncation for LLM, 12KB for display)
- `setup.rs` — DB creation, crypto initialization, orchestrator callback wiring
- `seed.rs` — Sample data seeding on first launch (DB data + user profile + session log history)
- `main.rs` — Entry point: CLI dispatch + GUI bootstrap (`run_gui`) + idle-watcher for background memory consolidation
