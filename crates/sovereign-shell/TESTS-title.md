# Title editing — the test suite (contract T1–T10)

Written 2026-10-17, before the code, against `doc/plans/TITLE-edit.md` (decided by
Céline the same day) and spec ch. 12, document window, *Title*. Whoever builds title
editing makes these pass **without editing the two test files**; a test that looks wrong
is reported, not worked around.

```
cargo test -p sovereign-editor --test t_title      # the editor's half (40 tests)
cargo test -p sovereign-shell title_tests          # the shell's half (23 tests)
```

Protected files: `crates/sovereign-editor/tests/t_title.rs`,
`crates/sovereign-shell/src/title_tests.rs`. Manual checks (drawing, real keyboard focus,
Narrator, a real IME): `EDITOR_V2_CHECKLIST.md`, section 14.

## Where the field lives (the seams the tests use)

The title field is part of the **document editor** (`sovereign-editor`), beside the body:
that is the only place where T2 ("one save for title and body"), T5 ("unsaved is unsaved")
and T6 ("the same editing rules") hold without duplicating the editor. The tests use:

| Item | Stub (replace it) | What it is |
|---|---|---|
| `sovereign_editor::EditorPart { Title, Body }` | — (an enum) | which part holds the caret |
| `Editor::title() -> String` | `todo!` | the title in memory, as typed |
| `Editor::part() -> EditorPart` | `todo!` | title or body |
| `Editor::focus_title()` | `todo!` | caret in the title, all of it selected |
| `Editor::title_selection() -> (anchor, focus)` | `todo!` | byte offsets into `title()` |
| `Editor::set_title_selection(anchor, focus)` | `todo!` | caret into the title, snapped to clusters |
| `SaveRequest::title: Option<String>` | always `None` today | the trimmed title to save; `None` = unchanged |
| `Editor::set_selection` | existing | now also puts the caret in the body (doc line added) |
| `Editor::caret_rect` | existing | the caret of whichever part holds it |
| shell `save_request(docs, key, st, &SaveRequest, explicit, pasted)` | `todo!` | one request → one `Edit` (title + content) |
| shell `EditorPane::start_on_title()` | `todo!` | a new document's window starts on its title |
| shell `Card::set_title(title, shaper)` | `todo!` | the card shows the saved title |
| `Editor::title_font() -> TitleFont { size, weight, body_size }` | `todo!` (added for T1 revised) | what the title is drawn with |
| shell `DocWindow::set_title(title, shaper)` | `todo!` (added for T8) | the window bar follows a saved title |
| shell `canvas::display_title(&str) -> &str` | `todo!` (added for T10) | the one display helper: empty → "Untitled" |

What `App` alone does (it needs a window and a GPU) is checked by reading the source, as
`step5_tests.rs` does: `new_document` calls `start_on_title`; the production part of
`app/editor_v2.rs` calls `.set_title(` (after a save).

## Tests per guarantee, and why each is red today

Red reasons: **todo** = a stub panics (`title()`, `focus_title()`, `start_on_title()`,
`save_request()`, `set_title()`); **assert** = the assertion fails on today's behaviour.

### T1 — the field

| Test | File | Red |
|---|---|---|
| `t1_the_title_is_a_single_line_editable_field` — one node labelled "Title", role `TextInput`, editable, value = title | editor | todo |
| `t1_the_title_is_above_the_body` — title caret above the body's first line | editor | todo |
| `t1_the_title_is_editable_at_once` — no Edit step (v2 has no view mode) | editor | todo |
| `t1_a_click_on_the_title_puts_the_caret_there` — and a click in the body puts it back | editor | todo |
| `t1_an_empty_title_shows_untitled_as_a_placeholder_not_as_text` — value "", placeholder "Untitled", never saved | editor | todo |

### T1 (revised after Céline's manual run, 2026-10-17, main 6cd4787) — in the page

