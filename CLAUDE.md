# Sovereign GE — Project Instructions

## Library Version Rule
Before writing or modifying any code that uses an external library (crate, pip package, npm module, etc.):
1. Check the **current latest version** on crates.io / PyPI / npm (use WebSearch or WebFetch)
2. Fetch the **latest API documentation** for that version — do NOT rely on memorized APIs from training data
3. Verify method signatures, constructors, and imports against the actual docs before writing code
4. If docs.rs fails to build for a crate, check the project's own hosted docs or GitHub source

This is critical — APIs change between versions and stale knowledge causes cascading build failures.

## Source of truth: the SPEC, not the plans

`doc/spec/sovereign_os_specification.md` is the **single source of truth** for
what the system is and how it should behave. **Refer to the spec before starting
any feature.** Plans are execution scaffolding only — they say *how/when*, never
*what* — and they are ephemeral (session/memory or clearly-dated notes), not
canonical. **If a plan and the spec disagree, the spec wins and the plan is
wrong.**

Why this rule exists (2026-07-10): the Social Backup & Recovery work drifted for
weeks because two overlapping *plan* docs (`p2p-completion-plan.md`,
`p2pbackupuserreadyplan.md`, now archived) quietly redefined the design — they
had guardians sharding a *backup/data key* and framed recovery as "passphrase +
guardian approval," when the spec says guardians shard the **Recovery Key** to
restore *access* without the passphrase (data lives on synced devices; the crowd
mesh is Phase 2). The code followed the plans, not the spec, and built the wrong
(deferred) feature. Nobody caught it because **nothing diffed the plans against
the spec.** The fix: design decisions live in the spec; plans don't get to
redefine them; and "does this match the spec?" is a required check before
building. When you find a plan-vs-spec conflict, treat it as a bug in the plan.

## Workspace Architecture

8-crate Rust workspace (~12k lines):

| Crate | Role |
|-------|------|
| `sovereign-core` | Shared types, config, interfaces, user profile, security primitives |
| `sovereign-db` | SurrealDB graph database — documents, threads, relationships, contacts, conversations |
| `sovereign-crypto` | Encryption (XChaCha20-Poly1305), key management, content signing |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tools, trust, voice pipeline |
| `sovereign-skills` | Skill registry and built-in skills (markdown editor, PDF export, video) |
| `sovereign-p2p` | libp2p peer-to-peer sync (experimental) |
| `sovereign-comms` | Communications — email (IMAP/SMTP), Signal (via presage) |
| `sovereign-app` | Tauri/CLI binary crate — Tauri bootstrap, CLI, setup, seeding. Builds as `sovereign-tauri` (desktop) + the `sovereign_app` cdylib (mobile) |
| `sovereign-shell` | Native Rust/Vello/winit/wgpu UI — the **default desktop UI**. Builds as `sovereign` (`sovereign.exe`). Calls the backend crates in-process (no Tauri IPC) |

**UI — two desktop frontends, one mobile:**
- **Default desktop UI = the native shell** (`sovereign-shell`, binary `sovereign`). Run with `_run.bat` or `cargo run -p sovereign-shell`.
- **Tauri is kept as an option**: the Svelte 5 + Tauri 2 frontend in `frontend/` still builds as `sovereign-tauri` (desktop — `_run-tauri.bat` / `_dev.bat`) and **is the mobile UI** (`cargo tauri android`, the `sovereign_app` cdylib — unaffected by the desktop default).
- The previous Iced-based `sovereign-ui` and `sovereign-canvas` crates were retired.

### Data-at-rest threat model

The at-rest model is **field-level encryption + OS full-disk encryption**, not whole-database encryption:

