# Thread sync — the tests (THREAD-sync S1–S11)

*Written before the code by the test author (2026-10-18, branch `p2p/tests-thread-sync`), for
`doc/plans/THREAD-sync.md` (decided by Céline 2026-10-18; spec ch. 03, *Thread deletion and the
fallback lane*; ch. 05 C-WIRE). Round 1 on `main` 468a801; round 2 on `main` 467a297, after the
plan's "Settled after the test author's report" (S2 and S1 clarified, S5 revised, S8–S11 added).
The implementer does not edit these files; a test that looks wrong is reported, not worked
around.*

## Protected files

| File | What |
|---|---|
| `crates/sovereign-p2p/tests/wire_thread_sync.rs` | S1 (the wire), S2–S6, S8–S11 over sync — two devices over the real sealed, signed row envelopes, no network |
| `crates/sovereign-docs/tests/ts1_one_thread_write_path.rs` | S1 — the one path (a source scan) and the stamps at the service |
| `crates/sovereign-docs/tests/ts6_thread_markers.rs` | S6, S7, S11 at the service |
| `crates/sovereign-docs/tests/s5_12_orphans.rs`, section *W7, now S6* | the three W7 tests, revised to S6 (below) |

Also protected as before: `crates/sovereign-p2p/tests/wire_common/mod.rs`,
`crates/sovereign-docs/tests/common/*` (not changed here).

## The stub

`crates/sovereign-docs/src/threads.rs` (declared in `lib.rs`): three methods on
`DocumentService`, each `todo!()`:

- `thread_create(name, description) -> Result<String, DocError>` — the id;
- `thread_update(id, name: Option<&str>, description: Option<&str>) -> Result<(), DocError>`;
- `thread_stamp(id) -> Result<Option<Stamp>, DocError>` — `None` for a row written before stamps.

A delete stays `delete_thread_to_trash` (it must use the path for the thread record). The names
avoid the store's (`create_thread`, `update_thread`, …) so the scan never confuses the path with a
store call. How a received row reaches the service, how the stamp is stored, what a marker is on
disk and how the manifest carries it are left to the implementer: the tests observe through
`thread_stamp`, the store's lists, the raw store, the manifest, the row diff and the sync outcome.

## Tests per guarantee

