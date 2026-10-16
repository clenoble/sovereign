# Specification changelog

*Newest first. One entry per change to the specification's substance: date, version (if any),
what changed, and who decided. Every change to a chapter in this folder adds an entry here in
the same commit.*

---

## 2026-10-10 — v2.0.13: tokens resolve only where they belong

Decided by Céline, on her question of what a hand-typed `[pii:` actually risks: the protection
against forged tokens moves from the editor to the resolver. A token resolves only in a document
listed among its record's sources; elsewhere it is ordinary text (ch. 09, *Where a token resolves*;
ch. 12, g7). The editor stores what is typed exactly, with no escape character. The real risk was
never typing (ids cannot be guessed, and the user can reveal their own data anyway) but content
written by the assistant, an import or a peer naming a real id — paths the editor escape did not
cover. Also allowed: the font library's read-only file mapping (`memmap2` via `fontique`) in the
editor's dependency check.
Clipboard payload (ch. 12): a paste uses Sovereign's token payload only if this process wrote it
and it is still the current clipboard item — the last path by which another application could
forge a link.

## 2026-10-10 — v2.0.12: the editor contract after its test suite

The test suite for C-TEXT, written before the editor, found weaknesses in the contract. Decided by
Céline:
- **Bidi arrows** move in visual order, as parley does; selections stay logical ranges (ch. 12, g5).
- **Figures for "usable"**: 1 MB = 1,000,000 bytes of text; keystroke painted within 16 ms (p95);
  open with first paint within 1 s (ch. 12, g9).
- **Losing editing to another device**: read-only, and typing there shows a warning rather than
  disappearing; true of every read-only state (ch. 12).
- **Tokens cannot be forged**: typed or pasted token syntax stays plain text; copy-paste inside
  Sovereign keeps the token (ch. 12, g7).
- **Clipboard**: no automatic clearing of ordinary text; a revealed personal-data value is cleared
  after 30 s (ch. 12, consistent with ch. 09).
- **Crash messages**: the shell drops panic payloads so no document text reaches stderr or logs
  (ch. 12).
Aligned without a decision: the token form in ch. 03 now matches ch. 09 and the code
(`[pii:<record_id>]`); v1 media migrate with `alt` pending and the hash computed by the document
service (ch. 03).

## 2026-10-10 — v2.0.11: taskbar, cloud fallback, tables

Decided by Céline:
- **Taskbar** (closes conflict 10, ch. 12): pinned **and** recent documents.
- **Cloud fallback** (closes conflict 14, ch. 14): abandoned. The optional cloud API listed in
  ch. 08 is marked dropped with it.
- **Tables in the v2 document format** (ch. 03): included in v2 only if cells hold typed values —
  numbers as numbers, with calculations possible; otherwise later.

## 2026-10-10 — v2.0.10: the editor's behaviours, settings, zoom

Decided by Céline:
- **Editor** (ch. 12, C-TEXT): documents up to 1 MB of text for now (to be raised later); one
  device edits a document at a time — opening it on a second device offers [Switch editing to this
  device] / [Read only] / [Cancel], with the non-destructive sync review as the offline fallback;
  the clipboard holds one item, is not cleared automatically, and Sovereign's copies are excluded
  from clipboard history, cloud clipboard and monitoring where the platform allows; automatic
  saving with versions; markdown source in text blocks.
- **Settings** (closes conflict 11): editable in the native shell, as in the Svelte UI.
- **Zoom** (closes conflict 12): wheel scrolls, wheel + Shift or Space zooms, cards uncapped for
  now; the modifier is to settle when built (Shift+wheel usually scrolls horizontally).
- **Appendix B** (closes conflict 13): where a newer decision differs from Appendix B, the newer
  decision holds; the appendix is to be realigned.

## 2026-10-10 — v2.0.9: document format v2 and the editor contract (drafts for review)

- **Document format v2 — blocks** (ch. 03): envelope `{"format":"sovereign.doc","v":2,"blocks"}`,
  block `type` + stable `id`, unknown blocks preserved exactly, stable serialisation, block types
  `text` (markdown source), `image`, `video`, `table`; media reference by path + hash; migration
  from the v1 envelope and from raw text.
- **The document editor — contract (C-TEXT)** (ch. 12): what the host gives, ten guarantees (each a
  test), and four behaviours proposed for decision (remote change while editing, clipboard,
  saving, text model).
- Both are drafts: the points marked **[choice]** await Céline before the test suite is written.

## 2026-10-10 — v2.0.8: the document editor

- **K2 decided (Céline):** an in-house editor on parley, rendered by vello, no webview — after a
  spike measured what isolating a webview editor would take. Built tests-first: the suite that
  can break it is written before it, by a different author, and asserts invariants only (ch. 12).

## 2026-10-10 — v2.0.7: document format and persona layout

