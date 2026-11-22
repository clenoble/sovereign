# 15 — Open Questions

Questions the specification cannot answer yet. Each one names what it blocks. Conflicts between
the specification, the code and the binding documents are listed separately in
`CHANGELOG.md` ("Open conflicts"); they are decisions too, but of a different kind: two
answers already exist and disagree.

*Consolidates:* specification v1.3 Open Questions (1–9) and §4 "Not settled"; decisions pending
recorded on 2026-08-04 and 2026-10-09.

---

## D1 — A gravity axis for power over a person

**Status:** Open — deferred by Céline on 2026-08-04, deliberately.

Action Gravity's five levels grade power over *data*, with friction scaling with
irreversibility (ch. 08). Shaping attention is a different axis, not a sixth level. Céline's
framing: recommendation engines optimise to *retain* attention; here the aim is to
*concentrate* it on what deserves it — and that raises another risk, just as large. Spotter's
attention allocation is the first use case. **Blocks:** any attention-shaping task hosted by
Sovereign (ch. 04).

## D2 — Host divergence versus the intimacy of what differs

**Status:** Open.

The challenge cursor is stored in Sovereign, not in the browser extension, while the stated
aim is a minimal gap between hosts. The widest gap therefore falls on the most intimate
artifact. Declaring the regime makes it legible; it does not remove it. **Blocks:** nothing
yet; it qualifies ch. 04.

## D3 — The `Document` ownership model: target or stale?

**Status:** ✅ Decided 2026-10-10 (Céline) — option (c): **two separate questions.** *Trust*
(did I write this, or did it come from outside?) is built now as a three-state field —
owned / external / undetermined, reversible both ways — with a simple provenance record (ch. 04).
*Ownership between people* (the v1 multi-party model) is kept as intent for a future
collaboration chapter, never mixed with trust again. The paragraph below is the question as it
stood.

The v1 schema describes `creator`, `current_owner`, `admins`, `contributors`,
`provenance_chain`; the shipped `Document` has a single `is_owned: bool` (ch. 03). Federation
and multi-user sync are on the roadmap, so the richer model may be the target rather than a
description that fell behind. **Blocks:** the undetermined-ownership tri-state (ch. 04),
because "undetermined" means something different under a boolean than under a provenance chain.

## D4 — What the PII sweep does

**Status:** ✅ Decided 2026-10-10 (Céline) — see ch. 09, *Idle sweep*. The paragraph below is the
question as it stood.

**Decided 2026-10-10 (Céline): detect without rewriting, review in a queue, mask on the fly
until reviewed.**
- A background scan **detects and inventories** personal data in every document not yet
  scanned (proposed records), **without rewriting the document**, and marks the document as
  analysed — so it can be summarised and read by the model.
- **Until the user has reviewed a document's findings, masking happens on the fly**: when the
  model reads the document, the detected values are replaced by their type labels at read time.
- Findings wait in a **review queue that is surfaced regularly** to the user (cadence and form to
  define). Reviewing confirms or dismisses; a confirmed value is then tokenized in the stored
  body (the raw original kept encrypted, as today).
- Nothing in a document is rewritten without the user's confirmation.

The idle sweep that rescans stored content for personal data was unwired in May 2026 and never
rewired. Céline, 2026-10-09: the question is not only re-enabling it but what is done with what
it finds, and it belongs with the browser decision (credentials taken from the PII manager).

## D5 — The desktop shell's undecided UI choices

**Status:** Open — listed 2026-10-17 by the parity triage between the native shell and the frozen
Tauri desktop. No decision exists for any of them, and none has a priority. Collected here on
2026-11-04, so that each is decided before a lot builds near it.

1. **Rendered markdown** in the document window and in the chat, and a **format toolbar**. The
   editor edits markdown source, and today its marks stay visible, dimmed (ch. 12).
2. **Select or open on a click**, and **bringing a card into view** from search and from the
   taskbar: the "go to" half of the earlier two-step model has no replacement.
3. **Taskbar launchers** for personal data, the browser, devices, alerts and guardians; and what
   **Ctrl+N** and **Ctrl+B** do.