| Guarantee | Tests |
|---|---|
| **S1** one stamped path, every live path | `ts1`: `s1_no_production_code_writes_a_thread_around_the_one_path` (scan); `s1_create_rename_and_delete_each_carry_a_new_stamp_from_this_device`; `s1_threads_and_documents_are_stamped_by_one_clock`; `s1_a_thread_write_with_the_clock_set_back_is_still_stamped_later`; `s1_after_a_restart_the_clock_starts_after_the_threads_stamps`; `s1_delete_thread_to_trash_stamps_the_deletion_after_its_documents`; `s1_the_new_thread_of_an_orphan_resolution_is_stamped`; `s1_a_thread_written_before_stamps_has_none`. `wire`: `s1_the_row_carries_the_stamp_inside_the_sealed_envelope`; `s1_a_received_thread_stamp_advances_this_devices_clock` |
| **S2** the later action wins, devices converge | `wire`: `s2_a_delete_made_after_a_rename_on_the_other_device_wins`; `s2_a_rename_made_after_a_delete_on_the_other_device_wins`; `s2_rename_vs_rename_the_later_wins_whichever_device_made_it`; `s2_the_later_action_wins_whatever_the_row_counters_say`; `s2_both_devices_converge_on_the_higher_stamp_whatever_the_system_clock_order` (direction and winner by stamp); `s2_an_action_made_after_receiving_a_stamp_wins_even_with_a_clock_behind`; `s2_two_renames_at_the_same_instant_converge_on_one` |
| **S3** | `wire`: `s3_a_rename_that_wins_over_a_delete_brings_the_thread_back_and_leaves_the_trash_as_it_is` |
| **S4** | `wire`: `s4_a_delete_that_wins_over_a_rename_discards_it_and_the_renaming_devices_documents_ask` |
| **S5** (revised) an unstamped row as if stamped at its `modified_at` | `wire`: `s5_between_rows_without_stamps_the_later_modification_wins_not_the_counter` (and thread rows still flow with an older build's manifest); `s5_an_older_devices_later_rename_wins_over_an_earlier_stamped_one`; `s5_a_stamped_rename_made_after_an_older_devices_rename_wins_whatever_the_counters`; `s5_a_row_without_a_stamp_more_than_5_minutes_ahead_is_refused`; with S6: `s6_an_older_builds_row_earlier_than_the_deletion_never_revives_a_marker`, `s6_an_older_builds_rename_made_after_the_deletion_recreates_the_thread` |
| **S6** shrink to the id | `ts6`: `s6_a_deleted_thread_nothing_points_to_shrinks_after_30_days` (29 days kept, 31 days marker); `s6_a_live_document_keeps_the_name_until_it_is_resolved`; `s6_a_trashed_document_keeps_the_name_until_it_is_purged`; `s6_a_thread_deleted_with_its_documents_shrinks_once_they_are_purged`; `s6_a_live_thread_is_never_shrunk`. `wire`: `s6_a_marker_refuses_an_older_live_row_and_the_name_does_not_come_back`; `s6_a_tombstone_with_the_same_deletion_stamp_brings_no_name_back`; `s6_a_rename_made_after_the_deletion_recreates_the_thread`; the two S5/S6 tests above. `s5_12_orphans`: the three `w7_s6_*` |
| **S7** never shown | every marker assertion (`assert_marker` in `ts6` and `wire`): not in `list_threads`, no lane (`active_thread`), not found by its old name, nothing to delete, not in the trash listing, no orphan, no alert; it **stays** in the sync manifest |
| **S8** 5-minute bound on thread stamps | `wire`: `s8_a_thread_stamp_more_than_5_minutes_ahead_is_refused` (a rename and a new thread 10 minutes ahead refused; applied once 4 minutes ahead) |
| **S9** 30 days from the deletion stamp | `wire`: `s9_a_deletion_received_late_shrinks_30_days_after_its_stamp` (arrives on day 20: kept at day 29, marker at day 31 — 11 days after arrival); `ts6`: the 29/31-day test for a local deletion |
| **S10** a marker and a named tombstone are the same row | `wire`: `s10_a_marker_and_a_named_tombstone_with_the_same_stamp_are_in_sync` (equal manifest hashes, nothing in the row diff); `s10_deleted_state_and_stamp_still_tell_rows_apart` (deleted vs live, two deletion stamps: different hashes; converged: equal) |
| **S11** a document arriving for a shrunk thread asks | `ts6`: `s11_a_document_arriving_for_a_thread_shrunk_to_its_id_asks_without_a_name`; `wire`: `s11_a_document_arriving_for_a_thread_shrunk_here_asks_without_a_name` (listed with no old name, live and shown; the marker stays nameless, no lane). The nameless wording itself already exists in the hosts (shell `orphans.rs` `OrphanLine::notice`, phone `syncAlerts.svelte.ts`) and is not re-tested here |

A marker, as asserted: `thread_stamp` is still the deletion stamp (the shrink issues none); the
record, if the store still lists it, is deleted with an empty name and description; the raw row,
if any, holds neither text in clear and no name token hashes.

## Round 2 changes (2026-10-18, after the settled answers)

- **S5 revised.** Removed `s5_between_rows_without_stamps_the_counter_decides_as_today`,
  `s5_a_row_from_an_older_build_never_overwrites_a_stamped_thread`,
  `s5_a_stamped_rename_wins_over_a_thread_last_written_by_an_older_build` (the "stamped always
  beats unstamped" reading). Added the four S5 tests above. `s6_a_row_from_an_older_build_never_revives_a_marker`
  became `s6_an_older_builds_row_earlier_than_the_deletion_never_revives_a_marker` (with a setup
  check that the edit is earlier than the deletion stamp), and its counterpart
  `s6_an_older_builds_rename_made_after_the_deletion_recreates_the_thread` was added. The
  older-build forge can now set the row's `modified_at` (body and envelope).
- **S1.** The scan has **no exception** any more: backup restore is a live path; the Tauri thread
  writers (`tauri_commands/threads.rs`, `browser.rs` `save_web_page`) are compiled into the phone,
  so they are not the out-of-scope desktop-only code. The exception table now carries a reason
  per entry.
- **S8, S9, S10, S11**: new tests above. `ts6`'s note on the 30 days now cites S9.

## The W7 revisions (S6 replaces S5-12 W7, round 1)

| Was | Now | Change |
|---|---|---|
| `w7_no_thread_tombstone_is_ever_purged_with_live_trashed_or_no_documents` | `w7_s6_a_tombstone_shrinks_to_its_id_only_once_nothing_points_to_it` | Reversed for "Emptied" (its document in the trash) and "Bare" (none): named at +0 days, **markers** from the ten-year tick; "Old drafts" (a live document points) keeps its name at every tick |
| `w7_a_document_whose_thread_was_deleted_keeps_asking_through_every_purge` | `w7_s6_a_document_whose_thread_was_deleted_keeps_asking_through_every_purge` | Same assertions (true under S6); renamed |
| `w7_the_trash_keeps_the_old_name_and_resolving_the_last_document_purges_no_tombstone` | `w7_s6_the_trash_keeps_the_old_name_and_the_tombstone_shrinks_once_nothing_points_to_it` | The end reversed: kept while the late trashed document points, a **marker** once it is purged |

Kept from the round-3 W7 design: clock-independent ticks (+0, +31 d, +10 y, +100 y), two per step.

## Red reasons (on the stubs)

42 tests (39 new, 3 revised): 41 red, 1 passing by design.

| File | Today |
|---|---|
| `ts1_one_thread_write_path.rs` (8) | the scan fails listing the 25 call sites to move; the other 7 panic on the stubs |
| `ts6_thread_markers.rs` (6) | all panic on `thread_create` |
| `wire_thread_sync.rs` (25) | `s5_between_rows_without_stamps_the_later_modification_wins_not_the_counter` fails on today's behaviour: the later modification with a lower counter is refused (0 written, 1 expected); `s5_a_row_without_a_stamp_more_than_5_minutes_ahead_is_refused` fails on today's behaviour: a row 10 minutes ahead is applied (today's bound is 24 h); `s5_a_stamped_rename_made_after_…` panics on `thread_update` after its older-build setup ran; `s6_an_older_builds_row_earlier_…` and `…_made_after_the_deletion_…` and every other one panic on `thread_create` |
| `s5_12_orphans.rs`, W7 now S6 (3) | two fail in `assert_shrunk` (nothing shrinks yet); `w7_s6_a_document_whose_thread_was_deleted_keeps_asking_…` **passes** — a guard |

The tests were not run against a throwaway implementation; the older-build harness (forged
rows verify and apply, a replay is refused, a manifest diff with an older build) was exercised by
the two S5 tests that reach no stub.

## Choices (test author)

1. **"The old-build peer gate applies"** is read as C-WIRE 12: the gate holds back documents only;
   thread rows keep flowing with an older build (asserted with its manifest).
2. **S6/S9:** the 30 days are counted from the deletion stamp's physical time on the service's
   clock (the 29/31-day boundaries), wherever the deletion was made.
3. **S6, equal stamps:** a tombstone received with the same deletion stamp as a local marker does
   not put the name back (S2: only a higher stamp wins; S10: they are the same row).
4. **S5's bound** is checked against the receiving device's clock, with the row's `modified_at`
   set in both the sealed body and the envelope, as an older build sends them.
5. **The scan** forbids, outside `sovereign-db` and `sovereign-docs/src/threads.rs`, every store
   call that writes a thread record (`create_thread`, `create_thread_with_id`, `update_thread`,
   `soft_delete_thread`, `delete_thread`, `restore_soft_deleted_thread`, `merge_threads`,
   `split_thread`, `set_thread_encryption`): method calls always, `Type::` calls when the type is a
   store. Test-only modules declared with `#[cfg(all(test, …))]` or `#[path]` are skipped.
6. **An older build** is simulated at the envelope (the body cut to today's thread fields,
   re-sealed, re-signed after checking the copied encoding against the sender's own signature);
   its writes are store writes, timed by the system clock.

## Run

`CARGO_TARGET_DIR=C:/cargo-target-tsync-tests`, `SOVEREIGN_DATA_DIR` a temporary directory:
`cargo test -p sovereign-docs --test ts1_one_thread_write_path --test ts6_thread_markers --test
s5_12_orphans` and `cargo test -p sovereign-p2p --test wire_thread_sync` give the results above;
`cargo check --workspace --exclude sovereign-ai --all-targets`, the mobile check and the writers
gate (`sovereign-docformat/tests/readers_gate.rs`) pass.
