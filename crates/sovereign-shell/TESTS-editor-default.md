# The editor in every build — the test suite (contract E1–E9)

Written 2026-10-19, before the code, against `doc/plans/EDITOR-default.md` (decided by
Céline the same day; E9 and the E3 / E6 / E8 precisions added after PANDA2's comments,
claude-coord 0115; E9 revised and the readings settled after this suite's first report,
"Settled after the test author's report"). Whoever makes the editor the shell's only document editor makes these
pass **without editing the test file**; a test that looks wrong is reported, not worked
around.

```
cargo test -p sovereign-shell editor_default_tests          # 41 tests + 1 self-test
cargo test -p sovereign-shell dev::                          # the dev-switch scan (E6)
cargo test --release -p sovereign-shell editor_default_tests # the release cfg (E1, E8)
```

Set `SOVEREIGN_DATA_DIR` to a fresh temporary directory for every run (the tests set one
themselves when it is missing; they never use `~/.sovereign`).

Protected file: `crates/sovereign-shell/src/editor_default_tests.rs`. It is a child module
of `app` (declared in `app.rs`, `#[path]`), so it drives the window manager's own paths —
`open_doc_window`, `open_doc_by_id`, `new_document`, `request_close_window`,
`resolve_unsaved`, `request_quit`, `run_skill`, `restore_commit`, `trash_document`,
`build_scene`, `build_a11y_tree` — on a real `App`. `App::new()` needs no window or GPU
(the headless `run_shot` builds one the same way). Stores: a real encrypted store over an
in-memory SurrealDB; the system clipboard is replaced by an in-memory one on the test's
thread, so no test touches the user's clipboard.

What only a winit event can reach (a key press: Esc) and what is about the build itself
(what a release build compiles, which names are gone) is checked by reading the source,
with comments blanked and `#[cfg(test)] mod … { … }` blocks removed
(`the_source_helpers_see_what_they_should` checks the reader on a sample).

## Seams (added with the tests; the implementer keeps their names and meaning)

| Item | Where | Today | What it is |
|---|---|---|---|
| `DocWindow::editor_pane()` / `editor_pane_mut()` | `panels.rs` | returns `v2` | the editor a document window holds |
| `Notice::said` | `app.rs` | set by `set_notice` | what a notice says, as text (title, body) |
| `EditorPane::status_text` | `app/editor_v2.rs` | widened to `pub(crate)` | the status line |
| `editor_v2::use_test_system_clipboard`, `clipboard_for_tests`, `pending_session_ends` | `app/editor_v2.rs` | `#[cfg(test)]` | an in-memory clipboard for the thread; the shared clipboard state; the queue of sessions to end |
| `App::a11y_frame_updates()` | `app.rs` | **`todo!`** | E8: the accessibility updates `render` sends after `build_a11y_tree` in a frame, as if the adapter were active (empty if `build_a11y_tree` carries everything) |

`DocWindow::editor_pane*` is gated like the `v2` field today; when the editor is compiled in
every build, the gate goes with it.

## Why red today

Two runs, both on `f99214f` + `8d1a9dd` (the contract with PANDA2's additions):

- **Default (no switch):** 3 pass (the self-test and two E7 additions that hold today), 39
  fail. Every behavioural test that needs an editor fails first at "the window holds the
  editor (E1)": without `SHELL_EDITOR_V2` a document opens in the plain window.
- **With `SHELL_EDITOR_V2=1` (today's development editor):** 22 pass, 20 fail. This run shows
  each guarantee's own red: E3 (the plain window still opens), E5 (an assistant write does
  not reach the editor), E8 (stub), E9 (a store replacement keeps the windows and the
  clipboard), and the scans.

Red reasons: **E1** = no editor without the switch; **todo** = a stub panics;
**scan** = the source still has what the guarantee removes; **assert** = the assertion fails
on today's behaviour even with the switch; **guard** = passes with the switch today (the dev
editor already does it) — it is red by default only because of E1, and keeps the behaviour
when the plain path goes.