Written on `shell/tests-title-page` against the implementation merged at 2c1072d (fixed band,
1.4 × bold): the reds below are that implementation's, not stubs, except `title_font`.

| Test | File | Red |
|---|---|---|
| `t1r_the_title_scrolls_with_the_body` — scrolled by N, the title's caret moves by N; far enough, it leaves the view | editor | assert (the band stays put) |
| `t1r_clicks_follow_the_scrolled_title` — a click on the scrolled title reaches it; where it was before scrolling is body | editor | assert (the band still takes the click) |
| `t1r_the_ime_area_follows_the_scrolled_title` | editor | assert |
| `t1r_the_title_is_1_1_times_the_body_size_semi_bold` — `title_font()`: weight 600, size / body = 1.1 | editor | todo |
| `t1r_the_title_line_is_about_1_1_times_a_body_line` — measured: title caret height / body caret height in 1.04–1.17 | editor | assert (1.304 today) |

The thin rule under the title is drawing: `EDITOR_V2_CHECKLIST.md` §14, item 3.

### T8 — the window's bar (new, 14 px [choice])

| Test | File | Red |
|---|---|---|
| `t8_the_window_bar_title_is_14_px_on_one_line_when_it_opens` — `DocWindow::new`: same width and height as the title shaped at 14 px, one line (also a ~5,000 px title) | shell | assert (21 px today) |
| `t8_the_window_bar_title_follows_a_saved_title` — `DocWindow::set_title` | shell | todo |
| `t8_the_save_path_sets_the_bar_title_through_the_window` — source read: no `d.title = …shape(…)` in `app/editor_v2.rs` / `app.rs`; `.set_title(` for card and window | shell | assert (`editor_v2.rs` shapes it by hand at 21) |

The ellipsis at the bar's edge is drawing: §14, item 5.

### T9 — a long title scrolls sideways (second manual run, main 595ce3d)

Written on `shell/tests-title-t9` against main 65f8d4d; the reds are that implementation's
(the field does not scroll sideways). Editor 300 px wide, a title of about 700 px.

| Test | File | Red |
|---|---|---|
| `t9_home_end_and_arrows_keep_the_caret_visible` — every Left/Right step, Home, End: caret inside the field, on the title's one line | editor | assert (End: caret at x≈698) |
| `t9_typing_past_the_edge_keeps_the_caret_visible` | editor | assert (37th character: x≈301) |
| `t9_a_selection_keeps_its_moving_end_visible` — Shift+End, Shift+Home, Shift+Right | editor | assert |
| `t9_a_click_near_the_edge_lands_on_the_text_shown_there` — scrolled to the end, a click at the right edge lands within 3 bytes of the end; at the left edge, past a third of the title | editor | assert (offset 36 of 86) |
| `t9_the_ime_area_is_at_the_visible_caret` | editor | assert (x≈717) |

### T10 — an empty title reads *Untitled* everywhere (second manual run)

| Test | File | Red |
|---|---|---|
| `t10_the_display_helper_reads_an_empty_title_as_untitled` | shell | todo |
| `t10_the_card_shows_untitled_and_keeps_the_empty_title` — `Card::set_title("")`: title stays "", layout shows "Untitled" | shell | assert |
| `t10_a_loaded_card_with_an_empty_title_shows_untitled` — `load_workspace` on a store written through the service | shell | assert (the card shows a lock and an id, the encrypted placeholder, and carries it as its title) |
| `t10_the_window_bar_shows_untitled` — on open and after `set_title("")` | shell | assert |
| `t10_every_surface_reads_titles_through_the_helper` — source read of `app.rs`, `app/editor_v2.rs`, `trash_ui.rs`: a card's or trash line's title read for display goes through `display_title(` **on the same line**; allowed raw: a search match (`.to_lowercase()`), the title handed to the editor (`open_doc_window`, `v2_reopen`) | shell | assert (10 sites: taskbar, restore chooser, unsaved prompt, History ×3, deck fan, search results, `run_shot`, trash rows) |
| `t10_the_surface_scan_sees_what_it_should` — the scan on a sample | shell | passes (self-test) |
| `t10_the_phone_shows_untitled_too` — `frontend/src/lib/components/mobile/*.svelte`: no other spelling than "Untitled" | shell | assert (`MobileCanvas.svelte:565` shows "(untitled)") |

