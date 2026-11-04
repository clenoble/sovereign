# Sovereign GE v0.0.12 — the corrective release

*Your data, your rules.* v0.0.12 corrects what v0.0.11 got wrong, gives every thread its own lane,
names your desktops, and makes the assistant act only on what you named.

**Update every device you pair.** A device on v0.0.11 keeps syncing with this one, but some fixes
below (deletions, moves, device identity) only hold once both sides run v0.0.12.

## History works again on the desktop

- **Restoring an earlier version from History works on the desktop**, for every version. In
  v0.0.11 it failed for every version.
- **On the desktop, when History cannot be read, it says so** instead of showing an empty list.
- **On the desktop, History names the device** each version came from, on a line that never
  overlaps the time.

## One lane per thread

- **Every thread has its own lane on the desktop canvas**, however many threads you have. Above 12
  threads, threads no longer share a lane, and a document created or dropped on a lane goes to the
  thread that lane shows.
- **The mouse wheel scrolls the lanes; Ctrl+wheel zooms.** Scrolling stops at the first and the
  last lane.
- **The "Unfiled" lane appears only when a document needs it.**
- **On the two days a year the clocks change, the time ruler labels each date once**, and it no
  longer crashes in the hour that repeats.

## Sync

- **Moving a document to another thread reaches your other devices.** Moved on two devices, the
  later move wins on both. Pinning stays each device's own.
- **Emptying the trash only removes documents on this device.** A deletion travels to your other
  devices; removing for good stays local, so a document you restored on one device is never
  removed by emptying the trash on another.
- **After the update, documents already in the trash keep their 30 days**, counted on this device;
  nothing is removed at the first start.
- **Each desktop marks its changes with its own identity**, so two desktops with the same name never
  overwrite each other's changes by mistake.
- **A change dated more than five minutes in the future is refused**, instead of being applied
  ahead of everyone else's.
- **The phone syncs again on the local network.** Its network layer now starts on Android, and
  Settings → Devices says what it can reach.

## Your desktops have names

