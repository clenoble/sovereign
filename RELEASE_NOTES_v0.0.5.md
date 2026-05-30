# v0.0.5 — Mobile (Android) + multi-device P2P sync

This release makes Sovereign GE a **mobile-capable, multi-device** app. It
adds an Android target (Tauri 2 mobile), a touch-native mobile UI built on the
same Svelte 5 frontend, an **AccountKey** key hierarchy that separates the
user-scoped data key from per-device keys, **PIN-encrypted QR device pairing**,
and a **row-level peer-to-peer sync** protocol over libp2p that reconciles the
encrypted graph across a user's paired devices. Desktop is unchanged in
behavior; everything mobile is additive and feature-gated.

> **Build status.** The Rust workspace, the Svelte frontend, and the full
> non-CUDA test matrix are green. The Android **APK is built in a WSL2/Linux
> environment** (see *Build / packaging* below) — a clean Windows
> `cargo tauri android build` is blocked by Smart App Control on unsigned
> crate build-scripts, which is one-way to disable, so the verified mobile
> build path is Linux/WSL2 or CI.

## Highlights

- **Android target.** `cargo tauri android` scaffolding under
  `crates/sovereign-app/gen/android/` (Gradle, manifest, launcher icons,
  Kotlin plugins). `sovereign-app` now exposes a `[lib]`
  (`staticlib`/`cdylib`/`rlib`) with two entrypoints — `run()`
  (`#[cfg(mobile)]`, JNI/`mobile_entry_point`) and `run_cli()` (desktop argv
  parsing). `main.rs` is a 7-line shim into `run_cli()`. A `mobile` Cargo
  feature set excludes desktop-only deps (cpal voice, native-tls email,
  pixel-bounded child webview) and runs SurrealDB in `kv-mem`.

- **Touch-native mobile UI.** A `MobileShell` swaps in below a responsive
  breakpoint (live device detection on resize, so Chrome devtools emulation
  flips the layout). New components: re-oriented `MobileCanvas`
  (vertical = time, horizontal = thread lanes) with 2-finger pinch zoom and a
  deep-zoom density strip, `MobileChatSheet` (AI chat + voice), `MobileDocReader`,
  context-aware `Fab`, `BottomSheet` (detents + fling), `LaneSwitcherSheet`,
  `MobileTaskbar`, and `SharePickerSheet` for inbound share-sheet content.
  Existing overlay panels (Settings, Contact, Inbox, Model, Onboarding, …) got
  a responsive pass.

- **Mobile primitives.** Haptics (`@tauri-apps/plugin-haptics` + a JS wrapper),
  voice capture in the chat sheet, and an Android share-sheet receiver
  (`SharePlugin.kt`) that hands shared text/URLs to `SharePickerSheet`.

- **AccountKey key hierarchy.** A user-scoped `AccountKey` (the data-encryption
  key) is now separated from per-device keys. New devices receive the
  *account* key via pairing rather than re-deriving a device-local key, so all
  paired devices decrypt the same graph. v0.0.4 stores that pre-date the
  account key fall back to the master-derived key (see migration below).

- **Device pairing (PIN-encrypted QR).** The host device shows a QR encoding a
  `PairPayload` (account key + peer id), encrypted under a short PIN. The new
  device scans it (`QrScanner.svelte` + `jsqr`, camera permission gated),
  enters the PIN, and imports the account key. Pairing payloads are
  versioned, expiring, and AEAD-authenticated — wrong PIN, expired, tampered,
  and schema-mismatch are all rejected (121 crypto tests cover these).

- **Row-level P2P sync.** `sovereign-p2p` gained a multi-table sync manifest and
  a bidirectional row-level protocol with **last-writer-wins** reconciliation
  over libp2p (mDNS discovery + noise + yamux/quic). `SyncService` does
  per-row get/apply across threads, entities, PII records, share records, and
  documents (documents use the commit chain). Auto-sync triggers on peer
  discovery and on app foreground (60s cooldown), with a connectivity gate and
  exponential backoff for Android.

- **Sync UI.** A taskbar sync indicator, a **Devices** section in Settings, and
  a paired-device path in onboarding (`PairQrPanel`). New `device` and `sync`
  stores back the indicators.

## Under the hood

