# 02 — Architecture

The system as built: the crates and what each owns, the binaries they produce, the platforms
they run on, how the user interface reaches the backend, the sidecars, and the build features
that decide what actually ships.

*Consolidates:* specification v1.3 §System Architecture Overview and Open Question 2 (tech
stack); `CLAUDE.md` (workspace architecture, sovereign-app modules); `CONTRIBUTING.md`
(architecture overview, data flow, feature table); `README.md`; `RELEASE_NOTES_v0.0.3`–`v0.0.9`;
archived `native-ui-migration.md`, `native-ui-migration-decisions.md` (D1, D2, Phase 4).

---

## Layers

**Status:** ✅ Built — the layer stack below is what the code does (code: `crates/*`).

```
┌─────────────────────────────────────────────────────────────────────┐
│ USER INTERFACE                                                        │
│  native shell (desktop)          │  Tauri + Svelte (mobile UI;        │
│  vello / parley / winit / wgpu   │  desktop frozen)                   │
│  in-process calls                │  Tauri IPC (invoke / emit)         │
├─────────────────────────────────────────────────────────────────────┤
│ AI ORCHESTRATOR  (sovereign-ai)                                       │
│  router 3–4B (resident once loaded) · reasoning 7B (on demand)        │
│  intent classification · agent loop + tools · action gate · trust    │
│  injection fencing · session log · consolidation · reliability       │
├─────────────────────────────────────────────────────────────────────┤
│ SKILLS (sovereign-skills)     │ COMMS (sovereign-comms, dormant)     │
│ 24 core skills · WASM sandbox │ IMPORT (sovereign-import)            │
├─────────────────────────────────────────────────────────────────────┤
│ LOCAL GRAPH  (sovereign-db)                                           │
│  embedded SurrealDB · EncryptedGraphDB field-encryption decorator     │
│  documents · threads · relationships · contacts · messages · commits │
├─────────────────────────────────────────────────────────────────────┤
│ KEYS & CRYPTO (sovereign-crypto)  │ SYNC & RECOVERY (sovereign-p2p)    │
│  key hierarchy · AEAD · Shamir    │  libp2p · pairing · sync · guardians│
└─────────────────────────────────────────────────────────────────────┘
        shared types, config, security primitives: sovereign-core
```

The orchestrator does not sit *between* the UI and the store on every path: the UI reads and
writes the graph directly for ordinary work (open, edit, move, pin), and goes through the
orchestrator for language requests and AI-proposed actions. Every AI-proposed write passes the
action gate (ch. 08).

*Replaces v1.x:* "Local graph database (JSON)" and "skill registry with native processes" —
the graph is embedded SurrealDB; skills are compiled Rust plus a WASM Component Model sandbox
(ch. 11).

## Crates

**Status:** ✅ Built — 13 workspace members, about 90k lines of Rust (measured 2026-10-09).

| Crate | Role |
|---|---|
| `sovereign-core` | Shared types, config, interfaces (`OrchestratorEvent`), user profile, security primitives (`ActionLevel`, bubble state), network guard. |
| `sovereign-db` | Graph store on embedded SurrealDB — documents, threads, relationships, contacts, conversations, messages, commits, PII records. `EncryptedGraphDB` decorates the raw store with field-level encryption (ch. 07). |
| `sovereign-crypto` | Key hierarchy, XChaCha20-Poly1305, Argon2id, Shamir (Recovery Key, roster, seal, store), content signing (ch. 06, ch. 07). |
| `sovereign-ai` | Orchestrator: router and reasoning models via llama.cpp, intent classification, chat agent loop and tools, action gate, trust, injection fencing, PII handling, session log, consolidation, reliability, voice, sidecar clients (ch. 08). |
| `sovereign-skills` | Skill registry, the 24 core skills, the WASM sandbox (ch. 11). |
| `sovereign-p2p` | libp2p node: pairing, encrypted sync, peer review, guardian enrollment, access-recovery driver, guardian-side release gate (ch. 05, ch. 06). |
| `sovereign-comms` | Email (IMAP/SMTP), Signal, WhatsApp types — dormant since v0.0.9 (ch. 13). |
| `sovereign-import` | Stub-first bulk import engine and its CLI (ch. 03). |
| `sovereign-docs` | ○ The document service (contract C-DOC, ch. 03): the one door for creating, reading, saving, deleting, restoring and purging documents. Contract and test suite only (2026-10-10); not built. |
| `sovereign-docformat` | ○ The stored document format v2 (C-ENC, ch. 03), with no I/O, so the editor can use it without storage crates. API only (2026-10-10); the editor's provisional model moves here when built. |
| `sovereign-relay` | Deployable relay and seed node with a store-and-forward mailbox; sees only ciphertext and routing metadata (ch. 05). |
| `sovereign-guardian` | Guardian-side engine: holds friends' Recovery Key shares, answers heartbeats, approves recovery (ch. 06). |
| `sovereign-guardian-app` | The friend-side desktop application (Tauri) wrapping `sovereign-guardian` (ch. 06). |
| `sovereign-app` | Tauri host and developer CLI. Builds the `sovereign_app` library used by the mobile app, and the `sovereign-tauri` desktop binary (frozen, not shipped; kept for the developer CLI). |
| `sovereign-shell` | The native desktop UI. Builds the `sovereign` binary — the desktop application (ch. 12). |

