# Specification changelog

*Newest first. One entry per change to the specification's substance: date, version (if any),
what changed, and who decided. Every change to a chapter in this folder adds an entry here in
the same commit.*

*Entries before v2.0.72 (2026-11-13) name the project's private plan files. Those files are not on
the public mirror. The entries are kept unchanged, as a signed record (Céline, 2026-11-13). Since
v2.0.72, the spec cites a chapter or a decision, never a plan file.*

---

## 2026-11-21 — v2.0.157: L6's status, partial (ch. 12)

- **Ch. 12, L6 (« The key is said with the field »):** the guarantee keeps its words; its status goes from built to
  partial. The description is set on each password input of the login and join forms, as built in rc2's lot R1;
  Narrator reads a description on request only (Narrator+0), not when the field takes the focus, so the reading
  at focus is not met yet. The bullet on the description says so.
- **Ch. 12, the gaps:** the login form's key is read in scan mode, or by Narrator+0 since L6, not at the focus.
  Open.
- **Decided by the Fable maille, by delegation of Céline (2026-11-14),** on Céline's pass with Narrator of
  2026-11-21 and the laptop's reading of AccessKit (a description becomes UIA's `FullDescription`, which Narrator
  reads on request). Written by PANDA2; read by the laptop, the contract's author.

## 2026-11-20 — v2.0.156: the phone's system log names the framework's line (ch. 07)

- **Ch. 07, « The phone's system log », its third sentence:** the framework the app is built on is named among
  what still logs under the app's process, with what it wrote on v0.0.13's candidate (one line at each start, a
  plugin's command, no path).
- **Decided by the Fable maille, by delegation of Céline (2026-11-14),** on WSL's measure of the candidate's APK
  (three lines under the framework's tag, the same in every session). No code changes.

## 2026-11-20 — v2.0.155: R6 built (ch. 12: TC6)

- **Ch. 12, Theme, TC6's status mark:** the chosen row's outline takes the text colour from one function of the
  theme, in the six drawings; none strokes its chosen row in the accent.
- Contract and tests by the laptop, code by PANDA2, second run by WSL; merged into main `f34383ed`. Written by
  PANDA2 after reading the hosts; to be read by the Fable maille.

## 2026-11-20 — v2.0.154: a chosen row's mark seen in both themes (TC6, specified)

- **Ch. 12, Theme, TC6:** in the six lists a person moves through by the keyboard, the outline that marks the
  chosen row reaches 3:1 in both themes, against the row (with and without the pointer) and the window under it.
  One function gives its colour, and the six drawings use it.
- **Decided by Céline, 2026-11-20** (« ok pour a »), on a measure of the light theme's accent outline at 2.65:1
  against the row. The reach to six lists, with one colour for every such outline, is the Fable maille's, by
  delegation of Céline (2026-11-14), on the same day. The contract and its test are by the laptop; the Fable
  maille read the contract before the test.

## 2026-11-20 — v2.0.153: R1 built (ch. 12: SQ11g, TC1 to TC5, L6)

- **Ch. 12, four status marks:** text on the accent and the gravity word (TC1, TC2, TC4); a focused text field's
  border, and every text field's border (TC3, TC5); the hint read with the field (L6); the browser's page under every
  window (SQ11g), with what is built about a lock, which this release does not have after login.
- Contract and tests by the laptop, code by PANDA2 (an agent on its brief), second run by WSL; merged into main
  `40cd226b`. Written by PANDA2 after reading the hosts; to be read by the Fable maille.

## 2026-11-20 — v2.0.152: the phone's system log (ch. 07), built

- **Ch. 07, a new subsection before « Authentication and personas »:** the app writes nothing to the phone's system
  log; the phone's entry installs no subscriber and a silent panic hook; the page's first script silences its
  console and uncaught errors on a phone. Said of what the app writes, not of the process's tag.
- Céline, 2026-11-20 (« I - oui, à faire »); the way: the Fable maille, by delegation of Céline, the same day. The
  rc2's lot R2, its contract private. Contract and tests by PANDA2, code by WSL, second run by the laptop; merged
  into main `6fb5cb7f`. Written by PANDA2 after reading the hosts; to be read by the Fable maille.

## 2026-11-20 — v2.0.151: R3 built (ch. 09 SW5's stamp; ch. 12, the keystroke field's name and K7)

- **Ch. 09, SW5:** a received review's stamp read as a time and bounded, and a new record with a stamp not taken
  arriving unreviewed: built, in `sovereign-p2p` (`merged_review` and the new-record branch of
  `apply_pii_record_row`, the one path a PII row takes).
- **Ch. 12:** the keystroke enrolment's field named « Password sample », and K7's sentence on the duress step:
  built, in `OnboardingWizard.svelte`, drawn by the phone and the Tauri desktop.
- Contract and tests by WSL, code by the laptop, second run by PANDA2 (merged into main `ed05aae3`). Status written
  by PANDA2 after reading the hosts; to be read by the Fable maille.

## 2026-11-20 — v2.0.150: AT4's status says the path it holds on

- **Ch. 08, C-HOST:** an « As built » line under « built in v0.0.13: AT4 […] in every host ». AT4 holds on the
  query path; a write a chat tool proposes in a conversation is looked up again by its names after the approval.
  That status said more than the code holds.
- Céline, 2026-11-20, on the Fable maille's page of the rc1 gates (block II: « à signaler dans les notes de
  release / à mettre au backlog »). Written by PANDA2; to be read by the Fable maille.

## 2026-11-20 — v2.0.149: the browser's page under every window; a focused field's border seen in both themes; the « Alt+F8 » hint read with the field (specified)

- **Ch. 12, SQ11g:** the Browser window's page is shown only while the workspace is on top, the Browser window is
  the top one of the workspace's windows, and no right-click menu is open. One function decides it, and the
  shell sets the web view's visibility from that answer only. What the shell draws above the windows without
  being a window of SQ11a (the taskbar, the status bar, the bubble, the « + » button, the trash's toast) is
  named as not covered.
- **Ch. 12, Theme, TC3:** a focused text field's border reaches 3:1 in both themes, against the field's fill
  and the surfaces fields are drawn on. One colour, from one function of the theme, for every field named.
- **Ch. 12, Accessibility, L6:** on the login and join forms, each secret field's description is the drawn hint,
  so the « Alt+F8 » key is read when the field takes the focus. The gap line says L6 will close it.
- **Decided by Céline, 2026-11-20** (« I - oui, à faire »; « 20. A corriger sans révéler »), on a page of
  findings that held these lines. L6's way is WSL's (2026-11-19). The contract is by the laptop, and the Fable
  maille reads it before the tests.
- **Ch. 12, Theme, TC5** (second commit under this version):
  - the text-field border (`input_border`) reaches 3:1 against every fill of the palette, in both themes;
  - the search box draws it, and the notice's and the unsaved-edits prompt's outlines follow;
  - the step to the focused border is reported, not required.
  - **Decided by Céline, 2026-11-20:** « Si tu es sûre de trouver une teinte qui fonctionne pour toutes les
    bordures de champ où qu'ils soient alors c, sinon a ». The tint was found by the Fable maille, which also
    decided the particulars, by delegation of Céline (2026-11-14), on 2026-11-20.
- **Ch. 12, Theme, TC1, TC2, TC4** (second commit under this version):
  - text on the accent reaches 4.5:1 in both themes, at the transparencies the shell draws it with;
  - so does the gravity word on its badges, « Approve » on its gravity colour, and the trash's « Empty »;
  - one function gives the text colour for a fill;
  - the lanes' labels, the launcher letters, the bubble styles and the avatars are left for the palette's design,
    and none reads less than in rc1.
  - **Decided by Céline, 2026-11-20,** on its reach: « a+ » (the accent in both themes, and the gravity word; the
    way PANDA2 had proposed). Contract by the laptop; the Fable maille reads it before the tests.

## 2026-11-20 — v2.0.148: a received review's stamp is read as a time; the keystroke field is named; the duress step speaks of password managers (specified)

- **Ch. 09, SW5:** a review received from another device is taken only when its stamp is in the form a review
  writes and no later than the bound on future times every synced row has (ch. 05); otherwise the review it
  carries is ignored and the local one stands. A review of the past, or within the bound, travels as before. A
  record that arrives new with such a review is created unreviewed and with no stamp.
- **Ch. 12, lot C:** the keystroke enrolment's field, which had a placeholder only, is named « Password sample »
  for a screen reader, by a label, on the phone and in the desktop app, its placeholder and K6's `autocomplete`
  kept.
- **Ch. 12, K7:** K6 stays as built; the app wizard's duress step says, in one sentence that is also the duress
  password field's description, that a password manager on the device may offer the main password at unlock and
  offer to save the duress one, and that whoever relies on a duress password keeps both out of it.
- Decided by Céline, 2026-11-20: « oui, à faire » for the review's stamp and the keystroke field (on the page of
  the first release candidate's findings); for K7, « c'est la responsabilité de l'utilisateur, on ne peut pas
  tout gérer à sa place », on the way the Fable maille put to her. The field's name, « Password sample », and K7's
  words are WSL's proposal, to be read by the Fable maille. Contract and tests by WSL (lot R3 of the second
  release candidate); code by the laptop; second run by PANDA2. The new record's case: the laptop's, met while
  coding; decided by the Fable maille, by delegation of Céline (2026-11-20); its clause and test by WSL.

## 2026-11-19 — v2.0.147: login errors say what failed, built (LE1 to LE6)

- **Ch. 07, LE1 to LE6:** built, read on main after the laptop's second run. What the hosts do is in the status
  line.
- Code by PANDA2 (an agent on its brief; LE6 by PANDA2), tests by WSL, second run by the laptop. Status written by
  PANDA2 after reading the hosts; to be read by the Fable maille.

## 2026-11-19 — v2.0.146: LE6, a login failure is announced

- **Ch. 07, the login errors (LE):** LE6 added. Each of the three sentences is in an alert when the login screen
  shows it, so a screen reader says it. The app's login error had no alert role, so LE's new sentences went
  unsaid; the desktop's login form had it already.
- Decided by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19, on the laptop's reading in LE's
  second run. Written by WSL, LE's contract and test author.

## 2026-11-19 — v2.0.145: SQ11 and AT13 built

- **Ch. 12, SQ11; ch. 08, AT13:** built, read on main after WSL's second run. What the hosts do is in each
  status line. SQ10's status keeps its words; SQ11a's order now governs it.
- Code by PANDA2 (an agent on its brief), tests by the laptop, second run by WSL. Status written by PANDA2
  after reading the hosts; to be read by the Fable maille.

## 2026-11-19 — v2.0.144: SQ11d, the mouse wheel passes under a window

- **Ch. 12, SQ11d:** the wheel passes under a window, as the zoom does. It moves the view and changes nothing, so the
  person can read what is under a question before answering it. A scroll that closes the right-click menu left
  under a window counts as moving the view.
- Decided by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19, on PANDA2's question from SQ11's
  code. Written by the laptop.

## 2026-11-19 — v2.0.143: SQ10 built (lots B and C); SQ9's second test runs with the others

- **Ch. 12, SQ10:** built on the desktop. Read on main `7da09b54`:
  - the shell's key handling asks the picker, then lot B's windows and the setup, then lot A's, then the menu;
  - a screen reader's actions reach the picker (never while locked), then lot B's, then the rest, the menu last;
  - the tree carries lot B's three dialogs, the setup's targets, the picker and the menu;
  - the keyboard's control is drawn with the shell's focus outline.
- **Gaps closed by it:** a screen reader presses only two kinds of control; the setup cannot be done from the
  keyboard alone; the welcome screen's buttons are read in scan mode only; « what SQ10 builds is still a gap
  until its lot is merged ».
  - The login form's « Alt+F8 to show », read in scan mode only, stays a gap: not checked against lot B.
- **SQ9:** its second test is no longer left out of the ordinary run: no drawing is left marked for a lot. It is
  still the shell's line of the release's accessibility audit.
- Lots B and C merged after their second runs (the laptop for B, PANDA2 for C); status written by the laptop.

## 2026-11-19 — v2.0.142: only the window on top answers (SQ11), and one question with a deadline at a time (AT13); both specified

- **Ch. 12, SQ11:** SQ2's rule for every window the shell draws as a modal, in six guarantees.
  - **One order** (SQ11a) for what the eye sees on top, what a click reaches, what takes a key and what is in the
    tree.
    - First comes the window with a deadline, the action confirmation or the injection prompt, never both.
    - Then the unsaved-edits prompt, the same-name question and a notice.
    - Then the windows the person opens, in the order the shell draws them today.
  - **Only the window on top is in the tree,** with the focus (SQ11b).
  - **Nothing under it takes a screen reader's action** (SQ11c).
  - **Its keys stay in it** (SQ11d), except the zoom (WCAG 1.4.4) and Ctrl+S under the unsaved-edits prompt.
    A click follows the same order.
  - **A question counts as shown only while it is drawn on top** (SQ11e). With SQ11a and AT13 it is never
    covered while its deadline runs.
  - **The workspace under a window stays readable,** and nothing in it acts (SQ11f).
- **Ch. 08, AT13:** at most one question with a deadline is asked at a time, from any turn.
  - The next is sent, and its deadline starts, when the one before has ended.
  - One waiting its turn is withdrawn if the session ends.
- **Decided:**
  - **SQ2's rule for every modal,** by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19,
    after reading lot A's code.
  - **What stays readable,** by Céline the same day, to be revisited with the canvas's redesign.
  - **The order,** by Céline the same day: « toute fenêtre avec auto-résolution après délai doit être
    au-dessus. Si jamais deux de ces fenêtres sont déclenchées en même temps, la 2eme attend la fin de la
    première pour apparaître ». She confirmed its reading, and the lot's place in v0.0.13 (« terminons ce point
    en v0.0.13 »).
  - **Where « the second waits » is held, the orchestrator by one lock,** by the Fable maille, by delegation of
    Céline (2026-11-14), the same day. Two turns can run at once in the shell, so two questions could otherwise
    be pending together.
- Contract by the laptop.

## 2026-11-19 — v2.0.141: login errors say what failed (LE1 to LE5, specified)

- **Ch. 07, « Login errors say what failed », new:**
  - « Invalid password » for a wrong password only, on both hosts and the command line;
  - a derivation that fails says that the password could not be checked; a failure after the password
    matched says that it is right and the account did not open; each with its cause;
  - opening a persona's database keeps its error;
  - what is shown names no path or file of the data directory, since a path can name a persona;
  - the app's lockout counts wrong passwords only, and every wrong guess still counts;
  - the words tell no persona apart, the cause included.
  - The three sentences and these rules as read by the Fable maille, by delegation of Céline (2026-11-14),
    on 2026-11-19.
- **Ch. 07, « Login throttle »:** one line, it counts a wrong password only from v0.0.13 (LE4).
- **Decided by Céline, 2026-11-19, through the Fable maille:** « a minima avoir un message d'erreur exact
  quand il y a un problème ». Finding the cause of an intermittent setup failure seen in the shell's tests is
  out of it, by her words.
- Contract by WSL; code by PANDA2; second run by the laptop.

## 2026-11-19 — v2.0.140: the shell's questions and notices built (SQ1 to SQ9)

- **Ch. 12, SQ1 to SQ9:** built on the desktop, SQ6 with v2.0.139's keys. Read on main `ba224fd2`:
  - the window's key handling calls the shell's key dispatcher before anything else, and it asks the
    questions, the notice and the toast first;
  - the screen reader's actions are drained into one function, which asks them first;
  - while locked, none of them takes a key.
  - SQ9's test that fails while a lot of SQ10 is under way is run by the release audit, not by every test
    run, until lots B and C are merged.
