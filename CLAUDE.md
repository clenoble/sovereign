# Sovereign OS — Project Instructions

## Library Version Rule
Before writing or modifying any code that uses an external library (crate, pip package, npm module, etc.):
1. Check the **current latest version** on crates.io / PyPI / npm (use WebSearch or WebFetch)
2. Fetch the **latest API documentation** for that version — do NOT rely on memorized APIs from training data
3. Verify method signatures, constructors, and imports against the actual docs before writing code
4. If docs.rs fails to build for a crate, check the project's own hosted docs or GitHub source

This is critical — APIs change between versions and stale knowledge causes cascading build failures.

## Build & Development

### WSL2 / Linux
- Source lives on NAS mount (`/mnt/nas/Current/Projets/03 - user-centered OS/`)
- Copy to WSL native filesystem (`~/`) before building for performance
- Always `rm -rf` target directory before `cp -r` (cp into existing dir nests instead of overwriting)
- Rust linker is rust-lld — be aware of `--as-needed` link ordering issues
- Limit parallel compilation to avoid OOM-crashing WSL: use `-j 4` (confirmed stable with 16 GB `.wslconfig`) or `-j 2` as fallback

### Windows (MSVC target)
- Use `build_sov.bat` wrapper — it sets `LIBCLANG_PATH`, `CMAKE`, and `PATH` automatically
- Typical build: `build_sov.bat build -p sovereign-app --target-dir Z:/cargo-target -j 2`
- `sovereign-ai` default feature is `cuda` — disable on machines without CUDA toolkit: `--no-default-features`
- C: drive is nearly full — always use `--target-dir Z:/cargo-target` to build on NAS
- Before rebuilding, kill stale processes and clean sovereign artifacts:
  ```powershell
  Get-Process -Name cargo,rustc -ErrorAction SilentlyContinue | Stop-Process -Force
  Remove-Item 'Z:\cargo-target\debug\deps\libsovereign_*' -Force -ErrorAction SilentlyContinue
  Remove-Item 'Z:\cargo-target\debug\.fingerprint\sovereign-*' -Recurse -Force -ErrorAction SilentlyContinue
  ```
- Windows needs `/FORCE:MULTIPLE` linker flag (MSVC) because `llama-cpp-sys-2` and `whisper-rs-sys` both embed ggml — this is set in `.cargo/config.toml`

## Testing

### WSL2 / Linux
```bash
cargo test -j 4
```

### Windows
```powershell
# All crates except sovereign-ai (which defaults to CUDA)
build_sov.bat test --target-dir Z:/cargo-target -j 2

# sovereign-ai specifically (skip CUDA)
build_sov.bat test -p sovereign-ai --no-default-features --target-dir Z:/cargo-target -j 2

# Integration test (builds + runs the binary as subprocess)
build_sov.bat test -p sovereign-app --test cli_integration --target-dir Z:/cargo-target -j 2
```

### Key gotchas
- **In-memory SurrealDB instances are isolated.** Each `create_db()` with memory mode creates a fresh DB. Tests requiring state across function calls must share a single DB instance or use persistent mode.
- **TOML backslash escaping on Windows.** When writing Windows paths into TOML strings, replace `\` with `/` — otherwise `\U`, `\t`, etc. are misinterpreted as escape sequences and config silently falls back to defaults.
- **Integration tests use persistent temp DBs** since each subprocess gets its own in-memory DB.

## User Confirmation Required
- When a problem can be solved either by installing a missing system package or by changing the code, **ask the user** which approach they prefer before proceeding
- Never run `sudo` commands to install packages without explicit user approval

## Git & NAS Push/Merge Workflow

The bare repo lives on the NAS. From WSL:

```bash
# 1. Mount the NAS (if not already mounted — requires sudo, ask user)
sudo mount -t drvfs 'Z:' /mnt/nas

# 2. Ensure git trusts the NAS path (one-time)
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS'
git config --global --add safe.directory '/mnt/nas/03 - user-centered OS/.git'

# 3. Set remote to WSL-accessible path (if still set to Z:\)
git remote set-url origin '/mnt/nas/03 - user-centered OS'

# 4. Push
git push origin main
```

The remote URL is stored as a WSL path (`/mnt/nas/03 - user-centered OS`), not the Windows `Z:\` path, because WSL git cannot resolve Windows drive letters.

To pull updates back to the working copy after editing on the NAS side:
```bash
git pull origin main
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
- `seed.rs` — Sample data seeding on first launch
- `main.rs` — Entry point: CLI dispatch + GUI bootstrap (`run_gui`)