Plus two Python sidecars outside the workspace (below), and the Svelte frontend in
`frontend/`.

*Replaces v1.x:* the 10-crate workspace with the Iced-based `sovereign-ui` and
`sovereign-canvas` crates. Both were retired in v0.0.3; the crates added since are
`sovereign-shell`, `sovereign-import`, `sovereign-relay`, `sovereign-guardian`,
`sovereign-guardian-app`.

## Binaries and user interfaces

**Status:** ✅ Built — one shipped desktop binary (the native shell), one mobile app; the
Tauri desktop binary still builds but is frozen (code: `crates/sovereign-shell`,
`crates/sovereign-app`, `frontend/`).

| Artifact | Built from | UI stack | Platform | Role |
|---|---|---|---|---|
| `sovereign` | `sovereign-shell` | vello 0.9 + parley 0.10 + winit 0.30 + wgpu, own widget layer; `wry` child webview for the browser | Windows, Linux (desktop) | **The desktop UI.** UI only — no CLI. |
| `sovereign-tauri` | `sovereign-app` | Tauri 2 + Svelte 5 / SvelteKit 2 | Desktop | **Frozen 2026-10-09, not shipped** (see below). Still builds: it carries the developer CLI subcommands (no subcommand = launch the UI). |
| Android app | `sovereign-app` (`sovereign_app` cdylib) + `frontend/` | Tauri 2 mobile + Svelte (touch layout) | Android | **The mobile UI.** |
| `sovereign-guardian-app` | `sovereign-guardian-app` | Tauri desktop | Desktop | The guardian's application (ch. 06). |
| `sovereign-relay` | `sovereign-relay` | none (service) | Linux server | Public relay / seed node (ch. 05). |

The shell and the frozen Tauri desktop binary are separate executables over the same backend
crates (they were already separate crates when the shell was added). The desktop store
(RocksDB) is single-process exclusive, so two processes cannot open the same workspace at once.
Data directory: the shell opens `data/sovereign.db` under the user's Sovereign directory
(`~/.sovereign` unless `SOVEREIGN_DATA_DIR` is set); the Tauri host — and therefore the
developer CLI, including bulk import — resolves a relative `[database] path` against the same
directory, and the default config path is the same `data/sovereign.db`. With the default
configuration both write the same store; a non-default `[database] path` makes the CLI write
elsewhere. On Android the mobile entry point sets `SOVEREIGN_DATA_DIR` to the app's sandbox.

*Replaces v1.x:* "UI framework: Iced 0.14" — the Iced UI was retired in v0.0.3 (Svelte + Tauri
became the sole frontend), and the native shell became the default desktop UI in v0.0.8. The
reason for the native shell: remove the web engine as the entire UI layer and remove the IPC +
serialization boundary.

### The native shell calls the backend in-process

**Status:** ◐ Partial — in-process calls built (code: `crates/sovereign-shell/src/app.rs`); not
every orchestrator event is consumed yet.

The shell links the backend crates and calls them as functions: no `invoke()`, no JSON
marshalling, no IPC surface, no content-security policy. The orchestrator's
`OrchestratorEvent` channel, which the Tauri host forwards with `emit()`, is drained directly
by the shell's event loop. The shell installs encryption the same way the Tauri host does:
after login it opens the persona's store and wraps it in `EncryptedGraphDB`, and fails closed
if that cannot be done (ch. 07).

◐ The shell does not yet consume every orchestrator event. Dropped today: suggestions and
link suggestions, sync conflicts, bubble state, reliability results, thread merge and split,
skill results, milestone events and recovery progress events, among others. Sync conflicts are
a correctness matter, not a display one. The work is tracked in the backlog.

### Tauri on desktop is frozen (decision 2026-10-09)