- **Content is field-encrypted** by `EncryptedGraphDB` (a decorator over the raw SurrealGraphDB): document titles + bodies, message bodies, thread/conversation names, contact names + notes + addresses, and all PII vault values are XChaCha20-Poly1305 encrypted under per-entity keys (per-persona key DBs — see [[CRYPTO-001]] duress isolation). Install fails *closed* (login aborts rather than writing plaintext).
- **Metadata/structure stays plaintext on disk** and cannot be opaquely encrypted without making the store unqueryable: record IDs, timestamps, relationship/graph edges, and the deterministic blind-index token hashes used for encrypted-search (the accepted CRYPTO-004 correlation tradeoff). Embedded SurrealDB+RocksDB has **no native at-rest encryption**.
- **For the offline-disk-theft threat, the metadata layer is covered by OS full-disk encryption** (BitLocker / FileVault / LUKS) — treat that as a deployment requirement, with field encryption as defense-in-depth on top. This is the disposition of audit finding ATREST-001 (re-scoped from "datadir not encrypted" to "metadata-at-rest leakage, covered by OS FDE").

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

The UI is a **Svelte 5 + SvelteKit 2 + Tauri 2.0** web app. Stack: Svelte 5.51, SvelteKit 2.50, Tauri 2.10, Vite 7.3.

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

## Build & Development

### Toolchain Prerequisites (Windows)

Install via `winget` if missing:
- **Visual Studio 2022 Build Tools** with C++ workload: `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
- **LLVM** (for `libclang.dll` needed by bindgen): `winget install LLVM.LLVM`
- **CMake**: `winget install Kitware.CMake`
- **Rust** (stable MSVC): `winget install Rustlang.Rustup`
- **Node.js** 20+ (for frontend): `winget install OpenJS.NodeJS.LTS`
- **CUDA Toolkit** (only if building/running with `--features cuda`): `winget install Nvidia.CUDA` — installs to `C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v<version>\`. The installer sets `CUDA_PATH` machine-wide but **does not propagate to existing shells** — restart any open terminal after install.

#### CUDA 13 runtime DLL gotcha (Windows)
CUDA **13.x** changed the runtime DLL layout: `cudart64_13.dll`, `cublas64_13.dll`, `cublasLt64_13.dll` live in `<CUDA_PATH>\bin\x64\`, **not** `<CUDA_PATH>\bin\` like CUDA 12. The installer adds `\bin` to system PATH but **not** `\bin\x64`. Symptom: a clean `--features cuda` build links fine but the resulting exe (or any test binary) fails immediately with `STATUS_DLL_NOT_FOUND` (0xc0000135).

For builds and `cargo test`, the env block must include both:
```bash
export PATH="$CUDA_PATH/bin/x64:$CUDA_PATH/bin:$PATH"   # bash
$env:PATH = "$env:CUDA_PATH\bin\x64;$env:CUDA_PATH\bin;$env:PATH"  # PowerShell
```

For shipping a `--features cuda` release exe, copy the 3 runtime DLLs next to it (~485 MB) so end users don't need a CUDA toolkit install — only NVIDIA drivers. Note: the default `sovereign.exe` (native shell) and the default `sovereign-tauri` build are **CPU-only** (`sovereign-ai` is pulled `default-features = false`), so they don't need the CUDA DLLs unless built with `--features cuda`.

### WSL2 / Linux
- If your source lives on a network mount, copy to WSL native filesystem (`~/`) before building for performance
- Always `rm -rf` target directory before `cp -r` (cp into existing dir nests instead of overwriting)
- Rust linker is rust-lld — be aware of `--as-needed` link ordering issues
- Limit parallel compilation to avoid OOM-crashing WSL: use `-j 4` (confirmed stable with 16 GB `.wslconfig`) or `-j 2` as fallback

### Windows (MSVC target)

#### Build scripts (from PowerShell or cmd.exe)
Three batch wrappers in the project root set `LIBCLANG_PATH`, `CMAKE`, and `PATH` from environment variables (with sensible defaults that match `winget install` locations):

| Variable | Default | Purpose |
|---|---|---|
| `SOVEREIGN_LLVM_DIR` | `C:\Program Files\LLVM\bin` | Directory containing `libclang.dll` |
| `SOVEREIGN_CMAKE_DIR` | `C:\Program Files\CMake\bin` | Directory containing `cmake.exe` |
| `SOVEREIGN_TARGET_DIR` | _(unset → cargo's `./target`)_ | Override cargo target dir (e.g. for a fast SSD or network drive) |

The wrappers:
- `_build.bat` — runs `cargo <args>` (pass any cargo subcommand + flags)
- `_check.bat` — runs `cargo check --workspace --exclude sovereign-ai` with `-j 4`
- `_run.bat` — runs the app

**Why the check is `--workspace` and not a crate list.** It was
`-p sovereign-app` until 2026-07-16, which left `sovereign-shell` — the
*default desktop binary* — outside the gate. It stopped compiling when the F1
backend added three `P2pEvent` variants and nobody noticed for a whole feature
*and* a live e2e, because every build went through the Tauri owner. A named
list has the same failure mode one crate later: `--workspace` fails **safe** (a
new crate is checked by default), a list fails **silent**. Warm cost ~2s.

`sovereign-ai` is excluded only as a *selected package* — its own
`default = ["cuda"]` would force a CUDA toolchain on every checkout. Its lib is
still checked, because every member pulls it through the workspace pin
`default-features = false`. Only its cuda-gated code is out of scope: check that
explicitly with `cargo check -p sovereign-ai --features cuda` when touching it.

**From a native Windows shell (PowerShell / cmd):**
```powershell
# Build a specific crate (uses defaults)
_build.bat build -p sovereign-skills -j 2

