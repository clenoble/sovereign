# S5-12 — a live document in a deleted thread asks: the tests

*Written 2026-10-17/18 on branch `docs/tests-orphans` (from `main` 980deaa), before the code,
against `doc/plans/C-DOC-adoption.md`, *S5-12 — a live document in a deleted thread asks:
contract* (W1–W6), spec ch. 03 (*Thread deletion and the fallback lane*, *Soft delete and 30-day
purge*), ch. 05 (C-WIRE), and `doc/plans/THREAD-name.md` (N1, N2, N5 for the new-thread name).
Author: Claude Opus 5.5 (laptop, sub-agent, test author). Whoever implements S5-12 makes these
pass **without editing the test files** below; a test that looks wrong is reported, not worked
around.*

## Test files (protected) and the stubs they need

| File | Guarantees |
|---|---|
| `crates/sovereign-docs/tests/s5_12_orphans.rs` | W1, W2 (service), W3 (service), W4, W6, W7 (round 2) |
| `crates/sovereign-app/src/orphans_tests.rs` | the phone's three commands (round 2) |
| `crates/sovereign-p2p/tests/wire_orphans.rs` | W2 over the real sealed envelopes, both directions; W4 travelling |
| `crates/sovereign-shell/src/orphans_tests.rs` | W3 (canvas), W4 (as the shell does it), W5, W6 (alerts window) |
| `frontend/src/lib/stores/syncAlerts.orphans.svelte.test.ts` | W5, W6 (phone store) |
| `frontend/src/lib/components/SyncAlerts.orphans.svelte.test.ts` | W4, W5, W6 (phone alerts sheet) |
| `frontend/src/lib/stores/canvas.mobile.orphans.svelte.test.ts` | W3 (phone fallback lane) |

Manual checks: `crates/sovereign-shell/TRASH_CHECKLIST.md`, section 7.

Stubs (API only; bodies `todo!()` / `throw`; the implementer fills them and may change their
insides, not the signatures the tests use):

- `sovereign-docs/src/orphans.rs` (re-exported): `OrphanedDocument` (id, title, thread_id,
  deleted_thread_name: Option, device; `Debug` prints no text), `OrphanResolution`
  (`MoveTo(thread)`, `NewThread(name)`, `Trash`), `OrphanResolved` (documents, thread_id), and
  `DocumentService::orphans_of_deleted_threads`, `resolve_orphan`, `resolve_orphans_of_thread`.
- `sovereign-shell/src/orphans.rs`: `HEADING`, `OrphanAction`, `ACTIONS`, `OrphanLine` (+ `notice`),
  `OrphanGroup`, `OrphansSection` (`empty`, `load`, `lines`, `count`, `groups`, `resolve`,
  `resolve_group`, `a11y_lines`), `status_segment`. `panels.rs`: `AlertsPanel::orphans()`
  (filled by the existing `load_alerts`). Module declared in `main.rs` with `#[allow(dead_code)]`.
- `frontend/src/lib/api/commands.ts` (real bindings, they fix the command names and shapes):
  `OrphanDto`, `OrphanResolutionDto` (`{kind: 'move_to', thread_id} | {kind: 'new_thread', name} |
  {kind: 'trash'}`, i.e. serde `tag = "kind"`, snake_case), `list_orphaned_documents`,
  `resolve_orphan {docId, resolution}`, `resolve_orphans_of_thread {threadId, resolution}`.
  **The three Tauri commands are not written**; when they are, add them to
  `ipc_classification_guard` (`sovereign-app/src/lib.rs`, gated) and to `generate_handler!`.
- `frontend/src/lib/stores/syncAlerts.svelte.ts`: `Orphan`, `syncAlerts.orphans`,
  `orphanFromDto`, `orphanNotice`, `orphansNotice`, `setOrphans`; `reloadSyncAlerts` must also
  load the orphans. `canvas.svelte.ts`: `FALLBACK_LANE_ID`, `FALLBACK_LANE_NAME` ("Unfiled", as the
  shell), `MobileLane`, `mobileLanes()`, `laneIdOf()`; `setLaneIndex` must count the fallback lane.