### E1 — one editor, every build

| Test | Red |
|---|---|
| `e1_a_card_opens_the_editor_with_no_switch` — `open_doc_window` | E1 |
| `e1_alerts_and_orphans_open_the_editor` — `open_doc_by_id` (Alerts and the orphan section both use it) | E1 |
| `e1_a_new_document_opens_the_editor_on_its_title` — `new_document`, caret in the title (T3) | E1 |
| `e1_shell_editor_v2_is_read_nowhere` — no production file (any subdirectory, `*_tests.rs` excepted) holds `"SHELL_EDITOR_V2"` | scan (`app/editor_v2.rs`) |
| `e1_the_editor_module_is_compiled_in_every_build` — no development-only `cfg` on `mod editor_v2;` | scan |

"In release builds alike": the behavioural tests run under `cargo test --release` (below).

### E2 — the plain path is gone

| Test | Red |
|---|---|
| `e2_no_production_code_keeps_the_plain_edit` — whole identifiers, every production file: `edit_buf`, `edit_origin`, `text_edit`, `TextEdit`, `save_text`, `begin_doc_edit`, `save_doc`, `save_active_doc`, `request_leave_edit`, `LeaveEdit`, `top_doc_editing`, `begin_edit_text`, `begin_edit`, `DocBtn::Edit/Save/Cancel` | scan (63 uses) |
| `e2_the_document_window_has_no_plain_edit_state` — `struct DocWindow` has no `editing`, `edit_buf`, `edit_origin`, `text_edit`, `dirty`, `needs_reshape`, `lbl_edit/save/cancel`; `impl DocWindow` no `begin_edit`, `cancel_edit`, `insert_char`, `backspace`, `shown_text`; `DocBtn` no `Edit/Save/Cancel` | scan |
| `e2_the_window_header_keeps_history_only` — an open document's header: History, nothing else | E1 (today [Edit, History]); guard with the switch |
| the plain AccessKit body label — in `e8_the_accessibility_tree_exposes_the_document_through_the_editor`: no `Label` node holds the body | todo |

### E3 — when the editor cannot open a document

Every notice is also checked against the E3 precision (`assert_plain_notice`): not empty, no
"usual way", no "Editor v2", no `{"`, no stored text, and none of *persona, duress, decoy,
another/other store, another/other workspace*.

| Test | Red |
|---|---|
| `e3_no_database_opens_no_window_and_says_why` — mentions the database | assert (a window opens) |
| `e3_an_unreadable_document_opens_no_window_and_says_why` — a card whose document is not in the store; mentions "read" | assert |
| `e3_an_encrypted_document_without_a_session_opens_no_window` — no-auth mode, a raw store, a row with a nonce; mentions "encrypt"; never the ciphertext | assert |
| `e3_the_editors_refusal_opens_no_window_and_never_quotes_the_json` — a v2 row the reader refuses (`"blocks"` not a list) | assert |
| `e3_a_newer_format_opens_read_only_with_the_editors_status_line` — `v: 99`: opens read only, status line "…newer version…" | E1; guard |

### E4 — nothing unsaved is lost on any way out

| Test | Red |
|---|---|
| `e4_the_close_button_never_loses_unsaved_edits` — after ×, the window stays (asking or saving) or closes only once the text is stored | E1; guard |
| `e4_a_failing_save_makes_the_close_button_ask_and_the_prompt_keep_the_text` — saves fail: × asks; Save fails again and keeps window and text; the prompt comes back; Discard closes; nothing written | E1; guard |
| `e4_escape_never_closes_a_document_window_without_the_unsaved_check` — reads `window_event`: the Esc arm that collapses a fanned deck and closes the top window, if it closes a window, names one of `request_close_window`, `v2_request_close`, `has_unsaved_edits`, `WindowContent::Doc`, `WindowKind::Doc` | scan (`self.windows.pop()`) |
| `e4_quitting_never_loses_unsaved_edits` — `request_quit` then what `about_to_wait` does, frame by frame | E1; guard |
| `e4_quitting_ends_every_editor_session` — nothing unsaved: quitting makes the session's last save a version | E1; guard |
| `e4_quitting_after_discarding_still_ends_the_session` — a failed save, the Quit prompt, Discard: the session still ends; the discarded text is not written | E1; guard |
| `e4_a_skill_never_runs_over_unsaved_edits` — `run_skill` (table-of-contents): nothing written, window and text kept | E1; guard |
| `e4_a_restore_never_runs_over_unsaved_edits` — `restore_commit` | E1; guard |
| `e4_moving_to_the_trash_never_runs_over_unsaved_edits` — `trash_document` | E1; guard |

