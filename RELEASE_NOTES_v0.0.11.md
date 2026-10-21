# Sovereign GE v0.0.11 — the editor release

*Your data, your rules.* v0.0.11 brings the new desktop editor to everyone, moves every document
to one versioned format, and adds a trash you can open, restore from and empty.

**Update every device you pair.** This version stores and syncs documents in a new format. A
device still on an older version keeps syncing threads and contacts with this one, but exchanges
no documents with it until it is updated; the app names the device that needs it.

## A real editor on the desktop

The minimal text box is gone: the new editor is the desktop's only document editor.

- **Caret and selection** move by character (emoji and accented letters included), by word and by
  line; **undo and redo**; **copy and paste**.
- **Input methods work**: composed text shows in place and is committed once.
- **Markdown is formatted as you type**: headings, bold and italic, code, links, lists, tasks and
  quotes show their formatting in place.
- **Lists continue** when you press Enter.
- **Titles are part of the page** and can be edited where you read them; a long title scrolls
  sideways in its field, and a document with no title reads *Untitled* everywhere.
- **Personal data stays a single unit** in the text: shown by its label, selected and deleted
  whole.
- **Nothing you type leaves memory except through a save**: no temporary file, no log line with
  your text. A failed save keeps your text and says so.
- **A document being edited on another device** opens read-only here, and says on which device.
- **Very large documents** (over about 1 MB of text) open read-only rather than slowing the app
  down.
- **When the editor cannot open a document**, it says why in plain words, instead of showing
  stored data.

## One document format, on every device

- **Your documents are converted once**, in the background, the first time you unlock after the
  update; a notice then says how many were converted (and how many could not be read and were
  left as they are). Each document's earlier state stays in its History. You can keep working
  meanwhile.
- **Desktop and phone now read and write documents the same way**: documents written on one open
  with their text on the other, and the desktop no longer shows raw stored data.
- **Skills read documents stored as plain text.** They still decline a document with several text
  blocks, or one that would gain or lose an image or a video.
- **When a document changed elsewhere**, your text is saved and the other version is kept in its
  History, without a question in between.

## A trash you can open

- **Deleting a document** is one click, with an Undo. Deleted documents stay in **Settings → Trash**
  for 30 days, where you can restore them or empty the trash; after 30 days they are removed for
  good.
- **A delete, a skill or a drag acts on the document you picked**, even if the canvas refreshed in
  between (after a sync, for example).
- **Deleting a thread** moves its documents to the trash, grouped under the thread's name; you can
  bring them back all at once or one by one, into a thread you choose. On the desktop this is done
  through the assistant.
- **A document whose thread was deleted** — here or on another device — stays visible in an
  "Unfiled" lane and in the Alerts window, which offers to move it, put it in a new thread, or
  move it to the trash — one document at a time, or all the documents of that thread at once.

## Threads

- **A new thread is named first**: the new-lane gesture asks for its name.
- **Rename a thread in place** by double-clicking its lane's label (Enter saves, Esc cancels).
  With the mouse for now; the assistant can also rename threads.
- **Across your devices, the later action wins**: a thread renamed on one device and deleted on
  another ends the same on both.

## Sync

- **Changes from your other devices apply on their own**, with the earlier state kept in History.
- **A change that looks suspicious raises an alert** instead: a notice in the status bar and an
  entry in the Alerts window, from which you can open the document or its previous version.
- **"Use this version"** in a document's History replaces the old "Keep mine" choice.

## The on-device AI

- **Every conversation turn ends with a plain outcome**: what was done and whether it worked, that
  the assistant only answered, or why it stopped. "Done" appears only for a change that happened.
- **When the assistant stops after 5 steps** without a final answer, it says so, lists what it did,
  suggests rephrasing or splitting the request, and names where it saved the step-by-step record of
  the turn (thread "Assistant traces"). That record is kept as external content, never as yours.
- **Names with characters such as "İ"** no longer make a rename or a move fail or target the wrong
  text.

## Recovery and login

- **An account is created once**: a second tap while setup is running, on the phone or the desktop,
  no longer replaces the account just created (before, that account could not be unlocked again).
  The same holds for a recovery from a backup. The phone shows that the account is being created,
  and disables its buttons meanwhile.
- **The login, setup and search fields are readable in the dark theme.**
- **Unlocking the phone with a saved password** no longer fails before the password is checked.
- **If the assistant's earlier conversation log can't be verified after the update**, it is set
  aside (kept on disk, not read) and a new one starts. A notice says so, once.

## Security and maintenance

- If the desktop app crashes, the message it prints never quotes your documents.
- Release builds log warnings only, unless you ask for more.
- Fixes in dependencies for TLS certificate verification.
- Dependency updates; one remaining advisory, in a component not included in the release builds,
  is documented.

## Known limitations

- **Markdown marks stay visible** (`#`, `**`, `[…](…)`), dimmed beside the formatting they produce,
  and there is no formatting toolbar yet.
- **Restoring an earlier version from History can fail on the desktop**: "Use this version" may
  answer "This version can't be restored: no such version". The earlier versions are still kept.
  Restoring also has no preview before you choose it.
- **Check the target when you approve an assistant action on similarly named threads or
  documents**: it may still apply to a close name. A fix is designed.
- **More than 12 threads**: the desktop canvas has 12 lanes, so above that some threads share a
  lane. A shared lane shows one thread's name, with the other threads' documents under it, and a
  document created or dropped there can land in another of those threads than the one named.
- **Moving a document to another thread** does not reach your other devices yet: the document
  then stays in a different thread on each device until it is moved there too.
- **Summarize** on the desktop fails on a long document (a long article, for example).
- **Restoring from a crowd backup** does not yet recognise the passphrase of accounts created since
  v0.0.8. Guardian access recovery is not affected.
- **The phone does not sync yet.** On Android the app's network layer does not start (since
  v0.0.9), so the phone neither pairs nor syncs with your other devices, and
  cannot run a guardian access recovery. Desktops are not affected.
- **The embedded browser's protections are being reworked**; treat it as an ordinary browser.
- **Bulk import and the guardian app are not part of the download assets** — build them from source
  (see `doc/RELEASE.md`).
- **Email stays dormant. No GPU acceleration** in the desktop app.
- **An old plain-text document whose whole text is a JSON object with a `body` field** is read as
  that field alone, on both desktop and phone.

## Downloads

| File | What it is |
|---|---|
| `sovereign.exe` | Desktop app for Windows (native shell, CPU-only) |
| `sovereign-linux-x86_64` | Desktop app for Linux (native shell) |
| `sovereign-v0.0.11.apk` | Android app (debug-signed) |
| `SHA256SUMS` | Checksums for the files above |

How each file is built, and the checks run before publishing: `doc/RELEASE.md`.

---

*Sovereign GE is developed by Céline Lenoble with Claude (Anthropic).*