# Override target dir for this invocation
$env:SOVEREIGN_TARGET_DIR = "D:\cargo-target"
_build.bat build -p sovereign-app -j 2
```

#### From bash (Claude Code / Git Bash)

**Critical: Git Bash `link.exe` conflict.** Git Bash ships `/usr/bin/link.exe` (GNU hard-link utility) which shadows the MSVC `link.exe` (linker). You MUST prepend the MSVC bin directory to PATH, otherwise linking fails with `link: extra operand`.

Full environment setup for bash (adjust version numbers and paths to match your installation):
```bash
# MSVC toolchain — replace 14.XX.XXXXX and 10.0.XXXXX.0 with the versions installed on your machine
MSVC_VER="14.44.35207"
WINSDK_VER="10.0.26100.0"
MSVC_ROOT="/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/$MSVC_VER"
WINSDK_ROOT="/c/Program Files (x86)/Windows Kits/10"

export PATH="$MSVC_ROOT/bin/Hostx64/x64:$WINSDK_ROOT/bin/$WINSDK_VER/x64:/c/Program Files/CMake/bin:$PATH:$HOME/.cargo/bin"
export LIBCLANG_PATH="C:/Program Files/LLVM/bin"
export CMAKE="C:/Program Files/CMake/bin/cmake.exe"
export LIB="$(cygpath -w "$MSVC_ROOT")\\lib\\x64;$(cygpath -w "$WINSDK_ROOT")\\Lib\\$WINSDK_VER\\um\\x64;$(cygpath -w "$WINSDK_ROOT")\\Lib\\$WINSDK_VER\\ucrt\\x64"
export INCLUDE="$(cygpath -w "$MSVC_ROOT")\\include;$(cygpath -w "$WINSDK_ROOT")\\Include\\$WINSDK_VER\\ucrt;$(cygpath -w "$WINSDK_ROOT")\\Include\\$WINSDK_VER\\um;$(cygpath -w "$WINSDK_ROOT")\\Include\\$WINSDK_VER\\shared"

