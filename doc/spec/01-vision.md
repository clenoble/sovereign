# 01 — Vision

What Sovereign GE is for, and the tenets every other chapter has to answer to. A design
choice that contradicts a tenet here is either a bug or a decision that has to be taken
explicitly (and recorded in `CHANGELOG.md`).

*Consolidates:* specification v1.3 §Vision & Philosophy (Core Tenets, Paradigm Shift);
the February 2026 design decisions (archived) (canvas, threads, halo); `README.md` (positioning);
Appendix C, `../legal/sovereign_os_ethics.md` §1.3, §3, §4 (binding constraints);
`doc/plans/archive/plans/native-ui-migration.md` (OS direction).

---

## The shift: from application-centric to content-centric

**Status:** ◐ Partial — the content model, canvas and skills exist; most work still happens
in built-in windows rather than through skills.

Software today imprisons data: a file belongs to the application that wrote it, and folders
are the application's filing system. Sovereign GE reverses the relation. **Content is the
primary citizen; software is a set of tools that operate on it.** Documents, contacts,
messages and their relationships live in one local graph owned by the user. They are placed
in time and in *intent threads* (projects, contexts), not in folders, and they are shown on a
spatial canvas where time runs left to right on the desktop and top to bottom on a phone
(ch. 12).

Sovereign GE is a local-first graphical environment — today a desktop application (the
native shell) and an Android app over the same backend, with an on-device AI, field-level
encryption and peer-to-peer sync, built in Rust and published under AGPL-3.0. It is a
prototype: the v0.1 bar is *data you can trust + daily driver on desktop* (ch. 15).

## Core tenets

**Status:** ◐ Partial — status per tenet below.

| Tenet | Statement | Status |
|---|---|---|
| **Data sovereignty** | The user owns the *Digital Master*: the local graph is the source of truth. Proprietary formats are exports, never sources. | ◐ The local graph is the store (ch. 03); the general export that makes the graph abandonable is specified, not built (ch. 04). |
| **Content first** | Data exists independently of software. Skills are called on data, not the other way around. | ◐ Documents, threads and relationships are independent records (ch. 03); 24 core skills run on documents (ch. 11). |
| **Skill-based architecture** | Monolithic applications are replaced by granular skills, orchestrated by an AI agent that proposes and the user approves. | ◐ Skills and the orchestrator exist (ch. 08, ch. 11); the orchestrator invokes few of them, and editing, inbox, browser and settings are built-in windows. |
| **Identity firewall** | Personal data and tracking are managed by the system, not left to each service. | ⚠ See below. |
| **Distributed resilience** | No reliance on a central cloud: devices sync peer to peer, and trusted people can restore access. | ◐ Device sync and guardian *access* recovery are built (ch. 05, ch. 06); crowd *data* backup is Phase 2. |
| **User sovereignty** | The user keeps ultimate control over data, identity and computing environment. The code is open and auditable. | ✅ AGPL-3.0, no telemetry, no account; no service holds the user's data in the clear (the relay sees only ciphertext, ch. 05). Friction scales with irreversibility (Action Gravity, ch. 08). |

*Replaces v1.x:* "Digital Master (Local Graph JSON)" — the graph is an embedded SurrealDB
database (RocksDB on desktop, SurrealKV on Android), not a JSON file (ch. 02, ch. 03). The
tenet is unchanged; only the storage form is.

### Identity firewall

**Status:** ⚠ Conflict — decision pending. Not built.

The v1 tenet reads: *"automated, kernel-level management of PII and cookies with synthetic
identity generation for external services."* Appendix C §3.1 is a binding constraint that
scopes the firewall to the tracking-prevention layer (cookie isolation, fingerprint and
referrer handling, tracker blocking) and forbids generating synthetic names, addresses or IDs.

> ⚠ **Conflict — decision pending (Céline).** v1 tenet "synthetic identity generation for external services" vs Appendix C §3.1 "the Firewall prevents tracking, it does not fabricate personas".

What exists today is the PII layer (detection, vault, ledger; ch. 09) and the embedded browser
with its SSRF guard (ch. 10). "Kernel-level" belongs to the OS direction below and is not a
property of the current application.

## What the tenets refuse

**Status:** ✅ Built — as constraints on what is shipped (Appendix C §3).

The tenets are bounded by Appendix C, which is binding:

- **Privacy without anonymity.** No bundled Tor, VPN or anonymising relay (Appendix C §3.2).
  Sovereign GE protects data at rest and in use; it does not hide the user's network presence.
- **Sovereignty without fabrication.** See the identity firewall above.
- **Small on-device models.** The shipped models are in the 3–8B range (ch. 14); larger models
  are a user choice, not a default.
- **No hidden flows.** No telemetry, no analytics, no cloud account. Transparency is part of
  the misuse mitigation (Appendix C §4.2).

## Design commitments that follow

**Status:** ◐ Partial — each is detailed, with its own status, in the chapter named.

1. **Local first.** Everything runs on the user's devices; network use is an explicit,
   gated action (ch. 08, ch. 10).
2. **Provenance is always visible.** Owned and external content look different everywhere —
   on the canvas, in the AI's answers, in the AI's state (the *Sovereignty Halo*, ch. 12,
   Appendix B §3). ◐ Not met on mobile: the Android canvas marks provenance with a colour bar
   only, without the shape cue (ch. 12).
3. **Hard barriers over good behaviour.** Constraints that matter are enforced by code paths,
   not by prompts (ch. 08, Appendix B §6).
4. **Surface, don't suppress.** Detected attacks, uncertain judgments and dropped items are
   shown to the user, not handled silently (ch. 04, ch. 08).
5. **Accessibility is a design constraint, not a retrofit.** WCAG 2.1 AA; provenance is
   carried by shape, not colour alone (ch. 12). ◐ Met on the desktop; on mobile provenance is
   colour plus a text label for assistive technology, which does not meet WCAG 1.4.1 visually
   (ch. 12).

## Direction: from application to environment

**Status:** ○ Specified, not built — a direction, not scheduled.

The native shell (ch. 02, ch. 12) is the first step toward an environment rather than an
application: the shell would become a Wayland compositor (Smithay) in which other programs
render as cards on the canvas. Nothing of this exists beyond the native shell itself, and no
release targets it.
