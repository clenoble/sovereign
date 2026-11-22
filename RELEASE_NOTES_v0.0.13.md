# Sovereign GE v0.0.13 — the consolidation release

*Your data, your rules.* v0.0.13 consolidates: your images and videos move into Sovereign's encrypted
store, a merged thread is kept with its end date, a failed login says what failed, and the desktop's
questions and setup can be heard and answered from the keyboard.

**Update every device you pair.** Until every device runs v0.0.13, documents do not sync between a
v0.0.12 device and a v0.0.13 one; threads and the other data still do. Update every device before you
merge threads.

## While your devices run different versions

- **Documents do not sync between a v0.0.13 device and a v0.0.12 one**, whichever side starts; threads
  and the other data keep syncing. Documents written on the older device stay there until it is
  updated, then sync as usual.
- **The updated device names the device to update**, in a notice and in the status bar.

## Your images and videos

- **Images and videos your documents point to are copied once into Sovereign's encrypted store**, at
  the first unlock after the update. The originals stay where they are. The copy runs in the
  background; it takes disk space, and time on that first launch.
- **A notice says how many were copied and how many could not be**, and that the list of what was not
  copied is kept, encrypted, on this device. It names no file and no document.

## Questions that wait for your answer

- **On the desktop, a question that ends by itself comes above every other window** (an action the
  assistant asks you to approve, or a decision on a suspected injection), and only one is asked at a
  time: the second waits until the first has ended, and its time starts then.
- **On the desktop, only the window on top answers.** Typed keys and clicks no longer reach what is
  under a dialog, and Ctrl+F opens Search from the workspace only. Zooming still works under any
  window.
- **On the desktop, the assistant's confirmation closes at the deadline the assistant set**, and your
  answer goes to the request on screen, never to another one.
- **On the desktop, closing the pairing window ends its offer**, by « Done », a click outside, Escape
  or Enter.

## Logging in

- **A failed login says what failed**, on the desktop and in the app. « Invalid password » now means
  a wrong password only. Otherwise you read « Your password could not be checked: … This is not a
  wrong password. » or « Your password is right, but your account could not be opened: … », with the
  cause.
- **On the desktop, a database that will not open says why.**
- **In the app, only a wrong password counts toward the lockout**, and a screen reader is told of the
  failure.
- **In the app, a password manager can keep and fill your main password.** The keystroke sample is
  never filled.

## Merging threads

- **A merged thread is kept, with an end date.** Its lane stops at the merge and shows « merged into
  '…' on … », on the desktop and on the phone. Its documents stay in it, and no document moves.
- **A merged thread takes no new document.** On the desktop, a document dropped on its lane goes to
  the thread it was merged into, with a notice that says so. Neither host offers it to receive a
  document.
- **A merge is undone by asking the assistant**, naming the thread, at any time and from any device,
  after a restart too.
- **Merging no longer always asks.** Since a merge can be undone, it is an ordinary change, as a
  rename is: once you have approved enough merges, the assistant may merge without asking, and the
  merge keeps its undo. Approvals counted before v0.0.13 do not count.
- **A merge reaches your other v0.0.13 devices**, and a rename made on a v0.0.12 device never undoes
  it.

## The on-device assistant

- **"Restore" brings back the version you name**: the previous one, or the one at a date and time as
  History shows it. When that date has several versions, or none, nothing is restored and the
  assistant lists them. A title containing "to" can be restored.
- **After the assistant could not find what you named, any change it then tries in the same request
  is shown to you first**, with the reason, even one it would usually make without asking.
- **In a direct request, if what an action names changes before you approve it, it is proposed again**; if it was deleted
  meanwhile, the action is refused and says so.
- **Asked to create a document without naming a thread, the assistant asks which one**, and creates
  nothing until you say.
- **A rename keeps the new name as you wrote it**, "The Archives" included, and the proposal shows it
  in quotes. When several threads match, they are listed with their creation date in local time.
- **Find & replace and Duplicate, asked in a conversation, are proposed and run**; the assistant names
  the skill it runs. It no longer offers to export a document.
- **Adopting a document from outside always asks**, whatever trust the assistant has earned.
- **A document whose title holds personal data is found** whether you type the value or the label
  Sovereign shows, and the assistant's replies show the label. Titles the assistant's tools read reach
  the model with personal data masked.
- **What you click acts on what is drawn there**, even after the lanes were reordered or reloaded: a
  lane's label, a document window and a skills menu row on the desktop, and the lane a new document
  goes to on the phone.

## History

- **On the phone, History says when it cannot read a version**, instead of "No saved versions yet".
  The version stays in its place, and is not offered for restore.
- **On the phone, History names each device by its name, and each version by its local time**, never
  by a technical id; restoring the version that is already the current one says so.
- **On the desktop, a version that cannot be read stays in its place and says so**, with no "Use this
  version". Each row's time is in local time, the row kept before a restore names the version by its
  time, and restoring the current version says so.

