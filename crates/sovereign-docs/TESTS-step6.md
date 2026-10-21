# Step 6 — the one-time migration and the end of the tolerant read: the tests

*Written 2026-10-18 on branch `docs/tests-migrate` (from `main` 8c5278e), before the code, against
`doc/plans/C-DOC-adoption.md`, *Step 6 … contract* (M1–M7), spec ch. 03 (*Content encoding*,
decision v2.0.42; C-DOC G2, G3) and ch. 07 (*Primary and duress personas*, ATREST-011). Round 2
(same day, after merging main 3155fcb): *Settled after the test author's report* — M2 revised,
M2b, the decoy seeded in v2 (M5), with PANDA2's review points (claude-coord 0112). Author: Claude
Opus 5.5 (laptop, sub-agent, test author). Whoever implements step 6 makes these pass **without
editing the test files** below; a test that looks wrong is reported, not worked around.*

## Test files (protected) and the stubs they need

| File | Guarantees |
|---|---|
| `crates/sovereign-docs/tests/step6_unlock_migration.rs` | M1, M2 (revised), M2b, M3, M5 (store level) — 19 tests |
| `crates/sovereign-docs/tests/step6_scans.rs` | M4 (both hosts, one seam), M6, M7 — 4 scans + 1 self-check |
| `crates/sovereign-app/tests/step6_duress_disk.rs` | M5 on disk (persistent stores, own process) — 1 test |
| `crates/sovereign-app/src/step6_tests.rs` | M5: the decoy seeded in v2, its four callers, its spread dates — 4 tests |
| `crates/sovereign-docformat/tests/readers_gate.rs` | M6: its two step-6 allow-list entries removed (the rest unchanged) |

`sovereign-app/src/lib.rs` declares `step6_tests` (`#[cfg(all(test, feature = "encryption"))]`).

