# sovereign-editor — the C-TEXT test suite

Written 2026-10-10, before the editor, against spec ch. 12 *The document editor —
contract (C-TEXT)* and ch. 03 *Document format v2 — blocks*. The editor is built
to make this suite pass; its author does not modify `tests/`.

```
cargo test -p sovereign-editor                                   # the suite
cargo test -p sovereign-editor --release -- --ignored g9_timing  # timing (g9)
```

**Amended the same day, twice, after Céline's decisions** — see *Decided by
Céline, 2026-10-10* below (decisions 1–10); each decided item is also marked
where it appears.

**State at writing:** 153 tests, 149 run in a normal pass, 4 `#[ignore]`d (timing).
141 fail on `todo!("editor not built yet")` — expected. **8 pass against the
stubs, by design.** One checks a decided constant (`g9_the_cap_is_one_million_bytes`:
`MAX_EDITABLE_TEXT_BYTES == 1_000_000`). The seven others are static checks of
what the editor may become (its dependency tree and its own source), and they
stay meaningful once it exists:
`g4_no_networking_crate_is_reachable_from_the_editor`,
`g4_dependency_walk_finds_networking_crates_where_they_are` (shows the walk is not
vacuous: from `sovereign-shell` it finds tokio, wry, sovereign-p2p),
`g4_source_scan_ignores_comments_only` (self-test of the scanner),
`g4_editor_source_uses_no_network_or_process_api`,
`g3_no_disk_or_crash_reporting_crate_is_reachable_from_the_editor`,
`g3_memmap2_is_allowed_only_through_fontique` (self-test of decision 9: parley
reaches memmap2 only through fontique and passes; the shell also reaches it
through winit / smithay-client-toolkit and is flagged),
`g3_editor_source_uses_no_file_or_panic_hook_api`.

## How the suite is built

- **Invariants only.** Each test names its guarantee (`g1_…` … `g10_…`, `b_…` for
  the decided behaviours) and quotes the spec line it checks. Where the spec
  leaves a choice open, the test accepts every reading that satisfies the
  sentence, or the choice is listed under *Uncertain* below.
- **The host is faked** (`tests/common/mod.rs`): the fakes record saves,
  clipboard writes (with their privacy flags), AccessKit updates, IME caret
  areas; a fake clock drives autosave; a fake resolver gives personal-data
  labels and revealed values. Saves complete only when a test says so
  (`Editor::save_finished`), so failed saves, retries, out-of-order completion
  and remote changes mid-save are reachable.
- **"The model"** is observed as `Editor::document()` and, for "exactly", as the
  bytes of `DocumentV2::to_json_string()` — which also catches lost ids,
  reordered blocks and reformatted unknown blocks.
- **Accessibility** is read with `accesskit_consumer`, the layer AccessKit's
  platform adapters are built on: "exposed" means what UIA / AT-SPI would get.
- **Geometry** uses a bundled font only (`tests/fixtures/Cantarell-Regular.ttf`,
  SIL OFL 1.1, see `tests/fixtures/README.md`). It is Latin-only *on purpose*:
  emoji, flags, Hebrew and Arabic lay out with `.notdef`, and grapheme movement
  must still hold — the guarantee does not depend on font coverage. Visual
  claims are orderings (x increases / decreases), true for any positive advance.
- **The v2 model is PROVISIONAL** (`src/doc.rs`): no v2 model exists in
  `sovereign-core` yet. It is to move to the C-DOC contract keeping its
  signatures; `tests/g2_model_v2.rs` moves with it.

## Decided by Céline, 2026-10-10

Decisions 1–8 answer findings of the first suite; 9 and 10 came later the same day (10 replaces part of 4).