Not covered by T10 here: `AlertRow::notice` says "a document" for an empty title (a sentence,
not a title display); the editor's own body label. Searching "untitled" does not find empty
titles (the match is on the stored title, and an empty query lists nothing — so an empty
title never reaches the search results; the scan still requires the helper there; not
specified, flagged).

### T2 — saved like the body

| Test | File | Red |
|---|---|---|
| `t2_a_title_change_is_saved_by_the_same_delayed_save_as_the_body` — same delay as a body change, measured on the fake clock | editor | todo |
| `t2_title_and_body_changes_made_together_land_in_one_save` — one request with both; nothing more after | editor | todo |
| `t2_ctrl_s_saves_the_title_at_once` — Ctrl+S and `Command::Save` | editor | todo |
| `t2_an_unchanged_title_is_not_rewritten` — `None` or the same title | editor | **passes on the stub, by design** (an invariant `None` satisfies) |
| `t2_a_failed_title_save_keeps_the_title_and_says_so` | editor | todo |
| `t2_a_title_change_reaches_the_store_through_the_service` | shell | todo |
| `t2_title_and_body_together_are_one_service_save` | shell | todo |
| `t2_a_conflicting_title_save_overwrites_nothing` — `Conflict`, nothing overwritten, rebased save lands, other title in History (G11) | shell | todo |
| `t2_a_title_only_save_belongs_to_the_editing_session` — `end_session` versions it | shell | todo |
| `t2_the_card_takes_the_saved_title` — title and re-shaped layout | shell | todo |
| `t2_the_save_path_updates_the_cards_title` — source read | shell | assert |

### T3 — a new document starts on its title

