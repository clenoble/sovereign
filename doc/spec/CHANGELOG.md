# Specification changelog

*Newest first. One entry per change to the specification's substance: date, version (if any),
what changed, and who decided. Every change to a chapter in this folder adds an entry here in
the same commit.*

---

## 2026-10-19 — v2.0.46: account creation is single-flight; readable fields in the dark theme (rc2)

Decided by Céline from rc1's checks. Ch. 07 *Auth store*: an auth store is created once — the
first write refuses an existing file, account creation (fresh or paired, phone or desktop) is
single-flight with its claim taken before the key derivation, and a refused request changes no
file; on rc1 a second tap during the derivation replaced `auth.store` with a new key and locked
the account for good (the same window exists in v0.0.10). Céline extended it the same day to every
first write of `auth.store` — the desktop's account creation (fresh and paired) and the phone's
backup recovery, which joins the same claim. Ch. 12 *Onboarding and the auth gate*: the mobile
wizard shows progress and disables its buttons while the account is created; text in the auth
gate's, the wizard's and the search window's fields reaches 4.5:1 against the field's fill in both
themes. To be built for rc2.

## 2026-10-19 — v2.0.45: the editor is the shell's only document editor, in every build (status)

Decided by Céline (2026-10-19): the plain text edit is removed completely, not kept as a fallback
("au pire du pire, on restaure une vieille version"). Merged: the C-TEXT editor is compiled into
every build and opens every document window; when it cannot open a document no window opens and a
notice says why (ch. 12, *Documents* and *The document editor* status lines; the stale description
of the plain editor removed).

## 2026-10-19 — v2.0.44: the one-time migration and the end of the tolerant read built on main (status only)

No design change. Merged (C-DOC adoption step 6): the migration runs once per persona at its first
unlock, in the background, then a notice; its record is sealed at rest (PANDA2's second position);
the deprecated field parser is removed and a test refuses any write of an old form (ch. 03,
*Content encoding* status line).

## 2026-10-18 — v2.0.43: renaming a thread in its lane label built on main (status only)

No design change. Merged (`7dd8fbe`): a double-click on a lane label renames the thread through the
stamped thread path (ch. 03, *Thread deletion* paragraph on threads across devices).

## 2026-10-18 — v2.0.42: the one-time migration runs at the first unlock, with a notice

Decided by Céline (ch. 03, *Content encoding*): she first remembered "converted when opened, never
in bulk"; the record of 2026-10-10 (the K1 proposal she validated) says "at its next write, and a
one-time migration converts the rest". She kept the recorded rule (option a) and added: the
migration runs once, in the background, at the first unlock after the update, and a notice says
how many documents were converted and that each earlier state is in its History.

## 2026-10-18 — v2.0.41: threads across devices built on main (status only)

No design change. Thread sync (v2.0.38) merged (`24c25a3`): one stamped path for every thread
write, the later action wins, deleted threads shrink to their id after 30 days. Ch. 03, *Thread
deletion*.

## 2026-10-18 — v2.0.40: the chat loop's round limit — built (status only)

No design change. T5 merged (`d801a2e`): ch. 08 *Chat agent loop* back to built, round limit
included (v2.0.35, v2.0.39). The loop has not been driven to the limit against a real model; that
wiring is checked by compilation and review.

## 2026-10-18 — v2.0.39: the round limit names the trace instead of linking it

Decided by Céline (ch. 08, *Chat agent loop*): when a turn stops at the round limit, the outcome
says where the trace is — the thread "Assistant traces" and the trace's exact title — and nothing
opens by itself. Replaces "a link to the trace, which opens it" (v2.0.35): chat lines carry no
action, and opening a window the user did not ask for is not the outcome's role. To be built.

## 2026-10-18 — v2.0.38: threads across devices — the later action wins; deleted threads shrink to their id; renaming in the label

Decided by Céline, answering three questions left by S5-12 (ch. 03, *Thread deletion*): for the same
thread id the later action wins (rename vs delete included), with clock stamps on every thread write;
a deleted thread's tombstone shrinks to its id after 30 days once nothing points to it (replacing
"never purged"); a thread is renamed in its lane's label or by the assistant. To be built.