## Sync and pairing

- **When sync stops, Sovereign says so**: « Sync stopped » in the status line, and once a notice with
  « Restart sync », on the desktop and in the app. Nothing restarts it by itself, and a restart that
  fails says why.
- **When a device joins under a name one of your paired devices already has**, Sovereign asks
  « Replace it » or « Keep both », on the desktop and in the app, and replaces nothing before you
  answer. Pairing a device again replaces its old record.
- **Restoring a previous value says when it fails**, a thread another device deleted included, and
  changes nothing; when sync has not answered yet, it says it may still be restored, never « Not
  restored ».
- **When the alerts, the documents whose thread was deleted or the unreadable documents cannot be
  read, Sovereign says so** in their place, on the desktop and in the app, instead of showing none.

## Personal data in your documents

- **On the desktop, Sovereign looks for personal data in your documents after you log in and after
  each sync.** Each value it finds is replaced by its token, as a save does, and marked « PII
  suspected - TBC » until you confirm or dismiss it.
- **What waits for review is counted, never pushed at you**: a count in the status bar on the desktop
  and in words on the phone, and a notice at login when more wait than at the last one. No window
  opens by itself.
- **The review groups values by type.** You confirm or dismiss one value, or several selected at
  once; never a whole type in one gesture.
- **Personal data in a title is shown by its label on the phone too**, and in the phone's reader. The
  phone's edit box still shows the stored form, and says so.

## Threads that share a name

- **In the app, when a skill such as the daily journal needs a thread whose name two of your threads
  share, it asks which one, once**, listing each with its documents and its creation date, and
  remembers your answer. Until v0.0.13 it refused for good.

## The assistant's conversation log

- **A conversation log from before encryption is set aside at the first login of v0.0.13**, kept on
  disk, and a new one starts. The assistant no longer remembers those conversations; the notice says
  so, and asking the assistant what happened to the log explains it. Only installs older than v0.0.9
  meet it.
- **A log set aside can be read, kept or deleted.** A window shows, as plain text, what still verifies
  and counts the rest. On the desktop you keep it, export it after a confirmation, or delete it; on
  the phone you keep it or delete it.

## Documents

- **An HTML export shows a document's raw HTML as text**, title included, and a link whose address is
  not a web, mail or relative address as text. Markdown formatting is still rendered.

## Accessibility

- **On the desktop, a screen reader hears the assistant's confirmation, the injection prompt and the
  unsaved-edits prompt, and presses their buttons**; the keyboard does too (Tab, Shift+Tab, Enter or
  Space, and Escape as before), and each says its keys in one line. Only the window on top is heard.
- **On the desktop, notices are heard and answered too**: a notice holds the focus, and its
  « Restart sync » is reached by Tab. The trash's toast takes a screen reader's click on « Undo »,
  and Ctrl+Z.
- **On the desktop, the whole first setup can be done by keys alone**, and a screen reader presses its
  buttons and choices. A group of choices (the bubble styles, the themes) is one Tab stop, and the
  arrows move inside it.
- **On the desktop, the recovery wizard, the pairing offer and the guardian enrolment are heard and
  answered from the keyboard**, and so are the bubble-style picker and the right-click menu on a lane's
  label, which the menu key opens.
- **On the phone, the unlock screen's password field and the setup's AI-nickname field are named** for
  a screen reader.
- **On the phone, the setup's keystroke sample field is named « Password sample »** for a screen
  reader, its placeholder kept.
- **On the desktop, the « Alt+F8 to show » hint is a password field's description**, on the login
  form and on the form that joins an existing account: Narrator reads it on request (Narrator+0), not
  when the field takes the focus.
- **On the desktop, the text on the orange buttons and on the action's gravity badge reads at 4.5:1
  or more**, in both themes, and so do « Approve » on its gravity colour and the trash's « Empty ».
- **On the desktop, a text field's border reads at 3:1 or more in both themes, and so does a focused
  field's border.**
- **On the desktop, the outline that marks the chosen row of a list reads at 3:1 or more in both
  themes**: in the question about a name already taken, the trash's « choose a thread », the chooser
  for documents whose thread was deleted, and the Alerts and Previous values windows.
- **Not reached yet, on the desktop:** the canvas and the shell's own controls. A document cannot be
  opened from the keyboard; the cards, the taskbar, the assistant's bubble, the « + » button, each
  window's close button and each window's own controls (History, Settings, Devices, Models, Alerts,
  the browser, search results…) are read by a screen reader but cannot be pressed by it. The document
  editor takes no action from a screen reader. These need a design of their own, and come next.

## Device names

- **A correction to v0.0.12's notes: the device name is not shared.** On the desktop, each workspace
  keeps its own device name, and a rename made in one does not reach another.

## On the phone