**Status:** ✅ Decided (Céline, 2026-10-09) — the native shell is the only desktop UI.

- **`sovereign-tauri` is no longer shipped on desktop** and receives no new desktop features.
  This formalises the release practice: the v0.0.9 desktop assets were already the native shell
  only (`sovereign.exe` for Windows, `cargo build -p sovereign-shell` for Linux), plus the
  Android APK.
- **Tauri remains the host of the mobile app and of the guardian app.** The Svelte frontend
  therefore stays maintained, for mobile.
- **Desktop parity with Tauri is no longer a goal.** The shell is the reference for desktop
  behaviour; where the shell lacks something Tauri had, it is a gap in the shell, measured
  against this specification, not against the Tauri UI.
- Reason: two desktop UIs over the same backend kept diverging, and several divergences were
  defects (ch. 03 content encoding and delete semantics, a security fix present in one UI and
  missing in the other). The structural cause is that part of the behaviour lives in the host
  layers rather than in shared crates; moving it is open work (backlog), to be specified before
  it is done.
- ◐ **Consequence to settle:** the developer CLI — including bulk import (ch. 03), which has no
  UI yet — lives in the `sovereign-tauri` binary, which was not part of the v0.0.9 release
  assets either. A shipped import path (a CLI binary of its own, or the shell's import window)
  is open.

### The Tauri host

**Status:** ✅ Built (code: `crates/sovereign-app/src/`) — mobile and CLI; frozen on desktop.

`sovereign-app` exposes the backend to the Svelte frontend as Tauri commands
(`tauri_commands/`) and forwards orchestrator events (`tauri_events.rs`). The embedded browser
is a Tauri child webview (`browser.rs`). The same crate holds the setup path (store creation,
crypto initialisation, orchestrator wiring) and sample-data seeding.

## Platforms and storage engines

**Status:** ✅ Built (code: `crates/sovereign-db/src/surreal.rs`).

| Platform | UI | Persistent engine | Notes |
|---|---|---|---|
| Windows | native shell | RocksDB | Daily-driver target. |
| Linux | native shell | RocksDB | The `wry` webview uses the system WebKitGTK on Linux. |
| Android | Tauri mobile | SurrealKV | RocksDB has no clean Android cross-compile; SurrealKV is pure Rust. The `mobile` feature set has no voice, email or embedded browser. |
| iOS | — | — | ○ Feature set declared (`mobile`), not built or tested. |

When both engines are compiled in, RocksDB wins. With neither, the store falls back to
in-memory and logs a warning; no shipped build is configured that way.

◐ The Android build has no release recipe in the repository: `cargo tauri android build` with
the `mobile` feature set (and default features off) appears only in a development note of May
2026, as a debug build. Which features the v0.0.9 APK was built with is not recorded; SurrealKV
on Android follows from the `mobile` feature set, not from a recorded release build. A release
recipe for the shell and the APK is open work (backlog).

## Data flow of a request

**Status:** ✅ Built (code: `crates/sovereign-ai/src/orchestrator.rs`).

1. The user types in the chat (or speaks, where voice is enabled), or submits a query from
   the mobile Search panel.
2. The UI calls the orchestrator — in-process in the shell, through a Tauri command on mobile.
   Chat (`handle_chat`) and a submitted search (`handle_query`) share one path: chat delegates
   to `handle_query`.
   In the native shell only chat goes through the orchestrator; its Search window filters
   document titles locally. On mobile, typing in Search queries the store directly; submitting
   the query sends it to the orchestrator.
3. The intent classifier (router model, heuristic parser first) picks an intent; the action
   gate checks its level; the action is executed or proposed for confirmation.
4. Chat intents run the agent loop: build prompt → generate → parse tool calls → execute →
   feed back, up to five rounds.
5. Results come back as `OrchestratorEvent`s and update the UI.
6. When the user is idle, background consolidation proposes links between documents (ch. 03).
   ◐ Not in the native shell, which does not run consolidation.

Details: ch. 08.

## Sidecars

**Status:** ◐ Partial — both exist and are optional; neither is started by the application,
and no shipped build can reach them.

| Sidecar | Language / stack | Default endpoint | Purpose |
|---|---|---|---|
| `jiminy-bridge` | Python, FastAPI | `127.0.0.1:9100` | Bridge to a Reachy Mini robot (simulated or real): maps orchestrator events to head pose, antennas, emotions and speech; speech-to-text listening and text-to-speech. |
| `jiminy-vision` | Python, own virtual environment (torch, MediaPipe, SmolVLM2) | `127.0.0.1:9101` | Always-on gesture detection and an opt-in, time-windowed scene description fed to the orchestrator as context. |

Both run as separate local processes with their own dependencies, so heavy ML stacks stay out
of the Rust binaries. Requests to them carry a shared bearer token when one is configured, and
the bridge rejects browser-originated requests. They are reached only through the Tauri
host's `jiminy` and `vision` features, which are in no shipped build: the Tauri desktop binary
is frozen, the `mobile` feature set does not include them, and the native shell has no sidecar
client. If a sidecar is unreachable the client logs once and degrades silently to "not
present".

## Build features — what ships

**Status:** ✅ Built (code: `crates/sovereign-app/Cargo.toml`, `crates/sovereign-shell/Cargo.toml`,
`_release_build.bat`).

### Tauri host (`sovereign-app`)

**Status:** ✅ Built (code: `crates/sovereign-app/Cargo.toml`).

| Feature | Default | Effect |
|---|---|---|
| `encryption` | on | Field-level encryption, key store, auth (ch. 07). |
| `encrypted-log` | on | Session log sealed at rest (ch. 08). |
| `rocksdb` | on | Desktop persistent engine. |
| `surrealkv` | off | Mobile persistent engine. |
| `cuda` | off | GPU inference through llama.cpp's CUDA backend. |
| `p2p` | off | libp2p node: pairing, sync, recovery (ch. 05, ch. 06). |
| `comms-email` / `comms-signal` / `comms-whatsapp` | off | Communication channels (ch. 13). |
| `web-browse` | off | Web fetch, readability extraction and reliability assessment for the embedded browser (ch. 10). |
| `wasm-plugins` | off | WASM skill sandbox (ch. 11). |
| `voice-stt` / `wake-word` | off | Whisper speech-to-text; Rustpotter wake word (ch. 08). |
| `jiminy` / `vision` | off | Sidecar clients (above). |
| `haptics` | off | Mobile haptics plugin. |
| `mobile` | off | `encrypted-log`, `encryption`, `haptics`, `surrealkv`, `p2p` — the Android feature set. |

`sovereign-ai` defaults to `cuda`, but every workspace member pulls it with
`default-features = false`, so a build is CPU-only unless `cuda` is named.

**Feature set in `_release_build.bat`:** `cuda,p2p,comms-email,web-browse`, plus the
defaults. That script builds the Tauri desktop binary, which is frozen and not shipped; it is
stale as a release recipe. A `cuda` executable needs the three CUDA runtime DLLs
beside it, or on `PATH`; only an NVIDIA driver is required on the user's machine.

### Native shell (`sovereign-shell`)

**Status:** ◐ Partial — CPU-only, email switched off, no voice/vision/GPU (code:
`crates/sovereign-shell/Cargo.toml`, `src/comms.rs`).

The shell has no features of its own: it always builds with RocksDB, field encryption, the
encrypted session log, the p2p node, the 24 core skills, the `wry` browser and the email
engine compiled in. It is **CPU-only** (no `cuda`), so `sovereign.exe` ships as a single
executable with no CUDA runtime.

- **Email is compiled in but switched off** in v0.0.9 by a constant in the shell's comms
  module; Settings → Comms reads "not in this release". The Tauri host never constructs an
  email channel, so a Tauri build with `comms-email` keeps email dormant too.
- ◐ Voice, vision, the sidecars, WASM plugins and GPU inference are not available in the
  shell; they are open work (backlog), not exclusions. Until the shell gains a `cuda` build,
  the desktop UI runs both models on the CPU, which is the main gap between what ships and the
  hardware target of ch. 14.

The v0.0.9 release assets (checked 2026-10-09): `sovereign.exe` (native shell, Windows),
`sovereign-linux-x86_64` (native shell), `sovereign-v0.0.9.apk`, `SHA256SUMS`. No Tauri desktop
build and no CUDA build were shipped.

## Developer and verification hooks

**Status:** ✅ Built — headless render (code: `crates/sovereign-shell/src/main.rs`,
`app.rs`); development switches compiled only into debug builds or with the `dev-switches`
feature.

The shell renders one frame headlessly to a PNG (`SHELL_SHOT`), with switches to open a given
window, wizard step or menu, so every screen can be checked without a desktop session; the
embedded webview is outside that render. The switches exist only in debug builds or with
`--features dev-switches`; a release build ignores them (code: `crates/sovereign-shell/src/dev.rs`).

Requirement: development switches are not available in release builds. ✅ Built (lot 0).