The title's half (an unsaved title is an unsaved edit of the window) is
`title_tests::t5_an_unsaved_title_is_an_unsaved_edit_of_the_window`.

### E5 — every refresh reaches an open editor

| Test | Red |
|---|---|
| `e5_an_assistant_write_reaches_an_open_editor` — a write through the service as the assistant, then `ActionExecuted`, then a frame | assert (the editor still shows the old text) |
| `e5_a_chat_turns_write_reaches_an_open_editor` — the same with `ChatTurnEnded` and a succeeded write | assert |
| `e5_with_unsaved_edits_the_editor_keeps_them_and_overwrites_nothing` — the editor keeps its text; its save lands; the assistant's state is in History | E1; guard |
| `e5_a_restore_reaches_an_open_editor` | E1; guard |
| `e5_a_skill_reaches_an_open_editor` | E1; guard |
| `e5_a_peer_sync_reaches_an_open_editor` — `v2_after_sync` | E1; guard |

### E6 — development-only stays development-only

| Test | Red |
|---|---|
| `e6_the_development_banner_and_name_are_gone` — no "Editor v2 (development", no `"Editor v2"` (notice titles), no `BANNER` in the editor | scan |
| `e6_the_status_line_stays` — "Saved…" after a save, "Save failed" after a failing one | E1; guard |
| `e6_the_editors_development_switches_are_not_compiled_into_release` — every `dev::var(`, `dev::flag(` and `"SHELL_…"` in `app/editor_v2.rs` (and `app/editor_v2/**` if it is split) sits inside an item under a development-only `cfg` (or `cfg(test)`) | scan (`SHELL_FAIL_SAVES_WHILE` in `v2_frame`, `SHELL_SEED_PII_TEST`, `SHELL_EDITOR_V2`) |
| `e6_shell_open_doc_opens_the_editor_or_is_gone` — if `"SHELL_OPEN_DOC"` is still read, its `if self.debug_open` block opens through `open_doc_window` / `open_doc_by_id`, never `DocWindow::new` | scan |
| `dev::tests::no_dev_switch_is_read_outside_the_gate` — now walks every subdirectory of `src/` | passes (a strengthening: nothing reads a switch directly today) |
| `dev::tests::the_dev_switch_scan_walks_every_subdirectory` — self-test on a sample tree two levels deep | passes |

`SHELL_DOC_EDIT`'s plain fields fall under E2's scan.

### E7 — what the plain path's tests guarded still holds

| Invariant | Covered by (existing) | Added |
|---|---|---|
| never stored JSON — v1 envelope, raw | `editor_v2::tests::opening_reads_raw_text_and_the_v1_envelope` | `e7_the_editor_never_shows_stored_json` — v1, raw **and v2**, text and accessible values (passes today) |
| a save writes v2 and makes a version | `editor_v2::tests::editor_saves_go_through_the_service`, `an_editor_save_runs_ingest` | — |
| a stale base overwrites nothing | `editor_v2::tests::a_conflict_overwrites_nothing_and_a_rebased_save_keeps_both`; `title_tests::t2_a_conflicting_title_save_overwrites_nothing` | — |
| a write error is never reported as success | `editor_v2::tests::a_failed_save_keeps_the_text_and_is_announced_once` (an injected error) | `e7_a_save_on_a_missing_document_is_an_error` — service level (passes today); `e7_a_refused_save_is_never_shown_as_saved_in_the_window` — the document trashed meanwhile: "Save failed", still unsaved (E1; guard) |

