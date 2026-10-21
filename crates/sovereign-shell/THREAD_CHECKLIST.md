# Naming a new thread — manual checks (contract N1–N7)

*What the tests cannot see: drawing, real keyboard focus in a window, a real input method,
a screen reader. Contract: `doc/plans/THREAD-name.md`; tests: `TESTS-thread-name.md`. Run on
a test workspace only (`SOVEREIGN_DATA_DIR=C:\tmp\sov-test`). The `+` and the canvas menu
exist in every build. Compare with the Tauri desktop's toolbar
`+` (*New thread*) if you have it at hand.*

1. **Opening (N1).** Click the `+` at the bottom → the menu → *New lane*: a small field
   appears with *Thread name…* dimmed in it and a *Create* button beside it; the caret
   blinks in the field without a click. Right-click on empty canvas → *New lane* → the
   same. No new lane appears yet.
2. **Create (N2).** Type `  Ideas  ` → Enter: the field closes, a lane named `Ideas`
   appears (no surrounding spaces). Again with the mouse: type a name → click *Create* →
   same. With the field empty or only spaces: *Create* looks inactive, clicking it and
   Enter do nothing, the field stays open.
3. **One line (N2).** Copy two lines from Notepad, paste into the field → one line, a
   space where the break was; Enter → the lane shows that one line.
4. **Cancel (N3).** Type a name → Esc: the field closes, no lane. Open it again → click on
   the canvas outside the field: it closes, no lane. Open it, then open a window (e.g. `c`
   for chat, or the taskbar) → note what happens to the field; no lane is created.
5. **Text rules (N4, N7).** In the field: arrows, Home/End, Shift+arrows, double-click a word,
   drag-select, Ctrl+A / C / X / V, Ctrl+Z. A name longer than the field scrolls sideways
   and the caret stays visible (as the title's T9). Microsoft Japanese IME: compose
   にほん → 日本 → the candidate window sits **at the field**; while composing, Enter
   commits the composition and does **not** create; Esc ends the composition, a second Esc
   closes the field. A Hebrew word: arrows move visually.
6. **Narrator / NVDA (N5).** Opening announces a dialog, *New thread*, then *Thread name,
   edit* (note the exact words, and whether the placeholder is read); typing is echoed; Tab
   (or the screen reader's next item) reaches *Create, button* — announced unavailable while
   the name is empty. Esc closes the dialog, and the reader says the focus left it.
7. **Duplicates (N6).** Create a second thread named like an existing one → accepted; two
   lanes with the same name.
8. **Both themes.** Settings → *Theme*: the field, its placeholder, the caret, the
   selection and *Create* (active and inactive) are readable in dark and light.

## §R — Renaming a thread in its lane label (contract R1–R8)

*Contract: `doc/plans/THREAD-name.md`, *Renaming a thread — decided 2026-10-18*; tests:
`TESTS-thread-rename.md`. Test workspace only, with at least three threads, one of them holding
documents. Every build.*

R1. **Opening.** Double-click a lane's coloured label at the left edge: a field appears over
    the label, the current name in it, all selected, the caret visible without a click. A
    single click on the label does not open it (note what it does: pan, nothing). A
    double-click on a card, on empty canvas, or on the time ruler does not open it. Scroll the
    canvas so a lane is half under the time ruler: its label still sits below the ruler and a
    double-click there opens the field. Zoom in and out: the double-click lands on the label as
    drawn, never beside it.
R2. **Save.** With the name selected, type `  Journal  ` → Enter: the field closes and the label
    reads `Journal` at once (no surrounding spaces), in the same lane, with its documents. Paste
    two lines from Notepad → one line. Restart the shell: the new name is still there.
R3. **Cancel.** Type something → Esc: the label keeps its old name. Again → click on the canvas
    outside the field: same. Again → open a window (`c`, the taskbar) or lock: note what happens
    to the field; the name is unchanged. Delete the whole name → Enter: the field closes, the
    label keeps its old name.
R4. **Text rules (R3 of the contract: N4, N7, T9).** Arrows, Home/End, Shift+arrows, double-click
    a word inside the field, drag-select, Ctrl+A / C / X / V, Ctrl+Z. A name longer than the
    field scrolls sideways, caret visible. Microsoft Japanese IME: the candidate window sits at
    the field; while composing, Enter commits and does **not** save; Esc ends the composition, a
    second Esc closes the field. A Hebrew word: arrows move visually.
R5. **Narrator / NVDA (R4).** With no window open, move through the canvas: each lane label is
    announced as *Rename thread <name>, button* (note the words). Open the field: *Thread name,
    edit*, with the current name read as its value. Note whether a screen-reader user can
    **activate** the label button to open the field (see TESTS-thread-rename.md, *Problems*:
    the shell's AccessKit action handler is a no-op today).
R6. **Duplicates (R5).** Rename a thread to the name of another: accepted, two labels alike.
R7. **Deleted thread (R6).** Delete a thread (canvas menu → its documents go to the trash): its
    label is gone, nothing to double-click. A document whose thread was deleted on another
    device sits in the *Unfiled* lane: double-clicking *Unfiled* opens nothing. Open a rename
    field, then delete that thread from the assistant (or let a deletion arrive by sync): note
    what happens to the open field before Enter; on Enter nothing is saved, the field closes
    with a notice, and the thread does **not** come back (contract R7).
R7b. **Unchanged name (contract R8).** Double-click a label → Enter at once: the field closes,
    nothing else happens. Retype the same name with spaces around it → Enter: same. (Across
    devices, once S1 is merged: rename a thread on the other device while this one is offline,
    open the field here before syncing and press Enter unchanged; after sync the other
    device's name stands.)
R8. **Across devices (S1, once THREAD-sync is merged).** With a paired device: rename the same
    thread on both while one is offline, the second rename later; reconnect: both show the later
    name. Rename here after the other device deleted it offline: as decided (S2/S3), the later
    action wins.
R9. **Both themes.** The field over the label, its text, caret and selection are readable in
    dark and light, and the field covers the old label text entirely.
