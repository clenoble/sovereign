# Sovereign OS — Project Instructions

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
| `sovereign-crypto` | Encryption (AES-256-GCM), key management, content signing |
| `sovereign-ai` | LLM orchestrator, intent classification, chat agent loop, tools, trust, voice pipeline |
| `sovereign-ui` | Iced 0.14 GUI — panels, chat, search bar, theming |
| `sovereign-canvas` | Spatial canvas — document cards, thread zones, drag-and-drop |
| `sovereign-skills` | Skill registry and built-in skills (markdown editor, PDF export, video) |
| `sovereign-p2p` | libp2p peer-to-peer sync (experimental) |
| `sovereign-comms` | Communications — email (IMAP/SMTP), Signal (via presage) |
| `sovereign-app` | Binary crate — CLI, GUI bootstrap, setup, seeding |

### AI Orchestrator Architecture (`sovereign-ai`)

The orchestrator uses local Qwen2.5 models via llama-cpp-2 (no cloud, no external servers):
- **Router (3B)**: Fast intent classification — classifies user input into ~20 action types
- **Reasoning (7B)**: Escalation model for complex/ambiguous queries (loaded on demand, unloaded after 5min idle)
- **Chat agent loop**: Multi-turn with tool calling — loads session history, gathers workspace context, iterates up to 5 rounds of generate → tool call → execute → feed back
- **6 read-only tools**: `search_documents`, `list_threads`, `get_document`, `list_documents`, `search_messages`, `list_contacts` — all Observe level (Level 0), no confirmation needed
- **Prompt format**: ChatML (`<|im_start|>role\n...\n<|im_end|>`)
- **Tool call format**: `<tool_call>{"name":"...","arguments":{...}}</tool_call>` — learned via few-shot

Key modules: `intent/` (classifier + parser), `llm/` (backend, prompts, context), `orchestrator.rs`, `tools.rs`, `action_gate.rs`, `trust.rs`, `injection.rs`, `session_log.rs`, `voice/`

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

## Build & Development

### Toolchain Prerequisites (Windows)

Install via `winget` if missing:
- **Visual Studio 2022 Build Tools** with C++ workload: `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
- **LLVM** (for `libclang.dll` needed by bindgen): `winget install LLVM.LLVM`
- **CMake**: `winget install Kitware.CMake`
- **Rust** (stable MSVC): `winget install Rustlang.Rustup`

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
- `cli.rs` — Clap CLI struct and Commands enum
- `commands.rs` — Async CLI handler functions (create/get/list/update/delete for docs, threads, relationships, commits, contacts, conversations)
- `setup.rs` — DB creation, crypto initialization, orchestrator callback wiring
- `seed.rs` — Sample data seeding on first launch (DB data + user profile + session log history)
- `main.rs` — Entry point: CLI dispatch + GUI bootstrap (`run_gui`)
