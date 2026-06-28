# Sovereign GE v0.0.8 — Native desktop UI

Sovereign GE keeps your documents, contacts, messages, and an on-device AI on
your own machine — local-first, no cloud. v0.0.8 replaces the desktop web UI with
a **native Rust interface**, adds a **guided first-run**, makes **multi-device
pairing and sync** work end-to-end, lets you **review changes a paired device
makes**, and lands a broad **security hardening** pass.

## A native desktop UI

The default desktop app is now a **native shell** built in Rust (Vello + winit +
wgpu) that calls the backend in-process — no web view, no IPC layer. It's leaner,
starts faster, and ships as a single self-contained `sovereign.exe` (CPU
inference — no CUDA runtime to bundle).

It reproduces the full workspace:

- **Spatial canvas** with a **calibrated timeline** — documents sit at their real
  time; the ruler adapts as you zoom (minutes/hours up close, days at mid-zoom,
  months/years far out), with a live "now" line. Press `f` to fit everything in
  view, `h` to snap back to now.
- **Contact-first inbox** — your messages organized by *person*, not by app. Open
  a contact to see their addresses, a tab per channel (email / Signal / …), and
  one unified conversation thread.
- **Tabbed settings** — Profile, AI, Security, Trust, Comms, Devices, Vision.
- **Document windows, embedded browser, devices & sync, an AI chat** that opens
  next to the orchestrator bubble, and a **`+` button** to create a lane or
  document.
- **Light & dark themes** and selectable orchestrator-bubble styles.

The previous Svelte + Tauri frontend is kept as a build option on desktop and
remains the **mobile** UI.

## Guided onboarding

First launch now walks you through an **8-step wizard** instead of a single
password box:

1. **Welcome** — your device designation, and a choice to set up fresh or pair
   with a device you already use.
2. **Name your AI**, **3. pick a bubble style**, **4. choose a theme**.
5. **Sample data** — optionally seed an example workspace (documents, threads,
   contacts) so the canvas isn't empty while you explore.
6. **Password** with a live strength meter, **7. an optional duress password**
   (opens a decoy workspace under coercion), and **8. an optional canary phrase**
   (a personal phrase shown after login so you'd notice tampering).

Paired devices skip seeding — they start empty and pull your real data via sync.

## Device pairing & sync

- Pair a phone (or second computer) by scanning a QR + code. The joining device
  derives its identity with **Argon2id** and imports the source's account key
  over an encrypted handshake.
- Discovery is robust: the joiner finds its peer over **mDNS** *and* the offer's
  addresses, so a stale/unreachable address self-heals instead of timing out, and
  the new device **auto-syncs** immediately after pairing.
- Synced documents now carry their **thread membership**, so they land in their
  real lane (with a clear **"Unfiled"** fallback if membership can't be resolved).

## Review changes from a paired device

Peer sync is now **non-destructive**: when a paired device overwrites a document
or record, the prior value is preserved and the change is **flagged for review**
rather than silently applied. A **review panel** (in both the native shell — press
`r` — and the mobile/Tauri UI) lists pending changes with a risk assessment, and
lets you **keep** the synced version or **restore** the prior one. Tampered or
out-of-order overwrites from a misbehaving device are rejected automatically.

## Security hardening

v0.0.8 went through a full pre-release security review. Without listing specifics,
this pass:

- strengthened how the AI handles untrusted and external content (synced
  documents, saved web pages, email);
- hardened the embedded browser and web fetch against reaching internal network
  resources;
- tightened verification of local AI models before they're loaded;
- improved the integrity guarantees of the on-device activity log;
- broadened privacy (PII) redaction, including locale-aware handling;
- landed platform-level hardening on Windows and Android, and updated
  dependencies.

## Other fixes

- New documents open straight into **edit mode**, and the editor is readable on
  the light theme (was dark-on-dark).
- Sample data seeds **only when you opt in** during onboarding — no more
  auto-seeding on a fresh/paired device.
- Auth: real login errors surfaced (not always "Invalid password"), password
  trimmed before use, and the canvas never flashes before auth resolves.

## Known issues

- **Pair and sync over Wi-Fi.** P2P sync over **cellular / mobile data** is
  currently unstable (frequent drops); on Wi-Fi it's reliable.
- Voice and the keystroke-dynamics enrollment step are not enabled by default.

## Install

**Windows** — download `sovereign.exe` and run it. CPU inference; no CUDA toolkit or runtime DLLs needed.

**Linux (x86-64)** — download `sovereign-linux-x86_64`, `chmod +x sovereign-linux-x86_64`, then run it. Needs a Vulkan-capable GPU driver.

**Android (arm64)** — download and install `sovereign-v0.0.8.apk` (allow "install from unknown sources"). Debug-signed.

Verify your downloads against `SHA256SUMS`.