- **Document format** (closes conflict 7): `content` becomes a versioned JSON document of typed
  blocks (`format: sovereign.doc`, `v: 2`); unknown block types are preserved; the current
  envelope and raw-text bodies migrate to it; the tolerant read is a bridge until then (ch. 03).
- **Persona layout** (ATREST-011): one directory per persona under a neutral name, and at least
  two or three persona directories always exist, activated or not; an unused slot must not be
  distinguishable on disk from one in use; orchestrator state moves into the persona directory
  (ch. 07).
- **Editor decision (K2)**: a spike checks whether a document webview can be fully isolated from
  the browser (own profile and processes, no network, one host-controlled channel) before it is
  decided.

## 2026-10-10 — v2.0.6: the PII sweep (D4)

- **D4 decided (Céline):** a background scan detects and inventories personal data without
  rewriting documents; until a document's findings are reviewed, values are masked on the fly
  when the model reads it; findings wait in a review queue surfaced regularly; a confirmed value
  is then tokenized in the stored body. Nothing is rewritten without confirmation (ch. 09,
  ch. 15). Unblocks "summarize" on documents that were never scanned, once built.

## 2026-10-10 — v2.0.5: trust and ownership separated (D3)

- **D3 decided (Céline), option (c):** *trust* (owned / external / undetermined, reversible both
  ways, undetermined treated as external) with a simple provenance record is built now (ch. 04);
  *ownership between people* (the v1 multi-party model) becomes a future collaboration chapter,
  kept separate (ch. 03, ch. 15). Unblocks the import trust field, ownership across sync, and an
  undoable adopt.
- **The halo follows the trust state**, with a third visual for undetermined (ch. 12). Céline
  proposed a grey parallelogram; the spec records it with a dashed outline and a "?" mark so the
  difference does not rest on colour alone — exact form to validate.

## 2026-10-10 — v2.0.4: six more decisions

Decided by Céline:
- **Confirmation** (closes conflict 9): in the chat; a modal only for the highest severity;
  silence counts as consent only for easily reversible low-level actions, and the orchestrator
  announces it — "I'm doing this unless you tell me otherwise" (ch. 08). Precised: the silence
  window lasts until the user's next action anywhere in Sovereign; Modify is the reversible
  level, provided each Modify action really has an undo (adopt, first commits and thread
  restore are gaps to close).
- **Sync wire**: evolves by added fields with defaults, not a protocol bump (ch. 05).
- **Recovery start** (closes conflict 5): allowed once 3 of 5 guardians are enrolled (ch. 06).
- **No AI on mobile for now**; llama.cpp leaves the Android build (ch. 14).
- **Guardian app** (closes conflict 17): mobile-first; port it to Android (ch. 06).
- **Next release**: v0.0.10 is the stabilisation release after the audit; the comms release
  moves to a later release, not numbered yet (ch. 13, ch. 15).
- **Public credit**: AI co-authorship stays visible, including in the squashed commits of the
  public mirror.

## 2026-10-10 — v2.0.3: three decisions

Decided by Céline:
- **Delete** (closes conflict 8): soft delete everywhere, with a trash the user can empty at
  any time; otherwise purged after 30 days. No separate permanent delete (ch. 03).
- **Session log on failure**: when the encrypted log cannot be opened or verified, logging stops
  and the user is told; nothing is ever written in clear; chat keeps working (ch. 08).
- **Lanes**: one lane per thread, no cap, vertical scroll; then search and filters; then
  low-zoom grouping or a heat map with details on hover (ch. 12).

## 2026-10-09 — v2.0.2: second-position review applied

- Seven independent reviewers checked every chapter against the shipped code — the desktop shell
  and the mobile app — and against each other (274 sections). Their corrections are applied:
  about 40 status lines changed (mostly ✅ → ◐ where a mechanism was built only in the frozen
  Tauri desktop, or only on one surface), body text corrected where it was false, and every
  passage that still treated Tauri desktop as a supported UI rewritten.
- **Ch. 12 now describes the shipped mobile UI** (time vertical, lanes paged by swipe, no decks,
  document reader, chat sheet) beside the desktop shell, and is renamed accordingly.
- Resolved from code: the mobile build has no embedded browser and runs no on-device model; the
  Tauri host never creates an email channel; with the default configuration the import CLI writes
  into the same store as the shell; the shell's sync has no periodic poll.
- Four open conflicts added (15–18). Conflict 5 restated without implementation detail.
- `doc/writing-skills.md`: corrected where it described WASM discovery, loading and approval as
  working; a note at the top says no shipped build loads WASM skills yet.
- As before, defects that matter for security are stated as requirements with a status; their
  detail is in the private backlog.

## 2026-10-09 — v2.0.1: Tauri on desktop frozen