**Each test file was run once against a throwaway implementation (not committed)** to check the
tests can pass together: service 20/20, sync 6/6, shell 11/11, vitest 19/19. That exercise found two
errors of mine in the tests (an id with a hyphen, which the canvas shows as `⟨…⟩`; the peer
alert's line reads "Alert 1 of 1, selected"), both fixed before the commit. It also showed that
**sync itself needs no change**: with the service condition and resolutions written, the six sync
tests passed on the existing C-WIRE path and row protocol.

## Guarantees → tests, and what each fails on today

Measured on this branch (`cargo test -p sovereign-docs --test s5_12_orphans`, `-p sovereign-p2p
--test wire_orphans`, `-p sovereign-shell orphans_tests`, `npx vitest run`): **56 S5-12 tests,
54 fail, 2 pass by design** (guards: the shell canvas already draws Unfiled; the phone store
survives a host without the command). Service 0/20, sync 0/6, shell 1/11, vitest 1/19. *(Rounds
2–3 add 3 W7 tests (1 guard passes) and 8 app tests (1 passes): 67 in all, 4 passing by design.)* The whole
vitest suite otherwise passes (351 passed, the 18 failures are these); `cargo check --workspace
--exclude sovereign-ai --all-targets`, the mobile check and `readers_gate` pass; `svelte-check`
reports only its two pre-existing errors.

### W1 — the condition (`s5_12_orphans.rs`)

| Test | Fails today on |
|---|---|
| `w1_a_live_document_in_a_deleted_thread_is_listed_with_its_title_old_thread_name_and_device` | `orphans_of_deleted_threads` is `todo!` |
| `w1_documents_in_the_trash_purged_or_in_a_live_thread_are_not_listed` | same |
| `w1_a_document_whose_thread_never_existed_here_is_not_listed` | same |
| `w1_the_condition_survives_a_restart` | same |
| `w1_a_document_written_here_lists_this_device` | same |
| `w1_the_listing_prints_no_text` | same |

### W2 — both directions

| Test | Fails today on |
|---|---|
| `s5_12_orphans::w2_a_thread_deletion_arriving_lists_the_live_documents_and_trashes_none` | `todo!` |
| `s5_12_orphans::w2_an_edit_arriving_for_a_document_trashed_with_its_thread_is_listed_when_it_wins` | `todo!` (asserts "listed exactly when live", whichever stamp wins) |
| `wire_orphans::w2_a_document_arriving_for_a_thread_deleted_here_is_listed` | `todo!` (setup asks the condition) |
| `wire_orphans::w2_a_thread_deletion_arriving_lists_the_live_documents_and_trashes_none` | same — thread rows first, then documents |
| `wire_orphans::w2_after_a_full_sync_both_devices_ask_and_nothing_is_lost` | same |

### W3 — never hidden

| Test | Fails today on |
|---|---|
| `s5_12_orphans::w3_a_listed_document_stays_in_every_list_readable_and_editable` | `todo!` |
| `s5_12_orphans::w3_the_purge_never_takes_a_listed_document` | `todo!` |
| `orphans_tests::w3_the_canvas_draws_a_waiting_document_in_the_fallback_lane` | **passes** — a guard: the shell already draws such documents in Unfiled (with fewer than 12 threads; the 12-lane collision in spec ch. 03 is not covered) |
| `canvas.mobile.orphans` (5 tests) | `mobileLanes` / `laneIdOf` throw; `setLaneIndex` stops at the threads; `MobileCanvas.svelte` does not call `mobileLanes(` |

### W4 — three ways out

| Test | Fails today on |
|---|---|
| `s5_12_orphans::w4_move_to_an_existing_thread_stamps_the_move_and_ends_the_question` | `resolve_orphan` is `todo!` |
| `…w4_a_new_thread_is_created_with_the_name_given_and_the_document_moves_there` | same |
| `…w4_trash_puts_it_in_the_trash_restorable_for_30_days` | same |
| `…w4_a_deleted_or_unknown_target_or_an_empty_name_is_refused_and_nothing_changes` | same |
| `…w4_a_document_that_is_not_listed_is_refused_and_nothing_changes` | same |
| `…w4_all_of_a_deleted_thread_move_together_into_one_new_thread` | `orphans_of_deleted_threads` `todo!` |
| `…w4_all_of_a_deleted_thread_go_to_the_trash_or_to_one_existing_thread` | `resolve_orphans_of_thread` `todo!` |
| `…w4_a_group_with_a_refused_target_changes_nothing` | same |
| `wire_orphans::w4_a_move_into_a_new_thread_travels_and_ends_the_question_on_both_devices` | `todo!` |
| `wire_orphans::w4_trash_from_the_phone_travels_to_the_laptop` | `todo!` |
| `wire_orphans::w4_a_move_to_an_existing_thread_travels` | `todo!` |
| `orphans_tests::w4_each_way_out_ends_the_question_at_once` | `OrphansSection::load` `todo!` |
| `orphans_tests::w4_a_refused_way_out_says_why_and_changes_nothing` | same |
| `orphans_tests::w4_the_group_action_resolves_all_of_one_deleted_thread` | same |
| `orphans_tests::w4_a_document_resolved_elsewhere_meanwhile_is_refused_and_dropped` | same |
| `SyncAlerts.orphans` (5 tests under "the three ways out") | no notice opens the sheet without a peer alert; no section |

### W5 — shown where alerts are

| Test | Fails today on |
|---|---|
| `orphans_tests::w5_the_section_lists_each_waiting_document_with_its_old_thread_and_no_text` | `load` `todo!` |
| `orphans_tests::w5_every_line_offers_the_three_ways_out_and_each_deleted_thread_its_group_action` | same |
| `orphans_tests::w5_the_alerts_window_carries_the_section_beside_the_peer_alerts` | `AlertsPanel::orphans` `todo!` |
| `orphans_tests::w5_with_no_peer_alert_the_window_still_shows_the_section` | same |
| `orphans_tests::w5_the_status_line_counts_the_waiting_documents` | `status_segment` `todo!` |
| `orphans_tests::w5_the_status_line_uses_the_count` | scan: `app.rs` never calls `status_segment(` |
| `syncAlerts.orphans` (5 of 6) | the store functions throw; `reloadSyncAlerts` does not load orphans |
| `syncAlerts.orphans::a host that cannot list them leaves the alerts loading as before` | **passes** — a guard |
| `SyncAlerts.orphans` (3 tests under "W5") | no status notice, no section |

### W6 — nothing else changes

| Test | Fails today on |
|---|---|
| `s5_12_orphans::w6_the_condition_raises_and_replaces_no_alert` | `todo!` |
| `s5_12_orphans::w6_a_peer_audit_alert_on_the_same_document_is_untouched_by_listing_and_resolving` | `todo!` |
| `orphans_tests::w5_the_alerts_window_carries_the_section_beside_the_peer_alerts` (its W6 half) | see W5 |
| `syncAlerts.orphans::the peer-alert notice is not changed…`, `SyncAlerts.orphans::leaves the peer alerts as they are` | store throws / no section |

## Guesses (mine, to confirm or overrule)

1. **"The device it last came from"** = the author device of the current state; for a write made
   here, this device. Tested only for a first arrival from the phone and a local document.
2. **Resolving a document that is not listed is refused** (stale view: resolved elsewhere, live
   thread, in the trash) — by analogy with S5-4 (AT8). Not stated in W4.
3. **A group is resolved only for a deleted thread**; a live thread id is refused.
4. **New-thread name**: trimmed, empty or spaces-only refused (THREAD-name N2), no other rule
   (newline → space is left to the field).
5. **Phone placement**: the section is in `SyncAlerts` (the phone has an alerts sheet, so the
   Settings fallback of W5 does not apply); a `role="status"` notice opens it when there is no
   peer alert. Labels: "Move to a thread…", "New thread…", a trash button, "Move", "Create",
   select labelled "Thread", field "Thread name" with placeholder "Thread name…"; the group is a
   `role="group"` named after the deleted thread, its buttons naming "all".
6. **Phone fallback lane** named "Unfiled" (shell parity), after the threads, shown only when
   needed.
7. **Shell status line**: a segment from `status_segment(n)`, only when n > 0, naming n.
8. A move keeps title, content and every version; "versioned per its rules" is not asserted further.

## Problems found in W1–W6 (reported 2026-10-18) and what became of them

- **W5 wording** — "…was deleted on this device" was false when the other device deleted the
  thread. **Settled** in the contract (main 6d7bab3): "<title> — its thread <old name> was
  deleted." The tests already asserted only the title, the old name and "deleted".
- **W1 vs the thread tombstone purge** — **settled as W7**, then revised: no thread tombstone is purged (tests below).
- **Sync can still restore a deleted thread** (`apply_thread_row` →
  `restore_soft_deleted_thread` when a live row wins, e.g. an offline rename). **Open for
  Céline**; not tested, on the coordinator's instruction.
- **The Tauri layer had no Rust test** — **added** (round 2, below).
- **Shell canvas, 12 lanes.** W3 passes on the shell today only below 12 threads (spec ch. 03,
  *As built*); the collision is not in this contract.

## Round 2 (2026-10-18): W7 and the phone's commands

On the coordinator's message (contract at main 6d7bab3), branch merged with main first. App
tests run once against a throwaway implementation (not committed): 7/8 (the registration scan
needs the wrappers); red on the stubs: 1/8 (the serde-shape test passes on the stub type).

**W7, round 3 (contract at main 1a33c1a): thread tombstones are kept — none is ever purged, for
now** (purging one reopens sync resurrection: the thread leaves the manifest and a paired device
still holding it live pushes it back under its old id). The round-2 W7 tests (purge a tombstone
nothing points to) were rewritten to assert the opposite. Clock-independent: purge ticks at
+0, +31 days, +10 years and +100 years by the service's clock, the tombstones checked after each,
so whatever the relation between the system clock (which stamps a thread's `deleted_at`) and the
service's, a tick falls long after the deletion and none may purge. Run against the throwaway
service implementation: 23/23.

| Test | Today |
|---|---|
| `w7_no_thread_tombstone_is_ever_purged_with_live_trashed_or_no_documents` | **passes** — a guard: nothing purges a thread tombstone today (`purge_tick` is documents only, `purge_deleted` has no caller). It fails if a purge of threads is added. |
| `w7_a_document_whose_thread_was_deleted_keeps_asking_through_every_purge` | `orphans_of_deleted_threads` `todo!` |
| `w7_the_trash_keeps_the_old_name_and_resolving_the_last_document_purges_no_tombstone` | `resolve_orphan` `todo!` (the trash-name half already holds) |

**The phone's three commands** (`crates/sovereign-app/src/orphans_tests.rs`, protected), at the
layer under the wrappers, as `doc_trash.rs` for the trash. Stub: `sovereign-app/src/doc_orphans.rs`
(`OrphanDto`, `OrphanResolutionDto` with serde `tag = "kind"`, snake_case, `From<…> for
OrphanResolution`, `list_orphaned_documents(docs, device_names)`, `resolve_orphan`,
`resolve_orphans_of_thread`), declared in `lib.rs` with `#[allow(dead_code)]`. The
`#[tauri::command]` wrappers are not written: register them in `generate_handler!` and add them to
`ipc_classification_guard`'s gated list (additions only).

| Test | Fails today on |
|---|---|
| `s5_12_list_orphaned_documents_names_the_document_its_old_thread_and_the_device` | `todo!` |
| `s5_12_the_list_travels_with_the_frontends_field_names_and_no_text` | `todo!` |
| `s5_12_the_resolution_reads_the_three_shapes_the_frontend_sends` | **passes** — serde on the stub type: the shapes are fixed |
| `s5_12_the_resolution_maps_to_the_service` | `From` is `todo!` |
| `s5_12_resolve_orphan_moves_creates_or_trashes_and_ends_the_question` | `todo!` |
| `s5_12_resolve_orphan_refused_says_why_and_changes_nothing` | `todo!` |
| `s5_12_resolve_orphans_of_thread_resolves_the_group_and_counts_it` | `todo!` |
| `s5_12_the_three_commands_are_registered` | not in `generate_handler!` |
