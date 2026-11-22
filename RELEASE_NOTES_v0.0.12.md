# Sovereign GE v0.0.12 — the corrective release

*Your data, your rules.* v0.0.12 corrects what v0.0.11 got wrong, gives every thread its own lane,
names your desktops, and makes the assistant act only on what you named.
<!-- no-claim -->

**Update every device you pair.** A device on v0.0.11 keeps syncing with this one, but some fixes
below (deletions, moves, device identity) only hold once both sides run v0.0.12.
<!-- no-claim -->

## History works again on the desktop

- **Restoring an earlier version from History works on the desktop**, for every version. In
  v0.0.11 it failed for every version.
  <!-- test: crates/sovereign-shell/src/lot1_tests.rs::h1_every_history_row_restores_through_the_shell_keyboard -->
- **On the desktop, when History cannot be read, it says so** instead of showing an empty list.
  <!-- test: crates/sovereign-shell/src/lot1_tests.rs::h3_a_store_error_opens_no_history_and_says_so_plainly -->
- **On the desktop, History names the device** each version came from, on a line that never
  overlaps the time.
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::n5_r1_a_local_rename_changes_what_history_alerts_and_orphans_show -->
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::r2_a_rows_message_line_never_overlaps_its_time_line_and_keeps_its_full_text -->

## One lane per thread

- **Every thread has its own lane on the desktop canvas**, however many threads you have. Above 12
  threads, threads no longer share a lane, and a document created or dropped on a lane goes to the
  thread that lane shows.
  <!-- test: crates/sovereign-shell/src/lanes_tests.rs::n2_103_threads_are_103_lanes_each_labelled_by_its_own_thread -->
  <!-- test: crates/sovereign-shell/src/lanes_tests.rs::n2_each_card_sits_in_its_own_threads_lane -->
  <!-- test: crates/sovereign-shell/src/lanes_tests.rs::n2_new_document_goes_to_the_thread_the_lane_shows -->
  <!-- test: crates/sovereign-shell/src/lanes_tests.rs::n2_a_dropped_card_goes_to_the_thread_the_lane_shows_through_the_stamped_move -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §26 -->
- **The mouse wheel scrolls the lanes; Ctrl+wheel zooms.** Scrolling stops at the first and the
  last lane.
  <!-- test: crates/sovereign-shell/src/lanes_api_tests.rs::n4_the_wheel_scrolls_the_lanes_vertically -->
  <!-- test: crates/sovereign-shell/src/lanes_api_tests.rs::n4_ctrl_wheel_zooms_about_the_cursor -->
  <!-- test: crates/sovereign-shell/src/lanes_api_tests.rs::n4_scrolling_stops_at_the_first_and_the_last_lane -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §26 -->
- **The "Unfiled" lane appears only when a document needs it.**
  <!-- test: crates/sovereign-shell/src/lanes_tests.rs::n3_no_unfiled_lane_while_every_document_has_its_thread -->
- **On the two days a year the clocks change, the time ruler labels each date once**, and it no
  longer crashes in the hour that repeats.
  <!-- test: crates/sovereign-shell/src/ruler_days_tests.rs::ruler_days_d1_one_day_ticks_label_each_date_once_across_the_25_hour_day -->
  <!-- test: crates/sovereign-shell/src/ruler_days_tests.rs::ruler_days_d1_one_day_ticks_label_each_date_once_across_the_23_hour_day -->
  <!-- test: crates/sovereign-shell/src/ruler_days_tests.rs::ruler_days_d4_ticks_are_increasing_and_in_range_at_every_interval_in_the_test_zones -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §24 -->

## Sync

