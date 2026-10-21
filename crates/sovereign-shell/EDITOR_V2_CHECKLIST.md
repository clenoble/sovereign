# Editor v2 in the shell — manual checks

The document editor (`crates/sovereign-editor`, contract C-TEXT, spec ch. 12) runs in
every document window of the shell, in every build (since 2026-10-19 the plain text edit is
removed and there is no switch; `doc/plans/EDITOR-default.md`). These are
the checks its test suites cannot make (`sovereign-editor/TESTS.md`, *Not tested
here*): real pixels, real keyboards, real input methods, real screen readers, the real
clipboard.

## Before you start

- **Use a test workspace.** Point the shell at a throw-away data directory:

  ```
  set SOVEREIGN_DATA_DIR=C:\tmp\sov-test
  cargo run -p sovereign-shell
  ```

  (Git Bash: `SOVEREIGN_DATA_DIR=/c/tmp/sov-test cargo run -p sovereign-shell`.) A release
  build (`--release`) is worth one pass too: it is what ships.
- Every document window has a status line at the top of its body (Saved / Unsaved
  changes / Saving… / Save failed, read-only reasons, the latest notice) and no banner.
  The header shows only [History]: the editor edits in place and saves by itself. There
  is no Edit / Save / Cancel any more.
- A document the editor cannot open opens no window; a notice titled *Document* says why.
- Look (since the finishing pass, section 13): the editor takes the shell's theme
  colours, draws markdown lightly (the source characters stay visible and editable),
  and the mouse wheel scrolls its text.

For each check: what to do → what you should see. Note anything else.

## 1. Typing and saving

1. Open a document, click in the text, type a sentence. → Text appears at the caret,
   once per key. Status goes *Unsaved changes* → (2 s after you stop) *Saving…* →
   *Saved*. Since step 2 a version appears in [History] only once enough has changed
   since the last one (about a short paragraph), on Ctrl+S, and when the window closes —
   not at every autosave.
2. Press Enter → exactly **one** new line (not two). Press Tab → nothing is typed; focus
   does not jump either (the shell has no focus order yet).
3. Arrow keys, Home/End, Ctrl+arrows (word), Ctrl+Home/End, Shift+arrows (select),
   PageUp/PageDown. Double-click selects a word, triple-click a paragraph, drag
   selects. → Caret and selection move as expected.
4. Ctrl+S → *Saving…* → *Saved* immediately, and a new version in [History].
5. Close the shell, reopen it, open the document. → Your text is there.

## 2. Accents and dead keys

1. On a French/Swiss keyboard: é, è, à, ç directly; then dead keys: `^` then `e` → ê,
   `¨` then `i` → ï, `´` then `e` → é. → One character each, no stray `^`/`¨`.
2. AltGr characters: `@`, `#`, `€`, `[`, `]`, `{`, `}`. → Typed once; AltGr+E does not
   trigger a shortcut.
3. Shortcuts type nothing: Ctrl+Z, Ctrl+Y, Ctrl+A, Ctrl+1. → No control characters or
   digits appear.

## 3. An input method (Windows: Microsoft Japanese IME)

1. Add Japanese (Microsoft IME) in Windows settings; switch to it (Win+Space), hiragana
   mode. Click in the document.
2. Type `nihongo`. → The composition (にほんご) shows **in place, underlined**, at the
   caret; the candidate window opens **next to the caret** (not in a screen corner).
3. Space to convert, Enter to commit. → 日本語 is inserted **once** (check it is not
   doubled, and no latin letters remain).
4. Start a composition and press Esc. → The composition disappears; the document is
   unchanged (status stays *Saved*).
5. Start a composition, then click elsewhere in the text. → Either committed once or
   dropped, never doubled.
6. Optional: Korean IME, Chinese Pinyin — same checks.

## 4. Emoji

1. Win+. (emoji panel) → insert 👍🏽, 🇨🇭, 👨‍👩‍👧. → Each is one character: one Left/Right
   arrow moves over it, one Backspace removes all of it.

## 5. Hebrew / Arabic line

1. Paste or type a line mixing scripts, e.g. `Shalom שלום world` and `مرحبا بالعالم 123`.
   → Text is shaped and ordered correctly; Left/Right move **visually** (Right moves to
   the right on screen even inside the RTL run); Shift+arrows select a **logical**
   range (the highlight may look split across the run boundary — that is expected).
2. Caret at the boundary between the LTR and RTL runs: note where it is drawn.

## 6. Undo / redo