### E8 — release builds

| Test | Red |
|---|---|
| `e8_the_accessibility_tree_exposes_the_document_through_the_editor` — after `build_a11y_tree` then `a11y_frame_updates`, reachable from the root: a `TextInput` "Title" holding the title, a `TextRun` holding the body, and no `Label` holding the body | todo |
| `e8_the_accessibility_path_is_the_same_in_every_build` — no development-only `cfg` in `build_a11y_tree` or `render` | scan |
| `e1_the_editor_module_is_compiled_in_every_build` (above) | scan |

**"Release cfg", approximated:** the test module is `#[cfg(test)]`, not gated on
`debug_assertions`, so `cargo test --release -p sovereign-shell editor_default_tests` runs
every behavioural test with `debug_assertions` off — the cfg a release build has. Today that
run **does not compile** (the module names `editor_v2`, absent from release builds): that is
E8's red. Two consequences for the release gate, to decide: `scripts/prepublish.sh` step 5b
runs `cargo test --release -p sovereign-shell dev::`, which compiles the whole test binary,
so **5b does not compile on this branch until the editor is in release builds**; and 5b
filters on `dev::`, so this branch adds the run
`cargo test --release -p sovereign-shell editor_default_tests` to 5b, in
`scripts/prepublish.sh` and `_prepublish.bat` (settled; PANDA2 reviews it). `cargo check -p sovereign-shell --release` and the binary size
before / after are not tests (the merge message states the size, per the contract).

### E9 — a store replaced under open editors leaves no editor state