# Then run cargo as usual (drop --target-dir to use cargo's default ./target)
cargo.exe check -p sovereign-ai --no-default-features -j 2
cargo.exe build -p sovereign-app -j 2
```

#### Key notes
- `sovereign-ai` default feature is `cuda` — disable on machines without CUDA toolkit: `--no-default-features`
- **Tauri crates and the frontend build — `devUrl` is what decides.** `generate_context!` needs *something* to serve. If `tauri.conf.json` declares a `devUrl`, dev builds (`cargo check`/`cargo build`) use it and **do not need the frontend output to exist**; only release builds embed `frontendDist`. If it declares **no** `devUrl`, `frontendDist` must exist even to `cargo check`, and the failure reads `The "frontendDist" configuration is set to "..." but this path doesn't exist` (older/less lucky cases surface only as "proc macro panicked"). Both apps declare one now (main app → `localhost:1420`, guardian app → `localhost:5175`, matching its vite `strictPort`), so a clean checkout checks without any `npm` step. `frontendDist` is relative to **the config file's directory**. *(Measured 2026-07-16 — an earlier note claimed the frontend output was needed before the macro in all cases; that is true only without a `devUrl`.)*
- Set `CARGO_TARGET_DIR` (or `SOVEREIGN_TARGET_DIR` for the batch wrappers) if you want to redirect build artifacts to a faster drive or shared cache. Default is cargo's `./target`. Forward slashes in bash, backslashes in cmd/PowerShell.
- Windows needs `/FORCE:MULTIPLE` linker flag (MSVC) because `llama-cpp-sys-2` and `whisper-rs-sys` both embed ggml — this is set in `.cargo/config.toml`
- Before rebuilding after errors, kill stale processes and clean sovereign artifacts:
  ```powershell
  Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force
  $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "target" }
  Remove-Item "$target\debug\deps\libsovereign_*" -Force -ErrorAction SilentlyContinue
  Remove-Item "$target\debug\.fingerprint\sovereign-*" -Recurse -Force -ErrorAction SilentlyContinue
  ```

#### Running the Tauri app from bash
When launching the Tauri binary `sovereign-tauri.exe` in the background (e.g. `./sovereign-tauri.exe &`), the shell reports exit code 0 almost immediately. **The app is still running** — the Tauri window runs in a separate GUI thread. The exit code 0 from the shell only indicates that the initial process setup completed. Use `tasklist | grep sovereign` or Task Manager to confirm the app is still running. Kill with `taskkill //F //IM sovereign-tauri.exe`. (The default native shell binary is `sovereign.exe` — kill that with `taskkill //F //IM sovereign.exe`.)

## Testing

**Test-as-you-go.** Build a feature's tests in the same change as the feature, not afterward — extract pure logic into testable functions and add unit tests alongside. Run the FULL suite before claiming green: `cargo test` (NOT just `cargo check` — check is lib-only and never compiles test targets, so broken/stale tests stay hidden), `vitest` for the frontend, and `pytest` for the Python sidecars (`jiminy-bridge`, `jiminy-vision`).

### Pre-release security audit (local-only)

Before every release, run the adversarial red-team workflow:
`Workflow({ name: "pre-release-security-audit", args: { version: "X.Y.Z" } })`. It
static-audits, fuzz/adversarial-tests, dependency-CVE-audits, and sandbox-attacks the whole
app across **19 dimensions** — the original 9 (`crypto`, `injection`, `gating`, `ipc`, `pii`,
`p2p`, `comms`, `web`, `supply`) plus 10 folded in after the v0.0.7 audit (`dos`, `wasm`,
`voice`, `sessionlog`, `android`, `sidecar`, `installer`, `sidechannel`, `atrest`,
`modeltrust`) — adversarially verifies each finding to drop false positives, and emits a
severity-ranked report + release verdict (find-only — it never edits source). Pass
`args.dims: [keys]` to run a focused subset (e.g. just the new attack classes), or
`args.depth` (`smoke`/`static`/`full`). The workflow script and its reports live under
`.claude/` and are **gitignored — never published to the github mirror**; see
`.claude/workflows/README.md` for the playbook.

**Pre-release checklist — bump every version number.** Before tagging a release,
verify all version strings are updated to the release version, not just the git
tag / release notes. They drift silently: as of v0.0.9 work, `crates/sovereign-app/Cargo.toml`
still read `version = "0.0.6"` (the app prints it while compiling — misleading).
Check at least: each crate's `Cargo.toml` `version`, the Tauri config
(`crates/sovereign-app/tauri.conf.json`), and any hard-coded version in the shell
status bar / about panel. (This note lives here because the canonical checklist is
the NAS-only `.claude/workflows/README.md`; mirror it there when on-LAN.)

