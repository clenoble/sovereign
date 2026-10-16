# Sovereign GE — Specification

**Version 2.0 — 2026-10-09.** The single source of truth for what Sovereign GE is and how it
behaves. Plans say how and when; they never redefine what. If a plan and this specification
disagree, the specification wins and the plan is wrong (see `CLAUDE.md`).

## Chapters

| # | Chapter | What it covers |
|---|---|---|
| 01 | [Vision](01-vision.md) | Philosophy and core tenets |
| 02 | [Architecture](02-architecture.md) | Crates, binaries, platforms, what ships |
| 03 | [Data model](03-data-model.md) | Storage, documents, threads, relationships, versions, consolidation, import |
| 04 | [Judgment, attention & ownership of trust](04-judgment-and-trust.md) | Assessments, attention, ownership at landing, export |
| 05 | [Sync & P2P](05-sync-p2p.md) | Transport, relay, pairing, sync protocol, peer review |
| 06 | [Recovery](06-recovery.md) | Guardian access recovery; crowd backup (Phase 2) |
| 07 | [Keys & data at rest](07-keys-and-at-rest.md) | Key hierarchy, field encryption, personas |
| 08 | [AI orchestrator](08-orchestrator.md) | Models, agent loop, Action Gravity, trust, injection defense, voice |
| 09 | [Personal data (PII)](09-pii.md) | PII by reference, vault, ledger |
| 10 | [Browser & web](10-browser-and-web.md) | Embedded browser, SSRF guard, incognito (A5), filtering proxy (O4) |
| 11 | [Skills](11-skills.md) | Core skills, WASM sandbox, contribution model |
| 12 | [User interface & canvas](12-ui.md) | Desktop (native shell) and mobile UIs, canvas, accessibility |
| 13 | [Communications](13-comms.md) | Email, Signal, WhatsApp (dormant) |
| 14 | [Hardware](14-hardware.md) | Requirements and model candidates |
| 15 | [Open questions](15-open-questions.md) | Decisions not yet taken |
| — | [Changelog](CHANGELOG.md) | Every change, newest first; open conflicts |

**Appendices**
- B — [UX principles](sovereign_os_ux_principles.md) (being realigned to the code; where they
  differ, ch. 08 and ch. 12 hold the built rules)
- C — [Ethics and binding design constraints](../legal/sovereign_os_ethics.md)
- [Writing skills](../writing-skills.md) — guide for third-party skill authors

## Reading the status lines

Every section carries one, right under its heading.

| Mark | Meaning |
|---|---|
| ✅ Built | The code does this. A pointer to the module follows. |
| ◐ Partial | Part of it is built; the line says which part. |
| ○ Specified, not built | Intended design, not in code. "Decided" when Céline settled it recently. |
| ⚠ Conflict | The specification, the code or a binding document disagree. Not resolved here: listed in the changelog and waiting for a decision. |
| ✗ Retired | No longer intended; the line says what replaced it. |

`Replaces v1.x:` under a section means the code took a different route from the earlier
specification, and the built route is now the design.

## Changing the specification

1. **Design decisions are made here first**, then built. A decision found only in a plan, a
   commit message or a coordination note is a bug in the specification.
2. Edit the chapter, update its status line, and **add an entry to `CHANGELOG.md` in the same
   commit**: date, what changed, who decided.
3. **Never resolve a conflict by writing.** Mark it `⚠` and add it to the changelog's open
   conflicts; Céline decides.
4. Write only in the chapter you are working on. Several people and agents edit this folder;
   separate files keep them from overwriting each other.
5. **This folder is published** on the public GitHub mirror. Describe requirements and status;
   never describe an unfixed security weakness in exploitable detail. Open work, including
   security findings, lives in the private backlog (`doc/plans/BACKLOG.md`, stripped from the
   mirror).

## History

v1.0–v1.3 were a single file (February–August 2026). The last one is kept unchanged in the
private archive. The changelog reconstructs what changed between them.
