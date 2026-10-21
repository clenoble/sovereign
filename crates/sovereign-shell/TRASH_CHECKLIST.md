# Trash, delete and thread delete in the shell — manual checks

C-DOC adoption step 5 (`doc/plans/C-DOC-adoption.md`), as decided by Céline on 2026-10-17
(spec ch. 03, *Soft delete and 30-day purge*, *Thread deletion and the fallback lane*). The
model behind each screen is tested (`src/step5_tests.rs`, `sovereign-docs/TESTS-step5.md`);
these are the checks the tests cannot make: what is drawn, where it is, what a click does in
the running shell, and the parts of `App` the tests reach only by a scan.

## Before you start

- **Use a test workspace**: `SOVEREIGN_DATA_DIR=C:\tmp\sov-test` (onboard a new account there).
  Never your real workspace.
- Create two threads (*Research*, *Old drafts*) with two or three documents each, and give one
  document a link to another (so a restore can show the link is back).

## 1. Delete is one click, with Undo (S5-8)

- [ ] Right-click a card → **Delete**: the document leaves the canvas **at the first click**.
      No "Confirm delete" row.
- [ ] A notice appears: *Moved to the trash — it can be restored for 30 days*, with **Undo**.
- [ ] **Undo** within about 10 s: the card is back, in its thread, at its place on the
      timeline; its text and History are unchanged.
- [ ] Wait until the notice disappears (about 10 s): no Undo any more; the document is in
      Settings → Trash.
- [ ] Open a document's window, type without saving, then Delete its card: refused, with a
      notice about unsaved edits; nothing deleted.
- [ ] Delete a document whose window is open (saved): the window closes.
- [ ] Right-click a card, then let a sync round or a document created from chat reorder the
      canvas before clicking Delete: the document deleted is the one you right-clicked (AT8).
      Same for Skills → a skill, and for dragging a card to another lane.

## 2. Settings → Trash (S5-11, S5-5)

- [ ] The trash is reached from **Settings**, with its count.
- [ ] Each line: title, thread name, date trashed, days left (30 on the day of the delete). No
      document text anywhere in the trash.
- [ ] Newest first.
- [ ] A document whose thread was deleted says its thread was **deleted** — never "Unfiled".
- [ ] No per-item "delete forever" anywhere (Q1).

## 3. Restore (S5-6)

- [ ] **Restore** a document whose thread exists: it is back on the canvas at once, in its
      thread, with its text, History and links; it leaves the trash list at once.
- [ ] **Restore** a document whose thread was deleted: the shell asks which thread (existing
      ones only; create a new thread first to "bring back" the old one). Cancel: nothing
      restored, and it says why. Choose a thread: the document appears in that thread at once.
- [ ] A deleted thread's documents are **grouped** under a heading saying the thread was
      deleted. **Restore all into a thread…** asks which thread, then brings every document of
      the group back into it at once; each document can also be restored alone.
- [ ] Nothing restored from the trash ever lands in the Unfiled lane.

## 4. Empty trash (S5-7)

- [ ] **Empty trash** opens a modal first (Destruct). Cancel: nothing purged.
- [ ] Confirm: the trash is empty, and the notice says how many documents were purged.
- [ ] A purged document does not come back after a sync with a paired device that has the same
      version (C-WIRE; see `SYNC_CHECKLIST.md`).

## 5. Deleting a thread (S5-10)

The desktop deletes a thread through the assistant ("delete the thread Old drafts"); a canvas
gesture is not decided (plan, Q5).

- [ ] The confirmation is a modal naming the thread and its number of documents, and saying
      they will be placed in the trash (restorable for 30 days).
- [ ] Confirm: the lane disappears; every document that was in it is in Settings → Trash, each
      saying its thread was deleted; a document already in the trash keeps its date and days
      left.
- [ ] If the delete fails (hard to provoke by hand — skip if you cannot), nothing is deleted:
      the lane and all its documents stay, and the notice says nothing was deleted.
- [ ] The thread never comes back: not after a restart, not after a sync round, and no screen
      offers to restore it. Restoring its documents asks for a thread (section 3).
- [ ] On the phone, deleting a thread shows the same confirmation and the same result.

## 6. Purge (S5-3)

- [ ] After unlock, the log shows no purge error. (The hourly purge itself is tested with an
      injected clock; to see it run, trash a document in a workspace whose clock you can move,
      not your real one.)

## 7. A document arrives for a deleted thread (S5-12)

Contract W1–W6 (`doc/plans/C-DOC-adoption.md`, *S5-12*). The model is tested
(`src/orphans_tests.rs`, `sovereign-docs/TESTS-s5-12.md`); these are the checks of what is
drawn. Needs two paired devices (or the phone and the desktop), both on test workspaces.

- [ ] **Arrival.** On both devices, a thread *Old drafts* with a document. Take device B off the
      network; on B, write a new document in *Old drafts*. On A, delete *Old drafts* (its
      documents go to the trash). Reconnect and let a sync round run.
- [ ] On A: the new document is on the canvas, in the **Unfiled** lane, readable and editable. It
      is **not** in the trash.
- [ ] On A: the status line shows how many documents wait; **a** opens the Alerts window, which
      has a section *Documents whose thread was deleted*, the document named with *Old drafts*,
      and three buttons: move to a thread, new thread, trash. No document text in the section.
- [ ] On B (after the same round): *Old drafts* is gone; the document written offline is in
      Unfiled and listed in B's Alerts window too; the documents A trashed are in B's trash.
- [ ] **Move to a thread**: choose an existing thread; the card moves there at once and leaves
      the section; after a sync round it is in that thread on the other device and gone from its
      section.
- [ ] **New thread**: the name field (placeholder *Thread name…*, as the new-thread gesture);
      Create is inactive while empty; the new lane appears with the document; it reaches the
      other device with its name.
- [ ] **Trash**: the document goes to Settings → Trash (30 days), saying its thread was deleted;
      it leaves the section on both devices after a round.
- [ ] **Several documents of one deleted thread**: the section offers an action for all of them
      ("all of *Old drafts*"): one new thread for all, or all moved, or all trashed.
- [ ] **A peer alert on the same document** (if you can provoke one) stays in the alerts list,
      unchanged, whatever you do in the section; dismissing it does not resolve the document.
- [ ] After a restart, a document still waiting is still listed.
- [ ] **Phone**: the same section in its alerts sheet (opened from the notice above the canvas,
      even when no change looks suspicious); the document is drawn in an Unfiled lane at the end
      of the lanes (swipe to it).

## Manual run — 2026-10-17, Céline, laptop (`main` 3218aec, `SHELL_EDITOR_V2`)

- **1. Delete with Undo:** validated, except two checks. *Unsaved edits, then Delete* could not be
  made: the editor's autosave saves before the click, so the document is saved then trashed
  (nothing lost; the refusal path needs `SHELL_FAIL_SAVES_WHILE` to be seen). *Reorder before
  Delete (AT8)* not done (needs a second device).
- **2. Settings → Trash:** validated for single documents — the trash, its lines, restore into the
  existing thread. **Empty trash**: validated (asks first, then empties).
- **Not tested:** everything that starts with deleting a thread (groups, "<name> - deleted",
  restore all into a chosen thread, thread delete all or nothing): the desktop deletes a thread
  only through the assistant, and the laptop has no model. To do on a machine with a model
  (PANDA2), or on the phone once an APK with step 5 is built.
- Found during the run: **new documents cannot be named** (no title field in the shell; the Tauri
  desktop had one) → decided the same day, `doc/plans/TITLE-edit.md`.