| # | Finding | Decision | In the suite |
|---|---|---|---|
| 1 | W1 / U3 — bidi arrows | **Visual.** Left/Right move to the next cluster in visual order (parley's `move_left` / `move_right`). A selection is still a logical range. | The two "logical" tests are removed. New: `g5_bidi_right_arrow_in_an_rtl_run_moves_to_logically_earlier_text`, `g5_bidi_visual_walk_visits_every_visual_place_in_order` (N clusters → all N+1 visual places, x monotonic, none skipped at run boundaries, Right and Left, LTR and RTL paragraphs), `g5_bidi_selection_is_a_logical_range` (what is copied is the logical substring). "Caret on a cluster boundary" kept everywhere. |
| 2 | W2 / U1 — numbers | **Accepted as proposed:** p95 keystroke→paint ≤ 16 ms on 1 MB; open + first paint ≤ 1 s; above the cap opens read-only. **1 MB = 1,000,000 bytes of UTF-8 text in `text` blocks.** | `MAX_EDITABLE_TEXT_BYTES = 1_000_000` in the API. New: `g9_the_cap_is_one_million_bytes`, `g9_exactly_at_the_cap_is_editable`, `g9_the_cap_counts_utf8_bytes` (500,000 two-byte characters are at the cap, one more is over). "Just over" is now cap + 1 byte; "just under" stays 999,000. |
| 3 | W13 / U24 — typing on the device that lost editing | The editor turns read-only **and** input received while read-only produces a visible notice ("this document is being edited on <device>"); the same for every read-only state — input never disappears without a notice. | New: `b_input_after_the_switch_gives_an_editing_elsewhere_notice` (each kind of editing input → an `EditingElsewhere` notice naming the device, model unchanged), `g1_input_in_a_read_only_state_is_never_dropped_silently` (opened read-only, newer format, over the cap → a `ReadOnly` notice for each kind of input, model unchanged, nothing saved). |
| 4 | W6 — token forgery | ~~Typed or pasted token syntax is never a token (stored escaped).~~ **Superseded by decision 10.** Still standing: **copy-paste inside Sovereign keeps the token** (atomic, same id); outside Sovereign the clipboard text carries the label, or the value if revealed. | API: `ClipboardContent { text, sovereign }` — an opaque Sovereign payload beside the text. Tests: see decision 10. |
| 5 | W8 / U16 — clipboard clearing | No automatic clearing for ordinary text. **A copy whose content includes a revealed personal-data value is cleared after 30 s.** | API: `ClipboardWrite.clear_after`. New: `g7_copy_including_a_revealed_value_asks_for_clearing_after_30s` (whole text, token alone, cut), `g7_copy_without_a_revealed_value_sets_no_expiry` (unrevealed label; plain text next to a revealed token). `b_no_automatic_clipboard_clearing` and `b_every_clipboard_write_is_marked_excluded` now also check `clear_after == None`. |
| 6 | W4 — panic messages | The shell installs a panic hook that drops the payload. | Not this crate: **covered elsewhere — shell panic hook (decided 2026-10-10).** |
| 7 | W7 / U15 — token id form | The spec follows the code: `[pii:<record_id>]`, ids contain `:`. | Every test uses that form only (`pii_record:<key>`, helpers `token()` / `rid()`). |
| 8 | W9 / U29 — v1 media migration | Images get `alt` empty and a "needs a description" flag; `ref.sha256` is absent until the document service (host) computes it; the editor never reads files. Such a block is valid. | Model API: `Block::{media_path, media_sha256, image_alt, needs_description}`. New: `g2_model_v1_media_migrate_with_description_and_hash_pending` (also survives write + re-read), `g2_model_described_image_does_not_need_a_description`, `g2_v1_document_saves_with_media_pending`. The flag's JSON name is the implementation's choice. |
| 9 | g3 dependency check — `memmap2` | **`memmap2` is allowed when reached through the font library**: parley → fontique maps font files read-only; unavoidable on parley (and `cargo metadata` unifies features workspace-wide). Allowed only when **every** path to it goes through `fontique`; any other path still fails. | `g3_no_disk_or_crash_reporting_crate_is_reachable_from_the_editor` cuts `fontique` out of the graph and fails if `memmap2` is still reachable. Self-test `g3_memmap2_is_allowed_only_through_fontique`: from parley, memmap2 is reachable but passes; from the shell it is also reached through winit / smithay-client-toolkit and is flagged. |
| 10 | W6 — the resolver rule (replaces the escaping part of 4) | **A token is shown as a token only in a document listed among its personal-data record's sources; anywhere else it is ordinary text.** The host applies it; the editor escapes nothing. Typing the exact form of a token linked to this document may become a token — accepted, not tested either way. Internal copy-paste keeps the token, and the editor tells the host to link the record to the target document. | API: `TokenDisplay::NotAToken`; `PiiDisplay::link_to_document(&[record_id])`, called at the paste of a Sovereign payload so the token is atomic at once. The fake resolver answers per document (`not_linked_here`; unknown ids are ordinary text). Removed: the three "typed / pasted syntax is never a token" tests and the "PiiDisplay never consulted" assertions. New: `g7_token_form_not_linked_here_is_ordinary_text` (shown as written, caret rests inside, Backspace removes one character, copied as its literal), `g7_same_token_form_is_atomic_where_linked`, `g7_typed_token_form_is_stored_byte_for_byte`, `g7_token_form_pasted_from_outside_links_nothing`, `g7_copy_paste_inside_sovereign_keeps_the_token_and_links_it` (A → B, unrevealed and revealed: same token, `link_to_document` called with its id, atomic in B immediately, saved as the token), `g7_same_text_without_the_sovereign_payload_announces_nothing`. `g7_unknown_record_token_is_atomic_too` became `g7_placeholder_token_is_atomic_too` (a record the host shows with a placeholder label). The 30 s tests and W17 are unchanged. |

## Guarantees → tests

| Guarantee (C-TEXT) | Tests |
|---|---|
| **1 Nothing typed is lost** (17) | `g1_input_in_a_read_only_state_is_never_dropped_silently`, `g1_every_typed_character_is_in_the_model`, `g1_multi_character_text_event_is_inserted_whole`, `g1_ime_commit_is_in_the_model`, `g1_enter_keeps_both_halves`, `g1_ctrl_s_saves_exactly_the_model`, `g1_typing_during_an_inflight_save_is_kept_and_saved_next`, `g1_failed_save_keeps_the_model_and_says_so`, `g1_failed_save_then_retry_saves_the_kept_model`, `g1_out_of_order_completion_does_not_hide_a_failure`, `g1_close_after_failed_save_asks_the_user`, `g1_close_with_unsaved_typing_saves_it_first`, `g1_remote_change_mid_edit_keeps_local_typing_in_the_model`, `g1_remote_change_during_failed_save_keeps_local_typing`, `g1_focus_loss_keeps_typing`, `g1_paste_inserts_the_whole_clipboard_text`, `g1_crlf_content_is_kept_byte_for_byte` |
| **2 Unknown blocks pass through; unchanged blocks keep ids and bytes** (27) | editor: `g2_v1_document_saves_with_media_pending`, `g2_editing_one_block_keeps_unknown_neighbours_bytes_and_order`, `g2_saving_an_unchanged_document_writes_the_same_bytes`, `g2_edited_block_keeps_its_id`, `g2_unknown_fields_of_an_edited_text_block_are_preserved`, `g2_unknown_top_level_fields_are_preserved`, `g2_unchanged_media_and_table_blocks_keep_their_bytes`, `g2_select_all_and_delete_keeps_unknown_blocks`, `g2_cut_all_keeps_unknown_blocks`, `g2_backspace_and_delete_at_block_boundaries_keep_unknown_blocks`, `g2_typing_on_an_unknown_block_does_not_alter_it`, `g2_explicit_remove_removes_exactly_that_block`, `g2_document_of_only_unknown_blocks`, `g2_empty_document_round_trips_and_accepts_typing`, `g2_newer_format_version_is_read_only_and_never_rewritten`, `g2_read_only_open_never_saves`, `g2_new_block_ids_are_128_bit_hex_and_unique`; model (`g2_model_v2.rs`): `g2_model_unchanged_v2_round_trips_byte_for_byte`, `g2_model_unchanged_pretty_printed_v2_round_trips_byte_for_byte`, `g2_model_unknown_blocks_are_read_as_unknown_and_kept`, `g2_model_raw_text_reads_as_one_text_block_holding_the_whole_string`, `g2_model_v1_envelope_migrates_in_order`, `g2_model_v1_media_migrate_with_description_and_hash_pending`, `g2_model_described_image_does_not_need_a_description`, `g2_model_newer_version_is_reported_and_kept_verbatim`, `g2_model_text_bytes_counts_text_blocks_only`, `g2_model_block_id_hex_round_trip` |
| **3 No plaintext leaves memory except through save** (7) | `g3_a_full_session_writes_no_file` (TMP/TEMP/TMPDIR/HOME/USERPROFILE/APPDATA/LOCALAPPDATA/XDG_* repointed to empty dirs, plus the working directory; one test in its own binary because env vars are process-wide), `g3_a_full_session_logs_no_document_text` (global `tracing` subscriber and `log` logger at TRACE), `g3_notices_and_debug_output_contain_no_document_text`, `g3_revealed_pii_value_is_not_logged`, `g3_no_disk_or_crash_reporting_crate_is_reachable_from_the_editor` (memmap2 allowed only through fontique — decision 9), `g3_memmap2_is_allowed_only_through_fontique`, `g3_editor_source_uses_no_file_or_panic_hook_api`. The session (`tests/common/session.rs`) runs every path: typing, IME, focus loss, pointer, copy/paste/cut, undo/redo, block removal, failed and successful saves, autosave, remote change, device switch, a document over the cap, a newer `v`, a password field, unreadable content. |
| **4 No network** (4) | `g4_no_networking_crate_is_reachable_from_the_editor`, `g4_dependency_walk_finds_networking_crates_where_they_are`, `g4_source_scan_ignores_comments_only`, `g4_editor_source_uses_no_network_or_process_api`. The walk runs `cargo metadata --locked --offline` and follows normal + build dependencies on every platform (dev-dependencies excluded). Forbidden: mio, socket2, tokio, tokio-\*, async-std, async-io, async-net, smol, polling, if-addrs, if-watch, netlink-\*, local-ip-address, get_if_addrs, reqwest, reqwest-\*, hyper, hyper-\*, h2, h3, h3-\*, http-body, ureq, isahc, curl, curl-sys, surf, attohttpc, minreq, websocket, tungstenite, tokio-tungstenite, async-tungstenite, ws, quinn, quinn-\*, quiche, rustls, rustls-\*, tokio-rustls, native-tls, openssl, openssl-sys, webpki, webpki-\*, trust-dns-\*, hickory-\*, mdns, mdns-sd, libmdns, igd, igd-next, libp2p, libp2p-\*, lettre, imap, async-imap, async-native-tls, presage, libsignal-\*, wry, webview2-com, webview2-com-sys, webkit2gtk, webkit2gtk-sys, tauri, tauri-\*, sovereign-p2p, sovereign-comms, sovereign-relay, sovereign-guardian, sovereign-db, sovereign-ai, sovereign-app, surrealdb, surrealdb-\*. Source patterns: `std::net`, `TcpStream`, `TcpListener`, `UdpSocket`, `ToSocketAddrs`, `std::process`, `Command::new`. |
| **5 Caret and selection by grapheme cluster; word/line per parley; bidi (visual arrows, decided 2026-10-10) with a correct visual caret** (12) | `g5_right_arrow_moves_one_grapheme_cluster`, `g5_left_arrow_moves_one_grapheme_cluster`, `g5_shift_arrows_extend_selection_by_cluster`, `g5_set_selection_inside_a_cluster_lands_on_a_boundary`, `g5_backspace_and_delete_remove_at_most_one_cluster`, `g5_pointer_clicks_land_on_cluster_boundaries`, `g5_word_movement_lands_on_word_boundaries`, `g5_home_end_up_down_respect_lines`, `g5_bidi_right_arrow_in_an_rtl_run_moves_to_logically_earlier_text`, `g5_bidi_visual_walk_visits_every_visual_place_in_order`, `g5_bidi_selection_is_a_logical_range`, `g5_bidi_visual_caret_position`. Samples: ZWJ family, ZWJ + skin tone, flags, tag-sequence flag, combining marks, Hangul conjoining jamo, keycaps, variation selectors, CR LF, Devanagari, Thai; Hebrew with niqqud, Arabic, digits after Arabic. |
| **6 Input methods** (12) | `g6_commit_inserts_exactly_once_at_the_caret`, `g6_commit_while_preedit_is_showing_replaces_it`, `g6_consecutive_compositions`, `g6_cancel_leaves_the_model_exactly_as_before`, `g6_escape_or_disable_mid_composition_leaves_no_fragment`, `g6_preedit_is_not_in_the_model_and_not_saved`, `g6_preedit_is_shown_in_place`, `g6_caret_area_is_reported_and_follows_the_caret`, `g6_focus_loss_mid_composition`, `g6_commit_without_preedit`, `g6_undo_after_commit_restores_the_previous_model`, `g6_composition_in_read_only_document_changes_nothing` |
| **7 Personal-data tokens atomic** (24; resolver rule, in-Sovereign paste and 30 s clearing decided 2026-10-10) | `g7_shift_arrow_selects_the_whole_token`, `g7_arrows_step_over_a_token`, `g7_selection_endpoints_never_rest_inside_a_token`, `g7_backspace_and_delete_remove_the_whole_token`, `g7_no_operation_edits_part_of_a_token` (Backspace, Delete, Ctrl+Backspace, Ctrl+Delete, typing, Enter, cut, paste, IME commit × every selection), `g7_token_is_shown_as_its_label`, `g7_revealed_token_shows_its_value`, `g7_copy_of_unrevealed_token_copies_the_label`, `g7_copy_of_revealed_token_copies_the_value`, `g7_copy_of_a_selection_holding_only_the_token`, `g7_cut_of_unrevealed_token`, `g7_placeholder_token_is_atomic_too`, `g7_undo_restores_a_deleted_token_exactly`, `g7_save_writes_the_token_not_its_display`, `g7_token_form_not_linked_here_is_ordinary_text`, `g7_same_token_form_is_atomic_where_linked`, `g7_typed_token_form_is_stored_byte_for_byte`, `g7_token_form_pasted_from_outside_links_nothing`, `g7_pasted_label_from_outside_is_plain_text`, `g7_copy_paste_inside_sovereign_keeps_the_token_and_links_it`, `g7_same_text_without_the_sovereign_payload_announces_nothing`, `g7_copy_paste_in_the_same_editor_keeps_the_token`, `g7_copy_including_a_revealed_value_asks_for_clearing_after_30s`, `g7_copy_without_a_revealed_value_sets_no_expiry` |
| **8 Undo / redo** (10) | `g8_one_burst_is_one_undo_step`, `g8_bursts_separated_by_a_pause_undo_separately`, `g8_redo_restores_exactly`, `g8_undo_walks_back_through_every_state`, `g8_save_does_not_clear_undo_or_redo`, `g8_autosave_does_not_clear_undo`, `g8_save_after_undo_writes_the_undone_model`, `g8_undo_history_is_not_in_the_saved_document`, `g8_undo_redo_on_empty_history_change_nothing`, `g8_undo_after_remote_change_removes_only_the_last_burst`; "not written to disk" is also covered by `g3_a_full_session_writes_no_file` (the session undoes and redoes). |
| **9 Large documents** (10 + 4 timing; numbers decided 2026-10-10) | `g9_the_cap_is_one_million_bytes`, `g9_exactly_at_the_cap_is_editable`, `g9_the_cap_counts_utf8_bytes`, `g9_just_under_the_cap_is_editable`, `g9_just_under_the_cap_in_many_blocks_is_editable`, `g9_just_over_the_cap_opens_read_only`, `g9_over_the_cap_in_many_blocks_opens_read_only`, `g9_large_unknown_block_does_not_count_as_text`, `g9_typing_across_the_cap_is_not_silently_dropped`, `g9_paste_across_the_cap_is_whole_or_refused`; `#[ignore]`: `g9_timing_typing_in_a_1mb_document_stays_within_one_frame` (p95 of keystroke→painted over 200 keys ≤ 16 ms), `g9_timing_opening_a_1mb_document_is_short` (open + first paint ≤ 1 s), `g9_timing_opening_far_beyond_the_cap_does_not_freeze` (5 MB: open + paint ≤ 1 s, read-only), `g9_timing_document_end_and_start_on_1mb` (Ctrl+End, Ctrl+Home + paints ≤ 32 ms). Bounds proposed by this suite, accepted by Céline 2026-10-10. |
| **10 Accessible** (11) | `g10_focused_text_value_is_exposed`, `g10_caret_is_exposed`, `g10_selection_is_exposed`, `g10_incremental_updates_track_the_model`, `g10_focus_follows_the_editor_focus`, `g10_updates_use_the_host_tree_id`, `g10_read_only_is_exposed`, `g10_password_field_never_exposes_its_content`, `g10_commands_are_reachable_by_keyboard`, `g10_every_block_is_reachable_by_keyboard`, `g10_keyboard_can_leave_the_editor` (no keyboard trap, WCAG 2.1.2) |

## Decided behaviours → tests

| Behaviour | Tests |
|---|---|
| One device edits at a time (5) | `b_input_after_the_switch_gives_an_editing_elsewhere_notice`, `b_device_switch_saves_what_it_has_then_turns_read_only`, `b_device_switch_with_nothing_unsaved`, `b_device_switch_with_failed_save_keeps_the_text_and_says_so`, `b_opened_read_only_never_changes_or_saves` |
| Clipboard (3, + g7 copy tests) | `b_every_clipboard_write_is_marked_excluded`, `b_each_copy_writes_the_whole_selection_once`, `b_no_automatic_clipboard_clearing` |
| Saving: automatic, with versions (5, + g1, g8) | `b_autosave_after_a_pause`, `b_autosave_with_a_coarse_host_timer`, `b_ctrl_s_saves_at_once`, `b_successive_saves_carry_successive_models`, `b_save_status_transitions` |
| Lists continue on Enter (21; decided 2026-10-14, spec v2.0.17; `tests/b_lists_continue.rs`) | `b_lists_bullet_markers_continue_with_the_same_marker`, `b_lists_ordered_items_continue_with_the_next_number_and_delimiter`, `b_lists_ordered_numbers_carry_over_digits`, `b_lists_indentation_is_preserved`, `b_lists_task_items_continue_unchecked`, `b_lists_enter_inside_an_item_splits_it`, `b_lists_caret_lands_after_the_new_marker`, `b_lists_enter_in_the_middle_of_a_list`, `b_lists_enter_on_an_empty_item_ends_the_list`, `b_lists_enter_on_an_empty_item_removes_only_its_marker`, `b_lists_second_enter_ends_the_list`, `b_lists_shift_enter_adds_no_marker`, `b_lists_shift_enter_on_an_empty_item_keeps_its_marker`, `b_lists_non_list_lines_do_not_continue`, `b_lists_nothing_continues_inside_a_fenced_code_block`, `b_lists_continue_again_after_a_closed_fence`, `b_lists_one_undo_removes_the_enter_and_its_continuation`, `b_lists_saved_source_holds_exactly_the_continued_text`, `b_lists_enter_committing_a_composition_does_not_continue`, `b_lists_enter_after_a_finished_composition_continues`, `b_lists_a_token_at_the_end_of_an_item_is_not_split`. Uncertain: U32–U39. |
| Text model: markdown source (2) | `b_new_text_is_a_markdown_text_block`, `b_markdown_markers_stay_editable` |
| Title field (30; contract T1–T7 decided 2026-10-17, `doc/plans/TITLE-edit.md`; `tests/t_title.rs`, written before the code) | listed with their red reasons, and the shell's half, in `crates/sovereign-shell/TESTS-title.md` |

## Not tested here, and why

To be covered by shell integration tests or manual checks.

1. **What the user sees.** Light markdown formatting (headings, emphasis, lists,
   links), the provenance accent, the unknown-block placeholder and its wording,
   how a composition is marked, the read-only and save-failed indications as
   drawn. Rendering needs pixels; nothing here inspects the vello scene.
2. **Exact visual caret placement.** Only orderings are checked (g5, g6). Caret
   placement at a bidi run boundary (affinity), caret height, real shaping of
   Hebrew/Arabic/emoji (the test font has no such glyphs).
3. **Real input methods and the real candidate window.** The tests send
   winit-shaped IME events and check that a caret area is reported; that the OS
   puts the candidate window there (Microsoft IME, Korean, Pinyin, Linux
   IBus/Fcitx on X11 and Wayland, Android keyboards) is manual. Platform quirks
   such as an IME that also sends the committed text as a key event cannot be
   told apart from typing at this level.
4. **Real screen readers** (Narrator, NVDA, Orca) and the shell's grafting of the
   editor's subtree into its own tree. AccessKit action requests from assistive
   technology (e.g. SetTextSelection) are not in the contract and not in the API.
5. **Operating-system crash dumps and swap** — stated in the contract as outside
   the editor's control. Only "the editor writes no file" is testable.
6. **Panic messages.** A panic cannot be provoked through the API. The suite bans
   panic hooks and print macros in the editor's source and checks that `Debug`
   output redacts text; a panic *inside a dependency* whose message quotes text
   is **covered elsewhere: the shell's panic hook drops the payload (decided by
   Céline 2026-10-10, W4).**