Stubs (API only; bodies `todo!()`; the implementer fills them and may change their insides, not
the signatures the tests use): `sovereign-docs/src/unlock_migration.rs`, re-exported —
`MigrationNotice { converted, not_converted }` + `text()`, `MigrationRun` (`AlreadyComplete`,
`Completed { notice: Option<_> }`, `Interrupted`), `run_unlock_migration(&DocumentService)`,
`unlock_migration_complete`, `unlock_migration_pending_notice` and `unlock_migration_notice_shown`
(M2b), `MigrationTask`, and the host seam `start_unlock_migration(Arc<DocumentService>, spawn,
notify)`. **The seam takes no persona, no path and no clock**: the completion record (with its
counts and whether the notice was shown) has to live in the persona's store (M1, M2b, M5). Adding a
`GraphDB` method for it is expected; regenerate `tests/common/faulty.rs` (`TESTS.md`, *Changing
tests/*).

Every test was run against a throwaway implementation (not committed: record in a process-wide
map, counts from `migrate_all`'s report; for the app tests, `seed_duress_db` writing
`DocumentV2::from_text`): docs 19/19, app 4/4, disk 1/1. Round 1's run found one error of mine,
fixed before the commit (see *Accepted*).

## Guarantees → tests, and what each fails on today

| Guarantee | Tests | Red today because |
|---|---|---|
| M1 once per store, at the first unlock, in the background | `m1_the_first_run_…`, `m1_a_later_unlock_…`, `m1_the_record_is_per_store`, `m1_the_seam_returns_at_once_…`, `m1_a_write_racing_…` | `todo!()` in the stubs |
| M2 (revised) notice when N ≥ 1 or M ≥ 1; none when both 0; names no document | `m2_the_notice_says_…`, `m2_nothing_converted_…`, `m2_the_notice_names_no_document`, `m2_unreadable_documents_alone_…`, `m2_a_store_with_only_unreadable_documents_…` | `todo!()` |
| M2b the record keeps the counts; a notice not shown is shown at the next unlock, once | `m2b_the_record_keeps_the_counts_…`, `m2b_a_notice_not_shown_…`, `m2b_the_seam_records_its_notice_as_shown`; the lock test accepts the notice shown once more after an interruption, never lost | `todo!()` |
| M3 crash / transient failure / lock midway; unreadable counted | `m3_a_crash_anywhere_…` (FaultyDb `crash_after`), `m3_a_transient_failure_…` (`fail_once_after`), `m3_locking_midway_…` (task aborted), `m3_an_unreadable_document_…` | `todo!()` |
| M4 both hosts, one seam | `m4_both_hosts_start_…` (scan), `m4_only_the_seam_runs_the_migration` (scan) | neither host calls `start_unlock_migration`; the second passes today and must stay green |
| M5 installs that already hold a legacy decoy: equal treatment | `m5_raw_seeded_decoy_rows_…`, `m5_primary_and_decoy_do_the_same_work_…`, `step6_duress_disk` | `todo!()` |
| M5 a decoy seeded after the update: v2, nothing to convert, no notice | `m5_a_decoy_seeded_now_holds_no_legacy_rows`, `m5_a_freshly_seeded_decoy_gives_the_migration_nothing_and_no_notice` | `seed_duress_db` writes raw text (the second also hits `todo!()`) |
| M5 the four callers go through the one seed; dates spread | `m5_every_decoy_seeding_goes_through_the_one_seed_function` (scan), `m5_the_seeded_decoy_rows_keep_spread_dates` | pass today by design; must stay green |
| M6 tolerant read gone | `m6_the_tolerant_read_does_not_exist`, readers gate | `content.rs` still defines `parse`, `parse_with_kind` (and its tests use them) |
| M7 nothing writes an old form | `m7_nothing_writes_an_old_form_…` | `content.rs:147`, the `"body"` key inside `parse_with_kind` (goes with M6) |
| M8 the store record sealed at rest (round 3, PANDA2 from-windows/0113) | `sovereign-db/tests/encrypted_over_surreal.rs` (`--features encryption`): `m8_the_store_record_is_sealed_at_rest_and_round_trips`, `m8_other_keys_cannot_read_the_store_record` | on `docs/migrate-impl` b04e8ab, `EncryptedGraphDB::put_store_record` forwards the value unsealed: the raw store holds it as written, and another persona's keys read it (a throwaway seal through `encrypt_with` passes both) |

The writers gate (`no_document_write_around_the_service_except_the_later_steps`) is green and
must stay green. M7 keeps `duress.rs` as an exception for assigning `content` (it still writes
into the raw store, out of scope); if the v2 seed stops assigning `content`, that exception goes
stale and the test says so — report it rather than edit it.

## The decoy's other rows (PANDA2's point 3)

The seed's contacts, conversation and messages carry **no document-format content**: contact
names and addresses, a conversation title, message bodies as plain text — none of them is a
document's `content`, none is read or written by the document service or by `migrate_all`. Nothing
to test for step 6.

## Readings and guesses (the test author's, to check)

- **M1 "per store"**: a new service over the same store reads the record; another store does not.
  A file keyed by device or in `SOVEREIGN_DATA_DIR` fails both this and the disk test.
- **M2b "shown"**: the seam records the notice shown after calling `notify`. A lock in between may
  show it once more at the next unlock (accepted by the lock test: one or two notices, same
  counts, never zero).
- **M3 "interruption"**: a store failure on one document is an interruption, not an unreadable
  document — the run is `Interrupted` (or `Err`), nothing is recorded, the next unlock converts it.
  **Unreadable** = content that claims to be a Sovereign document and cannot be read
  (`FormatError`). A newer-format document is left out of every exact count: whether it counts as
  "could not be read" is open.
- **M3 "overall"**: the notice of the run that finishes counts the documents converted by the
  interrupted runs too. Completion is recorded **exactly** when a run returns `Completed`.
- **M5 "exactly as a real persona's legacy rows"**: a real persona also holds plaintext rows
  (pre-login state, `tauri_commands/auth.rs`), so the decoy's raw rows are compared with a real
  persona's raw rows of the same content, and with its sealed legacy rows. "Same timing class":
  the seam makes no store call before its task runs, for either persona, and the task makes the
  same store calls in the same order.
- **M5 on disk**: neutral directory names are chosen by the test, so only the migration's own
  traces are judged (today's `…-duress.db` names are C-SESS work).
- **M5 dates**: "spread" is checked as at least half the documents with distinct creation times,
  all within the seed's ~6 months and none in the future.

## Accepted, not asserted

- **A crash between a version and the content write versions the legacy state twice** on resume
  (accepted by the contract, G4/G6). The tests assert "at least once" after a crash, and "exactly
  once" only for a write racing the migration (M1's "not twice").
- **The completion record as a trace** of a store unlocked since the update: left to the decoy-store
  redesign (BACKLOG §11), not tested.
