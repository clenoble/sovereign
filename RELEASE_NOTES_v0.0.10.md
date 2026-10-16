# Sovereign GE v0.0.10 — stabilisation

*Your data, your rules.* v0.0.10 follows a full audit of the code against its specification.
It fixes defects that could lose your work, tightens several protections, and says plainly what
does not work yet. There are no new features in this release; there is a sounder base.

The **native desktop app is now the only desktop app**. The earlier Tauri desktop app is frozen
and no longer shipped; Tauri remains the basis of the Android app.

## Your documents are safer

- **A skill can no longer empty a document.** Running a skill from the Skills menu could, on some
  documents, replace their text with nothing. Skills now refuse when they cannot read a document
  faithfully, never write an empty body over a non-empty one, and make a restore point first when
  a document had no history yet.
- **Closing an edited document asks first.** Esc, Cancel or the close button no longer throw away
  unsaved edits; you choose to save, discard or keep editing.
- **Saving tells the truth.** If a save, a move, a pin or a delete fails, you see the error; the
  app no longer reports success for a write that did not happen.
- **Delete is reversible.** Deleting a document from the canvas moves it out of view instead of
  erasing it at once. (A trash you can open and empty is coming.)
- **On the phone, search and version history no longer fail on accented text or emoji** near
  the length of their preview.
- **Documents written on the desktop now open on the phone**, instead of appearing empty — and
  editing them on the phone no longer erases their content.

## Recovery and guardians

- **Enrolling a guardian from the desktop is recorded**, and the roster advances.
- **Logging in cancels a recovery that was in progress on the same device** — if you have your
  passphrase, no recovery should continue.

## The on-device AI

- **The AI runs on the desktop only.** The Android app no longer contains the local inference
  engine; it never shipped a model to run with it.
- **No more "done" for actions that did nothing.** Requests the assistant cannot carry out are
  refused with a reason instead of being proposed or reported as completed.
- **Confirmations are cleaner**: a confirmation that expires closes by itself, and an answer only
  ever applies to the request it was given for.
- **Your conversation log is never written in clear.** If the encrypted log cannot be opened,
  logging stops and you are told; the assistant keeps working.

## Security hardening

- Development and diagnostic switches are no longer compiled into release builds.
- The embedded browser's address check blocks more private and reserved network ranges.
- Restore points in a document's history are now signed like every other version.
- Dependency updates; every remaining advisory has been traced and documented.

## Known limitations

Stated so you can plan around them; each is tracked and planned.

- **Unlocking the phone with a saved password can fail.** When Android's autofill enters the
  password, the app may refuse it with an "invalid args … missing field `key`" error before the
  password is even checked. Try typing the password on the keyboard instead; this is not
  guaranteed to work.
- **Document format.** Desktop and phone store documents slightly differently. Until the move to
  a single versioned document format, the desktop may show some documents as raw JSON, and skills
  decline documents stored as plain text.
- **The desktop editor is minimal**: no caret movement, selection or input-method support yet. A
  new editor is being built.
- **More than 12 threads.** The desktop canvas shows 12 lanes; with more threads, lanes are shared
  and a new or dragged document can land in the wrong thread. One lane per thread is planned.
- **Summarize** is not available on the desktop yet (it waits for the personal-data scan).
- **Sync**: a change of title alone, or moving a document to another thread, does not sync yet;
  the phone cannot be given a relay, so it syncs only on the local network.
- **The embedded browser's protections are being reworked** (a filtering proxy and private
  browsing); until then, treat it as an ordinary browser.
- **Bulk import and the guardian app are not part of the download assets** — build them from
  source (see `doc/RELEASE.md`). The v0.0.9 notes presented them as shipped; they were not in the
  assets.
- **Email stays dormant**; it moves to a later release.
- **No GPU acceleration** in the desktop app; the models run on the CPU.

## Downloads

| File | What it is |
|---|---|
| `sovereign.exe` | Desktop app for Windows (native shell, CPU-only) |
| `sovereign-linux-x86_64` | Desktop app for Linux (native shell) |
| `sovereign-v0.0.10.apk` | Android app (debug-signed) |
| `SHA256SUMS` | Checksums for the files above |

How each file is built, and the checks run before publishing: `doc/RELEASE.md`.

---

*Sovereign GE is developed by Céline Lenoble with Claude (Anthropic).*