- **SQ10:** lots B and C under way.
- Code by PANDA2, tests by the laptop, second run by WSL. Status written by PANDA2, after reading the hosts.

## 2026-11-19 — v2.0.139: « Restart sync » in a notice, from the keyboard (SQ6)

- **Ch. 12, SQ6:** a notice that offers « Restart sync » takes it from the keyboard.
  - Tab and Shift+Tab reach it, Enter or Space presses it, and Escape still closes.
  - The focus starts on the notice itself, so a stray Enter presses nothing.
  - The focused button is outlined, and a line says the keys.
  - Before, a click or a screen reader's Click pressed it, and no key did.
- Decided by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19, on PANDA2's reading of lot A's code.
  - Its reason: it is the dialog's convention and SQ4's own rule. A notice that offers an act and takes no key
    for it leaves a keyboard user without what a mouse user has.
- Contract and tests by the laptop.

## 2026-11-19 — v2.0.138: PO1 built

- **Ch. 05, PO1:** built on the desktop. Read on main `c9c814c7`: the shell's `close_pairing` sends
  `ClearPairingOffer` to the node by the send that waits, as the offer is armed; the click on « Done » or
  outside the card, and Escape and Enter, call it; nothing else closes the window; the duress decoy sends
  nothing. Status written by PANDA2, after WSL's second run.

## 2026-11-19 — v2.0.137: closing the guardian-enrollment window does not end its offer (Céline)

- **Ch. 06, *In-person enrollment*:** the enrollment offer keeps its 600 s and its three wrong proofs
  whatever its window does, unlike the pairing offer (ch. 05, PO1). Céline, 2026-11-19, with her reason (a
  third person, time, a window closed by mistake) and « à réfléchir »: the question stays open. Written so
  that a later reader does not mend it as PO1's twin.
- Written by the laptop.

## 2026-11-19 — v2.0.136: closing the pairing window ends the offer, on the desktop as on the app (PO1, specified)