- **On the desktop, each workspace has a device name**, asked at setup (prefilled with the
  computer's name) and renamed in the Devices window.
- **A new name is used at once** by pairing and by a guardian offer, without a restart.

## Accounts and setup

- **On the desktop, creating an account, unlocking and recovering no longer freeze the window**: a
  status line says what is happening, a second click starts nothing, and an error stays on screen.
- **A setup interrupted after the account was saved** opens with its password, and says once that
  setup was interrupted. The desktop and the phone each name what their own setup saves.
- **The setup's "show" button works again on the desktop.**
- **On the phone, showing a secret during setup** no longer lets the keyboard capitalise or correct
  it.
- **The phone's setup sends every confirmation it asks for**, and the account is refused when one
  differs.

## Typing and copying

- **The chat box and the search field are real text fields**: move the caret, select, type over a
  selection, delete in the middle, paste, and use input methods.
- **Cancelling a quit** no longer leaves "Discard" chosen for the next one.
- **On the desktop, what Sovereign copied is cleared from the clipboard when you quit.** What
  another application copied is left alone.
- **Personal data in a title** is shown and read out by its label, not as a raw token, on the
  desktop.

## The trash and documents without a thread

- **Emptying the trash removes what you saw**: a document that arrived in the trash after the view
  was drawn stays, and one restored meanwhile is left alone; on the desktop, the message says so.
- **Actions on documents whose thread was deleted** apply to the documents shown, not to ones that
  arrived since.
- **On the desktop, deleting a document another device already moved to the trash** says so, with
  no Undo.
- **"Restore into…" offers every thread**, in a list that scrolls by wheel and keyboard.

## The on-device assistant

When you ask the assistant for an action directly:

- **Threads and documents are named exactly.** "Work" never means "Work Archive", and "Report" never
  means "Quarterly Report Draft". When no name matches exactly, the assistant lists the close ones
  and asks; when two match, it does nothing and says so.
- **The proposal names what will be acted on**, and that is what is acted on once you approve.
- **Rename, move and merge use the names you wrote.** Put a name in quotes when it contains "to"
  ("rename 'Trip to Rome' to 'Holidays'").
- **An action whose names were read from your words always shows its proposal**, even after you
  approved that action five times; names you put in quotes keep the usual trust.
- **Merging two threads always asks**, as deleting does.
- **"Done" appears only for a change that happened**: a search or a summary reports what it found, a
  rename to the same name says "No change", and a failure says why, once.
- **In a conversation too, a rename to the same name or a move into the same thread says "No
  change"**, not "Done".
- **Summarize works on long documents**: it says first that it will work in parts and take longer,
  and declines a document too long even for that, suggesting a section.
- **A prompt longer than the model reads in one batch is fed in parts**; one longer than the
  model's window is refused in plain words.
- **Restoring a thread another device deleted** brings it back with its name.

## Documents that could not be read

- **Documents the conversion could not read are counted and listed**, in the desktop's Alerts
  window and in the app's alerts, by a short id and their date. Each is kept as it was, and
  Sovereign tries again after its next update.
- **You can keep saving while the conversion runs.**
- **A document open in the editor is not reported as "changed elsewhere"** by its own conversion.

## The assistant's conversation log

- **If the assistant's conversation log cannot be verified when you log in**, it is moved aside
  unread and a new one starts, every time, with a notice that says so. Nothing is deleted.
- **A log whose entries were reordered, or with text added after it was encrypted**, is now caught
  at login.

## Accessibility

- **The desktop's setup, login and join screens** are described to a screen reader as drawn: each
  field by its label, never a secret.
- **In the app, a screen reader hears the assistant's replies and the result of each action**, also
  with the chat closed. The buttons on the alert notices say what they open.

## Windows

- **The desktop app no longer uses OpenGL on Windows**; it draws with DirectX 12 or Vulkan.

## Known limitations

- **In a conversation with the assistant** (rather than a direct request), a write is proposed by
  the names the model used and looked up again after you approve.
- **The phone's History** shows "No saved versions yet" when it cannot read the versions, and names
  the device by its technical id. The device name, the clipboard clearing and the trash's
  "already in the trash" message are desktop-only for now.
- **Two desktops editing the same document at the same time**: the last save wins, and the other
  text is kept in the document's History. Merging both is planned for a later release.
- **The assistant reads requests in English.** When several documents share a title, it does not
  pick one: act from the document itself on the canvas.
- **"Restore" through the assistant** brings a document back to its previous version, whichever
  version you name.
- **If restoring a thread another device deleted fails**, nothing says so yet.
- **The phone cannot look up names**: it syncs on the local network and with devices given by IP
  address, not through servers given by name.
- **Markdown marks stay visible** (`#`, `**`, `[…](…)`), dimmed beside the formatting they produce,
  and there is no formatting toolbar yet.
- **Restoring from a crowd backup** does not yet recognise the passphrase of accounts created since
  v0.0.8. Guardian access recovery is not affected.
- **The embedded browser's protections are being reworked**; treat it as an ordinary browser.
- **Bulk import and the guardian app are not part of the download assets**: build them from source
  (see `doc/RELEASE.md`).
- **Email stays dormant. No GPU acceleration** in the desktop app.
- **An old plain-text document whose whole text is a JSON object with a `body` field** is read as
  that field alone, on both desktop and phone.
- **When a phone joins an existing account**, its password fields have no "show" button yet. They
  stay hidden while you type.
- **Merging two threads moves the documents and deletes the source thread.** From the next release,
  the merged thread will be kept, with an end date.

## Downloads

| File | What it is |
|---|---|
| `sovereign.exe` | Desktop app for Windows (native shell, CPU-only) |
| `sovereign-linux-x86_64` | Desktop app for Linux (native shell) |
| `sovereign-v0.0.12.apk` | Android app (debug-signed) |
| `SHA256SUMS` | Checksums for the files above |

**Linux under X11 needs `libxkbcommon-x11`** (`libxkbcommon-x11-0` on Debian and Ubuntu); without
it the desktop app stops at start. Under Wayland it is not needed.

How each file is built, and the checks run before publishing: `doc/RELEASE.md`.

---

*Sovereign GE is developed by Céline Lenoble with Claude (Anthropic).*