1. Type two words with a pause (a few seconds) between them. Ctrl+Z → the second word
   goes; Ctrl+Z → the first; Ctrl+Y (or Ctrl+Shift+Z) → back, in order.
2. Undo after a save → still works (saving does not clear undo).
3. Paste, cut, Enter — each undoable.

## 7. Copy / paste in and out

1. Copy text from the editor (Ctrl+C), paste into Notepad. → Same text.
2. Copy from Notepad, paste into the editor (Ctrl+V). → Same text.
3. **Windows clipboard history** (Win+V, history enabled in Settings): copy something in
   the editor. → It does **not** appear in the Win+V history list; with cloud clipboard
   on, it does not reach another signed-in PC.
4. A copy made in another app still appears in Win+V history (only Sovereign's copies
   are excluded).

## 8. A token copied between two documents

Needs a personal-data record linked to document A (e.g. from the PII dashboard or an
ingest), and A's text containing its token `[pii:pii_record:…]`.

1. Open A. → The token shows as its **label**, e.g. `[Email]`, and moves as one unit
   (one arrow press over it, one Backspace removes it whole; you cannot put the caret
   inside it).
2. Open document B, type `[pii:` + the same record id + `]` by hand. → Shown as the
   plain typed text, editable character by character (not a token: B is not among the
   record's sources).
3. In A, select text including the token, Ctrl+C. Paste into Notepad. → Notepad gets the
   **label** (`[Email]`), never the value and never the record id.
4. Paste into B (Ctrl+V). → The pasted token is a token at once (label, atomic). Save.
   Close and reopen B → still a token. (Behind it: at the **save**, not at the paste, B
   was added to the record's sources.)
5. Paste the token into a third document C, then delete it (Backspace) before the save
   happens (within 2 s), then let it save. Close C, reopen it, type the token form by
   hand. → Plain text, not a token: the paste linked nothing because the token was gone
   when C was saved.
6. Copy the token from A, then copy something else in Notepad, then paste into B. →
   Notepad's text is pasted; nothing is linked.
7. Reveal is not available in the editor yet (no value is ever shown).

## 9. A 1 MB document

1. Make a test document with just under 1,000,000 bytes of text (e.g. paste a large
   text file). Open it. → Opens within about a second; typing at the end and in the
   middle stays smooth (no visible lag per key).
2. A document with more than 1,000,000 bytes of text. → Opens **read only**; the status
   line says why. Typing shows a notice in the status line; nothing is lost silently.

## 10. Closing with unsaved edits

1. Type, then immediately (within 2 s) click × or press Esc. → The window stays a moment
   (*closes once saved*), saves, then closes. No prompt. Reopen → the text is there.
2. Make saving fail (e.g. rename/lock the data directory while the shell runs, or stop
   the disk), type, wait. → *Save failed (…) — your text is kept* in the status line, and
   one *Not saved* notice. Close the window → the **unsaved prompt** (Save / Discard /
   Keep editing). Keep editing → back, text intact. Discard → closes.
3. Same with quitting the app (window close button) while a document has unsaved text:
   it saves first and then quits; with a failing save, the unsaved prompt appears.

## 11. Narrator / NVDA

1. Start Narrator (Ctrl+Win+Enter) or NVDA. Open a document and click in the text.
   → Announced as an editable multi-line text with the document's title; reading the
   line / the document reads its text.
2. Move the caret with the arrows. → The character / word under the caret is read.
   Shift+arrows → the selection is announced.
3. Type a few characters → they are echoed.
4. A token is read as its label (`Email`), not as `[pii:…]`.
5. A read-only (over 1 MB) document → announced as read only.
6. Switch to another window and back → the editor's text is still exposed (not empty).

## 12. Sync (if you have two paired devices on the test workspace)

1. Edit a document on device B and let it sync while A has it open with no unsaved
   edits. → A's editor shows the new version, status line: *changed on a paired device*.
2. Same while A has unsaved typing. → A keeps its text; the status line says the other
   version exists. When A then saves, its text is stored and B's version is in
   [History]. The same happens when another writer on this device changed the document
   after A's editor read it — e.g. the assistant restored a version (*changed on this
   device or a paired one*). (The "being edited on <device>" warning and the switch-editing
   choice are **not wired yet**: the signal between devices does not exist.)

## 13. After the finishing pass

The four points of the manual run of 2026-10-11 (BACKLOG, *Editor v2 — from Céline's
manual run*). The contrast figures are computed in the tests; what only eyes can judge
is below.

1. **Theme.** Settings → *Theme* (switch light ↔ dark) with a document open. → The
   editing area follows at once: white with dark text in light, dark with light text
   in dark; the caret orange; the provenance stripe at the left edge in the theme's
   blue (owned) or red (external). Select some text → a clear blue highlight, the
   selected text dark on it in **both** themes (in dark the highlight is bright, like
   inverse video — say whether that suits you or a dimmer one is wanted; a dimmer one
   cannot keep both contrasts, see the report). Markup (`#`, `*`) inside a selection
   is as readable as the rest.
2. **Markdown drawn lightly.** Type (or paste) these lines:

   ```
   # Heading one
   ## Heading two
   Some **bold**, some *italic*, some ***both***, a `co*de*` span, snake_case_name.
   A [link](https://example.org) and https://example.org/bare.
   - a list item
   1. a numbered item
   - [x] a task
   > a quote
   ```

   → Headings larger and bold; `#`, `*`, `**`, backticks, `[`, `](…)`, list markers
   and `>` dimmed but **visible**; bold looks bold, italic slanted, link text blue and
   underlined, the bare URL too; nothing inside the backticks is formatted;
   `snake_case_name` is not italic. Then: arrows, Home/End, double-click and selection
   move over **every** source character, markup included, with no jump; typing `## `
   at the start of a line turns it into a heading at once and Backspace turns it back;
   Ctrl+C on `**bold**` copies the stars. Narrator reads the source text (whether it
   speaks the markup characters depends on its punctuation setting).
   Note how synthetic bold and italic look (they are drawn, not taken from a bold or
   italic font, so that the caret and line breaks stay those of the plain source).
3. **Mouse wheel.** In a long document (the 1 MB one of check 9 will do), wheel over
   the text → it scrolls, by three lines a notch, without moving the caret; it stops
   at the top and with the last line at the bottom (no blank page after it). Wheel
   over the status line → nothing moves. Then press an arrow key → the view jumps
   back to the caret. Wheel over the canvas or another window → unchanged behaviour.
4. **Chinese / Cantonese text.** Paste `我徹底咁測試佢。` → no box: 佢 is drawn (from
   Microsoft JhengHei UI; the other characters keep their current font). The
   terminal no longer repeats `ICU4X data error: No segmentation model for language:
   ja` (unless `RUST_LOG` is set — the default filter now hides it).

## 14. The title (contract T1–T10)

What the title tests cannot see (`TESTS-title.md`): pixels, the real keyboard focus in the
window, Narrator, a real input method, the card on the canvas. Use a test workspace.

1. **A new document** (the canvas's *new document* on a lane). → Its window opens with the
   **title field** at the top, above the body, with the caret in it and its text
   *Untitled* highlighted (selected text, not a grey placeholder). Type a name at once,
   without clicking → it replaces *Untitled*.
2. **Enter / Tab / Shift+Tab.** In the title press Enter → the caret jumps to the start of
   the body; nothing is typed. Back in the title (click it), press Tab → same. Anywhere in
   the body (start, middle, end, with a selection) press Shift+Tab → back in the title;
   nothing typed.
3. **Look (revised after your run of 2026-10-17: the title is part of the page).** The
   title is the top of the page, on one line: its text a little larger than the body's
   (1.1 ×) and **semi-bold** (not the heavy bold of the first build), with a **thin rule**
   under it in the theme's colours. A title cleared to empty shows *Untitled* dimmed, and
   the caret sits at its left, not after the word. In both themes (Settings → *Theme*):
   readable, the rule visible but discreet, caret visible, selection highlighted. A very
   long title stays on one line (it scrolls or is clipped; it never wraps into the body).
   Compare with the Tauri desktop's title if you have it at hand: same proportions.
4. **The title scrolls with the page.** In a long document (the 1 MB one of check 9, or
   paste a few screens of text), wheel down → the title and its rule go up with the text
   and leave the view; there is no fixed band above the text any more. Wheel back up → the
   title comes back. Scrolled a little (title half visible), click on it → caret in the
   title; compose with the Japanese IME there → the candidate window sits at the title
   where it is now drawn, not where it was.
5. **The window's bar** (T8). The bar at the top of the window still names the document,
   **small** (14 px — about the size of list names, smaller than the Inbox header) on one
   line; a long title ends with an ellipsis (…) at the bar's edge instead of running under
   the buttons. Scrolled down so the title field is out of view, the bar still says which
   document this is. Change the title and wait for the save → the bar follows.
6. **Mouse.** Click in the title → caret there; double-click a word → selected; drag →
   selection. Click in the body → caret back in the body.
7. **Saving and the card.** Change only the title and wait 2–3 s → status line *Saved*;
   the card on the canvas shows the new title. Change title and body, wait → one save
   (History shows one new entry at most, not two). Type `  Spaced  ` → after the save the
   card shows `Spaced` (no surrounding spaces). Type only spaces → the card's title is empty
   (note what it shows), and reopening the window shows *Untitled* dimmed.
8. **One line.** Copy two lines of text from Notepad, paste into the title → one line, a
   space where the line break was.
9. **Unsaved title.** Type in the title and within 2 s press Esc → the window says
   *closes once saved*, saves, closes; reopen → the new title. With a failing save
   (check 10.2's trick), change the title: closing asks (the unsaved prompt); deleting the
   card from its menu is refused (*unsaved edits*); restoring a version from History is
   refused.
10. **Input method and other scripts in the title.** Microsoft Japanese IME: compose
   にほん → 日本 in the title → the candidate window appears **at the title**, the composed
   text is underlined in place, committed once. Type a Hebrew word in the title → arrows
   move visually, Shift+arrows select, as in the body.
11. **Narrator / NVDA.** With the caret in the title → announced as *Title, edit* (or
   *editable text*) with its text; an empty title → *Title, edit, Untitled* or *blank*
   (note which). Tab → the body is announced (multi-line); Shift+Tab from the body →
   *Title* again. Typing in the title is echoed.
12. **Read only (when you can reach it).** T7 covers a document being edited on another
    device (not wired yet, see check 12) and one this version cannot read (a newer
    format: only a newer Sovereign can make one). If you have one → its title is plain
    text, not a field: no caret, no border; typing does nothing but the status-line
    notice. If not, skip. The over-1 MB document of check 9 is read-only for another
    reason that T7 does not cover: just note what its title does.
13. **A long title scrolls sideways** (T9, your second run). Make the window narrow and
    type a title longer than the field → the text slides left as you type, the caret
    always visible; it never wraps onto a second line. End → the end shows; Home → the
    start; arrows step through, the view following; Shift+End selects to the end with the
    end in view. Scrolled to the end, click near the right edge → the caret lands on the
    character you clicked; near the left edge → same. Compose with the Japanese IME at the
    end → the candidate window sits at the visible caret, not off to the right.
14. **An empty title reads *Untitled* everywhere** (T10). Clear a document's title and let
    it save → the card on the canvas, the window's bar, the taskbar pill, the deck fan
    when you lift a deck, the unsaved prompt,
    History's title, the trash (delete it, then Settings → Trash, and Restore → the
    "Restore “Untitled” into…" chooser) all say *Untitled*; nowhere a blank, a lock 🔒 or an
    id. Reopen the window → the title field is empty with *Untitled* dimmed (placeholder,
    not text). On the phone (if at hand): its card shows *Untitled*, not "(untitled)".
    (Search matches titles, so an empty title never appears in search results: nothing
    to look at there.)

## 15. The one-time migration's notice (C-DOC adoption step 6, every build)

Needs a test workspace holding documents written by v0.0.10 or earlier (made with the released
build, then opened with this one).

1. Unlock with this build → the canvas appears at once (the migration runs behind it); a few
   seconds later a notice titled *Document format* says "N documents converted to the new
   format — each one's earlier state is in its History." (or "1 document … its earlier state …").
   Open one of those documents' History → a version of its earlier state is there.
2. Lock and unlock again → no notice.
3. A workspace made with this build only → no notice at any unlock.
4. Phone (if at hand, its workspace from v0.0.10): the same text appears as a bottom toast that
   stays until OK.
5. Quit the shell right after unlocking (before the notice), relaunch and unlock → the notice
   appears then, once, with the same number.

## Crash messages (every build)

A panic never prints its message: stderr shows only
`thread '<name>' panicked at <file>:<line>:<col>` and *(panic message withheld: it can
quote document text)*. Nothing to try by hand unless a panic happens — if one does,
check that line and that no document text is in the terminal.

### §14 — manual run, 2026-10-17 (Céline, laptop, `main` 65f8d4d)

- **Validated:** 1, 2, 4, 6, 8, 9, and 3, 5, 7 except the two points below.
- **Failed → contract T9, T10** (`doc/plans/TITLE-edit.md`): a long title runs past the field and
  the arrows do not bring the hidden characters into view (3, 5); a title of spaces shows
  *Untitled* in the field but the card and the window's bar show nothing (7).
- **Not tested yet:** 10 (input method, other scripts), 11 (Narrator / NVDA), 12 (read only).
- Earlier the same evening (`main` 2c1072d): the title showed twice, fixed and large → T1
  revised, T8.