- **Decision (Céline):** the native shell is the only desktop UI. `sovereign-tauri` is no
  longer shipped on desktop and gets no new desktop features; Tauri stays the host of the
  mobile app and the guardian app (ch. 02). The v0.0.9 desktop assets were already shell-only,
  so this formalises the practice and ends desktop parity as a goal.
- **Effect on the open conflicts below:** 10, 11 and 12 no longer compare two UIs; each now
  compares the shell with the earlier design intent (taskbar *recent* documents; editable
  settings; wheel/zoom mapping), and stays open on that basis. 7 and 8 stay open as written
  — the content encoding and delete semantics are also used by mobile and import.
- **Found while recording it:** the developer CLI, which is the only way to run bulk import,
  lives in the Tauri binary and was not shipped in v0.0.9 either (ch. 02).

## 2026-10-09 — v2.0: consolidation into a chapter collection

The single-file specification (v1.3, 1600 lines) is replaced by a collection of chapters
(`README.md` is the index), and checked section by section against the code at v0.0.9.

- **Status on every section** (✅ built · ◐ partial · ○ specified, not built · ⚠ conflict ·
  ✗ retired). Before this, the file described a v1.0 design and the code had moved on in
  about a third of it, silently.
- **Brought in, from code and archived plans** — about twenty shipped mechanisms that the
  specification never described: stub-first import, the PII-by-reference design, duress
  persona, native shell and the two-binary model, onboarding, Action Gravity 1–5 and trust
  calibration, the user-decided injection gate, relay + mailbox, guardian app, pairing
  protocol, peer review of synced writes, session-log hash chain, model integrity, the SSRF
  guard, the at-rest model with OS full-disk encryption as a requirement, consolidation, the
  WASM skill sandbox, the orchestrator designation.
- **Corrected to match the built design** (each marked `Replaces v1.x`): 2D time × lane canvas
  (was 3D / semantic); native shell + Tauri (was Iced); Noise + last-writer-wins + peer review
  (was TLS + git-like merge); WASM Component Model (was Landlock / Unix-socket IPC / Python
  SDK); Rustpotter (was Porcupine); Action Gravity numbered 1–5 (was "Level 0" in places);
  24 core skills (was 8); comms dormant (appendix said implemented).
- **Decided 2026-10-09, not built** (Céline): incognito embedded browser with sign-in from
  the PII manager (A5); a filtering local proxy for the browser (O4); the PII sweep to be
  specified with A5 (ch. 09–10).
- **Removed from the spec folder** (moved to the private archive, `doc/plans/archive/spec/`):
  `User-Focused OS Specification.md`, `requirements.csv.txt`,
  `sovereign_os_model_user_system_instructions.md` — superseded drafts. The one live element
  of the first, the orchestrator's designation, is now in ch. 08.
- **Kept as appendices**: UX principles (`sovereign_os_ux_principles.md`, being realigned to
  the code — ch. 08 holds the built rules), ethics and binding constraints
  (`../legal/sovereign_os_ethics.md`), skill author guide (`../writing-skills.md`).
- The v1.3 single file is kept, unchanged, as `doc/plans/archive/spec/sovereign_os_specification-v1.3.md`.

*Asked by Céline (2026-10-09); written with Claude Opus 5.5 and five chapter-drafting agents
(Anthropic); the audit record is kept privately.*

### Open conflicts after v2.0 — decisions pending with Céline

Marked `⚠` in the chapters; listed here so none is lost. The consolidation did not resolve any
of them.

*Security and recovery*

1. **Identity Firewall** (ch. 01, 10; pointer in 09) — v1 design: kernel-level proxy with
   synthetic identities per domain. Appendix C §3.1: tracking prevention only, a Trusted Domain
   List, no fabricated personas. Nothing is built; the PII manager covers part of the intent.
2. **One backup tag instead of two** (ch. 06) — the spec requires independent `T_g`
   (guardians) and `T_f` (fragments); the code derives one tag for both. Matters once crowd
   backup (Phase 2) is live.
3. **Legacy data-key backup path** (ch. 06) — commands that make paired devices guardians of a
   backup key, with a majority threshold, are still registered; the spec says guardians hold
   the Recovery Key only and the threshold is never reduced. Keep, or remove?
4. **Owner cancellation of a takeover** (ch. 06) — the spec says the owner's login cancels a
   concurrent takeover; the code cancels a recovery running on the same device only.
5. ~~**Arming at five**~~ — **decided 2026-10-10**: recovery may start once 3 of 5 guardians
   are enrolled; five stays the roster size and the recommendation (ch. 06).
6. **Appendix C §3.3** (ch. 06) — "no time-delayed release" vs the 72 h delay (which still
   requires active approval: possibly a wording conflict); "no messaging channel" vs the
   mailbox and heartbeat (substance).

*Data*