Revised (settled): the shell has no lock, logout or persona switch; a store is replaced by
`install_document_service`, which every unlock calls (`try_login`, `try_onboard`,
`finish_onboarding`, `recovery_finalize`). The tests drive it as those do (`unlock_into`: the
incoming store's `db` and key first, then `install_document_service`) with document windows
open. When the shell gets a lock, E9 applies to it unchanged.

| Test | Red |
|---|---|
| `e9_replacing_the_store_closes_the_editors_and_ends_their_sessions_on_the_outgoing_store` — no document window; the session queue empty **as soon as the replacement returns** (before any frame); the session's last save a version **in the outgoing store**; nothing more ended after a frame. Together these prove nothing ends on the incoming store | E1; assert with the switch (2 windows stay) |
| `e9_a_replacement_looks_the_same_with_or_without_unsaved_edits` — no unsaved prompt; the screen after a replacement with unsaved edits equals the screen after one with no document open (locked, notice, prompt, windows, pending action, quit) | E1; assert with the switch (1 window stays) |
| `e9_nothing_of_the_editor_is_reachable_after_the_store_is_replaced` — no document window, no queued session, the clipboard's private payload and source document gone, nothing of the outgoing store on the canvas, no notice quoting it | E1; assert with the switch |
| `e9_a_revealed_copy_does_not_outlive_the_replacement` — a revealed value waiting on the clipboard for its clear timer is gone after the replacement | assert (in both runs) |
| `e9_a_replacement_closes_the_outgoing_stores_history_windows` — added in round 3 (E9 extended, "Settled after the implementer's report"): a document and its History (two versions) open, the store replaced; no History window remains, no window names the outgoing document, and the accessibility tree says nothing of it | assert at `c82efc5` (the History window stays) |

**The readers gate** (`sovereign-docformat/tests/readers_gate.rs`) scanned this file — it is
declared `#[cfg(test)] #[path] mod` in `app.rs`, and the gate skipped only `#[cfg(test)]`
items inside a file — and flagged its fixtures (`.create_document(`, `.update_document(`).
Round 3 taught the gate to skip files declared only under a `cfg` that requires `test`
(and what they declare), with `a_test_only_file_is_never_declared_without_cfg_test` and two
self-tests. Declared without `cfg(test)`, this file is scanned again (checked by mutation).
The other source scans: `step6_scans.rs` and `ts1_one_thread_write_path.rs` already read
`#[cfg(test)] #[path] mod` (their `more_test_only_files`), but treat any `#[cfg(…)]` line
containing "test" as test-only (an `any(test, …)` would skip a production file; none exists
today) and have no both-ways check; `s5_one_way_to_delete.rs` reads only
`#[cfg(test)] mod name;` directly, so it scans this file (it passes: no delete calls here).
Not changed: reported.

## Validation against a throwaway implementation

Done 2026-10-19 in a scratch copy of this branch (`git archive`), discarded afterwards,
never committed. What it did, roughly: the editor module and every editor call site
compiled in every build; `open_doc_window` opens only through `v2_open` (no window and a
"Document" notice otherwise, no "usual way"); the plain path deleted (`DocWindow` fields,
methods and buttons, `TextEdit` / `save_text`, `begin_doc_edit`, `save_doc`,
`save_active_doc`, `request_leave_edit`, `LeaveEdit`, the plain keyboard branch, the
`SHELL_DOC_EDIT` block, the plain tests listed below); the banner removed;
`SHELL_FAIL_SAVES_WHILE` and the token seed under the development `cfg`; `SHELL_OPEN_DOC`
through `open_doc_window`; Esc on a document window through `request_close_window`;
`v2_after_sync` after every orchestrator reload; `a11y_frame_updates` as the editor half of
`v2_push_a11y`; and, for the first version of E9, a `lock` closing every window, ending
the queued sessions, clearing the clipboard's private state (and a pending revealed copy).
For the revised E9, the same three steps at the top of `install_document_service`, before
the incoming service replaces the outgoing one (close the document windows, `v2_end_sessions`
on the still-current outgoing service, clear `ClipState`), tried in place with
`SHELL_EDITOR_V2=1` and reverted: the four E9 tests pass; with the drain left to the next
frame (the cross-store hazard) `e9_replacing_the_store_…` fails ("the queue was drained at
the replacement, not left to a frame").

- `cargo test -p sovereign-shell` on it: **282 passed, 0 failed** (this suite, the title
  tests, the dev scan and every other shell test).
- Mutations, each caught: the trash's unsaved check removed → `e4_moving_to_the_trash_…`;
  `v2_end_sessions(true)` removed from quit → `e4_quitting_ends_every_editor_session` and
  `e4_quitting_after_discarding_still_ends_the_session`; the session drain removed from
  the lock → the E9 lock test (now `e9_replacing_the_store_…`).
- Two test errors of mine found by it and fixed before the commit: the sealed-document
  test used `MockGraphDB`, whose ids do not survive the canvas's id form (now a raw
  in-memory SurrealDB, as no-auth opens); the E9 test read versions through the shell's
  service, which the replacement swaps (now through the outgoing store's own service).

- `cargo test --release -p sovereign-shell -- editor_default_tests dev::` on it (the release
  cfg: `debug_assertions` off, no `dev-switches`): **47 passed, 0 failed** — the 42 here and
  the 5 `dev::` tests, the E8 accessibility test among them.

On this branch itself: `cargo check -p sovereign-shell --all-targets` and
`cargo check -p sovereign-shell --release` compile (the stubs exist in every build);
`cargo test --release -p sovereign-shell` does not (E8's red, above).

## Plain-path tests that become meaningless

None deleted here: on this branch the plain path still exists and they still compile. The
implementer deletes these when the plain path goes (they cannot compile without it); their
invariants are covered as listed:

| Test | Uses | Covered by |
|---|---|---|
| `panels::doc_edit_tests::doc_window_edit_buffer_starts_from_the_text` | `begin_edit`, `edit_buf`, `dirty`, `text_edit` | `e7_the_editor_never_shows_stored_json`; `opening_reads_raw_text_and_the_v1_envelope` |
| `panels::doc_edit_tests::only_a_dirty_edit_buffer_counts_as_unsaved` | `begin_edit`, `insert_char`, `backspace`, `cancel_edit` | `e4_*` (window `has_unsaved_edits`), `the_editor_receives_each_keystroke_once` (`!has_unsaved` after a save), `t5_an_unsaved_title_is_an_unsaved_edit_of_the_window` |
| `panels::doc_edit_tests::edit_buffer_tracks_dirty_against_saved_body` | `edit_buf`, `dirty` | as above |
| `panels::doc_edit_tests::cancel_leaves_edit_mode` | `cancel_edit`, `editing` | `e4_a_failing_save_makes_the_close_button_ask_and_the_prompt_keep_the_text` (Discard writes nothing) |
| `panels::doc_edit_tests::newline_appends_to_buffer` | `insert_char` | `editor_v2::tests::the_editor_receives_each_keystroke_once` (Enter makes one line break) |
| `doc_writes::tests::the_window_text_save_writes_v2_and_versions` (361) | `TextEdit`, `save_text` | `editor_saves_go_through_the_service`, `an_editor_save_runs_ingest` |
| `doc_writes::tests::the_window_text_save_refuses_structure_and_stale_bases` (387) | `TextEdit`, `save_text` | stale: `a_conflict_overwrites_nothing_and_a_rebased_save_keeps_both`; structure: no longer applies (the editor edits structured documents) |
| `doc_writes::tests::a_write_error_is_never_reported_as_success` (448) | `TextEdit`, `save_text` | `e7_a_save_on_a_missing_document_is_an_error`, `e7_a_refused_save_is_never_shown_as_saved_in_the_window` |

`panels::doc_edit_tests::doc_window_shows_text_for_v2_v1_and_raw` reads `DocWindow::body_text`
only; it stays if `body_text` stays.

## Readings chosen (the implementer may not change them; the maille may)

- **E4 "asks first"** is read as *nothing unsaved is lost*: on ×, Esc or quit with unsaved
  edits the editor may save and then close (today's development behaviour) instead of
  showing a prompt; it must ask when the save fails. A literal "always a prompt" would
  contradict spec ch. 12's close-saves-first and today's editor.
- **E4 Esc** is tested on the generic Esc arm (the one that collapses a fanned deck). If the
  implementer routes document windows elsewhere, that arm must still show it (one of the
  guard names, e.g. a `WindowContent::Doc` check).
- **E3** checks the reason loosely ("database", "read", "encrypt"), and that the editor's
  refusal never quotes the stored JSON; it does not fix the wording.
- **E5** sources tested: the assistant (`ActionExecuted`, `ChatTurnEnded`), a restore, a skill,
  a peer sync. The other orchestrator events (`DocumentCreated`, `ThreadRenamed`,
  `DocumentMoved`, …) change no open document's text and are not tested.
- **E8** "through the editor's own nodes": the title as the editor's `TextInput` "Title", the
  body as `TextRun`s, reachable from the root of what the shell sends in a frame.
- **E9** the clipboard state (`ClipState`): its Sovereign payload, its source document,
  and a revealed value waiting for its clear timer (cleared at once, not left to the timer).
  Whether a replacement also clears ordinary copied text is not specified and not tested.
- **E9** unsaved edits at a replacement: their fate is not asserted; only that no prompt
  appears and the screen matches a replacement with nothing open.
- **Settled** ("Settled after the test author's report"): E4 = nothing unsaved is lost;
  E2 = History only (no menu); E8 = the frame's updates, release cfg = `--release`.

## Contract problems

All nine raised in the first report were settled by the coordinator on 2026-10-19
(`doc/plans/EDITOR-default.md`, "Settled after the test author's report"): E9 revised to the
store replacement (the `App::lock` stub is gone); the cross-store hazard is what
`e9_replacing_the_store_…` now proves; the clipboard reading, E4, E2 and E8 adopted; the
release-gate line added on this branch. Still open, not blocking: E1 "an orphan resolution"
opens no window (opening from the orphan section is what is tested), and E3's encrypted case
exists only in the development no-auth mode.