- **AccountKey migration (v0.0.4 → v0.0.5).** On first unlock of a pre-account-key
  store, `account_key_migration` re-encrypts existing rows from the old
  per-device key to the new account key. Best-effort and idempotent: a marker
  file prevents re-runs and rows that fail to decrypt under the old key are
  skipped rather than aborting the migration.

- **Auth invariant.** `authenticate()` derives device + master keys, probes each
  persona (primary / duress), and resolves the account key from the persona's
  `wrapped_account_key` when present (paired devices keep their imported key),
  falling back to a master-derived key otherwise.

- **DB feature-gating.** RocksDB is gated behind a `rocksdb` feature (desktop
  default on, mobile off → `kv-mem`). Sync row encode/decode and the
  get/apply trait methods landed in `traits`/`surreal`/`mock`/`encrypted`.

## Tests

Verified green on this machine (non-CUDA matrix):

- **Frontend (vitest):** 249 tests across 13 files — adds the mobile suites
  (`MobileChatSheet`, `canvas.mobile`, `device`, `longPress`) on top of the
  v0.0.4 set.
- **sovereign-crypto:** 121 (account key, auth, PIN/QR pair payload round-trip
  + rejection paths, vault, password-gen).
- **sovereign-p2p:** 50, including the end-to-end `e2e_sync` reconciliation test.
- **sovereign-core:** 46 · **sovereign-db:** 74.
- A `_test.bat` wrapper and `_dev.bat` (Tauri dev) were added.

`sovereign-ai`, `sovereign-comms`, and the `sovereign-app` integration tests
require the CUDA/CMake/NDK toolchain and are validated in the WSL2/CI build.

## Bug fixes in this release

- **Mobile build break:** `pii_sweep` constructed the contact hook with an
  undefined `device_key` (a leftover from the device_key→account_key rename).
  It is a hard compile error under `comms + encryption` — which the `mobile`
  feature set enables — and only escaped CI because the desktop `default`
  build omits `comms`. Now passes `account_key`.
- **Frontend listener leak:** the `+layout` and `+page` route components
  returned a cleanup from an **async** `onMount`, which Svelte ignores (it's a
  Promise, not a teardown fn), so keydown/visibility/Tauri-event/timer
  listeners never detached on unmount. Teardown moved to `onDestroy`.
- **svelte-check hygiene:** fixed a possibly-null narrowing in
  `SharePickerSheet`, added the required `tabindex` on the autofill dialog, and
  removed a stale-prop-capture in `VaultAddDialog`.

## Known limitations (tracked for v0.0.6)

- Mobile storage is `kv-mem` (process-lifetime only); persistent
  SQLite-backed SurrealDB on Android is planned next.
- Sync reconciles non-document tables with last-writer-wins but **entity LWW
  updates and non-document ID reconciliation** are deferred to v0.0.6
  (documented in `sync_service`); document branch-ancestry detection
  (`is_ancestor`) is a conservative stub.
- Android keychain integration for key storage is not yet wired (keys live in
  the app's data dir for now).
- **Signal channel (`comms-signal`) is not included in mobile builds.** Presage's
  `Manager::receive_messages()` returns a `!Send` stream (holds `ThreadRng`),
  which conflicts with `CommunicationChannel`'s `Send + Sync` `#[async_trait]`
  contract. A `LocalSet`-backed wrapper that runs presage on a single thread
  and proxies commands over an mpsc channel is needed before re-enabling it.
  Tracked for v0.0.6. The `signal.rs` channel code was updated to the current
  presage 0.7.0 API in this release as preparation.

## Build / packaging

- **Desktop** is unchanged: `_release_build.bat` with the same feature set as
  v0.0.4 (`cuda,encryption,p2p,comms-email,web-browse`); CUDA 13 runtime DLLs
  still need to be on PATH or bundled next to the exe.
- **Android (WSL2/Linux):** install the Android SDK + NDK, the
  `aarch64-linux-android` / `armv7-linux-androideabi` Rust targets, a JDK, and
  the Tauri CLI; the frontend is resolved cwd-agnostically by
  `scripts/tauri-build-frontend.cjs` via `tauri.android.conf.json`. Then
  `cargo tauri android build`. See `CLAUDE.md` for the full toolchain and the
  Smart App Control rationale for not building on Windows.
