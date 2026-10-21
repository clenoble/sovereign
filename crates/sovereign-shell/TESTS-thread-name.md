# Naming a new thread — the test suite (contract N1–N7)

Written 2026-10-17, before the code, against `doc/plans/THREAD-name.md` and spec ch. 12,
*New thread — named first, as in the Tauri desktop*. Reference behaviour: the Tauri
desktop's `new-thread-popup` (`frontend/src/lib/components/Canvas.svelte`, 677-688 and
804-814). Whoever builds it makes these pass **without editing the test file**; a test that
looks wrong is reported, not worked around.

```
cargo test -p sovereign-shell thread_name_tests     # 35 tests
```

Protected file: `crates/sovereign-shell/src/thread_name_tests.rs`. Manual checks (drawing,
real focus, a real IME, Narrator): `THREAD_CHECKLIST.md`.

## The seams

| Item | Stub | What it is |
|---|---|---|
| `new_thread::NewThreadPrompt::open(fonts, clipboard)` | `todo!` | the prompt, field empty and focused |
| `NewThreadPrompt::field()` / `field_mut()` | `todo!` | the editor's single-line field (`Editor::open_field(FieldKind::SingleLine, …)`) |
| `NewThreadPrompt::name()` / `can_create()` | `todo!` | the name as typed; Create active |
| `NewThreadPrompt::key(&WKey, text, mods)` | `todo!` | a winit key press (Enter creates, Esc cancels, the rest to the field, its text once) |
| `NewThreadPrompt::event(InputEvent)` / `command(Command)` | `todo!` | text, IME, pointer, focus; copy / paste / select all |
| `NewThreadPrompt::press_create()` | `todo!` | the button's action |
| `NewThreadPrompt::layout(w, h) -> PromptLayout { bounds, field, create }` | `todo!` | geometry shared by drawing and hit-testing; gives the field its width |
| `NewThreadPrompt::click(p, w, h)` | `todo!` | a click in shell coordinates |
| `NewThreadPrompt::close()` | `todo!` | dismissed by another path |
| `NewThreadPrompt::accessibility_tree()` | `todo!` | the prompt's own whole tree (inlined by the shell): a dialog "New thread" |
| `PromptStep { Open, Create(String), Cancel }` | — | the prompt's answer |
| `PLACEHOLDER`, `DIALOG_LABEL`, `FIELD_LABEL`, `CREATE_LABEL` | constants | "Thread name…", "New thread", "Thread name", "Create" |
| `new_thread::create_named_thread(&dyn GraphDB, name)` | `todo!` | trims, one line, `create_thread` with an empty description; `Ok(None)` for an empty name |

Read in `app.rs` (production part, comments stripped), because `App` needs a window and a
GPU: the `CtxAction::NewThread =>` arm calls `open_new_thread_prompt` and nothing that
creates; `fn open_new_thread_prompt` calls `NewThreadPrompt::open`; no `format!("New lane…`
remains; `PromptStep::Create(` is matched; `create_named_thread(` is called at exactly one
place, in a method that also calls `load_workspace_now`; `fn build_a11y_tree` mentions
`new_thread_prompt` (the `App` field holding the open prompt). In `new_thread.rs`: no
`list_threads` / `find_thread_by_name`.

## Tests per guarantee, and why each is red today

Red reasons: **todo** = a stub panics; **assert** = the assertion fails on today's code.

