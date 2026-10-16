# Releasing Sovereign GE

How the published artifacts are built and checked. The maintainers' publish procedure
(tagging, the public mirror, the GitHub Release) is kept separately; this document covers
what goes into a release and how to reproduce each artifact from source.

## What ships

| Asset | Platform | Built from | Recipe |
|---|---|---|---|
| `sovereign.exe` | Windows x86_64 | `sovereign-shell` (the native desktop UI) | `_release_build.bat` |
| `sovereign-linux-x86_64` | Linux x86_64 | `sovereign-shell` | `scripts/release-shell-linux.sh` |
| `sovereign-<tag>.apk` | Android, arm64 (aarch64) only | `sovereign-app` (`sovereign_app` library) + `frontend/`, Tauri mobile | `scripts/build-android.sh` |
| `SHA256SUMS` | — | the three files above | see [Checksums](#checksums) |

Both desktop artifacts are the **native shell**, built **CPU-only**: the shell has no `cuda`
feature today (it uses `sovereign-ai` with `default-features = false`), so the executables
need no CUDA runtime. A GPU build of the shell would first need a `cuda` feature in
`crates/sovereign-shell`.

**Not shipped:**

- **The Tauri desktop binary** (`sovereign-tauri`) has been frozen since 2026-10-09. It still
  builds (`_release_tauri_frozen.bat`, for developers), but it is not a release asset. Tauri
  remains the Android host.
- **The import CLI** (`sovereign-tauri import <folder> [--apply]`) lives in that frozen binary.
  Build from source: `cargo build -p sovereign-app --release`.
- **The guardian app** (`crates/sovereign-guardian-app`) is not distributed as a binary.
  Build from source.
- **The relay node** (`crates/sovereign-relay`) is server software, built from source by
  whoever runs one.

## Prerequisites

**Windows:** see `CLAUDE.md` → *Toolchain Prerequisites (Windows)* — Visual Studio 2022 Build
Tools (C++), LLVM, CMake, Rust (stable, MSVC), Node.js 20+. The scripts read the same variables
as the other `_*.bat` wrappers (`SOVEREIGN_LLVM_DIR`, `SOVEREIGN_CMAKE_DIR`,
`SOVEREIGN_TARGET_DIR`) plus `SOVEREIGN_CLANG_INCLUDE` and `SOVEREIGN_JOBS`; `_env.bat`
documents them. The shell needs the WebView2 runtime at run time (built into current Windows).

**Linux (Debian/Ubuntu package names; the CI installs the same list):**

```bash
sudo apt-get install -y build-essential pkg-config cmake clang libclang-dev \
  libssl-dev libdbus-1-dev zlib1g-dev libfontconfig1-dev libfreetype-dev libx11-dev libxdo-dev \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

WebKitGTK 4.1 (the shell's embedded browser) and GTK 3 are then also needed at run time.

## Building each artifact

### Windows shell — `sovereign.exe`

```cmd
_release_build.bat
```

Runs `cargo build -p sovereign-shell --release --locked`, copies the result to
`%SOVEREIGN_TARGET_DIR%\dist\sovereign.exe` (override with `SOVEREIGN_DIST_DIR`), and writes
`sovereign.exe.sha256` beside it. `--locked` builds exactly the committed `Cargo.lock`, the one
the gate audits. The shell is built on its own (`-p sovereign-shell`) so its features resolve
as they ship, without unification with the other workspace members.

### Linux shell — `sovereign-linux-x86_64`

```bash
scripts/release-shell-linux.sh
# equivalent to: cargo build -p sovereign-shell --release --locked
#                cp target/release/sovereign sovereign-linux-x86_64
```

Output in `$CARGO_TARGET_DIR/dist` (default `target/dist`), with its `.sha256`. On WSL with
the sources on a network mount, copy the tree to the WSL filesystem first.

### Android — the APK

Build on Linux or WSL2. The repository's Gradle scaffold (`crates/sovereign-app/gen/android`)
is customised to call `cargo tauri` on Linux/macOS; its Windows branch is not maintained.

Toolchain:

- JDK 17 (`JAVA_HOME`)
- Android SDK (`ANDROID_HOME`) with the platform matching `compileSdk` in
  `crates/sovereign-app/gen/android/app/build.gradle.kts` (36 at the time of writing) and its
  build tools
- Android NDK (`NDK_HOME`). The last recorded working build used NDK 26.1.10909125.
  `llama-cpp-sys-2` reads `ANDROID_NDK_ROOT`; the script sets it from `NDK_HOME`.
- Rust target: `rustup target add aarch64-linux-android`
- Tauri CLI 2.x: `cargo install tauri-cli --version '^2' --locked`
- Node.js (the Tauri build runs the frontend build through `scripts/tauri-build-frontend.cjs`)

```bash
scripts/build-android.sh --debug     # what has been published so far
scripts/build-android.sh --release   # unsigned, see below
```

Both run, from `crates/sovereign-app`:

```bash
cargo tauri android build [--debug] --target aarch64 --features mobile -- --no-default-features
```

`--features mobile` selects the Android feature set (`encrypted-log`, `encryption`, `haptics`,
`surrealkv`, `p2p`). `-- --no-default-features` is essential: the Tauri CLI has no flag for it,
and without it the desktop default `rocksdb` is pulled into the Android build and fails.

Gradle writes the APK under
`crates/sovereign-app/gen/android/app/build/outputs/apk/universal/<debug|release>/`; the
script copies it to the `dist` directory.

**Signing.** A `--debug` build is signed with the Android SDK's debug key, and the APKs
published so far were debug builds. The Gradle scaffold declares no release signing
configuration and no release key is kept in the repository, so a `--release` build comes out
**unsigned**: it has to be signed (`apksigner`) with a key held outside the repository before it
can be installed, and its checksum taken **after** signing. Release signing is an open item.

A debug APK keeps the native libraries' debug symbols and is large (hundreds of MB); keep it
under GitHub's 2 GiB per-asset limit.

## Checksums

Each recipe writes `<asset>.sha256` in `sha256sum` format (except for an unsigned release APK,
which is hashed after signing). With the assets renamed to their
published names and gathered in one directory:

```bash
sha256sum sovereign.exe sovereign-linux-x86_64 sovereign-<tag>.apk > SHA256SUMS
sha256sum -c SHA256SUMS
```

Users verify a download the same way (`sha256sum -c SHA256SUMS --ignore-missing`, or
`Get-FileHash -Algorithm SHA256 sovereign.exe` on Windows).

## The pre-publish gate

Run it on the commit to be released, before building the artifacts:

```cmd
_prepublish.bat
```
```bash
scripts/prepublish.sh
```

It stops at the first failing step:

1. `cargo check --workspace --exclude sovereign-ai --all-targets`
2. `cargo check -p sovereign-shell` — the shell with its own features, as it ships
3. `cargo check -p sovereign-app --no-default-features --features mobile` — the APK's feature
   set (host target)
4. `cargo test --workspace --exclude sovereign-ai`
5. `cargo test -p sovereign-ai --no-default-features`
6. `cargo audit` — exceptions only through `.cargo/audit.toml`
7. `npx vitest run` (frontend)
8. `npm audit --omit=dev --audit-level=high` (frontend)

All cargo steps use `--locked`. `sovereign-ai` is tested separately with
`--no-default-features` because its default feature is `cuda`.

**Advisories.** An open RustSec advisory fails the gate. It is either fixed (dependency update)
or accepted by a maintainer with a written justification next to its ID in
`.cargo/audit.toml`, saying why it is not reachable in the shipped builds and what would remove
it. Each exception is re-checked at every release.

Also before a release: bump every version string (each crate's `Cargo.toml`,
`crates/sovereign-app/tauri.conf.json`, any version shown in the shell), and check the release
notes against the code — a note describes what the shipped build does, not what is planned.

## Continuous integration

`.github/workflows/ci.yml` runs on the public repository after a push: the workspace checks
and tests (the gate's steps 1–5) on Linux with the packages above, `cargo audit`, the frontend
tests and `npm audit`, and the vision sidecar's tests. It complements the local gate; it does
not replace it, because it only reports once the code is already public.

## Open items

- The import CLI and the guardian app are not shipped as binaries ("build from source").
- The Windows executable is not Authenticode-signed; Windows SmartScreen may warn. Verify it
  against `SHA256SUMS`.
- The APK is not release-signed (see *Signing*).
- The APK's native library alignment for 16 KB memory pages (newer Android versions) is not
  verified.
- No GPU (CUDA) build of the shell.