- **On the phone, the app writes nothing to the system log**: neither its own code nor its page. The
  system, the web view and the framework the app is built on still log under the app's name; the
  framework writes one line at each start, naming an internal command and none of your data.

## On the desktop

- **The desktop app releases its database before it exits**, after stopping sync; a task that hangs
  does not hold the exit beyond a bounded wait.
- **On Linux under X11 without `libxkbcommon-x11`, the desktop app says what is missing and stops**,
  instead of crashing.

## Known issues

- **On Windows, the first opening of a new account's database can rarely fail** with « … could not be
  opened: … Access is denied ». Trying again works.

## Known limitations

- **Mixed versions:** a device still on v0.0.12 may announce conflicts that did not happen (« edited on
  both devices »), and never says that it is the one to update. Updating it ends them; nothing was
  exchanged, and History holds nothing new.
- **A merge made on a v0.0.12 device** still deletes the source thread, and on your v0.0.13 devices its
  documents arrive without a thread. Update every device before merging.
- **The media copy:** no screen lists what could not be copied, so you learn how many, not which, and
  no document shows its images yet. A file above 4 GiB is not copied.
- **Accessibility:** on the desktop, the canvas and each window's own controls are not reached by a
  screen reader or the keyboard yet (see Accessibility above). On the phone, provenance is shown by
  colour only.
- **In a conversation with the assistant** (rather than a direct request), a write is proposed by the
  names the model used and looked up again after you approve.
- **Undo through the assistant** is for a merge only; a rename or a move is undone by hand. The
  assistant does not export or import files.
- **Two desktops editing the same document at the same time**: the last save wins, and the other text
  is kept in the document's History. Merging both is planned for a later release.
- **The assistant reads requests in English.** When several documents share a title, it does not pick
  one: act from the document itself on the canvas.
- **The personal-data search** finds values by their patterns only.
- **A computer clock set wrong can empty the trash early**: the 30 days are counted on this device's
  clock.
- **The desktop has no lock and no log out yet**; the session stays open until you quit.
- **The device name, the clipboard clearing and the trash's "already in the trash" message** are
  desktop-only for now.
- **Contrast:** on the desktop, the hints (the dimmest text) and the text on the canvas's coloured
  lanes, the taskbar's launcher letters, the avatars and the bubble styles do not all reach 4.5:1
  yet, and in the light theme the taskbar's mark of an open window does not reach 3:1. Their colours
  wait for the palette's design.
- **On the phone, a screen reader announces three close buttons by their sign** (« × » in Inbox and in
  a contact's details, « ✕ » in Settings), and « New document » and « New lane » open the system's own
  prompt, whose field has no name.
- **On the desktop's login screen, « Forgot your password? Recover with guardians » is announced as a
  button, but only the mouse opens it**: a screen reader's press and the keyboard do not. The recovery
  wizard it opens answers the keyboard.
- **On the desktop, Shift+F10 does not open a lane's menu yet;** the menu key does.
- **On the desktop, a screen reader does not announce the trash's toast when it appears;** Ctrl+Z still
  undoes the deletion while it is shown.
- **A slow phone can stay on « Loading... »** when it starts: the screen asks three times, 7 seconds
  apart, whether the app is ready, then stops asking.
- **The phone cannot look up names**: it syncs on the local network and with devices given by IP
  address, not through servers given by name.
- **Markdown marks stay visible** (`#`, `**`, `[…](…)`), dimmed beside the formatting they produce,
  and there is no formatting toolbar yet.
- **Restoring from a crowd backup** does not yet recognise the passphrase of accounts created since
  v0.0.8. Guardian access recovery is not affected.
- **The embedded browser's protections are being reworked**; treat it as an ordinary browser.
- **Bulk import and the guardian app are not part of the download assets**: build them from source
  (see `doc/RELEASE.md`).
- **Email stays dormant**: it is turned off in v0.0.13, and its mail library is updated with it in
  v0.0.14, when email comes, read-only. **No GPU acceleration** in the desktop app.
- **An old plain-text document whose whole text is a JSON object with a `body` field** is read as
  that field alone, on both desktop and phone.
- **When a phone joins an existing account**, its password fields have no "show" button yet. They
  stay hidden while you type.

## Downloads

| File | What it is |
|---|---|
| `sovereign.exe` | Desktop app for Windows (native shell, CPU-only) |
| `sovereign-linux-x86_64` | Desktop app for Linux (native shell) |
| `sovereign-v0.0.13.apk` | Android app (debug-signed) |
| `SHA256SUMS` | Checksums for the files above |

**Linux under X11 needs `libxkbcommon-x11`** (`libxkbcommon-x11-0` on Debian and Ubuntu); without
it the desktop app says so and stops. Under Wayland it is not needed.

How each file is built, and the checks run before publishing: `doc/RELEASE.md`.

---

*Sovereign GE is developed by Céline Lenoble with Claude (Anthropic).*