4. **A document the assistant creates opening by itself.**
5. **Sync on returning to the window**, since the shell has no periodic poll.
6. **A preview of a version before restoring it.**
7. **Whether the sync alerts replace the "edited on both devices" list**, or the shell must also
   handle `SyncConflict` (ch. 05).
8. **Voice and vision in the shell at all**, and in what order relative to the wake-word
   dependency's precondition (ch. 14).
9. **Small ones:**
   - on-screen zoom buttons;
   - message times;
   - an unread dot on the inbox;
   - confirmation and refresh when a model is deleted;
   - copying the device id;
   - a camera scan when joining;
   - skills filtered by document type.
10. **Where a skill's file output goes:** a save dialog, a document, or a folder. Needed before the
    exports can work (ch. 11).
11. **Multi-line chat messages** (Shift+Enter, a multi-line composer): left as a question for Céline,
    for later, when the composer became a real text field (2026-10-24; ch. 12).

**Blocks:** each one blocks only its own feature. Rows 3 and 4 touch the email work of v0.0.14 (an
inbox launcher, a mail the assistant drafts).

## Roadmap and release bar

**Status:** Partly settled.

- **v0.1 bar** (decided 2026-07-06): *data you can trust + daily driver* on desktop. The
  acceptance test — Céline lives in it for a week; whatever stops daily use defines the v0.1
  backlog — has not been run.
- **v0.0.10** is the stabilisation release that follows the 2026-10-09 audit (decided
  2026-10-10). **v0.0.11** is the editor release, and **v0.0.12** the corrective release that
  answers the cloud review of v0.0.11 (published 2026-11-04).
- **v0.0.13, then v0.0.14:** two releases, one after the other (Céline, 2026-11-13: « et oui pour
  les 2 releases successives »). Until that day, v0.0.13 was to carry everything:
  - Céline, 2026-10-30: messaging is what blocks daily use; email only, Signal and WhatsApp later.
  - Céline, 2026-11-01: « on fait une grosse v0.0.13 », nothing cut.
- **v0.0.13 is the consolidation release.** It carries:
  - C-HOST whole (ch. 08; 2026-11-04);
  - the fixes chosen from the cloud review of v0.0.12;
  - the promised items: a merged thread kept, and the session log's sandbox;
  - the first part of the personal-data sweep, on documents (ch. 09).
- **v0.0.14 is the email release.** Its email is read-only, fetched by each device, with no write
  to the server (Céline, 2026-11-13; ch. 13). It is born behind the network gate (ch. 02). It also
  carries the sweep's second part, on messages. Sending comes later.
- **The week of trial (v0.1's acceptance test)** follows the email release, since mail is what it
  is for.
- **Phase 2** is crowd data backup (ch. 06).
- **Community contribution model**: skills grow from spec-as-seeds (ch. 11). Release timeline
  (alpha, beta, stable): not set.

## Resolved since v1.3

- *Tech stack* (v1.3 Q2 said Iced 0.14): native shell on vello/parley on desktop, Tauri/Svelte on mobile — ch. 02.
- *Testing* (v1.3 Q7 said 367 tests, no CI): about 3,000 Rust tests (test functions in the
  crates' sources, counted 2026-11-13) and a GitHub CI exist; `cargo test --workspace` is the
  gate. It did not compile from June to October 2026 without anyone noticing; since 2026-10-09
  `_check.bat` checks test code too (`--all-targets`). GitHub CI reports only on the mirror,
  after a push. It runs every crate's tests (`sovereign-ai`'s own tests without its default
  features, so without llama.cpp; the workspace run still compiles llama.cpp through the shell), and checks the shell and the APK's `mobile` feature set as they ship; it
  builds no release binary, so it is not a release gate by itself.
- *Bootstrap* (v1.3 Q6): 24 core skills and stub-first bulk import — ch. 11, ch. 03.
- *Spatial navigation and timeline* (v1.3 Q3, Q5): ch. 12; branching is not built (ch. 03).