- **Moving a document to another thread reaches your other devices.** Moved on two devices, the
  later move wins on both. Pinning stays each device's own.
  <!-- test: crates/sovereign-p2p/tests/wire_lanes_moves.rs::m3_a_document_moved_on_one_device_shows_in_its_new_thread_on_the_other -->
  <!-- test: crates/sovereign-p2p/tests/wire_lanes_moves.rs::m3_moved_on_both_devices_the_later_move_wins_on_both -->
  <!-- test: crates/sovereign-p2p/tests/wire_lanes_pin.rs::m4_a_sync_does_not_change_the_pin_on_either_device -->
- **Emptying the trash only removes documents on this device.** A deletion travels to your other
  devices; removing for good stays local, so a document you restored on one device is never
  removed by emptying the trash on another.
  <!-- test: crates/sovereign-docs/tests/item3_local_purges.rs::l1_a_purge_on_one_device_removes_nothing_on_the_other_whatever_it_holds -->
  <!-- test: crates/sovereign-p2p/tests/item3_local_purges.rs::l7_finding_2_over_the_wire_a_restore_on_b_survives_and_comes_back_to_a -->
- **After the update, documents already in the trash keep their 30 days**, counted on this device;
  nothing is removed at the first start.
  <!-- test: crates/sovereign-docs/tests/item3_trash_since.rs::l10_nothing_is_purged_at_the_first_tick_after_an_upgrade -->
- **Each desktop marks its changes with its own identity**, so two desktops with the same name never
  overwrite each other's changes by mistake.
  <!-- test: crates/sovereign-shell/src/item22_identity_tests.rs::d2_two_desktops_with_the_same_display_name_never_write_the_same_identity -->
  <!-- test: crates/sovereign-shell/src/item22_identity_tests.rs::d1_d6_each_personas_real_login_writes_its_own_peer_id -->
- **A change dated more than five minutes in the future is refused**, instead of being applied
  ahead of everyone else's.
  <!-- test: crates/sovereign-docs/tests/lot1_c_stamp_bound.rs::c2_a_decision_stamp_beyond_the_bound_refuses_the_document_whole -->
- **The phone syncs again on the local network.** Its network layer now starts on Android, and
  Settings → Devices says what it can reach.
  <!-- test: crates/sovereign-p2p/tests/item12_no_system_dns.rs::p1_node_builds_and_starts_without_system_dns_config -->
  <!-- test: frontend/src/lib/components/SettingsPanel.item12.svelte.test.ts::P4: shows "This phone cannot look up names: it syncs on the local network and with addresses given by IP." when the backend reports no DNS -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §21 -->

## Your desktops have names