16. **A forged Sovereign clipboard payload** written by another application (W17).
17. **That the host actually clears the clipboard** after `clear_after`, and only if
    it still holds that item — host (arboard) work; the editor only asks.
7. **stdout / stderr written by dependencies** — not capturable in-process. `log`
   and `tracing` are captured; the editor's own source is scanned for print
   macros.
8. **Network at runtime.** The check is static (dependency tree + this crate's
   source). A dependency that is not a "networking crate" but calls `std::net`
   would not be caught; a runtime socket check is not portable in-process.
9. **Clipboard flags actually applied** by the host on each platform (Windows
   history / cloud / monitoring exclusion formats, the KDE password-manager
   hint) — the editor only passes the flags; the host (arboard) applies them.
10. **The "editing on" signal over sync, the three-choice warning, and the offline
    fallback to peer review** (ch. 05) — host and sync, not the component.
11. **Encryption, commit, versions and personal-data ingest at save** — contract
    C-DOC. Here a save is a request with the exact content.
12. **Timing on real hardware.** The g9 timing tests are ignored in the normal run;
    they mean something only in `--release` on agreed reference hardware.
13. **Title editing, history, restore, pointer gestures beyond a click** (double /
    triple click, drag-select autoscroll), scrolling the caret into view — not in
    C-TEXT.