| Guarantee | Test | Red |
|---|---|---|
| N1 | `n1_the_prompt_opens_empty_with_create_inactive` | todo |
| N1 | `n1_the_field_takes_typing_at_once` (no click, no layout) | todo |
| N1, N5 | `n1_the_field_is_focused_and_shows_the_placeholder` — focus = TextInput, label, placeholder | todo |
| N1 | `n1_the_prompt_has_a_field_and_a_create_button` — layout: inside the window and the prompt, disjoint, field < half the window wide | todo |
| N1 | `n1_new_lane_opens_the_prompt_and_creates_nothing` — source | assert (the arm calls `new_thread()`) |
| N1 | `n1_the_shell_no_longer_invents_a_lane_name` — source | assert (`format!("New lane {n}")`) |
| N2 | `n2_enter_creates_with_the_trimmed_name` | todo |
| N2 | `n2_the_create_button_creates_with_the_trimmed_name` — action and click | todo |
| N2 | `n2_a_pasted_newline_becomes_a_space` — LF → one space; CR LF → one or two | todo |
| N2 | `n2_an_empty_or_spaces_only_name_creates_nothing` — Enter, button, click; Enter types nothing; prompt stays usable | todo |
| N2, N5 | `n2_create_is_announced_inactive_while_the_name_is_empty` | todo |
| N2 | `n2_the_store_gets_the_named_thread` — trimmed, one line, empty description, id returned | todo |
| N2 | `n2_an_empty_name_creates_no_thread_in_the_store` | todo |
| N2 | `n2_the_new_lane_shows_the_name` — `canvas::load_workspace` lane names | todo |
| N2 | `n2_the_shell_creates_on_create_then_reloads` — source | assert |
| N3 | `n3_esc_cancels` (also on an empty name) | todo |
| N3 | `n3_a_click_outside_cancels` — four window corners; inside the prompt off its parts: open | todo |
| N3 | `n3_closing_cancels` | todo |
| N7 | `n7_esc_while_composing_ends_the_composition_first` | todo |
| N4 | `n4_the_field_is_the_editors_single_line_field` — the field's own tree is a TextInput | todo |
| N4 | `n4_text_is_sent_once` — letters once, Space, no control characters | todo |
| N4 | `n4_caret_moves_by_grapheme_cluster` | todo |
| N4 | `n4_select_all_then_typing_replaces` — Ctrl+A and `Command::SelectAll` | todo |
| N4 | `n4_copy_cut_paste_go_through_the_clipboard` — privacy flags of an editor copy | todo |
| N4 | `n4_input_methods_compose_in_the_field` | todo |
| N7 | `n7_enter_while_composing_does_not_create` | todo |
| N4 | `n4_bidi_in_the_field` | todo |
| N4 | `n4_a_click_in_the_field_places_the_caret` — shell → field coordinates | todo |
| N4 (T9) | `n4_a_long_name_scrolls_inside_the_field` | todo; **then red until title T9 is built** |
| N5 | `n5_the_field_is_a_text_input_labelled_thread_name` — one TextInput, focused, value = name | todo |
| N5 | `n5_the_prompt_is_a_dialog_named_new_thread` — one Dialog "New thread" holding the text input, the button and the focus | todo |
| N5 | `n5_create_is_the_only_button` — one Button, "Create", Click action; no Cancel | todo |
| N5 | `n5_the_shell_announces_the_prompt` — source | assert |
| N6 | `n6_a_name_already_used_is_accepted` — two threads, same name | todo |
| N6 | `n6_the_prompt_takes_a_used_name_as_typed` | todo |

## Readings chosen (the implementer may not change them; the maille may)

- **The `+` button** keeps opening the canvas menu (spec ch. 12, *Context menu*: the `+`
  opens the menu, which also holds *New document*); its *New lane* row opens the prompt. In
  Tauri the toolbar `+` toggled the field directly. Tested: the menu row only. *Kept in the
  plan (main cb29150).*
- **Placeholder**: `Thread name…` with the ellipsis character, as the plan writes it (Tauri:
  three dots, `Thread name...`).
- **N5** (reworded in the plan, main cb29150): the prompt is an AccessKit `Dialog` named
  "New thread", holding the text input "Thread name" and the *Create* button (announced
  `disabled` while the name is empty or spaces only); Esc closes it; no Cancel button. The
  first wording ("button order") was read as one button after the field; that order is no
  longer asserted.
- **N7** (added to the plan from this reading, main cb29150): Enter or Esc during an
  input-method composition only end it (editor U35). Today the editor's single-line field
  returns `NotHandled` for Enter even with a pre-edit showing (`editor.rs`,
  `NamedKey::Enter`), so the prompt must check it. The commit arrives as its own event: the
  test sends it after the Enter.
- **"Closing the field"** (N3) is `close()`: any dismissal other than Esc or a click outside
  (the shell opening something over it, a lock). Which shell events close it is the
  implementer's; manual check §4.
- **"Small"** (N1): the field is narrower than half the window.
- **The creation path**: `create_named_thread` trims and turns line breaks into spaces again
  (the prompt already does), so a caller other than the prompt gets the same rule.
- Not tested: personal-data tokens pasted into a name (the thread name is stored as typed;
  the prompt's field resolves no tokens), undo across opening and closing, what the canvas
  draws while the prompt is open, where the prompt sits on screen.
