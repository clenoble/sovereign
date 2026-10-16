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

## Roadmap and release bar

**Status:** Partly settled.

- **v0.1 bar** (decided 2026-07-06): *data you can trust + daily driver* on desktop. The
  acceptance test — Céline lives in it for a week; whatever stops daily use defines the v0.1
  backlog — has not been run.
- **v0.0.10** is the stabilisation release that follows the 2026-10-09 audit (decided
  2026-10-10). The comms release — email together with its deferred hardening, Signal after
  (ch. 13) — moves to a later release, not yet numbered.
- **Phase 2** is crowd data backup (ch. 06).
- **Community contribution model**: skills grow from spec-as-seeds (ch. 11). Release timeline
  (alpha, beta, stable): not set.

## Resolved since v1.3

- *Tech stack* (v1.3 Q2 said Iced 0.14): native shell on vello/parley on desktop, Tauri/Svelte on mobile — ch. 02.
- *Testing* (v1.3 Q7 said 367 tests, no CI): about 1,200 Rust tests and a GitHub CI exist;
  `cargo test --workspace` is the gate. It did not compile from June to October 2026
  without anyone noticing; since 2026-10-09 `_check.bat` checks test code too
  (`--all-targets`). GitHub CI reports only on the mirror, after a push, and tests 3 of the 13
  crates (it builds neither shipped binary), so it is not a release gate by itself.
- *Bootstrap* (v1.3 Q6): 24 core skills and stub-first bulk import — ch. 11, ch. 03.
- *Spatial navigation and timeline* (v1.3 Q3, Q5): ch. 12; branching is not built (ch. 03).