## 2026-10-18 — v2.0.37: a document whose thread was deleted asks — built (status only)

No design change. S5-12 merged (`ed719c7`): ch. 03 *Thread deletion* now built on `main`, with the
technical choice that thread tombstones are never purged (purging them would let a paired device
re-create the thread); shrinking them to a content-free marker is left for Céline, as is a thread
deleted here and renamed offline elsewhere.

## 2026-10-18 — v2.0.36: new-thread naming and long/empty titles built on main (status only)

No design change. Merged: naming a new thread first (ch. 12, `537bd37`), in every build; a long
title scrolls sideways and an empty title reads *Untitled* everywhere (`c9eb90b`, editor v2).

## 2026-10-17 — v2.0.35: the chat loop's round limit — an explicit stop and a saved trace

Decided by Céline 2026-10-17 (relayed by the laptop maille, claude-coord from-laptop/0092; its
two additions confirmed by her), written by the PANDA2 maille. ch. 08 *Chat agent loop*:
- **Five rounds stay** (a setting may come later).
- At the limit the turn is **not a failure**: its outcome says it stopped after 5 steps without a
  final answer, lists what was done, invites a rephrase or a split, and links to the trace.
- **The trace is a Sovereign document**, saved at once through the document service (encrypted,
  PII ingest): **never owned** (trust undetermined, fenced like external content — it quotes tool
  results), provenance *the assistant*, in a thread **"Assistant traces"** created on first use;
  a fixed reading order (request; per round: tool, arguments, result as the model saw it, what
  came of it; why it stopped). If saving fails, the outcome says so, without a link.
- Status lines: *Chat agent loop* ✅ → ◐ (the limit's behaviour to be built); *Action Gravity*:
  the chat-outcome rule is built (`2492e2a`).

## 2026-10-17 — v2.0.34: a new thread is named first (as in Tauri)

Ch. 12: the shell's new-thread gestures ask for the name before creating the thread, as the
Tauri desktop did; renaming in place stays open. Written by the laptop maille on Céline's
instruction to continue without her, following her "as in Tauri" rule for the title; to confirm.

## 2026-10-17 — v2.0.33: the title is part of the page

Decided by Céline after trying v2.0.32 (the title showed twice, the field fixed and large): the
title field scrolls with the text, at 1.1 × the body size, semi-bold, as in the Tauri desktop;
the window's bar keeps the title, small. Ch. 12, document window, *Title*. To be built.

## 2026-10-17 — v2.0.32: document titles built on main (status only)

No design change. Title editing (v2.0.31) is merged (ch. 12, document window), in the editor v2
window; a shipped build gets it when the editor leaves its development switch.

## 2026-10-17 — v2.0.31: document titles editable in the shell, as in the Tauri desktop

Decided by Céline during the step-5 manual run, where new documents could not be named: the
title is an editable single-line field at the top of the document window, saved with the body,
*Untitled* while empty; a new document opens on its title. No *Rename…* in the card menu (ch. 12,
document window). The native shell never had it; the frozen Tauri desktop did. To be built.

## 2026-10-17 — v2.0.30: delete, trash and purge built on main (status only)

No design change. C-DOC adoption step 5 is merged (`cad9771`): ch. 03 *Deletion*, *Soft delete
and 30-day purge* and *Thread deletion* now say built on `main`, not yet released, with the
v0.0.10 description kept until the release; *Content encoding* notes that delete goes through the
service too. Not built: the warning for a document arriving by sync for a thread deleted here.

## 2026-10-17 — v2.0.29: the duress enrollment waits for a guardian; a one-time set-aside at migration; the shell profile split

Decided by Céline (her answers relayed by the laptop maille, claude-coord from-laptop/0091),
written by the PANDA2 maille. ch. 07 *After login*:

- **Duress guardian enrollment** shows the ordinary enrollment screen and waits for a guardian
  forever; no offer is armed, nothing listens, nothing is recorded. Replaces v2.0.27's stop with a
  message, which contradicted a running-sync screen (found by the T1 implementer).
- **The moved conversation log** whose anchor does not verify under the primary's key is set aside
  unread and the primary starts empty — lifts the last ch. 07 **[choice]**. The maille's reading
  adds two limits, marked as such: **once only**, at the first primary login after the change
  (afterwards a bad anchor fails closed and is reported, SESSIONLOG-001), and **one notice** to the
  primary.
- **The shell's user profile is split**: device preferences read before login (theme, bubble
  style) stay shared; the persona's identity (display name, nickname, designation) is per persona
  and read only after login. The phone host's profile: same class, not covered.

