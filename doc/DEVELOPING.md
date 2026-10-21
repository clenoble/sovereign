# Developing Sovereign GE

How to build, run and test the workspace. What the system is and does is in `doc/spec/`
(start at `doc/spec/README.md`); how releases are built and checked is in `doc/RELEASE.md`.

## Toolchain (Windows)

Install with `winget` if missing:

- Visual Studio 2022 Build Tools, C++ workload:
  `winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`
- LLVM (`libclang.dll`, for bindgen): `winget install LLVM.LLVM`
- CMake: `winget install Kitware.CMake`
- Rust (stable, MSVC): `winget install Rustlang.Rustup`
- Node.js 20+ (frontend): `winget install OpenJS.NodeJS.LTS`
- CUDA Toolkit, only for `--features cuda`: `winget install Nvidia.CUDA`. It sets `CUDA_PATH`
  machine-wide but not in shells already open: restart them.

**CUDA 13 runtime DLLs.** `cudart64_13.dll`, `cublas64_13.dll` and `cublasLt64_13.dll` live in
`<CUDA_PATH>\bin\x64\`, not `\bin\`, and the installer does not put `\bin\x64` on `PATH`. A
`--features cuda` build then links but fails at start with `STATUS_DLL_NOT_FOUND`. Prepend both
directories for builds and tests, and ship the three DLLs next to a CUDA executable. The default
shell (`sovereign.exe`) is CPU-only and needs none of them.

## Building

### Windows wrappers (PowerShell or cmd)

`_build.bat <cargo args>`, `_check.bat` (workspace check `--all-targets`), `_run.bat` (the
shell). They read:

| Variable | Default | Purpose |
|---|---|---|
| `SOVEREIGN_LLVM_DIR` | `C:\Program Files\LLVM\bin` | directory holding `libclang.dll` |
| `SOVEREIGN_CMAKE_DIR` | `C:\Program Files\CMake\bin` | directory holding `cmake.exe` |
| `SOVEREIGN_TARGET_DIR` | cargo's `./target` (`C:\cargo-target` in the release scripts) | build output |

The release scripts share `_env.bat`, which adds `SOVEREIGN_CLANG_INCLUDE` (clang's resource
include directory, for rocksdb's bindgen; auto-detected, `none` to disable) and `SOVEREIGN_JOBS`.

### From Git Bash (Claude Code)

```bash
export PATH="/c/Program Files/CMake/bin:/c/Program Files/LLVM/bin:$PATH"
export LIBCLANG_PATH="C:/Program Files/LLVM/bin"
export BINDGEN_EXTRA_CLANG_ARGS="-IC:/PROGRA~1/LLVM/lib/clang/<version>/include"   # rocksdb's bindgen
export CARGO_TARGET_DIR="C:/cargo-target"                                         # optional
```

The 8.3 short path (`PROGRA~1`) avoids the space splitting the argument. If linking fails with
`link: extra operand`, Git Bash's own `/usr/bin/link.exe` is shadowing the MSVC linker: prepend
the MSVC `bin/Hostx64/x64` directory to `PATH` (and set `LIB` / `INCLUDE` for the Windows SDK).

### WSL / Linux

Build on the native filesystem, not a network mount. Use `-j 4` (or `-j 2`) so WSL does not run
out of memory. `rm -rf` a target directory before copying into it.

### Feature notes

- **`sovereign-ai` defaults to `cuda`.** Without a CUDA toolkit build it with
  `--no-default-features`. It is excluded as a *selected package* from workspace commands, but
  its library is still compiled through the workspace pin `default-features = false`.
- **Why workspace checks are `--workspace`, not a crate list.** Until 2026-07-16 the gate
  checked one crate, and the default desktop binary stopped compiling for a whole feature
  without anyone noticing. A list fails silent when a crate is added; `--workspace` fails safe.
- Windows links with `/FORCE:MULTIPLE` (`.cargo/config.toml`): `llama-cpp-sys-2` and
  `whisper-rs-sys` both embed ggml.
- After a failed build, stale `cargo` / `rustc` processes can hold files: stop them, then remove
  `target/debug/deps/libsovereign_*` and `target/debug/.fingerprint/sovereign-*`.

### Tauri (frozen on desktop, the Android app)

- `devUrl` decides at **compile** time, the **Tauri CLI** at **run** time. With a `devUrl` in
  `tauri.conf.json`, `cargo check` / `cargo build` compile without the frontend built. But a
  binary built by plain `cargo`, `--release` included, loads the `devUrl` (`localhost:1420`) at
  run time and shows "can't reach this page" without a dev server. Only a Tauri-CLI build
  (`frontend/node_modules/.bin/tauri.cmd build --no-bundle`) serves the embedded frontend.
  `_dev.bat` runs the CLI's `dev` with Vite.
- Launched in the background from Bash, `sovereign-tauri.exe` reports exit code 0 at once while
  its window keeps running. Check with `tasklist`.
- Frontend: `cd frontend && npm install && npm run build`.

## Testing

Run the full suites before calling anything green. `cargo check` never compiles test targets.

```bash
cargo test --workspace --exclude sovereign-ai --locked -j 4
cargo test -p sovereign-ai --no-default-features --features encrypted-log --locked -j 4
cargo test -p sovereign-db --features encryption --locked -j 4
cargo check -p sovereign-app --no-default-features --features mobile --locked -j 4   # the APK's feature set
cd frontend && npx vitest run
```

- Without `--features encrypted-log`, the session-log tests are silently skipped; without
  `--features encryption`, the encrypted-store tests are.
- **Point `SOVEREIGN_DATA_DIR` at a temporary directory for test runs**, so nothing can reach
  a real workspace.
- In-memory SurrealDB instances are isolated: each `create_db()` in memory mode is a fresh
  database. Integration tests that run the binary as a subprocess use persistent temporary
  databases.
- Windows paths written into TOML strings: use `/`, or `\U`, `\t`… become escapes and the config
  silently falls back to defaults.
- The local pre-publish gate is `_prepublish.bat` (`scripts/prepublish.sh` on Linux); see
  `doc/RELEASE.md`.

## Editor and Git notes

- VS Code's `git.untrackedChanges` is `hidden` for this repository (SurrealDB files flood the
  panel): new files do not show up in Source Control, so `git add` them explicitly.
- Per-machine Git and remote setup stays out of the repository (`CLAUDE.local.md`,
  gitignored).
