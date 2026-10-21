# Renaming a thread in its lane label — the test suite (contract R1–R8)

Written 2026-10-18, before the code, against `doc/plans/THREAD-name.md`, *Renaming a thread —
decided 2026-10-18* (Céline: "dans l'étiquette ou via l'orchestrator"), its N1–N7 (which R3
and R5 cite), `doc/plans/THREAD-sync.md` S1 (the one stamped thread-write path), and spec
ch. 03 (*Thread deletion and the fallback lane*, decision of 2026-10-18) and ch. 12. Whoever
builds it makes these pass **without editing the test file**; a test that looks wrong is
reported, not worked around.

```
cargo test -p sovereign-shell thread_rename_tests     # 43 tests
```

Protected file: `crates/sovereign-shell/src/thread_rename_tests.rs`. Manual checks (drawing,
real focus, a real IME, Narrator, two devices): `THREAD_CHECKLIST.md` §R.

## The seams

| Item | Stub | What it is |
|---|---|---|
| `thread_writes::ThreadWrites::rename_thread(id, name) -> WriteFuture` | trait | **the seam to S1**: the one stamped thread-write path, as the shell uses it |
| `thread_writes::StampedThreadWrites(Arc<DocumentService>)` | built | the shell's binding of `ThreadWrites`: S1's path, `DocumentService::thread_update` (the stub held an `Arc<dyn GraphDB>`; S1 landed as a service method, so the binding holds the service; no test builds it) |
| `lane_rename::save_thread_rename(&dyn GraphDB, &dyn ThreadWrites, id, name) -> Result<RenameOutcome, String>` | `todo!` | reads the thread, trims, one line, writes through the seam; `Unchanged` (empty, or the name it has: R2, R8) and `ThreadGone` (deleted or missing here: R7) write nothing |
| `RenameOutcome { Written, Unchanged, ThreadGone }` | — | what the save did |
| `lane_rename::lane_labels(&[Thread]) -> Vec<LaneLabel { lane, thread_id, name }>` | `todo!` | the renamable labels: lane, the live thread the label shows, its name |
| `lane_rename::lane_label_nodes(&[LaneLabel]) -> Vec<accesskit::Node>` | `todo!` | one button "Rename thread <name>" per label |
| `canvas::lane_label_rect(lane_top, label_w) -> Rect` | `todo!` | the label pill, shared by drawing and hit-testing |
| `lane_rename::label_at(p, &[(lane, Rect)]) -> Option<lane>` | `todo!` | hit-testing the labels |
| `lane_rename::LabelClicks::press(hit, p, now) -> Option<lane>` | `todo!` | a double-click on one label (`winit_input::click_count` rules) |
| `lane_rename::LaneRename::open(thread_id, current_name, fonts, clipboard)` | `todo!` | the inline field, name selected, focused |
| `LaneRename::thread_id()` / `field()` / `field_mut()` / `name()` | `todo!` | the thread renamed; the editor's single-line field; the name typed |
| `LaneRename::key(&WKey, text, mods)` / `event(InputEvent)` / `command(Command)` | `todo!` | as `NewThreadPrompt`'s |
| `LaneRename::layout(label, w, h) -> Rect` / `click(p)` / `close()` | `todo!` | the field over the label; a click; dismissed by another path |
| `LaneRename::accessibility_tree()` | `todo!` | a text input "Thread name", focused |
| `RenameStep { Open, Save { thread_id, name }, Cancel }` | — | the field's answer |
| `lane_rename::FIELD_LABEL`, `LABEL_BUTTON_PREFIX` | constants | "Thread name" (the prompt's), "Rename thread " |

Read in `app.rs` (production part, comments stripped), because `App` needs a window and a
GPU: `fn build_scene` calls `lane_label_rect(` and no longer builds the pill inline
(`RoundedRect::new(6.0, py`); `LabelClicks`, `label_at(` and `LaneRename::open(` appear;
`RenameStep::Save` is matched; `save_thread_rename(` is called at exactly one place, in a
method that also names `StampedThreadWrites` and calls `load_workspace_now`; `fn
build_a11y_tree` calls `lane_label_nodes(` and mentions `lane_rename` (the `App` field holding
the open field). In `app.rs` and `lane_rename.rs`: no `update_thread(` call; in
`lane_rename.rs`: no `list_threads` / `find_thread_by_name`.

Two test doubles stand for S1: `RecordingWrites` (records each rename, answers `Ok` or a
given error) and `StoreStandIn` (writes the name with the store's `update_thread`, unstamped,
only to check what the canvas shows afterwards; the stamp is S1's own suite). Stores are
in-memory encrypted SurrealDB (`docs_host::testing`).

## Tests per guarantee, and why each is red today

Red reasons: **todo** = a stub panics; **assert** = the assertion fails on today's code;
**green** = a guard that holds today and must keep holding.

| Guarantee | Test | Red |
|---|---|---|
| R1 | `r1_a_double_click_on_a_label_opens_it` — second press within the multi-click time | todo |
| R1 | `r1_only_a_double_click_on_one_label_opens` — single, slow, two labels, off-label | todo |
| R1 | `r1_label_at_finds_the_label_under_the_point` | todo |
| R1 | `r1_the_label_rect_is_a_pill_at_the_lane_left_edge` — left edge, wider than its text, inside the lane at zoom 1 | todo |
| R1 | `r1_the_label_stays_below_the_time_ruler` | todo |
| R1 | `r1_the_shell_draws_and_hits_the_same_label` — source | assert |
| R1 | `r1_the_field_opens_with_the_current_name_selected` — whole name selected; typing replaces | todo |
| R1 | `r1_the_field_is_focused` | todo |
| R1 | `r1_the_field_lies_over_the_label` — covers the label's centre, inside the window | todo |
| R1 | `r1_the_save_names_the_thread_opened` — Save carries the id given at open | todo |
| R1 | `r1_labels_are_the_threads_the_lanes_show` — against `canvas::load_workspace` | todo |
| R1 | `r1_a_shared_lane_renames_the_thread_its_label_shows` — 13 threads, 12 lanes | todo |
| R2 | `r2_enter_saves_the_trimmed_name` | todo |
| R2 | `r2_a_pasted_newline_becomes_a_space` | todo |
| R2 | `r2_esc_cancels` | todo |
| R2 | `r2_a_click_outside_cancels` — three points outside; a click in the field keeps it | todo |
| R2 | `r2_closing_cancels` | todo |
| R2 | `r2_an_empty_name_changes_nothing_and_closes` — Enter → Cancel; save → `Ok(false)`, no write | todo |
| R2, S1 | `r2_the_save_goes_through_the_stamped_write_path` — one write, trimmed one-line name | todo |
| R2 | `r2_a_failed_save_is_reported` — `Err` back, one attempt | todo |
| R2 | `r2_the_label_shows_the_new_name` — same lane, same thread, new name | todo |
| R2 | `r2_the_shell_saves_through_the_seam_then_reloads` — source | assert |
| R2, S1 | `r2_no_rename_around_the_stamped_path` — source | green |
| R3 | `r3_the_field_is_the_editors_single_line_field` | todo |
| R3 | `r3_text_is_sent_once` | todo |
| R3 | `r3_copy_cut_paste_go_through_the_clipboard` — privacy flags of an editor copy | todo |
| R3 (N7) | `r3_enter_while_composing_does_not_save` | todo |
| R3 (N7) | `r3_esc_while_composing_ends_the_composition_first` | todo |
| R3 | `r3_a_click_in_the_field_places_the_caret` | todo |
| R3 (T9) | `r3_a_long_name_scrolls_inside_the_field` | todo |
| R4 | `r4_each_label_is_a_rename_button` | todo |
| R4 | `r4_the_open_field_is_a_text_input_labelled_thread_name` — focused, value = name | todo |
| R4 | `r4_the_shell_announces_the_labels_and_the_field` — source | assert |
| R5 | `r5_a_name_already_used_is_saved` | todo |
| R5 | `r5_the_rename_checks_no_existing_names` — source | green |
| R6 | `r6_no_label_for_a_deleted_thread_nor_the_fallback_lane` — deleted elsewhere, its document in *Unfiled* | todo |
| R6 | `r6_a_deleted_row_or_a_marker_never_gets_a_label` — rows with `deleted_at`, one emptied | todo |
| R7 | `r7_a_thread_deleted_meanwhile_saves_nothing` — `ThreadGone`, no write, stays deleted | todo |
| R7 | `r7_a_missing_thread_saves_nothing` | todo |
| R7 | `r7_the_shell_tells_the_thread_is_gone` — source: `RenameOutcome::ThreadGone` and `set_notice` where the save is made | assert |
| R8 | `r8_enter_with_the_unchanged_name_closes_without_saving` — as opened, retyped with spaces; a case change saves | todo |
| R8 | `r8_the_name_the_thread_has_writes_nothing` — `Unchanged`, no write | todo |
| P4 | `p4_the_label_buttons_do_not_depend_on_an_open_window` — source: `lane_label_nodes(` outside the window / canvas branch of `build_a11y_tree` | assert |

## Readings chosen (the implementer may not change them; the maille may)

- **The seam to S1** is a trait in the shell (`ThreadWrites`), with the binding in its own
  file (`thread_writes.rs`), so the source check on `app.rs` / `lane_rename.rs` holds whatever
  shape S1 takes — even if S1 turns out to be the store's `update_thread`, stamped.
- **Which thread a label renames.** Lanes are shared when there are more than 12 threads
  (thread *i* is drawn in lane *i* mod 12, `canvas::load_workspace`), and the label shows the
  last of them. The rename targets the thread whose name the label shows; the field carries
  its id from the double-click, so a reload under it does not retarget it.
- **No label, no rename**: the *Unfiled* fallback lane (no thread), a deleted thread and a
  row with `deleted_at` (a future marker included) have no `LaneLabel`, hence no button and
  nothing to double-click. A thread whose name is empty draws no label today, so it has none
  either (not asserted).
- **"The label shows the new name at once"**: the shell reloads the workspace right after
  the save (as the new-thread prompt does); not an optimistic in-memory change.
- **A failed save** is returned to the shell (it shows a notice, as the prompt's creation
  does); the label keeps its old name. One attempt, no retry.
- **The double-click** follows `winit_input::click_count` (500 ms, 4 px); the second press
  must be on the same label.
- **R8 is checked twice**: the field compares with the name it opened with (so a rename made
  elsewhere meanwhile is not overridden by a name the user left as it was), and the save
  compares with the name the thread has in the store. A change of case is a change.
- **R7 is checked at the save** (it reads the thread in the store before writing: deleted or
  missing → `ThreadGone`, no write); the shell shows a notice there. Whether the field also
  closes on a reload that drops its thread, before Enter, is the implementer's (manual R7).
- **P4** is read as: the label buttons are built outside the branch of `build_a11y_tree`
  that announces the top window or the empty canvas (between `self.windows.last()` and
  `let mut nodes`). While locked, not asserted.
- Not asserted: what a single click on a label does; the field's exact size.

## Problems found in R1–R6 (for Céline, through the maille)

*Settled 2026-10-18 (plan, main 3dc6e8e): P1 → R7, P2 → R8, P4 → the buttons are present
whether or not a window is open; P3 noted as the shell's general accessibility gap (BACKLOG §3),
not solved here. Kept below as raised.*

- **P1 — a rename racing a deletion.** R6 says renaming is "not for a deleted thread (there
  is no label to click)". But the field can be open when the thread is deleted (by the
  assistant, or by a deletion arriving through sync). On Enter, S2 ("the later action wins")
  would make this rename win and **bring the thread back**; R6's spirit says it should not.
  Undecided; not tested. Options: (a) the field closes when its thread disappears on reload,
  and the save refuses a thread deleted here (R6 wins locally; S2 still applies between
  devices); (b) the save goes through and revives the thread (S2 everywhere). Recommendation:
  (a).
- **P2 — Enter with the name unchanged.** R2 says Enter saves. A save stamps a write (S1);
  an unchanged name written later than a real rename made offline on another device would
  **win over it** (S2) and silently undo it. Recommendation: an unchanged name changes
  nothing and closes, like an empty one. Not tested either way.
- **P3 — keyboard and screen-reader access.** R1's only gesture is a double-click. R4 makes
  each label a button, but the shell's AccessKit action handler is a no-op
  (`app.rs`, `A11yAction::do_action`), and nothing gives the labels keyboard focus: a
  keyboard or screen-reader user cannot open the field from the label (only through the
  assistant). The tests assert the role and name only, not an activatable button. Céline to
  decide whether a keyboard gesture (e.g. F2 on a focused lane) belongs to this contract.
- **P4 — the label buttons and open windows.** Today the canvas is announced only when no
  window is open; R4 does not say whether the label buttons are announced with a window
  open. Not asserted.
