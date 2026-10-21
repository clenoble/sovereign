# C-DOC adoption step 5 — the tests (delete, trash, purge in every host)

*Written 2026-10-17 on branch `docs/tests-trash` (from `main` 14adca9), before the code, against
`doc/plans/C-DOC-adoption.md`, *Step 5*, guarantees S5-1 to S5-7 and S5-9 (Tauri commands only);
spec ch. 03 (*Soft delete and 30-day purge*, *Thread deletion and the fallback lane*, C-DOC G13
and G16) and ch. 08 (*Action targets*, AT8). Author: Claude Opus 5.5 (laptop, sub-agent, test
author). Whoever implements step 5 makes these pass **without editing the test files** listed
below; a test that looks wrong is reported, not worked around.*

**Céline answered Q1–Q4 the same day** (spec v2.0.28). The sections below the first two are
the original suite (4754fbe); **[Revised and added after Céline's answers](#revised-and-added-after-célines-answers-2026-10-17)**
at the end supersedes them where they differ: three tests renamed and inverted, S5-8, S5-10 and
S5-11 added. Still not tested: a per-item "delete forever" (Q1: there is none), a canvas gesture
to delete a thread (Q5, undecided).

## Test files (protected) and the stubs they need

| File | Guarantees |
|---|---|
| `crates/sovereign-docs/tests/s5_one_way_to_delete.rs` | S5-1 |
| `crates/sovereign-docs/tests/s5_every_host_purges.rs` | S5-3 |
| `crates/sovereign-docs/tests/s5_trash_entry_thread.rs` | S5-5 (service side) |
| `crates/sovereign-docs/tests/common/scan.rs` | the source-scan helper of S5-1 and S5-3 |
| `crates/sovereign-app/tests/cli_delete_trash.rs` | S5-2 (no account) |
| `crates/sovereign-app/src/step5_tests.rs` | S5-2 (account), S5-9 |
| `crates/sovereign-shell/src/step5_tests.rs` | S5-4, S5-5, S5-6, S5-7 |

Stubs (API only, bodies `todo!()`; the implementer fills them, and may change their insides,
not the signatures the tests use):

- `sovereign-docs/src/hosts.rs`: `purge_tick(&DocumentService) -> Result<PurgeReport, DocError>`,
  `PURGE_TICK_INTERVAL` (re-exported from the crate root).
- `sovereign-docs/src/types.rs`: `TrashEntry.thread_id: String` (additive), filled with an empty
  string in `service.rs` `list_trash` until step 5.
- `sovereign-app/src/commands.rs`: `delete_doc_with(&DocumentService, &str) -> Result<()>`.
- `sovereign-app/src/doc_trash.rs`: `TrashEntryDto`, `list_trash(docs, db, now)`,
  `restore_from_trash(docs, id)`, `empty_trash(docs) -> usize` — the layer under the three
  `#[tauri::command]` wrappers, which are not written yet (nor registered).
- `sovereign-shell/src/card_menu.rs`: `CardMenu` (`open_on`, `target_id`, `click_delete`,
  `open_skills`, `click_skill`) and `trash_target(docs, id)`.
- `sovereign-shell/src/trash.rs`: `TrashLine`, `TrashView` (`load`, `lines`, `restore`,
  `request_empty`, `empty_requested`, `cancel_empty`, `confirm_empty`).

The module declarations carry `#[allow(dead_code)]` while they are stubs.

## Guarantees → tests, and what each fails on today

Measured 2026-10-17 on this branch (`cargo test -p sovereign-docs`, `-p sovereign-shell step5`,
`-p sovereign-app --lib s5_`, `-p sovereign-app --test cli_delete_trash`): **31 step-5 tests,
28 fail, 3 pass by design** (the scan's self-test, the tick interval, and the CLI test's helper,
which does nothing when run alone). Every older test of `sovereign-docs` still passes.

| G | Test | Fails today on |
|---|---|---|
| S5-1 | `s5_one_way_to_delete::s5_1_no_production_code_outside_db_and_docs_deletes_around_the_service` | the four known sites: `sovereign-app/src/commands.rs` (`delete_document`, the CLI), `sovereign-app/src/lib.rs` (`purge_deleted`, the hourly loop), `sovereign-app/src/tauri_commands/documents.rs` (`soft_delete_document`), `sovereign-shell/src/doc_writes.rs` (`soft_delete_document`) |
| S5-1 | `s5_1_the_scan_sees_calls_and_only_calls` | passes — the scan itself: calls found (method, UFCS, split over lines, after a test module), definitions, strings, comments and `#[cfg(test)]` items ignored |
| S5-2 | `cli_delete_trash::s5_2_delete_doc_without_the_account_refuses_and_deletes_nothing` | `delete-doc` without an account exits 0 ("Deleted …") and hard-deletes the row |
| S5-2 | `step5_tests::s5_2_delete_doc_moves_the_document_to_the_trash_and_deletes_nothing` | `delete_doc_with` is `todo!` |
| S5-3 | `s5_every_host_purges::s5_3_the_tick_purges_what_is_30_days_in_the_trash_and_keeps_the_rest` | `purge_tick` is `todo!` |
| S5-3 | `s5_3_a_later_tick_purges_what_an_earlier_one_kept` | `purge_tick` is `todo!` |
| S5-3 | `s5_3_the_tick_interval_is_an_hour` | passes (a constant of the stub) |
| S5-3 | `s5_3_shell_and_mobile_call_the_purge_tick` | neither host's production code calls `purge_tick` |
| S5-4 | `step5_tests::s5_4_delete_targets_the_document_the_menu_opened_on_across_reloads` (shell) | `CardMenu` is `todo!` |
| S5-4 | `s5_4_confirming_on_a_document_trashed_meanwhile_does_nothing_and_says_so` | `CardMenu` is `todo!` |
| S5-4 | `s5_4_confirming_on_a_document_purged_meanwhile_does_nothing_and_says_so` | `CardMenu` is `todo!` |
| S5-4 | `s5_4_run_skill_targets_the_document_the_menu_opened_on` | `CardMenu` is `todo!` |
| S5-4 | `s5_4_app_menu_and_drag_carry_no_card_index` | `app.rs` still has `CtxTarget::Card(usize)`, `Skills(usize)`, `CtxAction::Delete(usize)`, `OpenSkills(usize)`, `RunSkill(usize, usize)`, `card_drag: Option<usize>` |
| S5-5 | `s5_trash_entry_thread::s5_5_a_trash_entry_carries_the_thread_the_document_was_in` | `TrashEntry.thread_id` is empty |
| S5-5 | `s5_5_a_trash_entry_keeps_the_thread_id_of_a_deleted_thread` | `TrashEntry.thread_id` is empty |
| S5-5 | `step5_tests::s5_5_the_trash_view_lists_newest_first_with_thread_date_days_left_and_no_content` (shell) | `TrashView` is `todo!` |
| S5-5 | `s5_5_days_left_count_down_from_30` (shell) | `TrashView` is `todo!` |
| S5-5 | `s5_5_a_document_whose_thread_is_gone_shows_the_fallback_lane` (shell) | `TrashView` is `todo!` |
| S5-6 | `s5_6_restore_brings_the_document_back_exactly_and_lists_it_at_once` (shell) | `TrashView` is `todo!` |
| S5-6 | `s5_6_restore_when_the_thread_is_gone_keeps_the_thread_id` (shell) | `TrashView` is `todo!` |
| S5-7 | `s5_7_empty_trash_asks_first_then_purges_and_reports_how_many` (shell) | `TrashView` is `todo!` |
| S5-7 | `s5_7_empty_trash_never_runs_without_its_confirmation` (shell) | `TrashView` is `todo!` |
| S5-9 (S5-5) | `step5_tests::s5_9_list_trash_newest_first_with_thread_date_days_left_and_no_content` (app) | `doc_trash::list_trash` is `todo!` |
| S5-9 (S5-5) | `s5_9_list_trash_days_left_count_down_from_30` | `doc_trash::list_trash` is `todo!` |
| S5-9 (S5-5) | `s5_9_list_trash_says_when_the_thread_is_gone` | `doc_trash::list_trash` is `todo!` |
| S5-9 (S5-6) | `s5_9_restore_from_trash_brings_the_document_back_exactly_and_lists_it_at_once` | `doc_trash::restore_from_trash` is `todo!` |
| S5-9 (S5-6) | `s5_9_restore_from_trash_when_the_thread_is_gone_keeps_the_thread_id` | `doc_trash::restore_from_trash` is `todo!` |
| S5-9 (S5-6) | `s5_9_restoring_what_is_not_in_the_trash_is_refused` | `doc_trash::restore_from_trash` is `todo!` |
| S5-9 (S5-7) | `s5_9_empty_trash_purges_the_trash_and_reports_how_many` | `doc_trash::empty_trash` is `todo!` |
| S5-9 | `s5_9_the_trash_commands_are_registered` | `list_trash`, `restore_from_trash`, `empty_trash` are not in `generate_handler!` |

Adding the three commands also means mirroring them in `lib.rs`'s existing
`ipc_classification_guard` (`ALL_COMMANDS`, and `GATED_COMMANDS`): that guard asks for it by design.

## Choices made here (for review)

- **The purge tick is a function of `sovereign-docs`** (`purge_tick`), shared by both hosts, so it
  can be tested with the service's injected clock. "At unlock and hourly" is host wiring a test
  cannot drive: what is checked is that both hosts' production code calls `purge_tick`, and that
  the interval is one hour.
- **S5-4 goes through a seam, `card_menu`**, because `App` cannot be built without a window and a
  GPU. The seam fixes the identity at `open_on`; `app.rs` is checked by a scan that the
  index-carrying variants and field are gone. Whether `App` actually uses `CardMenu` is a manual
  check. `Open(usize)` and `TogglePin(usize)` also carry an index (AT8 covers them; S5-4 does not
  name them): not scanned, except through `CtxTarget::Card(usize)`.
- **"If the document is gone, confirming does nothing and says so"** is read as covering a
  document trashed meanwhile (the service's `trash` is idempotent and would answer `Ok`), as well
  as one purged: both must return an error for the notice and change nothing.
- **Days left** = 30 minus the whole days elapsed; tested only at whole-day offsets (0, 1, 10, 29),
  so rounding inside a day is left open.
- **Thread gone**: the shell names the fallback lane "Unfiled" (ch. 03 names it so in the native
  shell). Mobile has no fallback lane yet: its DTO carries `thread_name: None` and the UI names it.
- **Restore when the thread is gone** keeps `thread_id` (the document is not re-filed); the lane
  it is drawn in is the canvas's existing fallback logic, not re-tested here.
- **No content in the trash view**: checked as the content not appearing in the shell lines'
  `Debug` output and in the mobile DTO's JSON.
- **Empty trash without the confirmation** must not purge; whether it answers `Err` or `Ok(0)` is
  left open.
- **S5-2 without the account** is a subprocess test; the document row is put in the database by a
  helper process (RocksDB keeps its lock until the opening process exits).
- **S5-5 cites G16 for "no content"**; G16 is "one event per change". The reference looks wrong
  (probably G17, readers get a typed view, or the trash entry's G13): reported, not acted on.
  *(The plan now says G17.)*

## Revised and added after Céline's answers (2026-10-17)

*Same author, on `docs/trash` (merged with `main` f3cc66d), after Céline answered Q1–Q4 (spec
v2.0.28; plan, *Céline's answers* and *What the answers change*). Measured against the
implementation of the first suite (f03b2d0) plus these stubs.*

### Test files added (protected like the others)

| File | Guarantees |
|---|---|
| `crates/sovereign-docs/tests/s5_10_thread_delete.rs` | S5-10 (the one path, in depth) |
| `crates/sovereign-ai/src/orchestrator_step5_tests.rs` (declared at the end of `orchestrator.rs`) | S5-10 (the assistant's `delete_thread`) |
| `frontend/src/lib/stores/trash.svelte.test.ts` | S5-11, S5-5/6/7 on the phone (store) |
| `frontend/src/lib/stores/deleteUndo.svelte.test.ts` | S5-8 on the phone (store) |
| `frontend/src/lib/components/TrashPanel.svelte.test.ts` | S5-11 (the trash in Settings) |
| `frontend/src/lib/components/UndoToast.svelte.test.ts` | S5-8 (one-click Delete in `ContextMenu`, the Undo notice) |
| `crates/sovereign-shell/TRASH_CHECKLIST.md` | the manual checks (rendering, placement, `App` wiring) |

Changed in files already protected: `s5_one_way_to_delete.rs` (a second scan, below),
`sovereign-shell/src/step5_tests.rs`, `sovereign-app/src/step5_tests.rs`.

### New stubs (bodies `todo!()` / throwing / empty)

- `sovereign-docs/src/hosts.rs`: `ThreadDeletePreview` (+ `confirmation_text`),
  `preview_thread_delete`, `ThreadDeleteReport`, `ThreadDeleteError { message }`,
  `delete_thread_to_trash(docs, db, thread_id)` — re-exported at the crate root. The one
  host-agnostic path for both the assistant and the phone.
- `sovereign-shell/src/trash.rs`: `TrashView::restore_into(docs, db, id, thread_id, now)`; for the
  grouped restore, `TrashGroup { thread_id, label, ids }`, `TrashView::deleted_thread_groups()`,
  `TrashView::restore_group_into(docs, db, deleted_thread_id, into_thread_id, now) -> usize`.
- `frontend/src/lib/stores/trash.svelte.ts` (grouped restore): `TrashGroup`,
  `deletedThreadGroups()`, `restoreGroup(threadId, intoThreadId)` — through `restore_from_trash`
  per document with the chosen thread (no new command).
- `sovereign-docs/tests/common/gen_faulty.py` and the regenerated `faulty.rs`:
  `FaultyDb::fail_once_after(n)` / `failed_once()` (test infrastructure).
- `sovereign-shell/src/delete_notice.rs` (new module): `DeleteNotice` (`new`, `doc_id`, `text`,
  `shown`, `undo`) and `UNDO_WINDOW` (10 s, the [choice]); time passed in as `Instant`.
- `sovereign-app/src/doc_trash.rs`: `restore_from_trash_to(docs, db, id, Option<thread>)`,
  `delete_thread_to_trash(docs, db, id) -> usize`.
- `frontend/src/lib/stores/trash.svelte.ts`, `deleteUndo.svelte.ts` (API, throwing),
  `components/TrashPanel.svelte`, `UndoToast.svelte` (empty).

### Tests → guarantees, and what each fails on today

Rust: **35 new or revised tests, 33 red, 2 green** (the 10 s constant; the phone list test whose
message alone changed). Vitest: **28 tests, 26 red, 2 green** (the 10 s constant; the toast shows
nothing without a notice). *(Updated after Céline's three more answers: all or nothing, grouped
restore. S5-12, a document arriving by sync for a deleted thread, is not written: a C-WIRE alert
kind built after the core of step 5.)* Every older `sovereign-docs` test
and every older vitest still passes; `svelte-check` reports only its 2 pre-existing errors.

| G | Test | Fails today on |
|---|---|---|
| S5-5 (rev.) | shell `s5_5_a_document_whose_thread_is_gone_says_its_thread_was_deleted` (was `…_shows_the_fallback_lane`) | the line says "Unfiled" |
| S5-5 (rev.) | app `s5_9_list_trash_says_when_the_thread_is_gone` | passes (`thread_name: None`; only its message changed) |
| S5-6 (rev.) | shell `s5_6_restore_when_the_thread_is_gone_needs_a_thread_and_says_so` (was `…_keeps_the_thread_id`) | `restore` succeeds without a thread |
| S5-6 (rev.) | shell `s5_6_restore_into_a_chosen_thread_brings_it_back_exactly_there` | `restore_into` is `todo!` |
| S5-6 (rev.) | shell `s5_6_restore_into_a_thread_that_does_not_exist_is_refused` | `restore_into` is `todo!` |
| S5-6 (rev.) | app `s5_9_restore_from_trash_when_the_thread_is_gone_needs_a_thread_and_says_so` (was `…_keeps_the_thread_id`) | `restore_from_trash_to` is `todo!` (and the old `restore_from_trash` must refuse too) |
| S5-6 (rev.) | app `s5_9_restore_into_a_chosen_thread_brings_it_back_exactly_there`, `s5_9_restore_into_a_thread_that_does_not_exist_is_refused`, `s5_9_restore_without_a_choice_when_the_thread_exists_goes_back_to_it` | `restore_from_trash_to` is `todo!` |
| S5-6 (rev.) | app `s5_9_the_restore_command_takes_an_optional_thread` | the command has no `thread_id: Option<String>` |
| S5-8 | shell `s5_4_*` (three) and `s5_8_*` that delete through the menu | `click_delete` arms on the first click (two clicks) |
| S5-8 | shell `s5_8_the_card_menus_delete_deletes_on_the_first_click` | same |
| S5-8 | shell `s5_8_the_notice_says_where_the_document_went_and_for_how_long` | `DeleteNotice` is `todo!` |
| S5-8 | shell `s5_8_the_undo_window_is_ten_seconds` | passes (a constant of the stub) |
| S5-8 | shell `s5_8_undo_while_the_notice_is_shown_restores_the_document`, `s5_8_after_the_window_undo_is_gone_and_the_document_stays_in_the_trash`, `s5_8_undo_after_the_document_left_the_trash_does_nothing_and_says_so` | two clicks, then `DeleteNotice` `todo!` |
| S5-10 | docs `s5_10_every_document_still_in_the_thread_goes_to_the_trash_then_the_thread_is_deleted` (each trashed now, new stamp, thread id kept; already-trashed untouched, not re-stamped; thread unlisted, its row kept as tombstone; other thread untouched) | `delete_thread_to_trash` is `todo!` |
| S5-10 | docs `s5_10_an_empty_thread_is_deleted_and_nothing_is_trashed`, `s5_10_an_unknown_or_already_deleted_thread_is_refused_and_nothing_changes` | `todo!` |
| S5-10 | docs `s5_10_a_thread_delete_is_all_or_nothing` (one transient failure at every point: on failure the thread stays, none of its documents in the trash, the earlier-trashed one untouched, the notice says nothing was deleted) — replaces `…_if_a_document_cannot_be_trashed_the_thread_is_not_deleted` | `todo!` |
| S5-10 | docs `s5_10_after_a_crash_a_rerun_converges` (*reading*: no deleted thread with a live document of it; the same delete run again completes) | `todo!` |
| S5-6 grouped | shell `s5_6_the_trash_groups_documents_by_deleted_thread` (one group per deleted thread, exactly its documents, heading says "deleted", live-thread documents in no group, each still a line) | `deleted_thread_groups` is `todo!` |
| S5-6 grouped | shell `s5_6_restore_all_brings_a_deleted_threads_documents_into_the_chosen_thread_at_once` (each exactly, in the chosen thread, listed at once, gone from the view; others stay) | `restore_group_into` is `todo!` |
| S5-6 grouped | shell `s5_6_one_document_of_a_group_can_be_restored_alone` | `restore_into` / `deleted_thread_groups` are `todo!` |
| S5-6 grouped | shell `s5_6_restore_all_into_a_thread_that_does_not_exist_is_refused` (deleted, unknown, the group's own thread: nothing restored) | `restore_group_into` is `todo!` |
| S5-10 | docs `s5_10_the_confirmation_names_the_thread_its_documents_and_the_trash` (name, count of documents still in it, "trash"; the preview changes nothing) | `preview_thread_delete` is `todo!` |
| S5-10 | docs `s5_one_way_to_delete::s5_10_no_host_deletes_or_restores_a_thread_around_the_one_path` | `orchestrator.rs` and `tauri_commands/threads.rs` call `soft_delete_thread` |
| S5-10 | ai `s5_10_the_assistants_delete_thread_sends_its_documents_to_the_trash` | the assistant soft-deletes the thread only; its documents stay out of the trash |
| S5-10 | ai `s5_10_without_the_document_service_no_thread_is_deleted` | locked, the thread is deleted and success reported |
| S5-10 | shell `s5_10_a_deleted_threads_documents_are_in_the_trash_saying_so` | `delete_thread_to_trash` is `todo!` |
| S5-10 | app `s5_10_the_phones_thread_delete_sends_its_documents_to_the_trash` | `doc_trash::delete_thread_to_trash` is `todo!` |
| S5-10 | app `s5_10_the_delete_thread_command_uses_the_one_path` | the command calls `soft_delete_thread` |
| S5-11 | vitest `trash.svelte.test.ts` (11): list, thread label ("deleted", never "Unfiled"), restore own thread, refuse without a thread and say why, restore into a chosen one, empty asks first, empty reports the count; **grouped**: groups by deleted thread, "restore all" calls `restore_from_trash` for each document with the chosen thread, one by one still works, a refused group restore restores nothing | the store throws (stub) |
| S5-8 | vitest `deleteUndo.svelte.test.ts` (5): one call to `delete_document` and the notice ("trash", "30 days"), 10 s window, Undo restores, gone after the window, refused once the document left the trash | the store throws (stub) |
| S5-11 | vitest `TrashPanel.svelte.test.ts` (8): lines with thread and days left; "deleted"; "Restore <title>" in one click when the thread exists; a dialog with a thread select when it was deleted; "Empty trash" opens a dialog, Cancel purges nothing, the confirmation empties; `SettingsPanel` contains the panel; **grouped**: one /restore all/i button for a deleted thread's group, its dialog restores every document of the group into the chosen thread, and each document keeps its own "Restore <title>" | the component renders nothing; Settings does not import it |
| S5-8 | vitest `UndoToast.svelte.test.ts` (4): `ContextMenu` Delete deletes on the first click and shows the notice; the toast (role `status`) says "trash" and "30 days" with **Undo**; Undo calls `restore_from_trash`; nothing shown without a notice (green) | no notice is set; the toast renders nothing |

### Choices made here (for review)

- **The thread-delete path lives in `sovereign-docs`** (`delete_thread_to_trash`, beside
  `list_trash_named`), taking the store like `list_trash_named` does, so the scan can allow the
  thread functions in exactly one host-facing crate. Both the orchestrator and the phone call it.
- **S5-1's thread scan allows `sovereign-p2p` for `soft_delete_thread`**, not only for
  `restore_soft_deleted_thread` as the brief said: sync applies a thread deletion received from a
  paired device to the thread row (`sync_service.rs:1033`), and running the whole thread delete
  there would trash documents that arrive with their own tombstones. The hard `delete_thread` is
  forbidden to hosts too (it would lose the tombstone sync carries).
- **The thread row survives as a tombstone** (`list_threads_including_deleted` shows it with
  `deleted_at`): asserted, on the strength of the spec's "survives only as what sync needs". That
  it is purged with the 30-day purge (plan) is not tested.
- **An unknown or already deleted thread is refused**, nothing changes.
- **All or nothing** (Céline, *Three more answers*) is tested with a **transient** failure: one
  store call fails — each call of the delete in turn — and every later call succeeds
  (`FaultyDb::fail_once_after`, added to the generator `gen_faulty.py` and regenerated; the
  existing `crash_after` is unchanged). On every failure: the thread stays, no document of it is
  in the trash, they are listed again, the one already in the trash before stays as it was, and
  the notice contains "nothing" (*my reading* of "says nothing was deleted").
- **A crash during the delete or its undo** (`crash_after`: every call fails from some point,
  so the undo itself cannot run) — *my reading, not Céline's answer*: after a restart the thread
  is never gone while one of its documents is out of the trash, and running the same delete again
  converges to the complete result. A crash may therefore leave some of its documents in the
  trash with the thread still there until the delete is run again; recovering that at startup
  instead would also pass.
- **The confirmation text**: tested as `ThreadDeletePreview::confirmation_text`; showing it in the
  assistant's proposal (AT5) and re-proposing when the thread changed (AT4) are C-HOST, not built,
  not tested.
- **The old `restore_from_trash` (no thread) must also refuse** a document whose thread is gone,
  or it stays a way into the fallback lane.
- **Restoring into a chosen thread when the document's own thread exists** is not tested
  (undecided whether the UI offers it).
- **The shell's trash line** keeps one text field, `thread_name`, which must say "deleted"
  (case-insensitive) and not contain "Unfiled"; the phone's DTO keeps `thread_name: None` and
  the Svelte line says "deleted".
- **The Undo window** is tested at 3, 9 and 11 s (not at the 10 s boundary). Undo after the
  window is refused (the notice is gone; the document stays restorable from the trash).
- **The phone's DOM contract** is kept small: titles, thread text, "N days", buttons named
  "Restore <title>", "Undo", /empty trash/i, a `dialog` for the thread choice (a `combobox`
  named /thread/i and a /restore/i button) and for the empty confirmation (/cancel/i, /^empty/i),
  a `status` role for the toast. The phone has no thread-delete screen today; its thread delete
  is tested at the command.
- **The orchestrator's tests** sit in a child module of `orchestrator.rs` (one declaration added
  at its end, like `orchestrator_unicode_tests.rs`), because `for_test` and `execute_action` are
  private. Nothing else in `orchestrator.rs` was touched.

### After the implementation (a6544b8): a harness fix and one more test

- **Harness bug (test author's):** `FaultyDb::disarm()` reset the crash budget but not the
  one-shot fault, so in `s5_10_a_thread_delete_is_all_or_nothing`, at the first `k` where the
  fault did not fire inside the delete, it stayed armed and failed the test's own next call
  (`list_trash`). Fixed in `gen_faulty.py` (`disarm()` also disarms the one-shot), `faulty.rs`
  regenerated. The test's assertions are unchanged. Measured: the six S5-10 tests pass 50 runs
  out of 50; `g13` passes.
- **S5-8, Undo after the thread was deleted** (within the Undo window). By Céline's rule nothing
  comes back from the trash into the fallback lane, so Undo restores nothing, says the thread is
  gone, and the document stays in the trash, restorable with a thread choice:
  - shell `s5_8_undo_after_its_thread_was_deleted_restores_nothing_and_says_so` — **red**: Undo
    restores the document into its deleted thread (`DeleteNotice::undo` calls the service's
    `restore_from_trash` directly);
  - vitest `deleteUndo.svelte.test.ts` "restores nothing when the document's thread was deleted
    meanwhile…" — green: the store asks for a plain restore (no thread) and passes the host's
    refusal on; the refusal itself is the host's, tested by
    `s5_9_restore_from_trash_when_the_thread_is_gone_needs_a_thread_and_says_so`.

### The label of a deleted thread (Céline, 2026-10-17)

"Ancien nom - supprimé", in English: **the thread's old name, then " - deleted"**. Pinned exactly:

- shell `s5_5_a_document_whose_thread_is_gone_says_its_thread_was_deleted` ("Lane - deleted"),
  `s5_10_a_deleted_threads_documents_are_in_the_trash_saying_so` ("Old drafts - deleted"),
  `s5_6_the_trash_groups_documents_by_deleted_thread` (group headings "X - deleted",
  "Y - deleted") — **red**: the shell shows "Lane (deleted thread)";
- app `s5_9_list_trash_says_when_the_thread_is_gone`, `s5_10_the_phones_thread_delete_sends_its_documents_to_the_trash`:
  the DTO keeps `thread_name: null` for a gone thread and carries the old name as
  **`deleted_thread_name`** (checked on the serialized DTO, so the tests compile before the
  field exists) — **red**: the field is absent;
- vitest `trash.svelte.test.ts` (`threadLabel` → "Old notes - deleted"; group labels) and
  `TrashPanel.svelte.test.ts` (the panel shows "Old notes - deleted"), the mocked DTO carrying
  `deleted_thread_name` — **red**: the phone shows "Deleted thread".

The field name `deleted_thread_name` is the test author's choice (`thread_name` keeps meaning a
live thread's name).