7. ~~**Document `content` encoding**~~ — **decided 2026-10-10**: a versioned JSON document of
   typed blocks; both current forms migrate to it (ch. 03).
8. ~~**Delete: soft or final**~~ — **decided 2026-10-10**: soft delete everywhere, with a
   trash the user can empty (ch. 03).

*Interface*

9. ~~**Confirmation**~~ — **decided 2026-10-10**: confirm in the chat; a modal only for the
   highest severity; silence = consent only for easily reversible low-level actions, announced
   ("I'm doing this unless you tell me otherwise") (ch. 08). The level mapping is to specify.
10. ~~**Taskbar documents**~~ — **decided 2026-10-10**: both, pinned and recent (ch. 12).
11. ~~**Settings**~~ — **decided 2026-10-10**: editable in the native shell, as in the Svelte UI (ch. 12).
12. ~~**Zoom input and card cap**~~ — **decided 2026-10-10**: wheel scrolls, wheel + modifier zooms, cards uncapped for now (ch. 12).
13. ~~**Appendix B drift**~~ — **decided 2026-10-10**: newer decisions overrule Appendix B; the appendix is to be realigned (ch. 12).
14. ~~**Cloud fallback on weak hardware**~~ — **decided 2026-10-10**: abandoned (ch. 14; the
    optional cloud API of ch. 08 is dropped with it).
15. **Appendix C §3.4 and the session log** (ch. 08) — App C requires an append-only filesystem
    flag (retired in favour of the hash chain) and lets the user delete the log all-or-nothing,
    while the integrity anchor refuses to append after a whole-log wipe.
16. **Appendix C §3.3 rule 6** (ch. 06) — the guardian app should store, approve and delete a
    shard; the delete duty is not built.
17. ~~**Guardian app platform**~~ — **decided 2026-10-10**: mobile-first; port the guardian
    app to Android (ch. 06).
18. **Provenance across sync** (ch. 05) — ch. 04 requires a document's provenance to survive;
    synced documents currently arrive owned on the receiving device.

*Pending decisions that are not conflicts* (two answers do not exist yet): D1–D4 in ch. 15.

---

## Before v2.0 — reconstructed from git history

Reconstructed on 2026-10-09 from `git log --follow` and a read of each diff. Entries marked
*(inferred)* have no commit of their own. Three substantive changes in July carried no
version number at the time; they are listed with the date they landed.

| Date | Version | Commit | Change |
|---|---|---|---|
| 2026-08-04 | 1.3 | `00ab8c8` | Added *Judgment, Attention & Ownership of Trust* (now ch. 04): judgments as `Assessment` records, `degraded` declared by the judge, consumer-driven caches, attention entries and user signals, the méta-miroir, ownership undetermined at landing, the general export; Open Question 9. Settled across Sovereign, Spotter and Crabe, arbitrated by Céline. The header was not updated (still read 1.2). |
| 2026-07-16 | — | `184db2b` | The KEK becomes the root of the at-rest content chain; key DB and Index Key sealed under the KEK, with migration; recovery must verify before it commits. Triggered by the first live recovery run. |
| 2026-07-10 | — | `33eebbf` | Identity proof resolved: out-of-band human proof per owner↔guardian pair (Céline), replacing a pre-shared recovery passphrase. |
| 2026-07-10 | — | `32159db` | Guardian recovery made canonical: two features (access recovery now, crowd backup in Phase 2), a dedicated Recovery Key wrapping KEK + AccountKey, separate `T_g` / `T_f` tags, recovery flow rewritten, owner can invalidate a takeover. Made after plans had silently redefined the design (see `CLAUDE.md`). |
| 2026-02-27 | — | `ab6b6ab` | Architecture diagram: "3D navigation" → "2D infinite canvas". Archive banners added to the two older drafts. |
| 2026-02-25 | — | `3c9c1de` | Renamed Sovereign OS → Sovereign GE. |
| 2026-02-22 | — | `7215637` | Moved from the repo root to `doc/spec/`. |
| 2026-02-15 | 1.2 | `94b4a25` | Working-prototype status: SurrealDB storage, relationship types aligned to code, skill "Current Implementation" vs "Target Architecture", Open Questions 1, 2, 7 resolved, appendix replaced by a status table. (Dated "February 13" in the file.) |
| 2026-02-08 | — | `ca7ccce` | Model suite: Qwen2.5 3B router + 7B reasoning, whisper large-v3-turbo, native runtimes, GGUF Q4_K_M. |
| 2026-02-08 | 1.0 / 1.1 | `9e4738f` | Initial commit (header v1.0, footer v1.1). |
| 2026-02-07 | 1.1 | *(inferred)* | Security hardening: tiered skill execution, key rotation, guardian auth, data/control plane, IPC auth, session-log encryption. |
| 2026-02-06 | 1.0 | *(inferred)* | Architecture phase. |