### WSL2 / Linux
```bash
cargo test -j 4
```

### Windows (from PowerShell / cmd)
```powershell
# All crates except sovereign-ai (which defaults to CUDA)
_build.bat test -j 2

# sovereign-ai specifically (skip CUDA)
_build.bat test -p sovereign-ai --no-default-features -j 2

# Integration test (builds + runs the binary as subprocess)
_build.bat test -p sovereign-app --test cli_integration -j 2
```

### Windows (from bash — Claude Code / Git Bash)
Set the full environment from the bash section above, then:
```bash
# sovereign-ai (skip CUDA)
cargo.exe test -p sovereign-ai --no-default-features -j 2

# Single crate
cargo.exe test -p sovereign-skills -j 2

# All crates except sovereign-ai
cargo.exe test -j 2
```

### Key gotchas
- **In-memory SurrealDB instances are isolated.** Each `create_db()` with memory mode creates a fresh DB. Tests requiring state across function calls must share a single DB instance or use persistent mode.
- **TOML backslash escaping on Windows.** When writing Windows paths into TOML strings, replace `\` with `/` — otherwise `\U`, `\t`, etc. are misinterpreted as escape sequences and config silently falls back to defaults.
- **Integration tests use persistent temp DBs** since each subprocess gets its own in-memory DB.

## User Confirmation Required
- When a problem can be solved either by installing a missing system package or by changing the code, **ask the user** which approach they prefer before proceeding
- Never run `sudo` commands to install packages without explicit user approval

## Git Workflow

**VSCode `git.untrackedChanges` is set to `hidden`** because SurrealDB `.db` files (10k+) flood source control despite being in `.gitignore`. This means new files won't appear in the Source Control panel — you must `git add <file>` explicitly.

Standard `git push` / `git pull` against whichever remotes are configured locally. Machine-specific remote setup (e.g. private bare repos on a network share, drive mounts) is intentionally kept out of this file — see `CLAUDE.local.md` (gitignored) if you need to record per-machine git workflow notes.

The public mirror is on GitHub: `https://github.com/clenoble/sovereign.git`.

## Code Style
- Rust: edition 2021, prefer safe code, minimize unsafe blocks
- Keep spike code simple and focused — no over-engineering
- Comments only where logic isn't self-evident

## sovereign-app Module Structure
The binary crate (`sovereign-app`) is split into focused modules:
- `cli.rs` — Clap CLI struct and Commands enum. **Subcommand is optional** — running `sovereign-tauri` with no args defaults to `run` (launches the Tauri UI). The dev CLI subcommands (create-doc, list-threads, …) live on this `sovereign-tauri` binary; the default `sovereign` binary is the native shell (UI only).
- `commands.rs` — Async CLI handler functions (create/get/list/update/delete for docs, threads, relationships, commits, contacts, conversations)
- `tauri_commands.rs` — 40+ Tauri `invoke()` command handlers (chat, documents, threads, contacts, settings, browser, suggestions, reliability)
- `tauri_events.rs` — `OrchestratorEvent` → Tauri `emit()` bridge with typed payloads
- `browser.rs` — Embedded Tauri webview lifecycle: create, navigate, back/forward/refresh, set bounds, destroy
- `web.rs` — Web content fetching via `reqwest` + `readability` text extraction (8KB truncation for LLM, 12KB for display)
- `setup.rs` — DB creation, crypto initialization, orchestrator callback wiring
- `seed.rs` — Sample data seeding on first launch (DB data + user profile + session log history)
- `main.rs` — Entry point: CLI dispatch + Tauri bootstrap (`run_tauri`: orchestrator, crypto, P2P, voice pipeline, session-log key, idle-watcher for background memory consolidation)