14. **The keyboard path that removes an unknown block** — the contract does not
    say what it is (see *Uncertain*, U11); only the command is tested.
15. **Mobile.** The editor is the shell's; the Android reader is a separate surface.

## Uncertain — interpretations chosen, for Céline

Each is a place where the contract allows more than one reading. The suite
encodes the reading given; changing it means changing the named tests.

- **U1 — "1 MB of text". Decided by Céline 2026-10-10:** 1,000,000 bytes of
  UTF-8 text in `text` blocks (unknown blocks, media references and JSON overhead
  do not count: `g9_large_unknown_block_does_not_count_as_text`). At the cap is
  editable, cap + 1 byte is read-only.
- **U2 — Growing past the cap while open** (typing, paste). Not specified. Read as:
  input goes in whole, or is refused with an `InputRefused` notice — never cut
  silently (`g9_typing_across_the_cap_…`, `g9_paste_across_the_cap_…`).
- **U3 — Bidi arrow movement. Decided by Céline 2026-10-10: visual.** Left/Right
  move to the next cluster in visual order (parley's `move_left/move_right`); in
  an RTL run Right goes to logically earlier text; at a run boundary the caret
  goes to the visually adjacent cluster; a selection is a logical range. The
  first suite's "logical" reading and its two tests are withdrawn.
- **U4 — "A correct visual caret".** Checked as orderings only: within an RTL run,
  logically later is visually left; within an LTR run, right
  (`g5_bidi_visual_caret_position`).
- **U5 — "Word and line movement follow parley".** Tested only as: Ctrl+arrows land
  on a UAX #29 word boundary, progress, and reach the ends; Home/End/Up/Down stay
  on / change line on explicit line breaks; Ctrl+Home/End reach the document ends.
- **U6 — Backspace granularity.** Not specified. Accepted: anything from one code
  point up to one grapheme cluster (parley deletes the previous code point inside
  a combining sequence), as long as the caret ends on a cluster boundary and
  nothing else changes (`g5_backspace_and_delete_remove_at_most_one_cluster`).
  Delete must remove the whole next cluster.
- **U7 — "Typing bursts".** Only the clear cases are tested: an uninterrupted burst
  is one undo step; a 60-second pause separates two. What else ends a burst (a
  caret move, a few seconds, a save) is not tested.
- **U8 — Undo of non-typing edits** (paste, cut, Enter, block removal, deletions):
  tested as reversible to the exact previous model when separated by pauses,
  not as to how they group.
- **U9 — Redo restores ids.** "Restore exactly the previous model" read to include
  the random id of a block created by Enter (`g8_undo_walks_back_through_every_state`).
- **U10 — A remote change while editing.** The contract only says the editor is
  notified. Read through guarantee 1 strictly: local unsaved typing stays in the
  in-memory model (`g1_remote_change_mid_edit_keeps_local_typing_in_the_model`).
  Whether the editor shows the remote version, merges, or asks is not tested.
- **U11 — "Removes one only by an explicit user action on that block".** Read as:
  `Command::RemoveBlock(id)` is that action; select-all + Delete/Backspace, cut
  of everything, and Backspace/Delete at the neighbouring block's edge are not
  (`g2_select_all_and_delete_keeps_unknown_blocks`, `g2_cut_all_keeps_unknown_blocks`,
  `g2_backspace_and_delete_at_block_boundaries_keep_unknown_blocks`). If selecting
  the block and pressing Delete counts as explicit, those tests change.
- **U12 — "Produces the same bytes".** Read strictly: an unchanged v2 document
  round-trips byte for byte even with formatting Sovereign did not write
  (indentation, `\u00e9`, `\/`, `1E-7`, `-0`, integers beyond 64 bits)
  (`g2_model_unchanged_pretty_printed_v2_…`, `g2_model_unchanged_v2_…`). If only
  Sovereign-written documents need to round-trip, the pretty-printed test can go.
  Duplicate keys inside an unknown block are not tested (their "JSON value" is
  ambiguous).
- **U13 — Line endings.** CR LF in existing content is kept byte for byte, no
  normalisation (`g1_crlf_content_is_kept_byte_for_byte`): "a raw-text body → one
  text block holding the whole string" plus guarantee 1.
- **U14 — Pre-edit is not part of the model.** "Cancelled cleanly" read as: the
  model never contains uncommitted composition, so no save (manual or automatic)
  writes it and cancel leaves the model byte-identical
  (`g6_preedit_is_not_in_the_model_and_not_saved`). Focus loss mid-composition may
  commit once or drop; both accepted (`g6_focus_loss_mid_composition`).
- **U15 — Token id form. Decided by Céline 2026-10-10:** the spec follows the
  code, `[pii:<record_id>]`, ids contain `:` (`pii_record:x7k2`); the tests use
  that form only. (Malformed token syntax is not tested: under the resolver
  rule it is whatever the host answers for it.)
- **U16 — Clipboard clearing. Decided by Céline 2026-10-10:** ordinary text is
  never cleared; a copy whose content includes a revealed personal-data value is
  cleared after 30 s (`ClipboardWrite.clear_after`).
- **U17 — Who makes token labels.** The contract says tokens show "their label";
  the API asks the host (`PiiDisplay`), which applies the reveal gate and the
  resolver rule (decision 10). In the fake, an id with no record is ordinary
  text (a record with no sources links no document) and an undecryptable record
  is a token with a placeholder label; how the real host treats a deleted record
  is the host's call and not tested.
- **U18 — The password field.** The guarantee sits in the document-editor contract
  but a password field is not a document. Read as: the same component in
  single-line password mode (`Editor::open_field(FieldKind::Password, …)`),
  exposed with role `PasswordInput`, no node exposing its content. Copy/cut out of
  a password field is not stated and not tested.
- **U19 — Shortcuts.** Only Ctrl+S is named. The tests use the Windows/Linux
  conventions Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z, Ctrl+C/X/V/A, Ctrl+Home/End,
  Ctrl+arrows, Shift+arrows/Home/End. Leaving the editor: at least one of
  Ctrl+Tab, Shift+Tab, Esc, F6, Tab must be left to the host — which one is open.
- **U20 — The caret on a non-text block.** A `Position` on an unknown/media block
  has offset 0, and the keyboard can reach every block
  (`g10_every_block_is_reachable_by_keyboard`) — so that the explicit removal is
  reachable without a pointer.
- **U21 — "After a short pause".** Tested as: a save of the current model happens
  within 60 s of the last keystroke, when the host polls at the deadlines the
  editor asks for, or once a minute. Not saving an unchanged document is not
  required.
- **U22 — Closing with unsaved changes and no failure.** "The unsaved prompt remains
  for the case where a save fails" read as: otherwise no prompt — the editor saves
  and the host closes after success (`CloseRequest::Saving`).
- **U23 — Device switch with a failing save.** The text is kept and the failure
  said; whether the editor turns read-only anyway is not tested.
- **U24 — Input in a read-only editor. Decided by Céline 2026-10-10:** it changes
  nothing and produces a visible notice — `EditingElsewhere` naming the device
  after a switch, `ReadOnly` otherwise. Navigation and copy need no notice; the
  suite checks text, Backspace, Delete, Enter, paste, cut and IME commit.
- **U25 — `Debug` is a log path.** "No log line containing document text" read as
  covering `{:?}` of the editor, the model and its blocks: they must redact.
- **U26 — "The focused text" in a multi-block document** — one accessible text for
  the document or one per block is open; the a11y tests use one text block (the
  read-only and incremental tests too).
- **U27 — Id case.** 32 hex digits, either case.
- **U28 — Empty stored content** (`""`) reads as `SourceForm::Empty`; which blocks it
  yields is not tested.
- **U29 — v1 migration of media. Decided by Céline 2026-10-10:** images get
  `alt: ""` and a "needs a description" flag, `ref.sha256` is absent until the
  document service computes it, and such blocks are valid. The flag's JSON name
  is left to the implementation (the tests read it through
  `Block::needs_description`). `decorative` is not checked.
- **U30 — The Sovereign clipboard payload** is opaque; the tests only require that
  a Sovereign editor pasting it recreates the same token, calls
  `link_to_document` with its record id, and that the payload never holds a
  revealed value. Undoing such a paste does not ask the host to unlink — not
  specified, not tested.
- **U31 — When the link is announced.** Read as: at the paste (so the token is
  atomic at once, as decided), through a host call — not deferred to the save
  request. The save then carries the token in its content like any other.

### Lists continue on Enter (decided by Céline 2026-10-14, spec v2.0.17)

- **U32 — Where "inside" an item starts.** Enter with the caret inside the content
  splits the item (tested). Enter with the caret inside the marker, before it, or
  just after it (before any content) is not tested.
- **U33 — An empty nested item.** "Its marker is removed and the line becomes
  empty" is read literally: the indentation goes too (the line is empty), not an
  outdent to the parent level (`b_lists_enter_on_an_empty_item_ends_the_list`).
- **U34 — Renumbering.** Enter in the middle of an ordered list inserts the next
  number; whether the following items are renumbered is not decided and not
  tested.
- **U35 — The Enter that commits a composition.** Read as: that Enter belongs to the
  input method. If the host forwards an Enter key while a pre-edit is showing, it
  neither continues the list nor inserts a line break; the commit lands where the
  composition was, once (`b_lists_enter_committing_a_composition_does_not_continue`).
  With winit the committing Enter normally never reaches the editor, so this is a
  guard. If Céline reads it more narrowly ("no continuation" only), that test's
  `starts_with` assertion goes.
- **U36 — Not tested:** the newline style of the continuation in a CRLF document; a
  tab or several spaces after the marker (`-\titem`, `-   item`); lists inside a
  block quote (`> - a`); `[X]` in upper case; Enter with a selection inside an item;
  an unclosed fence (read: it runs to the end of the text block).
- **U37 — Fences.** Both ` ``` ` and `~~~`, with an info string and with up to three
  spaces of indentation, open a fenced block in which nothing continues; after the
  closing fence, lists continue again.
- **U38 — Task items with any marker.** "The same marker … and an unchecked box"
  read together: `* [x] a` continues with `* [ ] `, `1. [x] a` with `2. [ ] `.
- **U39 — An item whose content is a personal-data token is non-empty**: Enter after
  the token continues the list; Enter just before it moves the whole token to the
  new item (`b_lists_a_token_at_the_end_of_an_item_is_not_split`).

## Contract weaknesses found while writing the tests

These are findings, not footnotes: each is a sentence of C-TEXT (or ch. 03 / 09)
that cannot be tested as written, or that conflicts with another.

- **W1 — "Word and line movement follow parley" vs "bidirectional text moves
  logically".** parley 0.10's `PlainEditor` arrow movement is visual
  (`move_left`: "Move to the next cluster left in visual order"). **Decided by
  Céline 2026-10-10: visual movement, logical selection** (see U3). Still open:
  "follows parley" for word movement is testable only by re-implementing parley.
- **W2 — Guarantee 9 had no numbers. Decided by Céline 2026-10-10:** p95 ≤ 16 ms,
  open + first paint ≤ 1 s, 1 MB = 1,000,000 bytes. Still true: timing depends on
  hardware, so `g9_timing_*` gates only in `--release` on agreed hardware.
- **W3 — The cap is about opening only.** Nothing says what happens when a document
  grows past it while open — a direct tension with guarantee 1.
- **W4 — Guarantee 3 and panics.** Rust's standard panic message for a bad string
  slice quotes the string (up to 256 bytes): a slicing bug anywhere in the editor
  or parley puts document text on stderr, and into any crash report or log that
  captures it. The contract forbids "a crash report written by Sovereign
  containing it" but no Sovereign component said what happens to panic messages.
  **Covered elsewhere: shell panic hook (decided 2026-10-10)** — the shell
  installs a panic hook that drops the payload; shell work, not this crate.
  "No swap file" stays outside the editor's power; only "writes no file" is
  testable.
- **W5 — Guarantee 4 is a dependency rule, not a behaviour.** "Depends on no
  networking crate" misses a non-networking dependency that calls `std::net`
  itself. The suite adds a source scan for the editor crate only.
- **W6 — Tokens are in-band text.** A user can type or paste a literal
  `[pii:<id>]` and so reference any personal-data record (shown, once revealed,
  as its value). The contract does not say whether typed or pasted token syntax
  becomes a token. And since copying copies the label, copy-paste inside the
  editor turned a token into plain label text. **Decided: resolver rule,
  2026-10-10** (Céline; replaces the first answer, "typed syntax is never a
  token, stored escaped"): a token is shown as a token only in a document
  listed among its record's sources; anywhere else its form is ordinary text,
  stored as typed. The host applies the rule (`TokenDisplay::NotAToken`).
  Copy-paste inside Sovereign keeps the token through the Sovereign clipboard
  payload, and the editor asks the host to link the record to the target
  document (`PiiDisplay::link_to_document`); outside, the text carries the label
  or the revealed value. What remains open is W17: the payload itself.
- **W7 — Token id format** differed between ch. 03 (`<n>`) and ch. 09 / code.
  **Decided by Céline 2026-10-10:** `[pii:<record_id>]`, the spec follows the code.
- **W8 — Clipboard clearing** conflicted between C-TEXT and ch. 09. **Decided by
  Céline 2026-10-10:** no clearing for ordinary text; 30 s for a copy that
  includes a revealed value.
- **W9 — v1 → v2 media migration could not produce valid v2 blocks.** **Decided by
  Céline 2026-10-10:** `alt` empty plus a "needs a description" flag, `ref.sha256`
  absent until the document service computes it; such blocks are valid; the
  editor never reads files.
- **W10 — Unknown-block removal** ("only by an explicit user action on that block")
  does not define range deletions across the block, nor a keyboard path, which
  guarantee 10 then requires.
- **W11 — The password field** is guaranteed in the document editor's contract
  while its own contract (the text-field contract, a stated prerequisite) is not
  written.
- **W12 — Remote change.** The editor receives "notice of a remote change" but
  nothing says what it does with it.
- **W13 — Typing into a read-only editor is silently dropped.** After *Switch editing
  to this device*, the other device's editor turns read-only; keystrokes typed
  there before the user notices are lost without a word. **Decided by Céline
  2026-10-10:** input in any read-only state produces a visible notice (see U24).
- **W14 — "Typing bursts"** are not defined.
- **W15 — "The same bytes"** is ambiguous for formatting Sovereign did not write (U12).
- **W16 — "The focused text"** is ambiguous for a block document (U26).
- **W17 (new, from decision 4) — The Sovereign clipboard payload is a forgery path
  of its own.** It travels on the system clipboard, where any application can
  write it, so a crafted payload could recreate a token for any record id — the
  forgery W6 closed for typed and pasted text. Not tested here: the payload is
  opaque. Options for the spec: authenticate the payload (e.g. a per-session key
  the host holds), or accept a payload only if it was written by this process
  and is still the clipboard's current item.