- **On the desktop, each workspace has a device name**, asked at setup (prefilled with the
  computer's name) and renamed in the Devices window. Both personas see and change the same name.
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::n2_the_setup_fields_ask_for_the_name_prefilled_with_the_host_name -->
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::n3_this_devices_rename_opens_cancels_refuses_and_saves -->
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::n1_both_personas_read_and_change_the_same_name -->
- **A new name is used at once** by pairing and by a guardian offer, without a restart.
  <!-- test: crates/sovereign-shell/src/device_names_tests.rs::n3_pairing_and_the_guardian_offer_use_the_new_name_without_a_restart -->

## Accounts and setup

- **The duress password must differ from the main one**, on the desktop and on the phone.
  <!-- test: crates/sovereign-crypto/tests/lot1_p4_duress_differs.rs::p4_create_refuses_a_duress_passphrase_equal_to_the_primary -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::p4_duress_equal_to_main_takes_the_wizard_back_to_the_duress_step -->
  <!-- test: frontend/src/lib/components/OnboardingWizard.p5.svelte.test.ts::P5a: next disabled when the duress password equals the main one once trimmed -->
- **On the desktop, creating an account, unlocking and recovering no longer freeze the window**: a
  status line says what is happening, a second click starts nothing, and an error stays on screen.
  <!-- test: crates/sovereign-shell/src/first_run_worker_tests.rs::b1_setup_derivation_runs_off_the_ui_thread_with_its_status_line -->
  <!-- test: crates/sovereign-shell/src/first_run_worker_tests.rs::b1_unlock_runs_off_the_ui_thread_with_its_status_line -->
  <!-- test: crates/sovereign-shell/src/first_run_worker_tests.rs::b1_b3_recovery_new_password_runs_off_the_ui_thread_once -->
  <!-- test: crates/sovereign-shell/src/first_run_worker_tests.rs::b2_b3_setup_is_inert_while_it_runs_and_a_second_submit_starts_nothing -->
  <!-- test: crates/sovereign-shell/src/first_run_worker_tests.rs::b4_unlock_failure_stays_locked_shows_an_alert_and_reenables_the_form -->
- **A setup interrupted after the account was saved** opens with its password, and says once that
  setup was interrupted. The desktop and the phone each name what their own setup saves.
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::b5b_an_interrupted_setup_shows_the_notice_once_at_the_next_login -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::b5_killed_inside_the_first_installation_the_account_opens_with_its_password -->
  <!-- test: crates/sovereign-app/src/lot1_onboarding_tests.rs::p2b_an_error_after_the_store_is_an_incomplete_setup_not_a_failure -->
- **The canary phrase is entered like a password**: masked, with the same "show" button as the
  password, on the desktop and on the phone. The setup's "show" button works again on the desktop.
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::k1_k2_the_canary_step_has_the_wizards_show_toggle_named_for_the_phrase -->
  <!-- test: crates/sovereign-shell/src/onboarding_reveal_tests.rs::k1_a_click_on_show_toggles_the_reveal_on_every_masked_step -->
  <!-- test: frontend/src/lib/components/OnboardingWizard.p5.svelte.test.ts::K3: both canary inputs are password inputs, as the password step’s are -->
  <!-- test: frontend/src/lib/components/OnboardingWizard.show.svelte.test.ts::K3: checked, both canary inputs are text; unchecked again, both are masked -->
- **On the phone, the duress password has the "show" button too**, and showing a secret no longer
  lets the keyboard capitalise or correct it.
  <!-- test: frontend/src/lib/components/OnboardingWizard.show.svelte.test.ts::K3: checked, both duress inputs are text; unchecked again, both are masked -->
  <!-- test: frontend/src/lib/components/OnboardingWizard.show.svelte.test.ts::K5: both duress inputs, masked and shown -->
  <!-- test: frontend/src/lib/components/OnboardingWizard.show.svelte.test.ts::K5: both canary inputs, masked and shown -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §25 -->

- **The phone's setup sends every confirmation it asks for**, and the account is refused when one
  differs.
  <!-- test: frontend/src/lib/components/OnboardingWizard.p5.svelte.test.ts::P5f: first device — password_confirm, duress_confirm and canary_confirm are the trimmed confirmations -->
  <!-- test: crates/sovereign-app/src/lot1_onboarding_tests.rs::p3_a_confirmation_absent_or_different_is_refused_before_anything_is_written -->

## Typing and copying

- **The chat box and the search field are real text fields**: move the caret, select, type over a
  selection, delete in the middle, paste, and use input methods.
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k1_each_is_the_editors_single_line_field -->
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k2_caret_movement_reaches_the_field -->
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k2_typing_replaces_the_selection -->
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k2_delete_mid_text_reaches_the_field -->
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k2_paste_reaches_the_field -->
  <!-- test: crates/sovereign-shell/src/chat_field_tests.rs::k2_an_input_method_commit_is_inserted_once -->
- **Cancelling a quit** no longer leaves "Discard" chosen for the next one.
  <!-- test: crates/sovereign-shell/src/lot1_tests.rs::d1_cancel_withdraws_the_discards_of_that_quit -->
  <!-- test: crates/sovereign-shell/src/lot1_tests.rs::d2_a_later_quit_asks_again_for_a_discarded_then_cancelled_document -->
- **On the desktop, what Sovereign copied is cleared from the clipboard when you quit or unlock
  another persona.** What another application copied is left alone.
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::k2_every_quit_path_runs_the_forgetting -->
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::k1_a_body_copy_from_the_editor_is_cleared_at_a_store_change -->
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::k1_a_title_copy_from_the_editor_is_cleared_at_a_store_change -->
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::k1_another_applications_copy_survives_a_store_change -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §27 -->
- **Personal data in a title** is shown and read out by its label, not as a raw token, on the
  desktop.
  <!-- test: crates/sovereign-editor/tests/l1_title_label.rs::l1_a_titles_token_is_labelled_with_its_label -->
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::l2_the_card_shows_a_tokens_label -->
  <!-- test: crates/sovereign-shell/src/clipboard_forget_tests.rs::l2_the_shells_accessibility_tree_holds_no_raw_token -->

## The trash and documents without a thread

- **Emptying the trash removes what you saw**: a document that arrived in the trash after the view
  was drawn stays, and one restored meanwhile is left alone; on the desktop, the message says so.
  <!-- test: crates/sovereign-docs/tests/item9_act_on_what_was_shown.rs::t1_a_document_a_peer_trashed_after_the_view_was_drawn_is_not_purged -->
  <!-- test: crates/sovereign-shell/src/item9_tests.rs::t1_desktop_empty_trash_clicked_keeps_what_a_peer_trashed_after_the_view -->
  <!-- test: crates/sovereign-shell/src/item9_tests.rs::t1_desktop_the_confirms_message_names_a_document_restored_meanwhile -->
- **Actions on documents whose thread was deleted** apply to the documents shown, not to ones that
  arrived since.
  <!-- test: crates/sovereign-docs/tests/item9_act_on_what_was_shown.rs::t2_trash_all_leaves_a_document_that_joined_later_waiting -->
- **On the desktop, deleting a document another device already moved to the trash** says so, with
  no Undo.
  <!-- test: crates/sovereign-shell/src/item9_tests.rs::t3_desktop_delete_of_a_document_a_peer_already_trashed_says_so_with_no_undo -->
- **"Restore into…" offers every thread**, in a list that scrolls by wheel and keyboard.
  <!-- test: crates/sovereign-shell/src/item9_api_tests.rs::t4_every_live_thread_is_offered_in_lane_order_as_named_options -->
  <!-- test: crates/sovereign-shell/src/item9_api_tests.rs::t4_the_keyboard_reaches_every_thread_and_keeps_it_in_view -->
  <!-- test: crates/sovereign-shell/src/item9_api_tests.rs::t4_the_wheel_scrolls_the_list_and_keeps_the_choice_in_view -->

## The on-device assistant

When you ask the assistant for an action directly:
<!-- no-claim -->

- **Threads and documents are named exactly.** "Work" never means "Work Archive", and "Report" never
  means "Quarterly Report Draft". When no name matches exactly, the assistant lists the close ones
  and asks; when two match, it does nothing and says so.
  <!-- test: crates/sovereign-ai/src/orchestrator_thread_names_tests.rs::a1_delete_work_never_acts_on_work_archive -->
  <!-- test: crates/sovereign-ai/src/orchestrator_thread_names_tests.rs::a1_no_exact_match_lists_at_most_five_close_names_and_asks -->
  <!-- test: crates/sovereign-ai/src/orchestrator_thread_names_tests.rs::a1_two_live_threads_with_the_same_name_are_ambiguous -->
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t1_report_never_resolves_to_quarterly_report_draft -->
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t1_two_exact_titles_are_listed_with_thread_and_date_and_the_canvas -->
- **The proposal names what will be acted on**, and that is what is acted on once you approve.
  <!-- test: crates/sovereign-ai/src/orchestrator_thread_names_tests.rs::a2_the_proposal_names_the_resolved_thread_and_its_consequence -->
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t2_after_approval_the_action_runs_on_the_resolved_id -->
- **Rename, move and merge use the names you wrote.** Put a name in quotes when it contains "to"
  ("rename 'Trip to Rome' to 'Holidays'").
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t6_router_fixtures_act_on_the_names_the_person_wrote -->
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t6_quoted_names_are_taken_as_given -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §23 -->
- **An action whose names were read from your words always shows its proposal**, even after you
  approved that action five times; names you put in quotes keep the usual trust.
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t7_unquoted_new_name_running_to_the_end_is_proposed_even_trusted -->
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t7_quoted_names_keep_trust -->
- **Merging two threads always asks**, as deleting does.
  <!-- test: crates/sovereign-ai/src/orchestrator_document_titles_tests.rs::t4_merge_threads_always_asks_even_after_five_approvals -->
- **"Done" appears only for a change that happened**: a search or a summary reports what it found, a
  rename to the same name says "No change", and a failure says why, once.
  <!-- test: crates/sovereign-ai/src/orchestrator_outcome_words_tests.rs::b4_every_action_of_the_classifier_list_says_done_only_for_a_real_change -->
  <!-- test: crates/sovereign-ai/src/orchestrator_outcome_words_tests.rs::b2_no_change_is_said_as_such -->
  <!-- test: frontend/src/lib/api/events.actionOutcome.test.ts::no change never reads as done -->
  <!-- test: frontend/src/lib/api/events.actionOutcome.test.ts::a failure says why once, with no second message and no "Failed:" prefix -->
- **In a conversation too, a rename to the same name or a move into the same thread says "No
  change"**, not "Done".
  <!-- test: crates/sovereign-ai/src/orchestrator_chat_no_change_tests.rs::c2_chat_rename_to_the_same_name_is_no_change -->
  <!-- test: crates/sovereign-ai/src/orchestrator_chat_no_change_tests.rs::c2_chat_move_into_its_own_thread_is_no_change -->
  <!-- test: crates/sovereign-core/src/chat_no_change_tests.rs::c1_mixed_writes_say_done_only_for_the_real_change -->
- **Summarize works on long documents**: it says first that it will work in parts and take longer,
  and declines a document too long even for that, suggesting a section.
  <!-- test: crates/sovereign-ai/src/orchestrator_summarize_parts_tests.rs::s2_a_long_document_is_summarized_in_parts_that_each_fit -->
  <!-- test: crates/sovereign-ai/src/orchestrator_summarize_parts_tests.rs::s3_the_reply_says_n_parts_before_starting -->
  <!-- test: crates/sovereign-ai/src/orchestrator_summarize_parts_tests.rs::s4_beyond_sixteen_parts_summarize_declines_and_sends_nothing -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §22 -->
- **A prompt longer than the model reads in one batch is fed in parts**; one longer than the
  model's window is refused in plain words.
  <!-- test: crates/sovereign-ai/tests/s6_prompt_chunks.rs::s6_a_prompt_beyond_the_window_is_refused_with_a_plain_error -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §22 -->
- **Restoring a thread another device deleted** brings it back with its name.
  <!-- test: crates/sovereign-p2p/src/sync_service.rs::a4_restoring_a_thread_a_peer_deleted_brings_it_back_live -->

## Documents that could not be read

- **Documents the conversion could not read are counted and listed**, in the desktop's Alerts
  window and in the app's alerts, by a short id and their date. Each is kept as it was, and
  Sovereign tries again after its next update.
  <!-- test: crates/sovereign-docs/tests/item7_conversion.rs::v1_an_undecryptable_row_is_counted_in_the_notice_and_left_as_it_was -->
  <!-- test: crates/sovereign-shell/src/item7_tests.rs::v2_one_alert_lists_each_undecryptable_document_by_short_id_and_date -->
  <!-- test: crates/sovereign-app/src/item7_v2b_tests.rs::b2_the_list_gives_the_line_of_the_undecryptable_document -->
  <!-- test: frontend/src/lib/components/SyncAlerts.unreadable.svelte.test.ts::B3: the heading (h3), the meaning, and the lines as a list labelled by the heading -->
  <!-- test: crates/sovereign-docs/tests/item7_conversion_api.rs::v3_a_different_version_retries_and_a_row_that_now_decrypts_is_converted -->
- **You can keep saving while the conversion runs.**
  <!-- test: crates/sovereign-docs/tests/item7_conversion.rs::v4_a_save_issued_mid_run_completes_before_the_run_ends -->
- **A document open in the editor is not reported as "changed elsewhere"** by its own conversion.
  <!-- test: crates/sovereign-shell/src/item7_tests.rs::v5_the_open_documents_own_conversion_raises_no_changed_elsewhere -->

## The assistant's conversation log

- **If the assistant's conversation log cannot be verified when you log in**, it is moved aside
  unread and a new one starts, every time, with a notice that says so. Nothing is deleted.
  <!-- test: crates/sovereign-ai/src/orchestrator_item10_tests.rs::l1_changed_ciphertext_is_quarantined -->
  <!-- test: crates/sovereign-ai/src/orchestrator_item10_tests.rs::l2_the_marker_decides_nothing -->
- **A log whose entries were reordered, or with text added after it was encrypted**, is now caught
  at login.
  <!-- test: crates/sovereign-ai/src/orchestrator_item10_tests.rs::l1_reordered_entries_are_quarantined -->
  <!-- test: crates/sovereign-ai/src/orchestrator_item10_tests.rs::l1_planted_plaintext_turn_is_quarantined -->

## Accessibility

- **The desktop's setup, login and join screens** are described to a screen reader as drawn: each
  field by its label, never a secret.
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::a1_while_the_wizard_is_shown_the_tree_is_the_wizards_current_step -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::a1_the_login_form_is_described_as_drawn -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::a1_the_join_form_is_described_as_drawn -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::a3_masked_wizard_fields_expose_their_length_only_even_with_show_on -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::a3_masked_login_and_join_fields_expose_their_length_only_even_with_show_on -->
  <!-- test: crates/sovereign-shell/src/first_run_tests.rs::k2_the_canary_fields_expose_their_length_only_even_with_show_on -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §25 -->
- **In the app, a screen reader hears the assistant's replies and the result of each action**, also
  with the chat closed. The buttons on the alert notices say what they open.
  <!-- test: frontend/src/lib/components/Chat.a11y.svelte.test.ts::an assistant reply pushed after render is announced, by a region already there -->
  <!-- test: frontend/src/lib/components/Chat.a11y.svelte.test.ts::a system message pushed after render (an outcome line) is announced -->
  <!-- test: frontend/src/lib/components/Chat.a11y.svelte.test.ts::panel hidden: an outcome line that arrives is announced -->
  <!-- test: frontend/src/lib/components/SyncAlerts.a11y.svelte.test.ts::with all three notices shown, the three buttons have three different names -->
  <!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §25 -->

## Windows

- **The desktop app no longer uses OpenGL on Windows**; it draws with DirectX 12 or Vulkan.
  <!-- test: crates/sovereign-shell/src/no_gl_windows_tests.rs::no_gl_windows_unset_gives_exactly_dx12_and_vulkan -->
  <!-- test: crates/sovereign-shell/src/no_gl_windows_tests.rs::no_gl_both_sites_call_render_context -->
  <!-- test: crates/sovereign-shell/src/no_gl_windows_tests.rs::no_gl_render_context_goes_through_the_descriptor -->
  <!-- test: crates/sovereign-shell/src/no_gl_windows_tests.rs::no_gl_no_render_context_new_in_production -->

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
<!-- manual: doc/plans/RELEASE-v0.0.12-checklist.md §28 -->

How each file is built, and the checks run before publishing: `doc/RELEASE.md`.

---

*Sovereign GE is developed by Céline Lenoble with Claude (Anthropic).*