- **Ch. 05, *Offer lifetime and limits*, PO1:** the desktop's pairing window closed by « Done », a click
  outside, Escape or Enter disarms the offer on the node; a proof arriving after is refused; the app already
  does the same. The Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19, as a lot of its own
  in v0.0.13 (tests the laptop, code PANDA2, second run WSL). A pairing that succeeds leaves the desktop's
  window open; closing it then has nothing left to disarm (the Fable maille's reading).
- Contract written by the laptop.

## 2026-11-19 — v2.0.135: lot B's cases written in; a group of choices takes the arrows; each dialog says its keys

- **Ch. 12, SQ10:** a group of choices (the setup's styles and themes, the bubble-style picker) is one stop
  for Tab, the arrows moving inside it; this amends the Fable maille's own first answer for the picker,
  where Tab also walked its styles. Each dialog of SQ10 says its keys in one line. Tab follows the drawn order. Lot B holds the whole
  setup by keys alone (its other steps too), the recovery wizard's field by its length only, the pairing
  offer's « Copy code » (the word as drawn) and its focus on « Done ». The Fable maille, by delegation of
  Céline (2026-11-14), on 2026-11-19, after PANDA2, WSL and the laptop all preferred the convention for a
  group of choices.
- SQ10's title reads « The other windows, in this release », so it promises no more than its list (the
  Fable maille's reading).
- Contract written by the laptop.

## 2026-11-19 — v2.0.134: the shell's windows reachable in v0.0.13 (SQ10, lots B and C, specified); the canvas and the chrome wait for a design; Ctrl+Z only where a click would

- **Ch. 12, SQ10:** the windows over the workspace are in the accessibility tree and answered from the
  keyboard in v0.0.13, in lots: B (the setup's welcome and choices, the recovery wizard while locked, the
  pairing offer, the guardian enrolment), C (the bubble-style picker, the right-click menu on a lane's label,
  and in the app the unlock screen's password field named « Password » and the setup's AI-nickname field,
  both labelled for a screen reader). Céline, 2026-11-19: « Et on met tous les éléments dès maintenant. »
  The phone's fields and lot C's cases: the Fable maille, by delegation of Céline (2026-11-14), on
  2026-11-19.
- **The email forms are not shown in v0.0.13** (email is off); v0.0.14's email lot builds them reachable
  from the start (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
- **The canvas and the shell's chrome wait for a design of their own, in the backlog at high priority**
  (Céline, 2026-11-19: « D et E demande une conception spécifique […] A mettre au backlog donc, mais en
  priorité haute. »): the cards, the taskbar, the bubble, the « + » button, the windows' close buttons and
  each window's own controls. They were missing from the first list put to her (the laptop's inventory,
  corrected the same day); she decided on the corrected one. Named in the gaps.
- **SQ7:** Ctrl+Z presses « Undo (Ctrl+Z) » only when a click on it would: never under a prompt, a notice or
  a modal window (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
- **SQ9:** a drawing function is in the tree, or marked for its lot of B or C, « email is off », or « a
  design first »; at the release candidate no lot mark is left, which a second test checks.
- Contract written by the laptop.

## 2026-11-19 — v2.0.133: the shell's questions and notices, heard and answered from the keyboard (specified)

- **Ch. 12, *Accessibility*, SQ1 to SQ9:** the action confirmation, the injection prompt and the
  unsaved-edits prompt become modal dialogs in the accessibility tree, their buttons pressed by a screen
  reader and by the keyboard (Tab, Shift+Tab, Enter or Space; Escape keeps its meaning); a notice drawn in
  a box becomes an alert dialog holding the focus; the trash's toast a status with its button. Every answer,
  by any hand, goes through the gate a click goes through (SQ5). A test names everything the shell draws
  against its tree (SQ9). The lot is Céline's (2026-11-19: « Dans ce cas, ce n'est pas ok. L'accessibilité
  est de base. »; « cela doit faire partie de la release »).
- **Five particular cases** by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19: the
  confirmation's name and its gravity word in ordinary case; a covered prompt leaves the tree; where the
  focus starts in each prompt; Enter presses the focused button in every prompt; the notices and the toast
  in this lot, with Ctrl+Z on the toast's « Undo (Ctrl+Z) », the one drawn change.
- **The gap line** now names the windows that stay out of reach (the pairing offer, the guardian enrolment,
  the recovery wizard while locked, the email forms, the bubble-style picker, the right-click menu, the
  setup's welcome and choices); whether they join v0.0.13 is Céline's, put to her the same day.
- Contract written by the laptop; tests the laptop; code PANDA2.

## 2026-11-19 — v2.0.132: C1 and C4 built; C-LOG built on both hosts

- **Ch. 08, RV1 and RV2 (item C1):** built. Read on main `2cbe6ffc`: `restore` reads the words after
  « restore » in her request, picks the version as RV1 says and pins it when the request is read; the run
  restores the pinned version and never « the previous one » at run time. Both hosts reach it through the
  orchestrator. A line under RV1: versions she cannot tell apart (several in one minute) send her to the
  document's History (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
- **Ch. 08, RT1 to RT5 (item C4):** built. A chat tool that finds no exact match (a write tool, or
  `get_document` and `list_documents`) sets a flag that lasts the turn; the next write in that turn is
  always proposed, with RT4's sentence first. Status written by PANDA2.
- **Ch. 08, C-LOG:** built, its second lot merged at `99a4e380`. Read on that main: the shell's log window
  calls `readable_for_the_person`, `delete_the_quarantine` and `export_for_an_expert` (`log_window.rs`); the
  app's `quarantined_log` commands call the first two, and the app gives its orchestrator Keep and Delete only
  (`set_log_choices`); both hosts handle `QuarantinedLogDiagnosed`; `QuarantinedLogWindow` is mounted in both
  chat views. No Export on the phone, as DG4 says (Céline, 2026-11-18). Status written by PANDA2.

## 2026-11-19 — v2.0.131: item 9 and History's names on both hosts, built

- **Ch. 03, ch. 11, two threads of the name sought (item 9):** built. Read on main `82f2373b`: the app's
  `thread_for_name` and `remember_thread_choice` (registered, gated), the choice sealed in the persona's store
  and never synced, the import's `TerminalChooser` at a terminal, and the one `ThreadChooser` window in the
  browser, the skills panel and the document panel. Status written by PANDA2.
- **Ch. 12, History on both hosts:** built. Read on main `82f2373b`: the phone's `list_commits` names its rows
  through `history_names` (`shown_message`, `local_time`), with this device's paired list and its own id, and its
  restore says when the version is already the current one (HN4). The desktop since item 19.

## 2026-11-18 — v2.0.130: C-LOG's first lot, « partial »; C5, History that cannot be read

- **Ch. 03, History, in v0.0.13 (item C5):** built, on both hosts. Read on main `3b8a9c57`: the app's
  `list_commits` and `histories_not_read` go through the document service; the phone's History and the
  document panel say its sentence; the desktop's rows say it, and offer no restore for a version that
  cannot be read. Status written by PANDA2, after its second run.
- **Ch. 12, History on both hosts:** still partial. The phone's rows still give the stored message and the
  raw time: C5 built HR1 to HR4, not HN1 to HN5 on the phone (read on main `3b8a9c57`).
- **Ch. 08, DG4:** on the phone, no Export in v0.0.13; Keep and Delete only, and its assistant is given
  those two choices (Céline, 2026-11-18, asked by the Fable maille).
- **Ch. 08, C-LOG:** partial. Read on main `cfcaf864`: the log's check runs at login on both hosts (the
  shell's `check_session_log_at_login`, the orchestrator's `set_session_log_key_for` through
  `quarantine_unverifiable`), and the diagnosis tool is in the chat loop. Nothing calls
  `readable_for_the_person`, Export or Delete: DG3 and DG4 are the second lot, in v0.0.13, before any
  release candidate (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-18). Status
  written by PANDA2.
- **LG2, as built:** the record lives in the persona's beacon file, outside the log's directory, written
  with each entry (a technical choice of the code, stated by WSL).
- **DG2:** « the last good verification » made exact: the last state the record holds, written by a
  session whose log had verified, said « The log was last recorded intact on {day} »; the times said to her
  are local, in History's form (the Fable maille, by delegation, 2026-11-18).

## 2026-11-18 — v2.0.129: statuses after the merges of v0.0.13

- **Ch. 05 and ch. 03, a restore that fails is said (item C2):** built. Read on main `e3333eb2`: the
  shell (Previous values) and the app (the list under the alerts) both call the sync layer's one
  restore and show its notice (status written by PANDA2, after the second run on Linux).
- **Ch. 05, PR1 and PR2 (item C3):** built. Read on main `9a5db999`: the desktop asks at `DevicePaired`
  and replaces through the unpairing; the app holds the question before `DevicePaired` is reported, its
  pairing panel asks, and « Replace it » goes through the body of « Forget ». A line added to PR1: History's
  earlier rows from a device paired again read as a forgotten device's, since nothing links its old id
  to the new one (the Fable maille saw it, PANDA2 wrote it). Two sentences that waited for the test (« A
  test first says whether it does », « If the test finds no old record left ») now say what it found: the
  old record stays, and PR2 removes it.
- **Ch. 12, History on both hosts (item 19):** partial, built on the desktop. Read on main `8e39d316`:
  the shell's History names devices and versions through the document service's `history_names`, each
  row's time in local time, and says when the version restored is already the current one. The phone's
  side is C5's.
- **Ch. 11, HTML Export (item 23):** built. Read on main `9600a19f`: both hosts register the skill, and
  its export goes through the filtered rendering.

## 2026-11-18 — v2.0.128: History's rows, in local time, and the row kept before a restore

- **Ch. 12, HN2 (item 19):** « Before restoring {commit id} » is shown « Before restoring the version
  of {YYYY-MM-DD HH:MM} » (or « … an earlier version »); every row's own time is in local time on both
  hosts (the desktop wrote it in UTC, against AT11) (the Fable maille, by delegation of Céline
  (2026-11-14), on 2026-11-18). « Before sync from » is written when a sync replaces an edit made
  here, not at every applied sync (a correction, written by PANDA2). From the laptop's three facts,
  read while writing HN's tests.

## 2026-11-18 — v2.0.127: C2, a restore that answers late

- **Ch. 05, Previous values (item C2):** past the host's bounded wait for the sync node, the notice is
  « No answer from sync yet for {what}. It may still be restored. », never « Not restored »; no node
  or a closed channel stays « sync is not running » (the Fable maille, by delegation of Céline
  (2026-11-14), on 2026-11-18).

## 2026-11-18 — v2.0.126: C-LOG, every failure has its finding

- **Ch. 08, C-LOG DG1:** the closed list gains LG3's plaintext log and LG4's caps and invalid UTF-8,
  so the diagnosis says what each failure is; the most recent quarantine is diagnosed, the others
  named; entries counted from 1. DG3: the breaking entry is counted among those not shown. Written
  by PANDA2, from what the C-LOG test author found missing.

## 2026-11-18 — v2.0.125: C-LOG, a plaintext log is never carried

- **Ch. 08, C-LOG:** LG3 has no exception: a log written only in plaintext, from before encryption,
  is quarantined at the first login of v0.0.13, not carried; LG6 carries only a v0.0.12 log that
  holds no plaintext line; the *Session log* section says the old tolerance ends (the Fable maille,
  by delegation of Céline (2026-11-14), on 2026-11-18). LG2's record sits outside the log's own
  directory, and a log longer than it verifies when each further entry is bound to its place; LG4's
  file cap is above what a session writes before rotation (written by PANDA2, read by the Fable
  maille).

## 2026-11-18 — v2.0.124: the contract lines of v0.0.13's last items

- **Ch. 08, Action targets:** RV1, the version named is the one restored, named « previous » or by a
  date and time as History writes them; several or none, nothing is restored and the day's versions
  are listed (Céline, 2026-11-18, « Date as History shows it »). RV2, a title containing « to » can be
  restored (written by PANDA2). RT1-RT5, a write retried after a no-match in the same turn is always
  shown for approval, even under earned trust; the model still sees the close names (Céline,
  2026-11-18, « A retried write always asks »; the turn as its scope, written by PANDA2; RT4's
  sentence, the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-18). One line
  in *Trust calibration* points to it.
- **Ch. 05:** a failed restore from Previous values is said where it was asked and changes nothing
  (item C2, written by PANDA2). PR1, pairing a device again replaces its record on both sides (item
  C3, written by PANDA2); PR2, when a device joins under a name a paired device has, the source side
  asks « Replace it » or « Keep both », and nothing is replaced without her answer (the Fable maille,
  by delegation of Céline (2026-11-14), on 2026-11-18). *Device names* points to ch. 12's rule for
  History.
- **Ch. 03:** HR1-HR4, History's read errors: the list's sentence, a locked session as a refusal, the
  session's count reported by the phone's sync alerts, an unreadable version kept in its place,
  never offered for restore, on both hosts (the Fable maille, by delegation of Céline (2026-11-14),
  on 2026-11-18, with the sentences « This version could not be read. », « The history of 1 document
  could not be read. » and « The histories of {n} documents could not be read. »; the desktop half
  added by PANDA2). FC1-FC6, two live threads of the name sought by Web, the journal or the import:
  asked once, remembered per persona and device, forgotten when the threads of that name change
  (Céline, 2026-11-13, « Demander une fois, retenir »; where the choice is kept and what forgets it,
  written by PANDA2); the import asks instead of refusing. The trash's 30 days: a clock set wrong can
  empty it early, said, with no guard (Céline, 2026-11-18, « Keep the clock, say it »). A failed
  restore of a thread a peer deleted points to ch. 05.
- **Ch. 07:** AS1-AS3, `auth.store` is never torn, at its first write or a later one, Android
  included (written by PANDA2).
- **Ch. 11:** HTML Export escapes raw HTML in the body and the title (the review's item 23, written by
  PANDA2), and shows as text a link's address that is not `http`, `https`, `mailto`, relative or a
  fragment (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-18); Daily Journal
  points to ch. 03's FC rule.
- **Ch. 12:** HN1-HN5, History on both hosts names devices and versions, never raw ids, and says when
  the version restored is already the current one (items C5 and the review's item 19, written by
  PANDA2); the phone's reader points to it.

## 2026-11-18 — v2.0.123: K6 built

- **Ch. 12, K6:** built in v0.0.13 (item C9, merged 2026-11-18), with what WSL's second run read in the
  app's wizard and login. Written by PANDA2.

## 2026-11-17 — v2.0.122: C-TPII built

- **Ch. 08, C-TPII:** status built in v0.0.13 (lot C11, merged 2026-11-17), with what the laptop's second
  run read in the hosts. Written by PANDA2.

## 2026-11-17 — v2.0.121: C-SWEEP's document half built

- **Ch. 09:** the statuses of PII by reference, the idle sweep, C-SWEEP and resolution say what lot
  LS(a) and C11 built: labels on both hosts, the phone's edit box excepted; the sweep's document half.
  « As built, the phone shows raw tokens » removed. Written by PANDA2 from WSL's second run of LS(a) and
  its reading of the hosts' labels; merged after C11, which shows a suggested link's titles, and after
  the lot that passes every tool turn through the mask, which makes SW3 whole toward the model.

## 2026-11-17 — v2.0.120: an undo by hand counts; `adopt` never through earned trust

- **Ch. 08:** asking the assistant for the opposite action counts as an undo; an external document is
  never adopted through earned trust (Céline, 2026-11-17). Written by PANDA2.
- **Ch. 08, Conversational confirmation:** an undo she can make by hand counts as an undo; AT7's undo
  through the assistant goes to a later release (Céline, 2026-11-17). Written by PANDA2.

## 2026-11-17 — v2.0.119: C-MERGE built

- **Ch. 03, C-MERGE:** status built in v0.0.13 (lot B1, merged 2026-11-17), with what its second run read
  in the hosts; the old behaviour put in the past. Written by PANDA2 from the laptop's second run.

## 2026-11-17 — v2.0.118: C-HOST, AT7's undo as built

- **Ch. 08, C-HOST:** AT7's undo by request id is built in the crate, with no host yet; « in every
  host » corrected. Found by the laptop's second run of B1; written as built by PANDA2.

## 2026-11-17 — v2.0.117: C-TPII, TP4's callers as built

- **Ch. 08, TP4:** the callers that resolve a document by title are the orchestrator's direct requests
  and the chat tools; the import, the skills' bridge and the browser's save resolve threads only. Read
  in the code by the test author of C-TPII. Written by PANDA2.
- **Ch. 08, TP1 and TP3:** a value is compared with its record ignoring case and surrounding spaces,
  as a title is; without the account key the second form is silently absent; the words given are
  echoed as given. The Fable maille, by delegation of Céline (2026-11-14).

## 2026-11-17 — v2.0.116: C-TPII, a title that holds personal data

- **Ch. 08, new contract C-TPII (TP1-TP5):** a document is found by its title as stored, with the
  values detected in the request replaced by their tokens, and as shown with labels; the exact-name
  rule's outcomes counted by document; titles in replies shown with labels. Céline decided the rule,
  its release (v0.0.13) and the title as shown (2026-11-17). Written by PANDA2.

## 2026-11-17 — v2.0.115: C-SWEEP, three particular cases met while coding LS(a)

- **Ch. 09, SW5, the dismissal's key:** a dismissal is the value's record in the dismissed state, matched
  against its encrypted value. The keyed hash was the means of « the store holds no raw value », which
  stands. Written as built, by PANDA2.
- **Ch. 09, SW5, the label's form:** one form, the model's, everywhere a label is shown; never the code's
  name. The Fable maille, by delegation of Céline (2026-11-14).
- **Ch. 09, SW5, one version:** the review's own version, after the usual restore point. The Fable maille,
  by delegation of Céline (2026-11-14). Written by PANDA2.

## 2026-11-17 — v2.0.114: C-SWEEP, the phone's edit box

- **Ch. 09, SW5:** the phone's edit box keeps the stored form of a token, for v0.0.13, as the one
  exception to labels on both hosts (Céline, 2026-11-17). The line above the box and its tie for a
  screen reader: the Fable maille, by delegation of Céline (2026-11-14).
- **Ch. 09, SW1:** « PII confirmed » and « PII dismissed » are cited as Céline's: she keeps the long form
  for the three states (2026-11-17). Written by PANDA2.

## 2026-11-17 — v2.0.113: C-SWEEP, the states' words

- **Ch. 09, SW1:** the suspected state reads « PII suspected - TBC », Céline's label, restored by her on
  2026-11-17; the two other states take the same form, « PII confirmed » and « PII dismissed » (the Fable
  maille, by delegation, by her reason). Written by PANDA2.

## 2026-11-17 — v2.0.112: C-SWEEP, the browser's saved page and taking back any confirmation

- **Ch. 09, SW1:** a page saved from the browser takes the import's origin, not the editor's, so it is
  suspected (it was written with the editor's origin).
- **SW5:** taking back a confirmation makes the value suspected again, whoever confirmed it.
- Both raised by LS(a)'s test author; written by PANDA2.

## 2026-11-17 — v2.0.111: C-SWEEP, SW1 keyed on the write's origin

- **Ch. 09, SW1:** the state follows the write's origin, not its path: only the editor confirms by
  default; the assistant's saves and creations, which share the editor's path, are suspected (Céline,
  2026-11-17, in the Fable maille's session: « uniquement l'enregistrement via l'éditeur. Pas l'import
  de document. »; the path's trap found by the Fable maille). Written by PANDA2.
- **Content shared from another app on the phone** takes the import's origin, so it is suspected (found
  by the laptop: it was written with the editor's origin).

## 2026-11-17 — v2.0.110: C-SWEEP revised, the sweep the same process as a save; D9's sentence on ingest

- **Ch. 09, the idle sweep,** decided again by Céline on 2026-11-17, replacing her decision of
  2026-10-10 (« Nothing in a document is rewritten without the user's confirmation »): the sweep
  replaces values as a save does, and marks them suspected, since she did not see it happen. Put to
  her by PANDA2 and, with its costs, by the Fable maille.
- **C-SWEEP, SW1-SW9,** revised against the code (the test author's reading) and her answers:
  one process with two states; masking where content is read, titles included, with what it does not
  cover named; no gesture on a whole type, several records at once; any record can be dismissed, and
  ingest honours a dismissal; dismissals and review state travel; the phone has the review and the
  signal; one routine in `sovereign-ai`, the app's `pii_sweep.rs` retired. Written by PANDA2.
  - **After the laptop's read:** the token's record id derived from the finding's identity, so two
    devices write the same version; when the sweep runs; what taking back a dismissal or a
    confirmation leaves.
  - **After the Fable maille's read:** the state follows the write's path (confirmed only where she
    sees it, in the editor; suspected on every other path); both hosts show a token as its label
    (Céline, 2026-11-17, for the phone).
  - **After the laptop's read:** a state is never lowered by another device's ingest, in either order of
    arrival.
- **Ch. 01, D9:** a sentence that ingest is how Sovereign keeps content, not an action above
  « observe », and that what it replaced without her seeing waits for her review. The wording is
  the Fable maille's, chosen by Céline on 2026-11-17 over PANDA2's.

## 2026-11-17 — v2.0.109: C-MERGE revised against the code; AT7 for auto-approved actions; C8's two neighbours; K5 and K6

- **Ch. 03, C-MERGE,** revised after the test author's reading and the second positions (WSL, the
  laptop, the Fable maille): the merge's own stamp, so that an older peer's rename no longer undoes a
  merge (found by WSL); an ended thread is the target of nothing, and a chain is followed to its live
  end; MG4's older-peer cases as they now are; the trust count starting at zero. Written by PANDA2.
  - **The undo is asked of the assistant** (Céline, 2026-11-17: « Assistant seul »).
  - **The words and the small behaviours:** the Fable maille, by delegation of Céline (2026-11-17).
  - **MG2 no longer draws the source's lane joining the target's,** on either host: the mark says
    where it was merged, which meets Céline's « an end date with an indication that it has been merged
    into another thread ». The drawn join came from the first draft, not from her (PANDA2, for
    simplicity; raised by WSL).
  - **MG5: the merge's undo at any time, by naming the thread,** since the two fields are the whole
    merge; refused if the thread is not ended (the Fable maille, WSL). AT7 says that the other
    undos are still kept in memory only.
  - **The words for a chain** name the live end (the Fable maille's correction to its own).
- **Ch. 08, AT7:** an action approved by earned trust keeps its undo (PANDA2, with B1).
- **Ch. 03, *What cannot be read is never shown as empty*:** the list of alerts and the list of
  orphans join its places (PANDA2, for C8).
- **Ch. 12, K5 and K6:** K5 names its four values; K6, the main password's `autocomplete`
  (Céline, 2026-10-28 and 2026-11-17).

## 2026-11-16 — v2.0.108: C-SUP built on both hosts

- **Ch. 05, C-SUP,** status: built for v0.0.13 on the shell and the phone app, held by tests on both
  hosts. The sentence « the node runs as a task that nobody watches » is gone; what is not built is
  said (the shell has no logout, switch or lock yet), and so are the two points of SU2 that no test
  holds (the Fable maille's read).
- Written by PANDA2 after the merge of lot LQ, from the code read in each host; no new decision.
  Raised by the Fable maille.

## 2026-11-16 — v2.0.107: C-AREA built on both hosts; the general rules' « to come » settled

- **Ch. 07, C-AREA,** status: built for v0.0.13 on the shell and the phone app, held by the walks'
  tests. AR7: the shell keeps the browser's profile in the open persona's area; the phone app has
  no embedded browser; the frozen desktop app still uses the default profile.
- **Ch. 01, *General rules*:** the three rows waiting on L1c's follow-up now say where each applies
  as built.
- Written by PANDA2 after the merge of L1c's follow-up, from the code read in each host; no new
  decision. The third row holds with the Recovery refusal in the shell's one form (the Fable maille,
  by delegation of Céline, 2026-11-16).

## 2026-11-16 — v2.0.106: C-SUP, the setup's notice wins at a login

- **Ch. 05, C-SUP, SU2.** When a login also has the setup's notice to show, the setup's notice is the
  one shown, and the stop stays in the status line; its notice comes at the first gesture that needs
  the node (the Fable maille, by delegation of Céline, 2026-11-16). Found while reading the shell's
  login paths for a start that fails.

## 2026-11-16 — v2.0.105: C-SUP, a start that fails is said as a stop; the decisions taken while building it

- **Ch. 05, C-SUP, SU1.** A start that fails is said as a stop, with the same words and gesture; the
  reason goes to the log (Céline, 2026-11-16). The stop reaches the screen after the login has
  answered, so the interface's reset at login never clears it (found by the Fable maille).
- **SU2,** by the Fable maille, by delegation of Céline:
  - while sync is stopped, a gesture that needs the node shows the stop's notice again (2026-11-15);
  - the status line puts « Sync stopped » first and keeps the rest (2026-11-16);
  - the app's notice closes with « Dismiss » (2026-11-16).
- **SU4.** A stop ends with its session (the Fable maille, by delegation of Céline, 2026-11-15).

## 2026-11-15 — v2.0.104: the recovery store is in `crypto/` on both hosts; `recovery/` is a recovery in progress

- **Ch. 07, C-AREA, AR2's table.** v2.0.103 still said `recovery/` for the shell. Both hosts open the same
  recovery store on the same folder: the four `crypto/` files. `recovery/` is something else on both
  hosts, an access recovery in progress, started before any login; it gets its own row, the primary's only,
  since what it recovers is the primary's account. A primary login clears it, a duress login leaves it alone.
- Found by the Fable maille, reading v2.0.103 in the code of both hosts; confirmed by the laptop maille,
  whose reading gave the verdict of `recovery/`.

## 2026-11-15 — v2.0.103: AR2 names the recovery store by what it is, on each host

- **Ch. 07, C-AREA, AR2's table.** The row said `recovery/`; the phone keeps its recovery store in
  `crypto/` (four files). The row now names the store on each host. A duress session's guardian
  decoy answers as a persona with no roster, whatever the primary's card says.
- Found by the laptop maille while writing the phone's guardian-decoy test; the decoy's answer was
  raised by the Fable maille.

## 2026-11-15 — v2.0.102: C-SUP, the P2P node's supervision (lot LQ)

- **Ch. 05, *The node's supervision — contract (C-SUP)*,** from Céline's « Lot avant rc1 +
  surveillance » (2026-11-13). Six guarantees: the node's end observed whatever its cause, the host
  saying that sync stopped, a restart with the session's keys, never a node in a duress session, and
  no failure reported for a stop the host asked for.
- **The restart is a gesture of the user's,** and the words are set (the Fable maille, by delegation
  of Céline, 2026-11-15). Nothing restarts the node by itself in v0.0.13.
- **SU5** covers a logout, a switch, a lock and the application's exit; **SU6** asks the tests to hold
  each host's call (Fable maille).
- The guarantees were proposed by the laptop maille, which codes LQ.

## 2026-11-15 — v2.0.101: C-MEDIA and the wire version built (lot L1b merged)

- **Ch. 03, C-MEDIA:** « built for v0.0.13 », with its test files; the old as-built paragraph is kept
  as « until v0.0.13 ». Still to come: inserting a medium from any host (no shipped host offers it;
  found by the Fable maille, checked by PANDA2), and showing a document's media, with the inert SVG.
- **Ch. 05, *Peers on an older build*:** a status line, « built for v0.0.13 », wire version 2 carried
  by each document request.
- **Ch. 01, *General rules*:** the two « to come » that waited for L1b are removed (the media notice,
  rule C).
- L1b was merged after PANDA2's second run, in two rounds. No decision; the record follows the code.

## 2026-11-14 — v2.0.100: AT6 built in the shell; one word in ch. 01

- **Ch. 08, *Action targets*.** AT6's follow-up was merged after the laptop maille's second run: the
  shell's prompt keeps the request id and the deadline, the answer is bound to the prompt on screen,
  and the modal closes at the carried deadline. The « not yet » of v2.0.95 is removed.
- **Ch. 01, *The user's role*.** « is not anticipated » said more than her « ne pas trop anticiper »:
  it now reads « not anticipated far ahead » (Fable maille).

## 2026-11-14 — v2.0.99: ch. 01, which start « the user's role » speaks of (Céline)

- **Ch. 01, *The user's role*.** Asked whether « surtout dès le début » meant the product's first
  versions or a user's first days, Céline answered: « dans le contexte de Sovereign, c'était dans la
  phase d'expérimentation de la plateforme. Je préfère ne pas trop anticiper les besoins de
  l'utilisateur au delà de ce qui est communément admis, pour ne le construire que si c'est
  réellement nécessaire. » Her sentence stays as it was; one clause says which start.

## 2026-11-14 — v2.0.98: ch. 01's two new sections get their status lines; places not yet built say so

- **Ch. 01, *The user's role* and *General rules*.** Every section carries a status line (README,
  *Reading the status lines*); these two had none. *The user's role* is « Decided », a stance;
  *General rules* is « Partial », each rule with its own status in its chapter.
- **The places not yet on main say « to come »:** AR2 before login and the shell's refusals (L1c's
  follow-up), the media notice and the refusal by role (L1b). Raised by the Fable and laptop mailles.

## 2026-11-14 — v2.0.97: ch. 01, the user's role and the general rules (Céline)

- **Ch. 01, *The user's role*.** Céline's sentence enters the vision: « l'utilisateur a des
  responsabilités et un rôle à jouer : ne pas vouloir tout faire à sa place ou tout faciliter,
  surtout dès le début. »
- **Ch. 01, *General rules*.** Asked whether one place listed the general rules, Céline decided that
  it is ch. 01, and public: « oui, elles peuvent tout à fait être publiques, c'est même mieux si un
  jour il y a d'autres contributeurs ».
  - The section indexes twelve rules, each with where it is stated in full and where it applies.
    A rule enters with the cases it has decided.
  - Eight were already in the spec. Four were formulated while deciding implementation cases, by
    the Fable maille, by delegation of Céline (2026-11-14), and are stated here in full.
  - The material was prepared by the Fable maille.

## 2026-11-14 — v2.0.96: the media notice says only what is built

- **Ch. 03, *The media store*.** Two clauses of the notice after the pass said what no build does:
  « show as « not on this device » », which no host renders, and « so that a later pass can repair
  it », with no repair pass built. The notice now says « M could not be copied. » and « The list of
  what was not copied is kept, encrypted, on this device. »
  - What Céline decided (« Ajouter le registre ») stays whole. The wording is the Fable maille's, by
    delegation of Céline (2026-11-14), on the laptop maille's report.
- **The neutralised block** is « to be shown » as « medium not on this device »; as built, no host
  shows a document's media blocks.
- **No screen shows the list** of what was not copied: the user learns how many, not which.

## 2026-11-14 — v2.0.95: AT6 is not yet built in the shell

- **Ch. 08, *Action targets*.** v2.0.94 said AT1 to AT12 were built in the shell. The Fable maille
  read that the shell still answers on the unbound decision channel, drops the request id and the
  deadline, and counts down on its own clock.
- **The status now says** « not yet » for AT6 in the shell. It is built in v0.0.13, as C-HOST
  whole was decided.

## 2026-11-14 — v2.0.94: C-HOST built (lot LH merged); the « As built » lines follow

- **Ch. 08, *Action targets* and *Action Gravity*; ch. 11, *A skill asked for in the chat*.** LH was
  merged into main after the laptop maille's second run. The status lines say « built for v0.0.13 »,
  and the « as built » sentences that described v0.0.12 now say what changed:
  - level 3: `find_replace` and `duplicate` are proposed and run;
  - the shell's and the phone's paths go by id;
  - AT10 replaced the chat tool's fallback.
- No decision; the record follows the code.

## 2026-11-14 — v2.0.93: v2.0.92's two retouches

- **Ch. 11 and the v2.0.92 entry:** the three questions about the assistant's import are the
  implementer's, not Céline's list, which stays open (Fable maille).
- **The v2.0.92 entry cited a private decision number;** it is now cited by its date and who took it,
  as the rule of v2.0.72 says.

## 2026-11-14 — v2.0.92: the assistant's import is postponed (Céline); what it says of export

- **Ch. 11, *A skill asked for in the chat*, and ch. 08 (AT9, the Action Gravity table).**
  - **Asked by the Fable maille,** Céline answered: « reporter l'import, il y a trop de questions de
    sécurité pour l'ajouter maintenant ». The assistant does not propose `import_file` in v0.0.13.
  - A later lot first answers the security questions of an import started by the assistant, which she
    has not listed. The implementer met three things it could not settle: a path never guessed, the
    thread (AT10), the provenance.
- **AT9's export sentence:** the assistant says that the Skills menu runs an export but saves no file
  yet. The earlier line sent the user to the menu as if it saved one. The wording is the Fable
  maille's, by delegation of Céline (2026-11-14).

## 2026-11-14 — v2.0.91: C-WIRE, the version rides on the document requests; nothing is remembered per peer

- **Ch. 05, *Peers on an older build*.** v2.0.90 closed the laptop maille's gap with an announcement
  that the answering side remembered per peer. The Fable maille showed how that memory fails: a note
  outliving its peer (a device put back on v0.0.12), and the order of two messages on two streams.
- **Now** the requests that fetch and push documents carry the requester's wire version, and the
  answering side judges by the request it answers. No version, or a lower or higher one: refused
  for documents, and the refusal is said, not answered as an empty list. Rows go on.
- **Toward an older requester, the refusal is the error response it already knows** (Fable maille;
  checked at tag v0.0.12 by both mailles: it logs it and goes on with its rows).
- **v2.0.90's announcement is withdrawn.** Technical; no new decision.

## 2026-11-14 — v2.0.90: C-WIRE, a peer is known by the version it announced; this device's sentence and AT7's reply (Céline)

- **Ch. 05, *Peers on an older build*.** Found by the laptop maille while building L1b: versions
  were noted only when this device asked for the peer's manifest. A v0.0.12 peer that started every
  round was never noted, so it was served documents and its states were applied.
- **Now:** a build that starts a round first announces its manifest, through the push both versions
  already accept. A peer that has announced no version is refused for documents, and rows,
  shards and backup fragments are unchanged. A lost announcement costs one round.
- It makes « whichever side starts » (guarantee 12, v2.0.86) true; no new decision.
- **This device's sentence, when it is the older one:** Céline kept the proposed text (« Le
  garder »).
- **C-HOST, AT7 (ch. 08):** a move's undo refused because its thread is gone names the thread
  (Céline: « Nommer le fil »).

## 2026-11-14 — v2.0.89: AT7, a move's undo when its thread is gone; the assistant renames threads only

- **C-HOST, AT7 (ch. 08).** Raised by the Fable maille, reading the LH tests:
  - a move's undo is refused when the thread it came from is gone since, and says so. It follows
    Céline's « Refuser et le dire »: nothing overwrites a later change;
  - the list of undoable actions named « renaming a document », which the assistant cannot do yet.
    The line now says so.

## 2026-11-14 — v2.0.88: « before any login » means on a device that has an account

- **C-AREA, AR2 (ch. 07).** Setup writes an identity before any login, and must, on a device with no
  account yet. v2.0.87's sentence now says « on a device that has an account ».
- **The commands that create an account or answer its designation** are refused once the device has
  an account, inside a duress session too. A check that names no persona, such as the new password's
  policy, still answers, since recovery uses it from the login screen (Fable maille).
- **Raised by the Fable maille,** which found a third command answering the primary's identity with
  no session: the setup's designation.

## 2026-11-14 — v2.0.87: C-AREA AR2, the identity before login, and the duress names on an older install

- **Found by the Fable maille, reading L1c's code on main.**
  - **On the phone,** two commands allowed before login answered and wrote the primary's identity. A
    process with no session fell back to the primary's directory. ch. 07 already said « before login
    nothing shows it »; AR2 now says it as a guarantee, on both hosts, and a session-less process has
    no identity directory.
  - **The duress persona's random names were drawn in the setup wizard only.** An install set up
    before v0.0.13 never passes there. The draw now also happens at the first duress login that
    finds no names of its own.
- Both precise AR2 as decided on 2026-11-13; no new decision.

## 2026-11-14 — v2.0.86: the media cap, the notices, the wire version, the guarded copy for every document, undo and AT1 in the chat (Céline); the media reference names the store

- **The cap (ch. 03, ME3).** Céline chose 4 GiB, on the laptop maille's proposal. A larger file is
  recorded as « too large ».
- **The notice after the pass.** Céline chose « Ajouter le registre ». The text says what was copied,
  that the originals stay where they are, how many could not be copied, and that the list of what
  was not copied is kept, encrypted, on this device, so that a later pass can repair it.
- ***Media reference*:** from v0.0.13 a media block names the store's medium (`ref.store`), not an
  outside path.
  - **A device still on v0.0.12 does not receive such a block** (below).
- **The wire version goes up with the document format (ch. 05, *Peers on an older build*).**
  - The Fable and laptop mailles found the gap: « older » meant « no wire version ». v0.0.12 and
    v0.0.13 both announce version 1, so Céline's rule of 2026-10-15 would not have fired between
    them.
  - **Asked,** Céline answered « Oui, l'appliquer ». The wire version goes to 2, and « older »
    becomes « a lower wire version, or none ».
  - **The refusals are the new build's:** it sends no document state, serves no request for one,
    and applies none. v0.0.12 compares no numbers, so it cannot refuse by itself.
  - **What she sees:** between the first device updated and the last, documents stop syncing.
    Threads and the other rows keep syncing, and both screens say which device to update.
  - **After the readings of the laptop and Fable mailles:**
    - **an older peer's purge markers are still applied.** A marker carries no document content, and
      a purge is her intent;
    - **the rule holds from both sides.** A build that meets a higher wire version refuses in the same
      way, and says that this device is the one to update. v0.0.12 lacked that half.
- **An older version in a document's history,** restored after the update, follows the pass's rule.
  The contract was silent on it, as the C-MEDIA test author found.
- **The pass's guarded copy holds for every document, whichever device wrote it last.**
  - **Found by the Fable maille, confirmed by the laptop maille:** the earlier rule neutralised every
    block of a document whose current state was written on another device. An image inserted on the
    desktop, followed by a word corrected on the laptop, would have gone « not on this device » on
    the desktop that holds it.
  - **That rule was not Céline's.** Her answer of v2.0.81 was about a document received and then
    saved here.
  - **Asked, she first read the history as recording each version's device.** It records the date,
    and what followed: a sync from a named device, or a local save. Judging each image by the edit
    that inserted it would have meant reading the history's chain.
  - **Asked again, she chose « Plus simple : copier partout ».**
  - **What follows from it:**
    - a block with no recorded hash is neutralised under a reason of its own, « no recorded hash »;
    - a video's thumbnail path follows the same guards;
    - the record is never synced or backed up by the database either;
    - a fourth test follows the life of a document across two devices.
- **The notice when the pass fails in the background:** Céline kept the laptop agent's text (« Le
  garder »).
- **C-HOST (ch. 08):**
  - **AT7:** an undo is refused if the object changed since the action, and the reply says why
    (Céline: « Refuser et le dire »).
  - **AT1:** the rule holds for every chat tool. A tool proposal whose object is not found is never
    shown (Céline: « Oui, partout »).
  - **AT9:** the export is withdrawn from the assistant in v0.0.13 (Céline: « Retirer l'export à
    l'assistant »), since no host saves an exported file yet.
- **C-AREA, AR4 (ch. 07):** in a duress session the screens say that nothing runs. Someone who knows
  Sovereign may notice it, and Céline accepted that risk (« Laisser ainsi »).

## 2026-11-14 — v2.0.85: the inert SVG binds the lot that first shows media; the record says « too large » and « unreadable »; a received version is neutralised too

- **C-MEDIA (ch. 03).** v2.0.84 made « an SVG is shown inert » a requirement « of the lot ».
- **The C-MEDIA test author found that no host shows a document's media yet:** the editor draws a
  placeholder, and the app's interface shows no document images. There is nothing to test in the
  lot that copies media.
- **The requirement, with its test, now binds the lot that first shows a document's media,** on each
  host.
- **A precision from the laptop maille's reading:** the app's Markdown preview of a document's text
  does show images named in the text, bounded by the app's content security policy, and an SVG
  there is inert by the browser.
- **The record's reasons** (raised by the Fable maille): a file above the cap, or one that cannot be
  read, would have been recorded as « absent » while it is on her disk.
  - Two reasons are added: « too large » and « unreadable ». A cap found too low can then be mended.
  - The record's encryption gets a marker test.
- **A received state kept as a version** (C-WIRE 7) is covered by the neutralisation on apply. A
  later restore of it has no outside path to read, and none is kept before the pass has run.
  Raised by the Fable maille, checked in the code: `apply_wire_version` stores the received content
  as it arrived.

## 2026-11-14 — v2.0.84: the migration pass's refusals, its list of formats (SVG included) and its record of what it neutralises (Céline)

- **C-MEDIA (ch. 03), *The media store*.** The migration pass said that « ME2's lexical refusals
  come first », and ME2 refuses absolute paths.
  - **The contradiction:** an outside path is absolute by nature, so read literally, the pass would
    have neutralised every image.
  - **Now:** the pass applies the other refusals (a network path, a device name, `..`, the `\\?\`
    prefix, an alternate data stream) without any file-system call.
  - **Found by the C-MEDIA test author** while writing the pass's test.
- **The formats, and what is neutralised.** Raised by the Fable maille: the tests left the list of
  formats to the implementer, and neutralising drops the path. As built, no host filters formats
  at insertion. A short list would have turned her images of another format into « not on this
  device », with no way back.
  - **Asked,** Céline chose « Liste large + registre »:
    - a broad list of formats in the contract, each with its test;
    - an encrypted record, in the persona's area, of every block the pass neutralises, with its
      former path and the reason. A list found too short can then be mended by a later pass.
  - **The third proposal, a counting run before the update, was not retained.**
  - **SVG, asked separately:** « Les accepter ». An SVG is shown without running its scripts and
    without loading anything it references.
- **After the readings of the laptop and Fable mailles:**
  - **the two weak members of the list get a check as strong as the guard's purpose.** An SVG is a
    file whose root element is `svg`, so an XML file with another root is not a medium. A BMP is
    accepted on a coherent header, not on « BM »;
  - **the record never leaves the device.** It is kept outside every synced store, since its paths
    name her folders.

## 2026-11-13 — v2.0.83: the session locks at the device's sleep, and the lock clears the key (Céline)

- **New section, ch. 07, *The lock at the device's sleep*.**
  - **As built,** neither host locks. The phone's session stays open when the screen goes off and
    comes back, and its screen protection is not a lock. The shell has no logout and no lock.
  - **Asked whether a lock was planned,** Céline decided: « A la mise en veille de l'appareil
    seulement pour commencer, le verrou efface la clé. A programmer avec C-SESS ».
- **What the section says:**
  - the trigger: the device's sleep, and only that, to begin with;
  - the lock clears the session's keys from memory, so an overlay does not meet it;
  - it is built with C-SESS;
  - what that contract will have to say;
  - what this does not protect against: a device taken while awake, and keys already paged out to
    the swap file.
- **This closes the open question « whether the shell gets a lock »:** both hosts get one.
- **After the readings of the laptop and Fable mailles, the contract's list gained four points:**
  - the lock ends everything a switch ends (AR4);
  - the sleep signal is named per platform, low-power sleep included, and tested on the machines;
  - what is unsaved is saved before the keys are cleared;
  - whether key memory is pinned.
- **One limit is now said plainly:** a computer set never to sleep is never locked by this rule. Many
  computers are set that way on mains.

## 2026-11-13 — v2.0.82: at a switch, an approval counts in its own persona only, and the interface starts from nothing

- **AR4 (ch. 07), from the walk's fifth round.** A proposal the primary left pending could still be
  approved after a switch to duress. The action failed against the duress store, but the approval
  was counted in the primary's earned trust. AR4 now says that an approval counts in its own
  persona only, since the orchestrator's state is the persona's (ATREST-011).
- **AR4, the interface** (raised by the Fable maille). The rule « a switch ends everything » was
  about the backend only, while the interface holds state of its own in memory. At a switch, the
  interface now starts from nothing; its test lives in the interface's own suite.
  - Today the one login that follows other screens in the same interface is the phone's setup that
    ends « incomplete ». A lock, or C-SESS, would add more.

Both precise what « a switch ends everything » (v2.0.80) already meant; no new decision.

## 2026-11-13 — v2.0.81: C-MEDIA migrates every document once at the update, instead of copying at the next save

The Fable maille found two cases that the lazy copy of v2.0.79-v2.0.80 left open:
- **a round trip through another device lost a document's images,** when the document had not been
  saved since the update on the device that held them;
- **a path received before the update** would still be copied at the owner's next save.

**The rule now (ch. 03, C-MEDIA, *The media store*):**
- **A migration pass at the update,** on each device, before any sync is applied. Each outside path
  is refused lexically, or read under the cap and copied only if its hash matches the recorded one.
  Otherwise it is neutralised.
- **Insertion is then the only gesture that copies.**
- **A neutralised block resolves by its hash** in the local store.
- **The cost, said in the notes:** disk space and time at the first launch.
- **A document last written on another device** has its paths neutralised by the pass, whatever the
  file.
- **The residue, a choice of Céline's.** The laptop maille found that the hash does not bound every
  case, because a received block's hash was filled on the receiving device. For a document received
  before the update and then saved here, the code cannot tell a peer's path from her own image.
  Asked « in doubt, copy or remove », she answered « Dans le doute, copier ».
  - **That first question held two errors,** found by the Fable and laptop mailles:
    - removing in doubt would have removed every image inserted before the update, not « some »;
    - « nothing leaves the device » was false, since a block's hash syncs.
  - **The PANDA2 maille put it to her again, corrected and with two guards:**
    - only a real image or video is copied;
    - a medium copied in doubt never leaves the device until she has seen and kept it.
  - **Her answer:** « Copier, avec les garde-fous ».
- **After the readings of the Fable and laptop mailles:**
  - **the pass reads a file's first bytes first.** A file that is not an image or a video is
    neutralised there and never read further;
  - **every medium the pass copies is marked « copied by the migration ».** The code cannot tell
    the doubtful copies from the others, so the second guard rests on this mark, which cannot be
    rebuilt later;
  - **« received content never has its paths read » now says « from the update on, except once, by
    the migration pass »;**
  - **three tests instead of two:** the legacy test was split into a document last written
    elsewhere (never read) and the residual, last written here (read at most once).

When to make the copy is a technical choice, PANDA2's. Céline is told of its cost.

## 2026-11-13 — v2.0.80: a switch of persona ends everything the previous session started; the generated names' risk accepted

- **AR4 (ch. 07),** after the Fable maille's reading: a switch ends the previous session's node and
  keys, and also its timers and background tasks (the hourly purge, the sweep) and its pending
  proposals. A test forces the purge's tick and the sweep's pass after a switch. The walk alone
  could not see it: it lasts seconds, and the purge is hourly.
- **The duress persona's generated names (ch. 07, AR2).** A generated name next to a typed one may
  look generated. Asked whether the wizard should also propose generated names for the primary,
  Céline answered « Laisser ainsi »: the risk is accepted, and said.
- **C-MEDIA (ch. 03), a hole closed.** The laptop maille, which codes L1b, found that an owned document
  received by sync and then saved by its owner would have its peer-named path read. A received
  state's outside paths are now neutralised when it is applied: the block keeps name and hash and
  shows « medium not on this device ».

## 2026-11-13 — v2.0.79: a duress session runs no P2P node; the setup markers written once; where the phone says what it cannot read

- **AR4 (ch. 07).** Céline, asked what a duress session does on the network: « Aucun moteur ».
  - **In a duress session, Sovereign does not sync:** no node, nothing announced, no pairing. Guardian
    enrollment keeps its decoy screen.
  - **At a switch, the primary's node is stopped and its KEK cleared.**
  - **Pairing shows the same decoy as guardian enrollment.** The laptop maille raised it as a
    question of plausibility. Céline: « Le même leurre que les gardiens ».
  - **What this does not protect against:** the device is absent from the network while a duress
    session is open.
  - This replaces v2.0.77's « sync restarts under the active persona's device key ».
- **AR2's table (ch. 07),** after the walk's second setup order, duress first, which caught four « read
  only » files being written:
  - `onboarding_started`, `onboarding_done` and `account_key.migrated` are « written once, by whichever
    persona logs in first », and say nothing of which;
  - `crypto/device_id` is never created by a duress session, since it starts no node;
  - the device preferences get their own file, `device_preferences.json`;
  - each read-only file's reason says what it holds.
- **ch. 03, the unreadable rule:** where the phone says it (the screen where it happens, and its sync
  alerts); sync's case waits for the convergence lot.
- **ch. 03, C-MEDIA, after its tests were written:**
  - only the user's own save of a document she owns copies an outside path;
  - received, imported and assistant-written content never has its paths copied or read;
  - conversion and duplication copy nothing;
  - after the copy, the block names the store's medium;
  - C-DOC's G18 is replaced on its first clause.

## 2026-11-13 — v2.0.78: what the first tests taught the contracts; undo for rename, move and merge; one rule for what cannot be read

The first tests of v0.0.13 were written against the wave-1 contracts: LH's by an agent of the PANDA2
maille, and the C-AREA walk by an agent of the laptop maille. Their authors reported what the
contracts left unsaid, and the Fable maille read both. This entry settles them.
- **C-HOST (ch. 08):**
  - **AT4:** for Destruct, an edit inside a contained document counts; a target deleted meanwhile
    is refused; « proposed again » is a new proposal with its own request id.
  - **AT6:** the proposal carries its deadline.
  - **AT7:** Céline, 2026-11-13: « Renommer, Déplacer, Fusionner ». These have an undo; creating a
    document and restoring a version do not.
  - **AT8:** the three shell paths still by position are named, each to go through a shared
    function that takes an id.
  - **AT1, `restore`:** the author device when the version records one. Today none does.
- **C-AREA (ch. 07):**
  - **AR2:** a verdict for each shared file the walk met, either « into the area » or « an
    exception, may change or read only », each with its reason. A file not in the table fails.
  - **What the duress persona's copies start with,** raised by the laptop maille as a question of
    plausibility. Céline: « la persona de contrainte a des noms d'appareil et nom de profil random
    générés automatiquement. L'utilisateur peut les changer s'il le souhaite ensuite en se loguant
    sur cette session. »
  - **AR8:** the check finds the persona-blind path functions itself, to a fixed point. It found 13,
    where the spec named four.
- **ch. 03, *What cannot be read is never shown as empty*:** one rule for a class found in four
  places. What cannot be read is kept, counted and said. Its boundary: another persona's rows look
  absent, by C-AREA.

## 2026-11-13 — v2.0.77: v0.0.13's first contracts (L0, wave 1): C-AREA, C-MEDIA, and C-HOST completed

Céline, 2026-11-13: « on continue, j'aimerais que vous puissiez commencer à coder ce soir ». The
contracts that need no new decision of hers come first.
- **C-AREA (ch. 07, *One persona, one area*; lot L1c).**
  - AR1 to AR5: the duress walk over every feature; shared files named with their reason; the
    paired list per persona, never emptied on a decryption failure; no primary key in a duress
    session after an incomplete setup; the walk extended at each release's end.
  - It is decided in the scope of 2026-11-04 (« (a) Tout, + 3 petits »), and the walk at each
    release is from the same day.
- **C-MEDIA (ch. 03, *Received content is hostile*; lot L1b).**
  - ME1 to ME4: a received path is never opened; media are resolved inside the media store only;
    streamed and bounded; every caller.
  - Content-addressed media is decided as a direction, not built in v0.0.13.
- **C-HOST completed (ch. 08, *Action targets*; lot LH):** AT1 for every action, and AT9 to AT12.
  Three of them are Céline's answers of 2026-11-13:
  - **AT10:** « Demander le fil ». With no thread named, `create_document` asks.
  - **AT11:** « Heure locale partout ».
  - **AT12:** « Garder le nouveau nom. Dans la fenêtre de confirmation, l'assistant propose le
    nouveau nom "Les Archives" entre guillemets pour que l'utilisateur puisse s'assurer que c'est
    bien ce qu'il voulait. »
  - **AT9** is her decision of the same day on a skill asked for in the chat (ch. 11).
  - The as-built passages these replace are marked as such.

**After the second positions of the laptop and Fable mailles (same evening):**
- **C-MEDIA:**
  - **The media store is defined.** Céline: « oui copie chiffrée des médias dans sovereign ».
    Sovereign keeps its own encrypted copy, in the persona's area. Inserting copies. Existing
    documents are copied at their next save.
  - **ME1** names what « from outside » means.
  - **ME2:** lexical refusals come first, then a resolution inside the store.
  - **ME4** names the seven call paths and two more test sources.
- **C-AREA:**
  - **AR1** covers the features as built: the browser is added, the P2P steps use a second node, and
    a feature it cannot drive fails.
  - **AR2's exceptions** say whether they may change or only be read.
  - **AR6,** a differential walk, so that reads count.
  - **AR7,** nothing is written outside the directory. The browser gets one profile per persona.
    Céline: « Un profil par persona ».
  - **AR8,** a ratchet on `sovereign_dir()`.
- **C-HOST:**
  - « to be built in v0.0.13 »;
  - where it lives (D16);
  - **AT9:** only the user's own message asks, and a skill runs at its level, whatever that is.
- **The personal-data sweep's queue** (ch. 09). Céline: « Un signal discret » and « Par type de
  donnée ».

**The rest of wave 1, the same night:**
- **C-MERGE (ch. 03, lot L5, B1):** MG1 to MG6. A merge keeps the source thread, with an end date
  and « merged into ». It is reversible, and at level 3. Céline: « Niveau 3 normal ». This replaces
  v0.0.12's « merging always asks ».
- **C-LOG (ch. 08, lot L5, B2 and the log's format):**
  - LG1 to LG6: entries bound to their place, roll-back detected, nothing unsealed, bounded and
    streamed, checked at login, the older format carried once;
  - DG1 to DG5: the diagnosis from facts, never from the altered text, with keep, export or delete.
  - From the design of 2026-10-26.
- **C-SWEEP (ch. 09, lot LS):** SW1 to SW9. One contract for documents and messages, with Céline's
  cadence and form.
- **AR8 (the Fable maille's reading):** it counts every persona-blind path function, and the counts come from the
  check, not the spec.

## 2026-11-13 — v2.0.76: two releases, v0.0.13 (consolidation) then v0.0.14 (email, read-only); the personal-data sweep in two parts; the server's read flag read, every folder but spam and trash

**Decided by Céline on 2026-11-13.** The Fable maille, a second position on the release's scope,
put the questions to her and relayed her words (the private coordination channel, its folder of
that name).
- **The cut:** first « d'accord pour découper la release en 2 parties parallèles ou successives si
  cela a du sens », then « et oui pour les 2 releases successives ». The question she answered was:
  « La coupe en deux releases successives, v0.0.13 pour la consolidation et v0.0.14 pour le courrier
  en lecture seule, tes décisions du 13/11 marquées « v0.0.13 » suivant le courrier. »
  - So her decisions of the same day marked « v0.0.13 » apply to v0.0.14, the email release:
    « Lecture seule en v0.0.13 », « La cible dès v0.0.13 », P14 « dès v0.0.13 ». They are quoted
    as she said them, with that note (ch. 13).
- **The sweep:** « on coupe le balayage des données personnelles, première partie sur les docs, cela
  découple cette fonction du mail, tout en obligeant à la concevoir aussi pour le mail. » Documents
  in v0.0.13, messages in v0.0.14, one contract for both (ch. 09).
- **Read state:** Sovereign reads the server's read flag and never sets it. Asked whether Sovereign
  should show as read what she had already read in another mail client, she answered « ok » (ch. 13).
  ch. 13 also notes, as built, that the dormant engine opens the inbox read-write and marks what it
  fetches as read, which the email release changes.
- **Folders:** « l'ensemble des dossiers […] tous les courriers doivent être lisibles », and « non pour
  spam et corbeille pour le moment, je veux voir à l'usage si c'est vivable. je pense que oui »
  (ch. 13).

- **What P1's immutability covers** (raised by the Fable maille, read by the laptop maille, asked by
  the PANDA2 maille): « L'original reçu ». The message as received, its raw original kept encrypted,
  is immutable. Its stored body is tokenised at ingest, as for documents.

**Where:**
- ch. 01 (the P13 sentence; C-COMMS; P14; P1's row);
- ch. 02 (*Network egress*, v0.0.14);
- ch. 09 (*Idle sweep*);
- ch. 13 (*Email*, with a note on the release numbers);
- ch. 15 (*Roadmap*: the two releases; the week of trial after the email release).

## 2026-11-13 — v2.0.75: email in v0.0.13 is read-only, fetched by each device, with no write to the server

Decided by Céline on 2026-11-13, in answer to the question whether her strategy session changes
v0.0.13 (put to her by the PANDA2 maille, which relayed her words):
- **Read-only:** « Lecture seule en v0.0.13 ». Sending comes in a later release. This settles P13
  for v0.0.13 (ch. 01).
- **Fetching:** « La cible dès v0.0.13 ». Each device fetches by IMAP from v0.0.13 on; no device
  fetches for the others (ch. 13).
- **No write to the server:** « Aucune écriture au serveur ». Marking read stays on the device, and a
  delete hides the message on the device. For v0.0.13 this replaces her decision of 2026-10-30
  that marking read goes back to the server. Read state is local to each device, a known limit.
- **The password on each device:** « Au choix de l'utilisateur ». Entered on each device,
  or kept account-wide, encrypted and synced, as for the keys of model providers (P17b).

Where: ch. 01 (*Decided on 2026-11-13, from the session's proposals*, the P13 sentence), ch. 13
(*Email*), ch. 15 (*Roadmap*, v0.0.13).

*— maille Sovereign, laptop (Claude Opus 5.5), 2026-11-13*

## 2026-11-13 — v2.0.74: the strategy session's proposals (P1–P23), as Céline decided them; the compositor direction retired

Céline, 2026-11-13: « J'aimerais examiner avec toi les propositions de la note stratégique en les
regroupant par lot fonctionnel ». The 23 proposals of the strategy session of 2026-11-10 were put to
her in eight groups, one question at a time, each with a recommendation.

**Adopted as recommended:**
- P1, P2, P3 (mail and contacts) and P15 (no link in a mail visited automatically): for v0.0.13's
  C-COMMS.
- P14: one gate for all network egress, from v0.0.13 on (ch. 02, *Network egress*).
- P11 with P16: a remote model only when chosen, and as a Transmit action.
- P5: delegations, starting with the rotation of the account key, to revoke a device.
- P6 and P7: a node is blind or authorised; at a third party, blind.
- P12: the project never operates infrastructure.
- P8: a collaborator's identity is distinct from the guardian protocol.
- P20: « no platform can hold the user ».
- P22: no reopening of Tauri against the shell; a trial of the shell on Android, measured on touch
  input, the phone's keyboard and TalkBack.
- P4, P10 and P19, the technical ones, with no objection.

**Adopted with her own precision:**
- **P18** (the shell's reliability score): « La retirer », rather than the warning I had recommended.
- **P17b** (the keys of model providers): « par device ou account-wide au choix de l'utilisateur ».
- **P9** (the order of collaboration). She asked first what the « mesh among strangers » meant: « Pas
  de partage de documents en clair, mais par contre, sauvegarde encrypté de fragments oui ». The last
  step is ch. 06's Feature 2, the opt-in crowd backup of encrypted fragments.
- **P21** (the Wayland compositor). After a discussion: « Je ne veux pas que les autres logiciels
  entrent dans Sovereign, cela n'aurait aucun sens. Mais leurs contenus, oui. »
  - ch. 01's *Direction: from application to environment* becomes *the contents of other software,
    never the software*.
  - The compositor is retired, on Linux too. That is PANDA2's reading of her words, given to her with
    an invitation to correct it.

**After the laptop maille's second position (same day):**
- **Appendix C §5.1** is completed, Céline: « Compléter ». The software runs entirely on the user's
  hardware, except a remote model the user explicitly chooses, reached through the user's own
  account with its provider (P11).
- **The compositor's retirement** was confirmed by Céline: « Oui, retiré partout ».
- **ch. 02, *Network egress*:**
  - the as-built list now follows the code: no model download by the app, the Tauri web fetch,
    `net_guard`, and the sidecars on loopback;
  - only the gate's first part comes in v0.0.13.
- **P2:** « the Sent folder is fetched », not « synced », since each device fetches by IMAP.

**Still open:** P13 (email read-only first, part of Céline's question 1 on v0.0.13). Later: P17a, P23.

**Where it is written:**
- **ch. 01:** a section *Decided on 2026-11-13, from the session's proposals*, and the *Direction*
  rewritten (P21).
- **ch. 02:** *Network egress* (P14).
- **ch. 05:** the node, merge rules and roles.
- **ch. 07:** rotation and delegations, and the providers' keys.
- **ch. 08:** remote models.
- **ch. 10:** the reliability score.
- **ch. 13:** what C-COMMS will carry.

The record of the review, with every question and answer: the PANDA2 maille's session of 2026-11-13.

## 2026-11-13 — v2.0.73: the direction of 2026-11-10 (D1–D18), with its contradictions resolved one by one

Céline, 2026-11-13: « intégrer D1 à D18 s'il n'y a pas de contradictions, sinon, les résoudre une
par une ». D1–D18 are Céline's decisions from a strategy session held on 2026-11-10.

**C1 — Appendix C §1.3 and messaging.**
- **The conflict:** §1.3 listed « No communication platform (no messaging, email, or social
  features) ». Email is planned for v0.0.13 (Céline, 2026-10-30), and collaboration is a goal at
  2–5 years (D6).
- **Resolved by Céline:** the line is revised, not removed. The new line is « No communication
  network of its own »: the user's own accounts with existing services, from their own devices; no
  anonymity added; collaboration only between verified contacts.
- **How the text came to be:**
  - her choice of option: « Réviser la ligne »;
  - on the first draft: « standard protocols might be too restrictive, at least IMAP and SMTP. I'd
    like to be able to add WhatsApp, Signal or others in the future. But the core stays true : no
    communication network of its own. »;
  - on the final text: « oui pour la nouvelle formulation ».

**C2 — D14 and « no AI on mobile for now » (2026-10-10).**
- **The conflict:** D14 asks for the same functions on every host as soon as the hardware allows, and
  ch. 14 records the decision of 2026-10-10.
- **Resolved by Céline:** « Cible + étape ». D14 is the target. The decision of 2026-10-10 is the
  current step, a choice of order. The return of local AI to the phone is planned when she decides,
  after a measurement on a current phone (ch. 14, *Mobile*).

**C3 — D7 and the definition of the product in ch. 01.**
- **The conflict:** D7 read « Sovereign est le nœud personnel, un cœur qui tourne sans interface »,
  while ch. 01 defines Sovereign GE as a local-first graphical environment.
- **Resolved by Céline, who made D7 precise:**

  > le noeud personnel sans interface est un add-on, une redondance que l'on peut ajouter au choix
  > de l'utilisateur pour renforcer la sécurité. Sovereign GE reste le nom général. A terme, je pense
  > qu'il faudra parler de Sovereign ecosystem, qui incluera crabe et spotter.

- **What follows:**
  - ch. 01's definition stands;
  - the node with no interface is written as an optional add-on (ch. 01, *Direction*);
  - « Sovereign ecosystem », with Crabe and Spotter, is noted as the longer-term name of the whole.

**C4 — D2 and the concepts of ch. 01.**
- **The conflict:** D2 names contacts and documents as the two fundamental concepts. ch. 01 listed
  documents, contacts and messages, placed in intent threads.
- **Resolved by Céline:** « Deux concepts, fils = organisation ».
  - Contacts and documents are the fundamental concepts.
  - Threads organise them.
  - Messages are the exchanges with contacts, placed in the threads.
- **ch. 01 is reworded to match.** What a message is exactly stays open (the synthesis's P1, not
  decided).

**C5 — D12 and ch. 07's full-disk-encryption requirement** (raised by the laptop maille).
- **The conflict:** D12 reads « sans dépendre d'un matériel ni d'un éditeur de système », while ch. 07
  requires the OS's full-disk encryption for what stays in clear.
- **Resolved by Céline:** « Partout + réduire ».
  - D12 means running on any machine one owns.
  - The requirement stays for now.
  - The direction is to shrink what depends on it.
  - Each host declares what its system can see (ch. 07, *What stays in clear*).

**C6 — D9 and the automatic purge** (raised by the laptop maille).
- **The conflict:** D9 reads « Sans l'utilisateur, le nœud n'agit jamais au-dessus du niveau
  observer ». The 30-day purge of the trash, a level-5 act, runs at unlock and then hourly, without
  the user.
- **Resolved by Céline:** « Règle posée = toi ». A standing rule the user has set counts as the user.
  So does a sync, which is the user's own act on another device (D4).
- **Written as a precision of D9.**

**Precisions written without a contradiction** (shown to Céline in the review table, with no
objection):
- **D9:** earned trust applies only on a request the user is making. The laptop maille checked it in
  the code: no background path reaches the gate. D9 binds the devices as well as the optional node:
  the purge Céline ruled on in C6 runs on the desktop and the phone.
- **D3:** replaces the inbox order, which was built, never decided.
- **D4:** each guardian's duty is bound to the enrolling device (ch. 06).
- **D16:** a rule, with the as-built gap (C-SESS).
- **D17:** the target that will replace C-WIRE's pairwise sync, together with each device fetching its
  own mail by IMAP (Céline, 2026-11-13: « je veux que chaque machine soit autonome »).

**Where it is written:**
- **ch. 01:** a section *Direction decided on 2026-11-10 (D1–D18)*, and the two concepts in *The
  shift* (C4).
- **ch. 05:** the target, D17 and D11, at the head of *Sync protocol*.
- **ch. 06:** D4, the guardian's binding.
- **ch. 07:** D12 (C5).
- **ch. 08:** D9, D10 and D16.
- **ch. 12:** D3.
- **ch. 13:** IMAP on each device.
- **ch. 14:** D14 (C2).
- **Appendix C §1.3** (C1).

The rest of the strategy session, proposals P1–P23 and its open questions, is not decided and is not
in the spec.

## 2026-11-13 — v2.0.72: the whole specification checked against the code (every status line; round 3 of the stale passages; the private plans out of the public text)

Céline, 2026-11-13: « ce que j'aimerais d'abord, c'est 1. consolider la spec entièrement en fonction
de ce qui est déjà codé […] 2. intégrer D1 à D18 s'il n'y a pas de contradictions, sinon, les résoudre
une par une ». This entry is step 1. D1–D18 (her strategy session of 2026-11-10) come in a later entry.

**How it was checked.**
- Five read-only audits compared each chapter's status lines and statements with the code at `main`
  `7874517`, about 310 status lines in all. They confirmed 89 differences and left 19 uncertain.
- Five writers, one per group of chapters, each re-read the code before writing. ch. 06 was written
  by the PANDA2 maille.
- The PANDA2 maille re-read the heaviest findings at the source before the writers started. One
  audit's claim was wrong (ch. 10, `source_url` « still unset ») and was corrected in the brief.
- The laptop maille gives the second position before main.

**No change of substance, except Céline's two decisions of 2026-11-13 below.** Every other edit
states what is built, or closes a conflict that Céline had already decided. Where the code does not follow a decision, the decision stays written and the built
behaviour is written beside it.

**Conflicts already decided, now closed in the text:**
- **5, arming at five** (ch. 06): decided 2026-10-10, 3 of 5. The code still arms at 5/5, and the
  alignment is for a later release (Céline, 2026-11-04).
- **8, delete** (ch. 08): soft delete everywhere, 2026-10-10. The canvas Delete goes to the trash
  through the document service.
- **13, Appendix B** (ch. 12, Appendix B): newer decisions hold, 2026-10-10. The appendix is still
  to be realigned. A new row records that no Annotate action is built (they are refused before the gate).
- **D3** (ch. 03, 04): decided 2026-10-10. Trust is built as three states. Multi-party ownership is
  deferred.
- **The guardian app's platform** (ch. 06): mobile-first, 2026-10-10.

**Corrections, by chapter.**
- **ch. 02:**
  - the layers gain the document service and the editor;
  - 16 workspace members;
  - `sovereign-docs` and `sovereign-docformat` are built;
  - the `llama` and `comms` features;
  - Android has no local AI;
  - the shell's one feature is `dev-switches`;
  - the assets are those of v0.0.12.
- **ch. 03:**
  - 47 document fields;
  - the `purge_marker` and `store_record` tables;
  - `source_url` is set for web provenance;
  - the `peer_review_*` fields are retired;
  - format v2 is built;
  - ownership is the three-state trust;
  - the commit MAC is on every path;
  - restore is whole and refuses another document's commit;
  - auto-commit follows the service's policy and `end_session` on both hosts;
  - delete and the trash, and C-DOC adoption steps 1–6, were released in v0.0.11;
  - the provenance record is partly built;
  - the import trust notice says the documents land undetermined;
  - the phone shows a count of suggestions, with no accept or dismiss.
  - **Where the code differs from G12:** the assistant's *adopt* sets only `is_owned`.
- **ch. 04:**
  - the chapter and *Ownership* are ◐: trust, provenance, `set_trust` and the adoption operations are
    built in the service, and no surface calls the adoption gestures.
- **ch. 05:**
  - sync applies through `apply_wire`;
  - a peer's deletion raises no review flag;
  - the poll's backoff, and which hosts poll;
  - the manifest's document entry;
  - the strings for an older peer;
  - the 24-hour bound applies to rows;
  - C-WIRE is in the present tense, and its guarantees are tested;
  - both UIs offer 3 of the 7 restorable tables.
- **ch. 06:**
  - both hosts record enrollments and run the recovery flow, since v0.0.10;
  - the release-build requirement is built.
- **ch. 07:**
  - `salt` is written privately; only `keystroke.store` is still a plain write;
  - `AuthStore::save_new` is built.
- **ch. 08:**
  - action names are checked;
  - `merge_threads` is handled;
  - action targets are ◐ (resolved before the proposal; AT4, AT6 and most of AT8 not yet);
  - every create makes a version (G5);
  - a wiped session log is quarantined;
  - the peer-change analysis runs in the shell;
  - the duress persona's log directory.
- **ch. 09:**
  - ingest runs in the document service's hook on every write, on both hosts;
  - where a token resolves is built for documents.
- **ch. 10:**
  - « Save page » records the URL as web provenance and `source_url`.
- **ch. 11:**
  - 23 of the 24 skills are in the menu;
  - what the shell does with a skill's result;
  - raw-text documents are no longer misread;
  - a WASM update that changes media is refused;
  - provenance is ◐.
- **ch. 12:**
  - nine Settings tabs, with Trash, whose rows act;
  - the `a` and `v` windows;
  - the document window with no Edit button;
  - THREAD-name, Title (T1–T10), the ruler, the lanes (M2, N4–N6, I1–I4), lot 1 D3 and H3/H4;
  - the confirmation's timing;
  - the phone's reader, with History and save alerts;
  - the canary field's drawn toggle;
  - the field fills and the single-flight account creation, released in v0.0.11.
- **ch. 13:**
  - email is for v0.0.13. It is still off.
- **ch. 14:**
  - Android runs no inference.
- **ch. 15:**
  - about 3,000 tests;
  - what CI runs.

**The private plans out of the public text** (BACKLOG §13, Céline: « nettoyer en v0.0.13 »):
- every path into `doc/plans/` and every private plan file named as a source is replaced by a
  neutral mention;
- the numbers of the private coordination channel are removed;
- 0 matches remain outside this changelog.
- **Earlier entries of this changelog are kept unchanged,** as a signed record. A note at the head
  says so.
- **The rule is in `CLAUDE.md`:** the spec cites a chapter or a decision, never a plan file.
- Céline chose this scope on 2026-11-13: « Garder + une ligne ».

**Decided by Céline on 2026-11-13, during the consolidation:**
- **Sample data is opt-out.** ch. 12 said « opt-in », while both wizards start with the option on. Her
  words: « données d'exemple sont opt-out, à corriger ». The spec now follows the code.

**Not written, and why:**
- **Four findings went to the private backlog** (§16): security and privacy findings, which are not
  for the public spec, and code comments.
- **A skill asked for in the chat** (ch. 11), new, ○ not built. Her words: « l'assistant devrait indiquer
  l'existence du skill et l'utiliser sans attendre ». Then, on how it fits the action levels: « selon le
  niveau ». A skill that only reads runs at once. A skill that changes a document goes through the
  proposal and confirmation of level 3 (ch. 08). Until it is built, the orchestrator only returns a
  hint to the Skills menu.

*— maille Sovereign, PANDA2 (Claude Opus 5.5), 2026-11-13*

## 2026-11-04 — v2.0.71: what v0.0.13's contracts will stand on (stale passages, round 2; the built details of ch. 03, 05, 07, 08, 11, 12; the open UI choices)

Céline: « est-ce qu'il ne faudrait mieux pas faire le rattrapage de la spec avant de lancer les
contrats de la v0013? », then « Fait le lot avant L0 ce soir ». Every statement was checked against
the code at `main` `1cde251`. The work was split by chapter between two writers, and the laptop
maille read it all. PANDA2 gives the second position before main.

**Corrections of stale text — no change of substance.**
- **ch. 05:**
  - *C-WIRE*'s status is ◐ Partial, with its amendments since 2026-10-15: purges are local, the bound
    sits inside the clock, the device in a stamp is the peer id, a move is a stamped write. It names
    where the build differs: the shell drops `SyncConflict` (guarantee 7), and both UIs restore 3 of
    the 7 restorable tables (guarantee 15).
  - The manifest no longer "keeps the byte hash for older peers": a build older than v0.0.11 is sent
    no document (Céline, 2026-10-15, C-WIRE 12).
  - "What `apply_remote` lacks today" becomes what the service needed, all of it built in step 4 and
    released in v0.0.11.
  - Conversation titles and contact-address removals propagate since v0.0.11.
  - *Thread membership* loses its older-peer sentence.
  - *Non-destructive peer overwrite* and *Concurrent-edit conflicts* say what is built under the
    decisions of 2026-10-15: no review flag, the analysis on every received change, the alerts and
    Previous values; both edits kept without asking, "Edited on both devices" in the app. "Keep
    mine" goes.
- **ch. 03:** the G24–G31 status: built in step 4, released in v0.0.11, as amended by v0.0.12.
- **ch. 12:**
  - Delete from the canvas is one click to the trash, with Undo in the notice, on the desktop and the
    phone (Céline, 2026-10-17). This replaces "a two-step confirm" in three places and "a soft delete
    without confirmation" on mobile.
  - *Documents* and C-TEXT were released in v0.0.11.
  - Settings' Recovery tab states the built rule: recovery turns on at 5/5. Beside it, ○ the decision
    of 2026-10-10, not built: recovery may start at 3 of 5.
- **ch. 08, the session log:** the check runs each time its key is installed. The frozen Tauri
  desktop installs it at each login; the native shell at the assistant's first use after login (the
  first chat message or the first reliability check of a page), not "at each login".
- **ch. 15, *Roadmap*:** v0.0.11, v0.0.12 and v0.0.13 as decided. v0.0.13 is the email release
  (Céline, 2026-10-30 and 2026-11-01), with C-HOST whole (2026-11-04).

**Built behaviour newly written.**
- **ch. 05:**
  - thread rows in *Rows*: one stamped path, the later stamp wins, an unstamped row compared at its
    time, a deleted thread compared on id, state and stamp;
  - thread stamps under the 5-minute bound;
  - *Deletions and purges* in full, and backups' markers without a fingerprint (v0.0.12 item 3;
    Céline, 2026-10-23).
- **ch. 03:**
  - G25 qualified by the bound, and G27 condensed from item 3;
  - *Restore:* History while locked, and the restore of an unchanged version, which adds none;
  - the conversion's notices word for word, with rows under purge left out, a notice never lost,
    and typed text kept;
  - the unreadable list (V2b);
  - **Find or create by name** for the callers that cannot ask: the exact name, created when
    there is none, two refused;
  - the thread deletion as built: one stamped path, a rename against a deletion, the shrink.
- **ch. 07:**
  - a retried pairing record replaces the first;
  - how a phone account made before v0.0.12 with duress = main behaves (Céline, 2026-10-23: no
    detection; the public notes say nothing of it, Céline, 2026-11-04);
  - the decoy stamps with the duress persona's peer id;
  - nothing reads `set-aside-*` or `quarantine-*`, and the old marker is ignored.
- **ch. 08:**
  - a no-change writes and stamps nothing; its reasons and words; a case-only rename is a rename;
    the approval counts whatever follows (item 11b; Céline, 2026-10-28);
  - `split_thread`'s words;
  - *Action targets*: the exact replies with no match and with two matches, the request patterns
    and the words stripped, and `create_document` with a named thread (items 6 and 23; Céline,
    2026-10-24 and 2026-10-25);
  - the session log's quarantine directories, its notice as `ActionRejected`, and the cases left
    unchanged.
- **ch. 11:** Daily Journal and `find_or_create_thread`, by exact name.
- **ch. 12:**
  - the editor, the only one since 2026-10-19: a document it cannot open (four plain notices), a
    change by the assistant, a skill or a restore, and a store replacement;
  - after a quit's Cancel, the documents count as unsaved;
  - the phone wizard's mismatch words.

**Open, newly listed: ch. 15, D5,** the desktop shell's UI choices that no one has decided (the
parity triage of 2026-10-17, and multi-line chat messages).

---

## 2026-11-04 — v2.0.70: the spec catches up with v0.0.12 (stale passages; the three rc2 lots)

An audit of the plans against the spec, asked by Céline before the v0.0.12 plans were archived,
found passages that state the opposite of what is built, and behaviour released in v0.0.12 rc2 with
no spec text. Each passage was checked against the code at `main` `fd1057b`. Written by the laptop
maille; PANDA2 reads it in second position before it goes to main.

**Corrections of stale text — no change of substance.** Each states a decision already taken, or
what is already built:
- **Purges are local** (Céline, 2026-10-23; built in v0.0.12): ch. 05, the manifest's purge
  markers carry the id and the deletion's stamp, with v0.0.11's `fingerprint` field sent empty;
  what `apply_remote` needed (items 4 and 7) no longer names a fingerprint; C-WIRE guarantee 10 is
  the local purge, replacing "a purge removes only the same version". ch. 03, G24–G31's operations:
  `purge_fingerprint` is gone.
- **The device in a stamp is the peer id** (Céline, 2026-10-24; v0.0.12 item 22): ch. 03, the
  document service is given the persona's peer id, not "this device's name".
- **One lane per thread** (v0.0.12 lanes lot): ch. 03, the fallback lane as built today (one lane
  per thread, Unfiled only when needed, on the desktop and the phone); ch. 12, the status bar's
  thread count has no cap; a lane's hue is its thread's (Céline, 2026-10-24); the wheel scrolls the
  lanes, Shift+wheel scrolls in time and Ctrl+wheel zooms (Céline, 2026-10-10 and 2026-10-24): the
  *Zoom range* section's ⚠ goes, its conflict having been closed on 2026-10-10.
- **Threads** (Céline, 2026-10-17 and 2026-10-18; v0.0.12 item 6, A4): ch. 03, a thread's
  `stamp` field is listed and `modified_at` no longer said to decide across devices; "open with
  Céline" goes, the later action winning having settled it; a thread deleted here is not restored
  here, and comes back only by a later action elsewhere; a thread a peer deleted is restored from
  the previous values; built and released in v0.0.11, not "on main". ch. 12, a thread is renamed in
  place by double-clicking its lane's label ("not decided" goes).
- **Moves travel** (v0.0.12 item 15): ch. 05, *Sync protocol*, *Documents* and *Thread
  membership* said that a title-only edit and a move do not propagate. That was the wire before
  v0.0.11; such a peer is now sent no document. Since v0.0.11 a document travels by the C-WIRE
  contract, so a title edit travels, and a move too since v0.0.12. The direction is by stamp.
- **Two devices editing one document at the same time**: ch. 12, the decision of 2026-10-10 ("one
  device edits a document at a time") was never built. The chapter now says what is built: the
  last save wins and the other text is kept in History, as the v0.0.12 notes say. It adds Céline's
  direction of 2026-10-23, decided and not built, for a later release: not as a block; both devices
  edit, fork, and reconcile at the next sync when the edits do not conflict, keeping both when they
  do.
- **Decided, no longer "pending"** (Céline, 2026-10-10): ch. 12 *Confirmation UX* (in the chat; a
  modal only for Destruct) and *Settings* (editable in the shell) lose their ⚠; both are ○ decided,
  not built.
- **Status lines**: ch. 07 *After login*, the duress enrollment's waiting screen and the persona's
  identity profile are built (2026-10-17, released in v0.0.11); ch. 06 *Owner cancellation*, the
  phone's login cancels a recovery on that device (built 2026-10-17, released in v0.0.11).

**Released behaviour newly written — the three rc2 lots of v0.0.12** (found by Céline on rc1,
2026-11-01; her decisions the same day: "corriger dans rc2", and "accessibilité est essentielle";
built and released in v0.0.12, 2026-11-04):
- **A summary is shown** (ch. 08 *Models and runtime*, "Where a summary is shown"; the Observe line
  of *Action Gravity* no longer lists summarize as undisplayed): one assistant message with the
  title, the text and the number of parts, on the desktop and in the app; the desktop opens the
  chat when none is open; "The summary could not be shown." for unreadable data; `parts` missing
  counts as 1. A notification instead of opening the chat is for later (Céline, 2026-11-01).
- **What a screen reader hears** (ch. 12 *Accessibility*): every drawn Label spoken by its value;
  the chat's messages; the locked screens' title, "Step N of 8", copy and hint, "Unlock Sovereign"
  among them; the show/hide toggle on every secret screen, Alt+F8 from any field ("Alt+F8 to show"
  in the login and join hints), and a screen reader's Click on it while locked.
- **Each setup step announced** (ch. 12 *Accessibility*): a group named by the title and "Step N
  of 8", focused when no field is; node ids of each screen's own, so every change of step, and
  between login and join, moves the focus.
- ch. 12 *Accessibility*, the gaps: rewritten from the code. "Actions are not handled" and
  "validation with Narrator has not been done" go: Céline tested with Narrator on 2026-11-01 (rc1)
  and 2026-11-04 (rc2). The gaps that remain are named, with the observations of 2026-11-04: no
  keyboard-only setup, the welcome screen's buttons read in scan mode only (its title is announced),
  the login hint read in scan mode only.
- ch. 08, `list_models`: the shell names the models in its outcome line; it is no longer listed
  among the undisplayed Observe results (second position, PANDA2).

---

## 2026-11-04 — v2.0.69: the app's duress and canary steps get the password step's "show" toggle (v0.0.12)

ch. 12, *The canary phrase is entered like a password*: **K3** rewritten, **K4** extended, **K5**
added. Contract and tests: PANDA2, `doc/plans/V0012-phone-show.md`.

- **The gap.** The first K3 said the app's canary inputs are password inputs "as the password
  step's are". But the password step also has a "Show password" checkbox, and the duress and
  canary steps had none. Found by PANDA2's 0c read of rc1 (claude-coord 0196). Céline had chosen
  on a description of the phone that was wrong. She decided again, on 2026-10-28: the toggle
  also goes on the phone's duress and canary steps.
- **K3**: the duress and canary inputs are masked by default. They have the password step's
  checkbox, with one state for the whole wizard, labelled "Show password", or "Show phrase" on
  the canary step.
- **K4**: what is sent is the same with "show" on or off.
- **K5** (Céline, 2026-11-04): those four inputs carry the password step's `autocapitalize`,
  `autocorrect`, `autocomplete` and `spellcheck` attributes.
  - Shown as text, a field otherwise lets the keyboard capitalise or correct the secret. A duress
    password typed "abc" with "show" on could be saved as "Abc", and never match at login.
  - For these two steps this does the BACKLOG item on `autocomplete` (§13). `new-password` for
    the main password remains there.

---

## 2026-11-01 — corrections: three duress defences are the Tauri app's, not the desktop's (no version)

During her rc1 test on the laptop, Céline created a duress account in the desktop shell. It opened
empty. ch. 07 said "built in the shared login path" for *Decoy seeding*, *Login timing padding* and
*Login throttle*. Read at rc1 (`20425d1`), all three live in `sovereign-app` (`duress.rs`,
`tauri_commands/auth.rs`), and the shell, the only desktop UI shipped, calls none of them. BACKLOG §1
already said so for the decoy and the throttle. The three status lines now say where each defence
is built and where it is not. No change of substance: the design is unchanged and still to build in
the shell.

---

## 2026-10-30 — corrections: the PII sweep is decided, not "with A5" (no version)

The sweep was decided on 2026-10-10 (v2.0.6, D4). Three passages still carried the 2026-10-09 plan to
specify it together with A5. PANDA2 raised the contradiction while merging the v0.0.13 priority
lists; the laptop checked it and made the correction, at PANDA2's request (claude-coord 0189):
- ch. 09 *Idle sweep*: its last paragraph;
- ch. 09 *Not built*: the sweep's line;
- ch. 10 *A5*: the sweep is no longer part of A5's open questions.
No change of substance. Still to design: the review queue's cadence and form, and the three
preconditions in ch. 09.

---

## 2026-10-28 — v2.0.68: the canary phrase is entered like a password (v0.0.12)

Céline, 2026-10-28: "The canary phrase is a secret" (answer to PANDA2's 0c read, recorded by the
laptop, claude-coord 0176) and, asked how it should be entered, "comme les mots de passe" (PANDA2
session). ch. 12, *Accessibility*: K1–K4, the canary step masked on screen and in the tree, with the
wizard's single "show" toggle, and password inputs in the app. ch. 07, *Canary phrase*: it is a
secret; the pairing code is not. Contract: `doc/plans/V0012-canary-masked.md` (PANDA2). First
numbered v2.0.67 on its branch; renumbered when item 11b took v2.0.67 on main.

---

## 2026-10-28 — v2.0.67: "Done" only for a real change, in a conversation too (v0.0.12 item 11b)

Item 11b of v0.0.12 (`doc/plans/V0012-item11b-chat-no-change.md`, merged on `main` 5f0d8fa; Céline,
2026-10-28, on PANDA2's read of the release notes, claude-coord 0175; PANDA2's second position,
0184). ch. 08 *Action Gravity*: the chat loop's write tools report "No change" with its reason, like the
direct path.

---

## 2026-10-26 — v2.0.66: an unverifiable conversation log is quarantined and said, every time (v0.0.12 item 10)

Item 10 of v0.0.12 (`doc/plans/V0012-item10-session-log.md`, merged on `main` 5ed9d68; Céline's
answers to the proposal, 2026-10-26: Part A now and Part B in the next release, quarantine and a
fresh start, her notice, widened the same day for a crash or a full disk; PANDA2's second position,
claude-coord 0171 and 0173). ch. 08 *Session log*:
- the check (chain, decryption, anchor, beacon) at every login;
- the quarantine and the fresh log;
- the notice;
- failing closed when the move fails;
- no crash and no file bytes in messages;
- each persona's own log on a one-orchestrator host;
- the diagnostic sandbox announced for the next release.

ch. 07 *After login*, *Migration*: the v0.0.11 one-time set-aside replaced.

## 2026-10-26 — corrections: two stale passages (no version)

Found by PANDA2 while writing v2.0.55–65 (claude-coord 0169); corrected by the laptop against the
code at `main` `edba463`:
- ch. 12 *Accessibility*, gaps: the canvas summary no longer announces "12 thread lanes" whatever the
  workspace holds. It counts the real threads (`lanes.thread_count()`, since the lanes lot, v2.0.50).
- ch. 02: the Android build has a recipe, `scripts/build-android.sh`, documented in `doc/RELEASE.md`.
  The published APKs are debug builds; a release build is unsigned until signed outside the
  repository.

## 2026-10-26 — v2.0.65: the app's notice buttons name their object; its chat is announced (v0.0.12 F-a11y)

F-a11y (`doc/plans/V0012-frontend-a11y.md`, merged on `main` 4ac3868, 2026-10-25; gaps found during
second positions that day; PANDA2's contract; the laptop's second position, claude-coord 0161).
ch. 12 *Accessibility*, the Svelte frontend: each notice button's accessible name begins with its
visible text and names its object (F1); the chat is a log named "Conversation", with new assistant
and system messages announced once by a polite region, the person's own not repeated, and replies
announced while the panel is hidden (F2); the frontend type-checks with no error (F3). The short
visible texts, the separate region and announcing while hidden are PANDA2's choices. **F4 is
process, not in a chapter**: `_prepublish.bat` gains step 7b, `svelte-check --threshold error`
(PANDA2's choice; the gate being shared, it waited for the laptop's yes, given in 0159). The Linux
gate (`scripts/prepublish.sh`) has neither 7b nor 9, and `doc/RELEASE.md` still lists steps 0 to 8
(found by the laptop, 0161).

## 2026-10-26 — v2.0.64: the Tauri app lists the documents it could not read (v0.0.12 item 7, V2b)

Item 7's V2b (`doc/plans/V0012-item7-v2b-app-list.md`, merged on `main` 9f99b7f, 2026-10-25; a gap
found by the laptop at item 7's build, claude-coord 0153: the phone had a count and no list; under
Céline's item 7 decision of 2026-10-24, count them and list them; PANDA2's contract; the laptop's
second position, 0158). ch. 03 *Content encoding*, completing v2.0.53's "V2b, to follow": one
wording in `sovereign-docs` for both hosts; the app's Alerts dialog lists the documents, with a
notice while any remain (PANDA2's choice); a refused or locked request shows an empty list.

## 2026-10-26 — v2.0.63: no OpenGL backend on Windows (v0.0.12)

`doc/plans/V0012-no-gl-windows.md`, merged on `main` 3d1a1a4, 2026-10-25 (W1–W6, W3b). Céline's
decision, 2026-10-24 ("Retirer sous Windows maintenant", via the laptop's claude-coord 0141;
confirmed after PANDA2's measurement: "on l'enlevait à moins que tu aies une objection"); Linux
keeps OpenGL. PANDA2's contract; the laptop's second position, 0150. ch. 12 *Native shell stack*:
on Windows the instance is DX12 and Vulkan only; one function builds the render context;
`WGPU_BACKEND` stays a development switch that decides when set, empty or unknown values included
(PANDA2's choices); `WGPU_BACKEND=gl` is not offered to users as a fallback, since a GL-only start
panicked once in six (amended at 9816dae, agreed by the laptop); the measured gain is small.

## 2026-10-26 — v2.0.62: trash and orphan actions act on what was shown (v0.0.12 item 9)

Item 9 of v0.0.12 (`doc/plans/V0012-item9-act-on-what-was-shown.md`, merged on `main` f879443,
2026-10-25; the cloud review of v0.0.11, R15 and R20, in the scope Céline confirmed on 2026-10-23;
PANDA2's contract; the laptop's second position, claude-coord 0144 on the contract and 0148 on the
build). ch. 03 *Soft delete and 30-day purge*: Empty trash purges the ids the view showed, those
still in the trash, and names the skipped ones; an orphan group's action applies to the ids it
showed, zero when none is still waiting; Delete tells "already in the trash" apart, with no Undo;
"Restore into…" lists every thread, scrolls, and is an accessible list; the C-DOC operations list
names the trash outcome and `empty_trash_of`. The wording of the skipped
message and the zero result are PANDA2's choices.

## 2026-10-26 — v2.0.61: the clipboard forgets at quit and at a store change; no raw token in a title (v0.0.12 item 8)

Item 8 of v0.0.12 (`doc/plans/V0012-item8-clipboard-labels.md`, merged on `main` e3e8c6c,
2026-10-25; the cloud review of v0.0.11, R19 and R18, scope confirmed by Céline 2026-10-23;
PANDA2's contract; the laptop's second position, claude-coord 0144 on the contract and 0147 on
the build). **ch. 12 *Clipboard*** was reworded at merge (62de4dd, without a changelog entry until
this one): any copy this process made — document text, a shell field, the pairing code — is
cleared at quit and when the store changes, so it cannot cross to another persona; another
application's copy is left alone (Céline, 2026-10-24: "Effacer à la fermeture aussi", which
reverses "no automatic clearing of ordinary text" of 2026-10-10); the pairing code carries the
same exclusion marks as any copy (PANDA2's choice, K5). A crash or a kill clears nothing: no code
runs then. **ch. 12 *Documents*, title:** a token in a title is shown and announced as its label in
the editor's label and wherever the shell draws a title; `[PII]` when the label cannot be read. ch. 09's two status lines no longer say that no UI renders tokens.

## 2026-10-26 — v2.0.60: the time ruler across a daylight-saving change (v0.0.12)

Two merges on the desktop's time ruler, one entry. **The panic** in the hour repeated when clocks
go back (merged on `main` 37f303b, 2026-10-24; no contract, the cause and fix in claude-coord
from-windows 0150; found by the item 8 test author; PANDA2's fix, the laptop's second position,
0146): the ruler aligns by durations, not by setting local clock fields; in v0.0.11 too, and no
v0.0.11 erratum for it (Céline, 2026-10-25: "Trop tard pour l'ajouter, c'est déjà passé. Il faudra
corriger avant l'année prochaine"). **The days follow the calendar**
(`doc/plans/V0012-ruler-calendar-days.md`, merged on `main` 41e45af, 2026-10-25; found by the
laptop; PANDA2's contract; the laptop's second position, 0151 and 0153): day ticks are calendar
dates, each labelled once on 23-, 24- and 25-hour days; multi-hour ticks are wall-clock hours
(PANDA2's choice, agreed by the laptop); minute and 1-hour ticks unchanged. ch. 12 *X — calibrated
time*.

## 2026-10-26 — v2.0.59: devices have names, and History says them (v0.0.12 items 14, 19)

Items 14 and 19 of v0.0.12 (`doc/plans/V0012-items14-19-devices.md`, merged on `main` c7a78e4,
2026-10-24; both put in by Céline, 2026-10-23; PANDA2's contract; the laptop's second position,
claude-coord 0139 on the contract and 0141 on the build). ch. 05 gains *Device names*: one name per
workspace, shared by both personas, overriding the configuration; asked at setup, prefilled with
the computer's name (Céline, 2026-10-24: "Le nom de l'ordinateur"), at most 64 grapheme clusters;
renamed in the Devices window, sent from then on without a restart; a paired device renamed here
only; History names the device ("this device", or "…" and the last 8 characters of a forgotten
id) on one line that never overlaps the time. The name is not the identity (item 22). Céline
approved the choices PANDA2 put to her on 2026-10-24 ("je suis d'accord avec tes choix"). ch. 12:
pointers from *Text fields*, *Documents* (History) and *Onboarding*. The orphans follow a local
rename since item 22, not before (the contract's known limit at merge). ch. 12 *Settings*: the conflict note says the device name is now set and renamed, the conflict standing for the other settings.

## 2026-10-26 — v2.0.58: the desktop's first run works off the UI thread and says what it shows (v0.0.12 items 18, 20)

Items 18 and 20 of v0.0.12 (`doc/plans/V0012-items18-20-first-run.md`, merged on `main` 7e2f6c9,
2026-10-24; both put in by Céline, 2026-10-23; PANDA2's contract; the laptop's second position,
claude-coord 0131 on the contract and 0138 on the build). ch. 12 *Onboarding and the auth gate*:
setup, unlock, recovery's new password and the join's login run off the UI thread, with a status
line, one at a time, the error on screen and in the tree; after the store, a failure goes to the
login form; a refused duress = main goes back to the duress step; `try_onboard` deleted (the
laptop's proposal). ch. 12 *Accessibility*: the locked screen's tree describes the step drawn,
names each field by its label, never announces a secret, has no empty name; "the onboarding
wizard is not in the tree" removed from the gaps. ch. 07 *Auth store*: the desktop's interrupted
setup, a "setup started" record and a setup-complete marker, the notice once, and no notice for
accounts made before v0.0.12 (PANDA2's choice). The other choices (no progress bar or cancel,
closing not blocked, "show" not announced) are PANDA2's, open to Céline's review.

## 2026-10-26 — v2.0.57: the chat composer and the search field are the editor's field (v0.0.12 item 21)

Item 21 of v0.0.12 (`doc/plans/V0012-item21-composer.md`, merged on `main` 9e93a9b, 2026-10-24;
asked by Céline, 2026-10-23; PANDA2's contract; the laptop's second position, claude-coord 0130 on
the contract and 0137 on the build). ch. 12 *Native shell stack*: the stale "end-pinned caret
only" line replaced by *Text fields* (which fields use the editor's single-line field, which three
are still buffers), and the composer and search: Enter and Escape keep their meaning, an Enter
during a composition stays in the field (a rule of the field, inherited by the rename and
new-thread fields), the search refreshes on every edit, "Message" and "Search" in the
accessibility tree, nothing new leaves memory, a pasted paragraph becomes one line. Ctrl+Z after a
send brings the message back: kept by Céline (2026-10-24, "Garder"). The other choices are
PANDA2's, open to her review.

## 2026-10-26 — v2.0.56: the pre-publish gate checks the notes' claims (v0.0.12 item 17; process)

Process, not behaviour of the software: no chapter changes. The specification has no chapter on
the release process, which lives in `doc/RELEASE.md` and `_prepublish.bat`; this entry records the
decision so it can be found from here. Céline's decision 4 on the cloud review of v0.0.11
(2026-10-23, `doc/plans/CLAIMS-verifier.md`): every claim of the release notes names the test that
proves it, and a verifier checks it before publishing. The verifier, `scripts/check-release-claims.py`,
is the laptop's (merged on `main` acb3f8d). Item 17 and the claims step, PANDA2's (merged on
`main` 582b1aa, 2026-10-24; the laptop's go-ahead, claude-coord 0133): every check of
`_prepublish.bat` reads `%errorlevel% neq 0` (before, `if errorlevel 1` let a negative exit code
pass); steps 4–7 write the lists of the tests they ran, and step 9 runs the verifier on the notes
named as the gate's argument, against those lists. The notes as an argument, rather than a version
read from `Cargo.toml`, is PANDA2's choice. Not in that merge: the other two parts of item 17
(`build-android.sh` deleting the old APK; `[profile.test]` debug info).

## 2026-10-26 — v2.0.55: the phone wizard checks what it sends (v0.0.12 P5)

P5 of v0.0.12, the frontend half of item 4 (`doc/plans/V0012-p5-wizard.md`, merged on `main`
b96fcb4 for P5a–P5f and 4de8a4b for P5g–P5i, 2026-10-24; PANDA2's contract; the laptop's second
position, claude-coord 0131 and 0135). ch. 12 *Onboarding and the auth gate*: Next at the duress
and safety-phrase steps only for both fields empty or both filled and equal (and, for duress,
different from the main password), on trimmed values; Skip empties the step and sends nothing;
Get Started checks duress ≠ main again; the confirmations are sent; "Already onboarded" leads to
the login screen on both flows; the interrupted-setup notice once, after the login. The duress
message takes the desktop's and the store's words (P5i; the rule is Céline's, 2026-10-23, ch. 07).
The other choices are PANDA2's (2026-10-23 and 2026-10-24; Céline: "commencer tous les 2 sans moi
ce soir"), open to her review.

## 2026-10-25 — v2.0.54: the assistant acts on what it names (v0.0.12 items 6, 11, 13, 23)

The assistant lot of v0.0.12 (`doc/plans/V0012-lot-assistant.md`, `V0012-item23-document-titles.md`,
merged on `main` 3d52bb2; PANDA2's second position, claude-coord 0160–0165). ch. 08:
- *Action Gravity*: `merge_threads` at Destruct (Céline, 2026-10-25); every action ends with one
  outcome (changed, reported, no change, failed), "Done" only for a real change.
- *Action targets*, "Built in v0.0.12": threads and documents by exact name only, resolved before
  the proposal, which names them; the two names of a rename, move or merge read from the person's
  words; a name read from unquoted words always proposed, never run by trust alone (Céline,
  2026-10-25).
- *Models and runtime*: a prompt longer than the batch decoded in chunks; Summarize in parts, at
  most 16, with a 15 % margin on the estimate.
ch. 05 *Rows*: restoring a thread a peer deleted.

## 2026-10-25 — v2.0.53: the migration counts and lists what it cannot read (v0.0.12 item 7)

Item 7 of v0.0.12 (`doc/plans/V0012-item7-conversion.md`, merged on `main` 1ed2354; Céline's
decision of 2026-10-24; PANDA2's second position, claude-coord 0159). ch. 03 *Content encoding*:
undecryptable rows are counted, left as they were, listed in the desktop's Alerts window by short
id and date, and retried only after a build version change; the migration locks per document; an
open editor takes its own conversion silently. The Tauri app's list (V2b) is to follow.

## 2026-10-25 — v2.0.52: the node starts without a system DNS config (v0.0.12 item 12)

Item 12 of v0.0.12 (`doc/plans/V0012-item12-phone-p2p.md`, merged on `main` 3122982; confirmed by
Céline, 2026-10-23; PANDA2's second position, claude-coord 0155 and 0156). ch. 05 *Swarm and
transports*: the DNS transport is optional; without a system resolver config the node runs
without it, dials no named address, and says so in Settings → Devices. The phone run (P5) is on
the v0.0.12 checklist.

## 2026-10-24 — v2.0.51: the device in a stamp is its peer id (v0.0.12 item 22)

Item 22 of v0.0.12 (`doc/plans/V0012-item22-device-identity.md`, merged on `main` 6240f6b; added by
Céline, 2026-10-24; PANDA2's second position, claude-coord 0150). ch. 05 *Stamps*: the device in a
stamp is the persona's libp2p peer id, not the display name; stored stamps keep their name; the
desktop fails closed when the identity cannot be derived.

## 2026-10-24 — v2.0.50: one lane per thread, moves travel (v0.0.12 lanes lot)

The lanes lot of v0.0.12 (`doc/plans/V0012-lot-lanes.md`, merged on `main` f4e4b7d; PANDA2's second
position, claude-coord 0148). ch. 12 *Y — thread lanes* moves to built: one lane per live thread, no
cap, one mapping used everywhere, Unfiled only when needed; the wheel scrolls the lanes and
Ctrl+wheel zooms (Céline, 2026-10-24); a lane's colour is its thread's (Céline, 2026-10-24); only
visible lanes drawn, every label in the accessibility tree. ch. 05 gains *Moving a document*: a move
is a stamped write that travels, through one service operation; the pin stays on each device
(Céline, 2026-10-24). The shell holds raw ids only (no SurrealDB display form).

## 2026-10-24 — v2.0.49: purges are local, built (v0.0.12 item 3)

Item 3 of v0.0.12 (`doc/plans/V0012-item3-local-purges.md`, merged on `main` a7b067a; PANDA2's
second position, claude-coord 0139). ch. 05 *Deletions and purges* moves to built, with what the
build settled: **peers on v0.0.11** keep syncing documents, their purge markers read as deletions
(Céline, 2026-10-24); our markers send the field v0.0.11 requires, empty; "trash since" floors the
30 days at the first tick after an upgrade **[choice]**; a document restored from a backup into the
trash enters this device's trash at the restore; stored fingerprints are erased at the first unlock;
the plan neither fetches nor pushes copies a marker would ignore **[choice]**. *Peers on an older
build:* a v0.0.11 peer is not one. ch. 03 G27 built; G28 loses the fingerprint. Review of v0.0.11,
findings 2, 3, 13 and 14, closed.

## 2026-10-24 — v2.0.48: v0.0.12 lot 1 built (the clock's bound, account creation, History, quit)

Lot 1 of v0.0.12 (`doc/plans/V0012-lot1.md`, merged on `main` 2561d2a; PANDA2's second position,
claude-coord 0137). Status lines move to built:
- **ch. 05, stamps:** the 5-minute bound is inside the clock, for every stamp received, stored or
  restored; a counter at its maximum moves to the next millisecond (review finding 1).
- **ch. 07, auth store:** login or setup is decided by `auth.store`; an interrupted or failed setup
  after the store opens, with a one-time notice (Céline, 2026-10-23); the store refuses a duress
  passphrase equal to the primary one on every path (Céline, 2026-10-23: fix it now); the backend
  checks the confirmations (review findings 4, 9).
- **ch. 12:** Cancel during a quit withdraws its Discards (review finding 6).
- **ch. 03, restore:** the native shell's History rows come from the document service; History
  restore works again on the desktop (the review's History cause).

## 2026-10-23 — v2.0.47: purges are local

Decided by Céline, after the cloud review of v0.0.11 (findings 2, 3 and 13; the reviewer's advice
4): **a purge of the trash affects only this device's documents.** Remote documents, live or in
their trash, are put in the trash or stay there, until their own term or a manual purge on that
device. What travels after a purge is the deletion (id and stamp), never the purge; no fingerprint
travels. The 30 days run on each device's own clock, from the moment the document entered its
trash. A live state from another device with a later stamp than the deletion brings a purged
document back, live (Céline, same day). Replaces the rule of 2026-10-15 (a purge marker with a
fingerprint purged a peer's identical copy). Ch. 05 *Deletions and purges*, the manifest line and
*Backup restore*; ch. 03 G13, G19, G27, G28. Not built: planned for v0.0.12.

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