| Test | File | Red |
|---|---|---|
| `t3_focus_title_selects_the_whole_title` — typing replaces it (empty and non-empty) | editor | todo |
| `t3_enter_or_tab_moves_the_caret_to_the_start_of_the_body` — `Handled`, nothing typed | editor | todo |
| `t3_shift_tab_from_anywhere_in_the_body_comes_back_to_the_title` — start, middle, end, a selection, a later block | editor | todo (`part()`; past it, Shift+Tab is `NotHandled` today) |
| `t3_tab_and_shift_tab_alternate_between_title_and_body` (also T6's "reachable by Tab") | editor | todo |
| `t3_a_new_documents_pane_starts_on_its_title` | shell | todo |
| `t3_winit_enter_tab_and_shift_tab_move_between_title_and_body` — winit's `"\r"` / `"\t"` texts; Shift+Tab from the body's end | shell | todo |
| `t3_creating_a_document_starts_on_its_title` — source read of `new_document` | shell | assert |
| `t1_t3_a_new_document_is_created_with_the_stored_title_untitled` — source read of `new_document` | shell | **passes today, by design** (decided 2026-10-17: keep today's "Untitled"; guards it) |

### T4 — one line

| Test | File | Red |
|---|---|---|
| `t4_enter_never_puts_a_line_break_in_the_title` — Enter and Shift+Enter | editor | todo |
| `t4_a_pasted_newline_becomes_a_space` — LF → one space exactly; CR LF and lone CR → spaces only | editor | todo |
| `t4_no_line_break_enters_the_title_by_other_paths` — text events, IME commits | editor | todo |
| `t4_the_saved_title_is_trimmed` — and nothing stays "unsaved" because of the spaces | editor | todo |
| `t4_a_title_of_spaces_only_is_saved_empty` — saved `""`, reopened shows the placeholder | editor | todo |
| `t4_the_store_gets_the_trimmed_title` | shell | todo |

### T5 — unsaved is unsaved

| Test | File | Red |
|---|---|---|
| `t5_an_unsaved_title_is_an_unsaved_edit` | editor | todo |
| `t5_closing_with_an_unsaved_title_answers_as_with_an_unsaved_body` — same `request_close` answer before and after a failed save | editor | todo |
| `t5_an_unsaved_title_is_an_unsaved_edit_of_the_window` — `DocWindow::has_unsaved_edits`, the predicate the delete and the restore refuse on | shell | todo |
| `t5_the_unsaved_prompts_save_carries_the_title` | shell | todo |

### T6 — accessible, same editing rules

| Test | File | Red |
|---|---|---|
| `t6_the_title_field_is_announced_with_its_value_and_focus` — accessible focus, value, selection; Tab / Shift+Tab move it | editor | todo |
| `t6_typing_in_the_title_updates_the_accessible_value` — the live updates | editor | todo |
| `t6_caret_moves_by_grapheme_cluster_in_the_title` | editor | todo |
| `t6_selection_and_select_all_stay_in_the_title` | editor | todo |
| `t6_copy_cut_paste_in_the_title` — same privacy flags as a body copy | editor | todo |
| `t6_input_methods_compose_in_the_title` — pre-edit not committed, committed once, cancel, candidate area at the title | editor | todo |
| `t6_bidi_in_the_title` — visual walk, logical selection | editor | todo |

### T7 — read-only stays read-only

| Test | File | Red |
|---|---|---|
| `t7_a_read_only_document_shows_its_title_as_text` — opened read-only, newer format, editing moved: no text input carries the title; a non-body node shows it | editor | assert (today only the body's name carries it) |
| `t7_no_input_changes_a_read_only_title` | editor | todo |

## Readings chosen (the implementer may not change them; the maille may)

- **Default part.** An editor opened on a document starts in the **body** (the existing
  C-TEXT suite types into the body without moving there). Only `focus_title` /
  `start_on_title`, a click, or Shift+Tab put the caret in the title.
- **In-memory vs saved title.** The title in memory is what was typed; only the saved title
  is trimmed (trimming in memory at an autosave would move the caret under the user). After
  a successful trimmed save, nothing is unsaved.
- **`SaveRequest::title`.** `Some(trimmed)` when the title changed; `None` or the same
  title when it did not (both accepted).
- **A pasted CR LF** may become one or two spaces; a pasted LF becomes exactly one.
- **T7, "as text".** Strict: no text input — read-only or not — carries the title. The body
  may keep the title as its accessible name (it does today). `TooLarge` (over 1 MB) is not
  covered: it is neither "another device" nor "cannot be read".
- **"In view mode and in edit mode" (T1)**: the editor v2 window has no view mode; tested as
  "editable at once, no Edit step".
- **T6 "reachable by Tab"** (decided 2026-10-17, 99c662b): the field order title → body.
  Tab from the title goes to the body's start, Shift+Tab from anywhere in the body to the
  title; tested by T3's tests and the accessible focus following. Tab in the body is not
  the editor's (left to the host) and not tested here.
- Not tested: personal-data tokens in the title (the service's ingest may tokenize the
  title; T6 does not list tokens), undo in the title, what the card shows for an empty
  title.

## Settled (coordinator, 2026-10-17; `doc/plans/TITLE-edit.md` at 99c662b)

1. **The stored title of a new document.** Kept as today: `"Untitled"`, which T3 selects
   so typing replaces it; the *Untitled* placeholder is only for a title cleared to empty.
   `t1_t3_a_new_document_is_created_with_the_stored_title_untitled` is no longer ignored.
2. **Keyboard.** Shift+Tab from **anywhere** in the body (not only its start) goes to the
   title — Tauri's field order; the body does not use Tab or Shift+Tab. That order is what
   T6's "reachable by Tab" means.