The status line of the section moves to ◐ (per-persona state and beacon, and recovery primary-only,
built in `296c8b7`).

## 2026-10-17 — v2.0.28: delete gestures, the trash in Settings, a deleted thread's documents go to the trash

Decided by Céline on the four questions of C-DOC adoption step 5 (ch. 03, *Soft delete and
30-day purge*, *Thread deletion and the fallback lane*):

- **No per-item "delete forever"**: emptying the trash (which asks first) or the 30 days are the
  only permanent deletes.
- **Deleting a thread** moves every document still in it to the trash and deletes the thread for
  good; to bring a thread back, create a new one and assign the documents to it from the trash.
  The confirmation says the documents will be placed in the trash.
- **Deleting a document is one click**, with a notice that it can be restored for 30 days and an
  **Undo**.
- **The trash is in Settings.**

Same day, three precisions (Céline, raised while writing the tests): deleting a thread is all or
nothing; a deleted thread's documents are grouped in the trash and can be restored all at once
into a chosen thread or one by one; a document arriving by sync for a thread deleted here raises a
warning asking whether to move it, create a new thread for it, or trash it.

Not yet built (adoption step 5).

## 2026-10-17 — v2.0.27: a duress enrollment stops before the guardian's device

Decided by Céline (answer "c"), written by the PANDA2 maille. ch. 07 *After login*: in a duress
session guardian enrollment stops before it involves the guardian's device — no offer a guardian
could scan is produced, so nothing reaches a guardian; where it stops, the message is one an
ordinary enrollment can also give. This lifts the **[choice]** "what a guardian's device holds
after a duress enrollment". The exact stopping point and message are the maille's reading, to be
confirmed by Céline. The other ch. 07 [choice] (a moved log whose anchor does not verify under the
primary's key) is still open.

## 2026-10-17 — v2.0.26: ch. 03 labels caught up with decisions already taken (status only)

No design change. Flagged by the PANDA2 maille (K1), extended by the laptop maille to the same
kind of label: the *Deletion* status and the native-shell paragraph still said the shell deletes
permanently (soft since v0.0.10); the conflicts table still listed document delete as pending (K3,
decided 2026-10-10); the D3 box said D3 blocks the undetermined-trust tri-state (K9, decided
2026-10-10, separates trust from multi-party ownership; D3 itself stays open). *Content
encoding* (K1): status ○ → ◐ — the v2 format and its single reader are built and used through the
document service on `main`; the conflict box is kept as the v0.0.9 record, with the one residual
case (a raw document whose text is a JSON object with a `body`) stated.

## 2026-10-17 — v2.0.25: per-persona state after login; chat outcome; login cancels recovery on the phone; typing timings optional

Specified by the PANDA2 maille from Céline's answers of 2026-10-16 (Q1 c, Q2 a, Q3 b) and the
cloud review of v0.0.10 (findings 1, 2, 8, 14, 20); to be built (plan
`doc/plans/PANDA2-PLAN-2026-10-17.md`).

- **ch. 07, new *After login — every persona's state stays its own*:** every file written after
  login stays in the persona's own files; the orchestrator's state is per persona; guardian
  recovery belongs to the primary; **in a duress session enrollment appears to work and records
  nothing** (Céline); no user-visible difference from a fresh install; the shared orchestrator
  directory becomes the primary's at migration (Céline). Two **[choice]** left for Céline: what a
  guardian's device holds after a duress enrollment; a moved log whose anchor does not verify
  under the primary's key (proposed: set aside unread, start empty).
- **ch. 07 *Keystroke dynamics*:** typing timings are optional input; a missing or malformed
  sample never makes a login fail.
- **ch. 06 *Owner cancellation*:** the rule now applies to every host — login clears the stored
  recovery and the in-memory session, and a step already running cannot bring it back; a duress
  login cancels nothing.
- **ch. 08 *Action Gravity*:** **a chat turn always ends with an explicit outcome** (Céline):
  writes and their result, answered only, or failed/stopped with the reason; "done" only of a
  successful write.

## 2026-10-16 — v2.0.24: action targets — identity, not position or name (C-HOST AT1–AT8)

Decided by Céline, on a principle proposed by the PANDA2 maille after the cloud review of
v0.0.10 and discussed with the laptop maille (ch. 08, *Action targets*). Between proposal and
execution an object is referred to by its id: resolved before proposing (in the type), ambiguity
asks, re-checked at execution, request ids, the same id for undo and in every host's menus and
lists. Céline's three decisions: a target found by **partial match** is always confirmed
explicitly, never by silence (AT3); what invalidates a proposal is the target **and its contents**
for Destruct, the target alone otherwise (AT4); the confirmation shows a document's title, thread
and date of last change, a thread's name and document count (AT5). To be built with C-HOST.

## 2026-10-16 — v2.0.23: status lines corrected after the cloud review of v0.0.10

No design change. A second-position review of v0.0.9 → v0.0.10 (private:
`doc/plans/review-2026-10-16-cloud/`) showed five status lines claiming more than the code does,
or out of date. Corrected at Céline's request by the PANDA2 maille; findings stay in the private
backlog, not here.

- **03 · Restore**: ✅ → ◐ (a restore failing midway; the restored version not checked to be the
  document's own).
- **03 · Soft delete and 30-day purge**: the shell soft-deletes since v0.0.10 (the line said it
  deleted permanently); no UI can restore a deleted document yet (K3 trash, decided, not built).
- **07 · Primary and duress personas**: ✅ → ◐ (isolation not complete after login; C-SESS, C-HOST).
- **08 · Action Gravity**: direct requests no longer report false success; chat turns still do,
  and targets are resolved by name after approval (C-HOST).
- **06 · Owner cancellation of a takeover**: holds on the desktop, not reliably on the phone.

## 2026-10-15 — v2.0.22: the incoming-change analysis never makes sync wait

Decided by Céline after step 4's core was built: the deterministic checks on a received document
change run at once during sync; the model's verdict runs afterwards in the background, for every
received document including first arrivals, and raises the same alert if it finds something
(ch. 05, C-WIRE 13). Built step 4 core (`c151797`) runs the model inline; the interface pass moves
it to the background.

## 2026-10-15 — v2.0.21: what sync needs from the document service (C-DOC G24–G31), tests first

For C-WIRE (v2.0.20): eight new C-DOC guarantees (ch. 03, *What sync needs from the service*) —
the wire's fields kept and missing ones never overwriting known ones, hybrid-logical-clock stamps
with a 5-minute refusal bound **[choice]**, concurrent edits keeping both, purge markers with
fingerprints, the normalised hash and fingerprint, the stored state for sending, backup restore
only onto an empty workspace, no review flag and alerts. G19 changed: no review flag on a change
from her own devices. Tests written before the code, for these and for the 17 C-WIRE guarantees
(`sovereign-docs`, `sovereign-p2p` over real sealed envelopes without the network, one in
`sovereign-ai` for the alert decision); they fail until step 4 builds them.

## 2026-10-15 — v2.0.20: the sync wire contract (C-WIRE) — draft

Draft for Céline's review (ch. 05, *Sync wire — contract (C-WIRE)*), no code and no tests yet:
what travels for a document (v2 content, trust and the user's trust decision, provenance with
copies, creation time, stamps, author device — all inside the sealed envelope, added with
defaults per K8), stamps by hybrid logical clock instead of the wall clock **[choice]**, states
compared on a normalised form (so a migrated and a legacy copy of one document compare equal),
concurrent edits keeping both versions on both devices (her 2026-10-12 rule for save conflicts),
every remote state applied through `apply_remote` (with the list of what that API lacks),
tombstones and purge markers, backup restore through the service, rows (conversation titles,
contact-address sets, restorable tables). Step-4 plan in the private adoption plan.

Decided by Céline the same day, on the draft's five questions (now guarantees; 17 in all):
- **Devices on an older build**: no document exchange until updated, a notice on both screens;
  rows keep syncing.
- **Purges**: a purge removes the other device's copy only if it is the same version (the marker
  carries a fingerprint of title and content, never content); a differing copy is kept and comes
  back to the purging device; purging it again then removes it from both. C-DOC G13 (ch. 03) gains
  this one exception.
- **Backup restore**: only onto an empty workspace (a new device, a just-recovered account); a
  device that holds documents catches up by sync. Her fallback for a backup next to existing data
  (second empty profile, export, import) depends on the general export, decided 2026-08-04 and not
  built.
- **"Keep mine"** is replaced by **"Use this version"** (a version restore that syncs like any
  edit), mobile and shell.
- **Alerts, not reviews, for changes from her own devices** (her option (b)): the systematic
  review (a flag on every change, the review lists, the badge) is retired — everything is in
  History; the incoming-change analysis (`peer_audit.rs`: injection scan, wholesale-rewrite check,
  model verdict where loaded) stays, runs on every received change, is silent when nothing is
  suspicious and raises an alert only on a detection, pointing at the document and its previous
  version. Her reasoning: security belongs first at pairing and sync, but a paired device can be
  compromised (stolen, or carrying a trapped page); she never re-reads her own gestures, only the
  rare alerts. Previous values of rows stay restorable. C-DOC G19 (ch. 03) to change with step 4.

## 2026-10-15 — v2.0.19: C-DOC guarantees G20–G23 (duplicate, adoption gestures, outside content) — draft for review

Written from v2.0.18's decisions, tests first (`crates/sovereign-docs/tests/g20_*`–`g23_*`,
failing against the service until it is extended):
- **G20 Duplicate** (ch. 03): a copy keeps its tokens as they were in the source (linked under
  G14's conditions), keeps the source's trust (never raised or lowered), has its own first version
  and goes through ingest; provenance *copy of <source>* carrying the source's provenance
  **[choice]**; the source is unchanged; a trashed, purged or read-only source is refused.
- **G21 Adoption preview**: exactly the candidates the selector names now (not trashed, not owned),
  import folders matched by path components, the date warning, nothing changed. **Decided by
  Céline the same day: thread, import-folder and date gestures take undetermined documents only;
  an external document is adopted only when she picks it (one document or an explicit selection)**
  — adopting a thread must never silently move fenced web pages into the trusted plane (also in
  ch. 04).
- **G22 Adoption apply**: exactly the previewed documents still candidates (the preview's
  selection, never recomputed), one recorded trust decision and one event each, whole-or-nothing
  per document, never a standing rule; lowering stays per document **[choice]**.
- **G23 Nothing from outside is created owned** (web and import refused as owned); the default
  state for each creator is the host's, within that bound.

## 2026-10-15 — v2.0.18: trust at creation, adoption gestures, duplicate

Decided by Céline after C-DOC adoption step 3 (creators):
- **What the user writes inside Sovereign from scratch is owned by default** (new document, journal,
  a copy of her own document, what the assistant writes at her request); **what comes from outside
  lands undetermined** (imports, a skill reading an external file) **or external** (web).
- **Imports land undetermined** — replacing the 2026-07-18 "imports land owned" (ch. 03, *Ownership
  at import*).
- **Adoption gestures** (ch. 04): one document, a thread, an import folder, a bulk selection, or by
  original creation / last modification date older than a chosen date. Each shows a preview first
  and applies only to the documents selected now — never a standing rule. A date is not proof of
  authorship; the preview says so.
- **Duplicate becomes a document-service operation** (C-DOC), so a copy's personal-data tokens
  stay tokens (today a duplicate of a document with tokens is refused). Contract and tests first.

## 2026-10-14 — v2.0.17: lists continue on Enter

Reported by Céline in the manual run (checklist section 13): starting a list with `-`, `*` or `1.`
and pressing Enter did not create the next item. Decided: Enter continues a list (same marker, next
number, same indentation, unchecked box for tasks), Enter on an empty item ends it, Shift+Enter
breaks the line inside the item (ch. 12, C-TEXT behaviours).

## 2026-10-12 — v2.0.16: the editor in daily use

Decided by Céline after the manual run of the editor (ch. 12, C-TEXT behaviours):
- **Save conflicts keep both versions without asking**, on the shell and on mobile: the edited text
  is saved on the current revision, the other state stays in History, a notice says so; a choice
  is offered only if that save conflicts again.
- **Dark-theme selection stays bright blue with dark text**, the only way to meet both 3:1 for
  the highlight and 4.5:1 for the selected text.
Recorded: the editor follows the shell's theme (WCAG AA pairs), and Han text falls back across the
system's CJK fonts (the cause of the missing 佢: a single Japanese fallback font on Windows).

## 2026-10-11 — v2.0.15: the document service contract after Céline's review

Decided by Céline:
- **Versions by amount of change, never by time** (ch. 03, *Auto-commit policy*, C-DOC G6): the
  50-saves / 5-minutes rule is replaced. A version is made when the change since the last
  version reaches a threshold; also at Ctrl+S, at the end of an editing session, and after every
  non-editor write. The measure (a distance between two states: characters outside the common
  prefix and suffix, per text block and title, plus a weight per non-text block changed) and
  the threshold are proposals, **to calibrate**.
- **Titles, captions and alt texts go through personal-data ingest** (C-DOC G10, ch. 09); tokens
  may appear in them and the resolver rule applies there too (G15).
- **Hook failure** stores the content, marked not scanned (G10) — was a choice, now decided.
- **Purge leaves a contentless marker** (G13) — decided; its storage and wire form stay open in
  C-WIRE.
- **Trust across devices follows who decided, not the direction** (G12): an explicit trust
  decision on one device (adopt or lower) travels and is applied on the others in either
  direction; sync without such a decision never changes trust. The later decision wins by its
  recorded time (proposed, **[choice]**). The decision's wire field is open in C-WIRE.
Recorded (main session, technical): pasted-token links happen at the save (G14); the editor's
save request carrying the pasted tokens is a later C-TEXT change.

## 2026-10-10 — v2.0.14: the document service contract (C-DOC, C-ENC) — draft for review

- **Document service — contract (C-DOC)** (ch. 03, new section): what the service is and owns,
  what it is given (the store through the encrypted layer only, a commit policy, a
  personal-data ingest hook, media file access, a clock, an event sink), its operations, and
  **19 guarantees**, each tested before the service exists (`crates/sovereign-docs/tests`,
  map in `crates/sovereign-docs/TESTS.md`). Plus an adoption list: 141 call sites measured,
  replaced by service calls in six steps (readers first, `migrate_all` last).
- **C-ENC made precise** (ch. 03): a non-empty stored string never reads as empty; a
  `sovereign.doc` that cannot be read is read-only like a newer `v`; **migration is
  deterministic per document id** (new — the editor's provisional model gives migrated
  blocks random ids, which would make two devices migrating the same document conflict
  forever); migration keeps the timeline position.
- **Two new crates** (ch. 02): `sovereign-docs` (API + tests) and `sovereign-docformat` (the
  v2 model's API, kept free of storage crates so the editor can depend on it).
- Proposed, marked **[choice]**: saves of one editing session may replace each other
  between versions; a failing ingest hook stores the content as given, marked not scanned; a
  stale save is refused (optimistic revision); web content cannot be created owned; sync
  never raises trust to owned; provenance *assistant* and *unrecorded*; per-item purge from
  the trash. Weaknesses found while writing the tests are listed in `TESTS.md` for Céline.

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
