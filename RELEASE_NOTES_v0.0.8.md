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
lets you **keep** the synced version or **restore** the prior one. Forged
high-counter overwrites are rejected outright.

## Security hardening

v0.0.8 ran a full pre-release security pass. Highlights:

- **SSRF guard** — the embedded browser and web fetch can't reach loopback,
  cloud-metadata, or private/internal addresses (shared by both UIs so it can't
  drift).
- **Model trust** — unlisted / hot-swapped local models are verified
  trust-on-first-use, not loaded blindly.
- **Prompt-injection fencing** — untrusted content (synced docs, saved web pages,
  email) has every model format's reserved control tokens redacted before it can
  reach the AI.
- **Tamper-evident session log** — a truncated or forged log now **fails closed**
  instead of being silently re-anchored.
- **PII** — locale-aware redaction (incl. Swiss AVS) at both ingest and read time.
- **Runtime/installer** — DLL search-order hardening on Windows, input-size caps
  against UI-thread denial-of-service, and Android `allowBackup=false` +
  `FLAG_SECURE`, plus login-timing and action-gating fixes.
- **Dependencies** — patched the advisories flagged by `cargo-audit`
  (quinn-proto, lopdf).

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

## Install — Windows

1. Download `sovereign.exe`.
2. Run it.

The native shell runs CPU inference and needs no CUDA toolkit or runtime DLLs —
just the app and your local models. Verify your download against `SHA256SUMS`.
