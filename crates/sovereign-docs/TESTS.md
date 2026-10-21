# sovereign-docs — the test suite of the document service (C-DOC, C-ENC)

*Written 2026-10-10, before the service exists, against spec ch. 03, *Document service —
contract (C-DOC)*; amended 2026-10-11 with Céline's decisions (see *Decided 2026-10-11*).
Author: Claude Opus 5.5 (laptop, worktree `docs/contract-tests`).
Whoever builds the service makes this suite pass **without modifying `tests/`** (see
*Changing tests/* for the one mechanical exception).*

## How the suite works

- **Real stack.** Every test runs the service over `EncryptedGraphDB` → `FaultyDb` →
  in-memory `SurrealGraphDB` (`tests/common/mod.rs`, `Stack`). The raw store and the
  encrypted layer stay in the test's hands, so what is *stored* is checked independently of
  what the service *says*.
- **Stored content is checked with `serde_json`**, never with `sovereign-docformat`: the suite
  does not trust the model it tests. The format crate is exercised only through the service's
  views and in three direct tests (`g02_format_*`, `g06_default_policy_decides_*` uses the
  policy, not the format).
- **Fakes, all deterministic:** a clock moved by hand; an event sink that records; a
  personal-data hook (pass-through, tokenize `alice@example.com` in any field, fail, or answer
  with a field it was not given); a commit policy (never, always, or the shipped policy with a
  given threshold); media files (path → hash); the device name `laptop`.
- **Crash injection.** `FaultyDb` (generated) lets *k* store calls through and then fails
  every call: the process "stops" at an arbitrary point. Crash tests try every *k* until the
  operation completes without reaching it (cap 400), then check an invariant at each *k*.
- **Invariants only.** No test asserts a message string, an id format, a number of internal
  store calls or an order of internal calls. Where a test fixes something the contract leaves
  open, it is listed under *Uncertain*.

Run: `cargo test -p sovereign-docs` (Windows build env: see the workspace notes). Against the
stubs every test fails on `todo!("document service not built yet")` — see *Counts*.

## Decided 2026-10-11, and what changed here

| Decision | Who | Spec | Tests |
|---|---|---|---|
| Versions by amount of change, never by time (the 50-saves / 5-minutes rule removed); measure and threshold to evaluate | Céline | G6 | `g06_*` rewritten: threshold honoured exactly against `change_amount`; hours of time and tiny edits make no version; more change never fewer versions; the measure is zero only for equal states, symmetric, never shrinks as a change grows. No default number is asserted. |
| Hook failure stores the content, marked not scanned | Céline (was Uncertain 3) | G10 | unchanged (`g10_a_failing_hook_*`) |
| Purge leaves a contentless marker | Céline (was weakness 1) | G13 | unchanged; storage and wire form open in C-WIRE |
| Titles, captions and alt texts go through ingest | Céline (was weakness 6) | G10, G15 | hook request is now every text field (`IngestItem { field: TextField, text }`); `g10_the_hook_sees_every_text_field_*`, `g10_no_raw_value_settles_in_a_title_caption_or_alt_text`, `g10_a_title_only_save_runs_the_hook_on_the_title`; `g15_tokens_in_titles_and_captions_follow_the_same_rule`; tokens carry their `field` |
| Trust across devices follows who decided, not the direction | Céline (was Uncertain 7) | G12 | `set_trust` records a `TrustDecision` (device, time); `RemoteDocument.trust_decision`; `g12_set_trust_records_*`, `g12_a_decision_on_a_paired_device_is_followed_in_both_directions`, `g12_the_later_decision_wins`, `g12_sync_never_lowers_trust_without_a_decision_either`, `g12_a_synced_web_document_arrives_owned_only_with_a_decision` |
| Pasted-token links happen at the save; the editor's `SaveRequest` carrying pasted tokens is a later C-TEXT change | main session (technical) | G14 | unchanged (`g14_a_pasted_token_not_in_the_saved_text_*`) |

## Added 2026-10-15: G20–G23 (duplicate, adoption gestures, outside content)

Written from the decisions of 2026-10-15 (spec v2.0.18, ch. 04) **after** the service was built
and adopted (main 0e95344). Same method: the operations are API stubs (`duplicate`,
`preview_adoption`, `apply_adoption` — `todo!`, plus a `todo!` arm for the new
`Provenance::Copy` in the provenance storage); whoever extends the service makes these pass
without changing `tests/`. One older test changed with the decision:
`g01_import_create_from_markdown_stores_v2` now creates its import **undetermined** (it used the
owned default; G23 refuses an owned import).

| G | Guarantee (ch. 03) | File | Tests |
|---|---|---|---|
| 20 | Duplicate: content, own first version, ingest; tokens stay what they were (linked under G14's conditions only); trust never raised or lowered; provenance *copy of*; source unchanged; trashed / purged / read-only / missing refused; `create` refuses a copy provenance | `g20_duplicate.rs` | 10 |
| 21 | Adoption preview: exactly the candidates now (thread, import folder by path components, selection, dates strictly before — thread, folder and date gestures undetermined only, a selection undetermined and external), the date warning, nothing changes | `g21_adoption_preview.rs` | 6 |
| 22 | Adoption apply: exactly the previewed candidates (never a later arrival), skipped ones reported, own decision + one event each, nothing else changes, whole-or-nothing per document under a crash sweep, never a standing rule, owned untouched, external left fenced by thread/folder gestures and adopted when picked | `g22_adoption_apply.rs` | 10 |
| 23 | Nothing from outside created owned (web, import, a skill's import); outside content lands external or undetermined; written / assistant may be owned | `g23_nothing_from_outside_owned.rs` | 3 |

The corpus for G21–G22 is `tests/common/adoption.rs`.

**Measured 2026-10-15 against the service at main 0e95344 + these stubs** (`cargo test -p
sovereign-docs`; re-measured after Céline's decision on external documents, same day): the 154
tests of G1–G19 pass (including the changed g01 import test); of the 29 new ones, **27 fail and 2
pass**. Failing: all 10 of G20 (`duplicate` is `todo!`; the `create`-refuses-a-copy test reaches
the `todo!` arm for `Provenance::Copy`), all 6 of G21 and all 10 of G22 (`preview_adoption` is
`todo!` — G22 tests stop at their preview), and
`g23_web_and_import_content_cannot_be_created_owned` (the service refuses web + owned but creates
an owned import). Passing, by design: `g23_outside_content_lands_external_or_undetermined` and
`g23_what_the_user_or_the_assistant_writes_may_be_created_owned` — the bound's allowed side,
which the built service already honours; they guard against an implementation that over-refuses.

## Added 2026-10-15: step 4 (sync) — G24–G31 and the 17 C-WIRE guarantees

Written from spec ch. 05, *Sync wire — contract (C-WIRE)* (all five questions answered by Céline
on 2026-10-15) and ch. 03, *What sync needs from the service* (G24–G31), before step 4 is built.
API stubs (`todo!`): `sovereign-docs/src/wire.rs` (`apply_wire`, `stored_state(s)`,
`purge_markers`, `apply_purge_marker`, `restore_backup`, `raise_alert`/`alerts`/`dismiss_alert`,
`normalized_hash`, `purge_fingerprint`, `Stamp`, `WireMeta`, …); `sovereign-p2p/src/wire.rs`
(`SyncService::with_documents`/`with_analysis`/`apply_purge_markers`, `plan_documents`,
`restorable_tables`, `restore_backup`, `IncomingAnalysis`, `WIRE_VERSION`), plus new fields with
serde defaults on `SyncManifest` (`wire_version`, `document_wire`, `purge_markers`) and
`EncryptedCommit` (`sealed_wire`, `wire_nonce`) — nothing fills them yet;
`sovereign-ai/src/peer_audit.rs` (`is_alert`). No existing type changed shape apart from those
added fields; every existing constructor still compiles (the two `EncryptedCommit` literals in
`sync_service.rs` and one each in two in-file unit tests gained the two empty fields).

| Guarantee | Where | File | Tests |
|---|---|---|---|
| G24 fields kept; missing never overwrite; safe defaults (= C-WIRE 2) | docs | `g24_wire_fields.rs` | 4 |
| G25 stamps, not clocks: monotonic, received stamps advance the clock, 5-min bound, fast-forward, older ignored | docs | `g25_stamps.rs` | 5 |
| G26 concurrent edits keep both; two devices converge with both versions | docs | `g26_concurrent_edits.rs` | 3 |
| G27 purge markers by fingerprint: same version purged anywhere, differing kept and comes back, purged version never back, unknown doc records the marker | docs | `g27_purge_markers.rs` | 5 |
| G28 normalised hash and fingerprint | docs | `g28_normalised_hash.rs` | 6 |
| G29 stored state for sending, round trip, trash in, purged out | docs | `g29_stored_state_export.rs` | 3 |
| G30 backup only onto an empty workspace; one version; markers recorded | docs | `g30_backup_restore.rs` | 3 |
| G31 no review flag; alerts listed until dismissed; dismissing changes nothing | docs | `g31_alerts_not_reviews.rs` | 3 |
| C-WIRE 1, 3, 4 (order and skew), 5, 6, 7, 8, 9 | p2p | `wire_documents.rs` | 9 |
| C-WIRE 10 | p2p | `wire_purges.rs` | 3 |
| C-WIRE 11 (source scan), 12 (older build), 15 (restorable tables) | p2p | `wire_gate_old_build_rows.rs` | 3 |
| C-WIRE 13, 14, 16, 17 | p2p | `wire_alerts_versions_clear_backup.rs` | 4 |
| C-WIRE 13 (the alert decision on a verdict) | ai | `sovereign-ai/tests/wire13_alert_decision.rs` | 4 |
| C-WIRE 2 | docs | `g24_wire_fields.rs` (lives in the service) | — |

**The p2p seam.** Two devices, each with its own encrypted in-memory store, document service,
sync service, identity key and fake clock, paired with the account's pair key
(`sovereign-p2p/tests/wire_common`). A round runs what `node.rs` runs between two connected nodes —
manifests, the document plan, envelopes both ways, purge markers — over the **real sealed, signed
envelopes**, handed from one sync service to the other **without the network**. The node's own
request chaining is not exercised (see *Not tested here*).

**Changed existing tests (deliberately).** In `g19_sync_apply.rs`, by C-WIRE 13 (Céline,
2026-10-15: no review flag on changes from her own devices):
`g19_an_overwrite_versions_the_local_state_and_flags_it_for_review` became
`g19_an_overwrite_versions_the_local_state_and_raises_no_review_flag`, its two assertions that the
flag **is** set now assert it **is not**; `g19_a_tombstone_moves_the_document_to_the_trash_restorably`:
the same inversion of one assertion. The file's header quote follows the amended G19. Both fail
against the current service, which still sets the flag. Also: `tests/common/mod.rs` gained a
device name per stack (`Stack::with_device`, `Stack.device`) and the `wire` helpers module; no
existing test's behaviour changed by it.

**Measured 2026-10-15** (`CARGO_TARGET_DIR=C:/cargo-target-contract`, `SOVEREIGN_DATA_DIR` set to a
temp dir), against the service at c090bd2 plus these stubs:
- `sovereign-docs`: G1–G23 — **180 pass, 2 fail** (the two changed g19 tests); G24–G31 — **0 of 32
  pass**, every one on a `todo!` in `src/wire.rs`; the crate's 3 unit tests pass.
- `sovereign-p2p`: the existing suites pass (145 unit tests, e2e backup 3, guardian enroll 3,
  pairing 2, sync 2); the C-WIRE suites — **0 of 19 pass**: 17 on the `with_documents` /
  `plan_documents` / `restorable_tables` stubs, and `wire11_sync_makes_no_document_write_of_its_own`
  on what it is meant to find — nine document writes in `sync_service.rs` (lines 562–656) and one in
  `backup.rs:111`.
- `sovereign-ai` (`--no-default-features`): `wire13_alert_decision` — **0 of 4 pass**, on the
  `is_alert` stub; the preconditions on the existing heuristic (`injection_detected`,
  `wholesale_rewrite`) already hold.

## Step 5 (delete, trash, purge in every host)

The tests of C-DOC adoption step 5 (S5-1 to S5-7, S5-9's Tauri commands), across
`sovereign-docs`, `sovereign-shell` and `sovereign-app`, are listed in `TESTS-step5.md`, with
what each fails on before step 5 is built.

## Guarantees → tests (154 tests up to G19; 183 with G20–G23)

| G | Guarantee (ch. 03) | File | Tests | Also covered in |
|---|---|---|---|---|
| 1 | One form is written (v2) on every write path; invalid content refused, nothing stored | `g01_one_form_written.rs` | 11 | g09 legacy restore, g19 legacy peer |
| 2 | Every stored form is read; non-empty never reads empty; newer/unreadable read-only, never rewritten; reading writes nothing | `g02_every_form_read.rs` | 12 (2 on the format directly) | g19 newer peer |
| 3 | Migration: prior state versioned first; deterministic across devices; metadata and timeline kept; no ingest; trashed migrated; read-only skipped; idempotent | `g03_migration.rs` | 11 | g18 migration hashes |
| 4 | Never overwrite without a restore point; whole-or-nothing writes under crash; failed create leaves nothing listed | `g04_restore_points.rs` | 8 (3 crash sweeps) | g09, g19 |
| 5 | A version from creation | `g05_initial_version.rs` | 4 | g19 new peer doc |
| 6 | Versions by amount of change (policy threshold), never time; explicit; end of session; non-editor always; no repeats; the measure's properties | `g06_commit_policy.rs` | 14 | g04 end of session |
| 7 | Skill output versioned before and after; text-only writes never touch other blocks; ambiguous plain text refused | `g07_skill_output.rs` | 8 | g01, g04 |
| 8 | Only MAC'd commits; invalid MAC listed and never restored; legacy listed unsigned | `g08_macd_commits.rs` | 3 | — |
| 9 | Restore version: new version, current kept, trust/provenance/thread kept, refusals | `g09_restore_version.rs` | 8 | g04, g13 |
| 10 | PII ingest on every text write, every time; every text field (title, text blocks, captions, alt) and nothing else; failure stores as given, unscanned; rogue answer refused | `g10_pii_ingest.rs` | 9 | g03 (no ingest on migration), g11 (none on identical) |
| 11 | Stale revision refused; one of concurrent saves lands; identical save is a no-op | `g11_concurrency.rs` | 6 | g07 stale skill |
| 12 | Trust and provenance set at creation; trust changed only by a user decision, here or received from a paired device (either direction, later wins); sync without a decision changes nothing; web not owned without a decision; legacy rows | `g12_trust_provenance.rs` | 15 | g03, g09 |
| 13 | Trash keeps and hides; restore exact; purge removes document, versions, key, edges, suggestions, PII sources, orphan discovered records; vault kept; contentless marker; sticks against sync; interrupted purge completes | `g13_trash_and_purge.rs` | 12 (1 crash sweep) | g09, g16 |
| 14 | Token links only from an editor's Sovereign paste, from a linked source, present in the text | `g14_token_links.rs` | 7 | — |
| 15 | Resolver rule: token where linked; placeholder for a deleted record there; ordinary text elsewhere; same in titles and captions | `g15_resolver_rule.rs` | 5 | g13, g14 |
| 16 | One event per completed change, after it is stored; none otherwise | `g16_events.rs` | 4 | g03, g09, g11, g12, g19 |
| 17 | Typed view: blocks, plain text, labelled text; lists without content; Debug never prints text | `g17_typed_view.rs` | 5 | g02, g15 |
| 18 | Media hashes filled from readable files, never changed once present | `g18_media_hashes.rs` | 4 | — |
| 19 | Sync applies without losing anything: prior versioned + flagged; tombstone → trash; revive; purged ignored; legacy → v2; newer verbatim | `g19_sync_apply.rs` | 8 | g12, g13, g16 |

Adversarial cases the brief asked for, and where they are:
crash between commit and write → `g04_a_crash_anywhere_in_a_save_*`, `*_create_*`, `*_skill_output_*` ·
save of identical content → `g11_an_identical_save_*`, `g19_an_identical_peer_state_*` ·
migration of each legacy form (envelope with/without media, raw, empty, invalid JSON, v3,
read-only) → `g02_*`, `g03_*` · two saves racing → `g11_of_concurrent_saves_*` · purge leaving
nothing (tables enumerated: document row, commits, document key, `related_to` both ways,
`suggested_link` both ways and every status, `pii_record` sources, orphan records, vault
entries, service lists) → `g13_*` (`assert_nothing_left`) · restore after purge →
`g13_a_purged_document_is_never_restored_*`, `g09_restore_is_refused_*` · trash with suggested
links and PII shared with another document → `g13_trash_hides_*`,
`g13_trashed_documents_leave_the_pending_suggestions_*` · PII ingest skipped on one path →
`g10_every_write_path_calls_the_hook_*` · token pasted A → B → `g14_a_paste_*` · same token
typed into B → `g14_the_same_token_typed_*` · trust not changed by a save →
`g12_no_write_changes_trust_*` · sync applying a tombstone → `g19_a_tombstone_*`.

## Counts against the stubs

154 tests compile (`cargo test -p sovereign-docs --no-run --locked`, no warnings). Run once
against the `todo!` stubs on 2026-10-11: **0 passed, 154 failed**, and all 154 panics read
`document service not built yet` (2026-10-10, before the amendment: 0 passed, 143 failed).
Every test reaches a `todo!`: the stack itself is real, but `DocumentService::new` is a stub,
and the tests that call no service (`g02_format_*`, `g06_default_policy_honours_*`,
`g06_the_measure_*`, `g12_undetermined_is_fenced_*`) call stubs of the format, the policy,
`change_amount` and `Trust::is_fenced`. A test that passed against the stubs would be testing
nothing.

## Not tested here, and why

- **Encryption at rest** of what the service writes: the encrypted layer's own suite
  (`sovereign-db/tests/encrypted_over_surreal.rs`) covers it; the service holds the store only
  through that layer (G8, by type).
- **Real personal-data detection**: C-PII. Only the hook contract is tested (calls, what is
  stored, failure handling). Whether a real hook writes sources with correct spans is C-PII's
  suite.
- **Real file hashing**: `MediaFiles` is a fake; computing SHA-256 of a file is the host
  implementation's, tested there.
- **Who wins a sync** (LWW, manifest, wire fields, signatures): ch. 05 / C-WIRE. The service
  only applies a decided state.
- **Persona isolation**: C-SESS. The service is built per persona with that persona's store.
- **Performance**: the contract states no figure (e.g. migrating 50,000 documents, saving a
  1 MB document). Worth adding once figures are decided.
- **That callers no longer parse `content`**: an adoption property, not a service property.
  Suggested for CI once adopted: a grep that fails on `ContentFields::parse` or `.content`
  of a `Document` outside `sovereign-docs` / `sovereign-db`.
- **Key database persistence across a crash**: the crash tests stop store calls, not the key
  database file; a key minted in memory and never saved is ch. 07's question.
- **Legacy commit cut-off**: undecided (backlog); tests only require unsigned versions to be
  *listed as such*, not restored or refused.
- **Thread moves, position, pinning**: out of the contract (C-LANE).
- **The trash view, history window, any UI**: shell and mobile.
- **Defaults at creation** (ch. 04, 2026-10-15: owned when written from scratch inside
  Sovereign — a new document, a journal entry, what the assistant writes at her request;
  undetermined for imports and a skill's external file; external for the web): **host policy**,
  not the service's. The creator passes the trust in `NewDocument.trust`; the service cannot
  tell "written from scratch" from a request. What the service enforces is the bound — nothing
  from outside is created owned (G23) — and the copy rule (G20). The defaults belong in each
  host's tests (`sovereign-ai/src/docs_host.rs`, the import crate, the Tauri commands).
- **How the preview is shown** (count, list, the date warning's wording): the host's; the
  service returns the list and the warning.
- **Step 4 — the network and the node** (C-WIRE): the p2p tests hand sealed envelopes from one
  sync service to the other; `node.rs`'s request chaining, mDNS, relays and the
  `SyncConflict`/notice events are not exercised. The old-build notice on both screens and the
  shell/mobile alert surfaces are UI.
- **Step 4 — rows**: conversation-title and contact-address convergence (no store setter, weakness
  24); signed Lamport row sync is covered by the existing p2p suites.
- **Step 4 — export fallback**: Céline's fallback for a backup next to existing data depends on the
  general export, decided 2026-08-04 and not built.

## Uncertain — interpretations chosen, for Céline

1. **Restore-point scope** (G4): "never overwrite without a restore point" read as *across
   writers*: saves of one editing session may replace each other between versions (else every
   2-second autosave is a version). Intermediate states of a session survive only in the
   editor's undo, in memory.
2. **A title-only save of a legacy document converts its content to v2** (G1): a write is a
   write; spec ch. 03 says "converted at its next write".
3. ~~Hook failure stores the content, marked not scanned~~ — **decided by Céline 2026-10-11**,
   as chosen (G10).
4. **A stale-but-identical save is a conflict, not a no-op** (`g11_an_identical_save_from_a_stale_*`).
5. **Web content cannot be created owned** (G12); the user adopts it afterwards with
   `set_trust`.
6. **A peer that sends no trust lands undetermined, provenance *paired device*** (G12).
7. ~~Sync never raises trust to owned; lowering undecided~~ — **decided by Céline 2026-10-11:
   trust across devices follows who decided, not the direction** (G12). What remains a choice:
   - **The later decision wins, by its recorded time** (`g12_the_later_decision_wins`). Two
     opposite decisions made on two devices while apart: the one made later stands, wherever
     it arrives. Equal times: not specified and not tested (a tie-break by device name would
     do).
   - **A web document arriving by sync owned but without a decision** lands not owned (which of
     external or undetermined is not asserted) — creation rules hold over sync.
   - **Setting the trust a document already has records no new decision**
     (`g12_set_trust_records_*`).
8. **Restore does not carry trust back** with the version (G9).
9. **Migration computes media hashes** (it is a write, G18) but runs no ingest (G3: the text did
   not change).
10. **Purge removes a discovered record left with no source**, hard (not soft-deleted); a vault
    entry stays with its source list emptied (G13).
11. **Plain text on a document with no text block adds a text block at the start** (G7).
12. **`plain_text` joins text blocks with one blank line** (`"\n\n"`).
13. **A trashed document is readable** (`trashed_at` set) rather than an `InTrash` error;
    writes to it are refused.
14. **Thresholds are inclusive**: a change equal to the policy's threshold makes a version;
    exactly 30 days in the trash purges.
15. **Event kinds**: a skill's output emits `Saved`; a document created by sync emits
    `Created`; trashing by a peer tombstone emits `RemoteApplied` or `Trashed` — only the
    count and the id are tested there, not the kind.
16. **Token labels** are not asserted (only `Token { .. }`); ch. 09 gives `[Email]` as an
    example, not a rule.
17. **Sync of a document trashed locally with a live remote state** revives it (ch. 05,
    "a live winning state restores a locally deleted record").
18. **Per-item purge from the trash** exists (`purge_from_trash`), refused unless trashed —
    read as consistent with "no separate permanent delete".
19. **The measure of change** (G6, "to be evaluated"): a distance between the last version's
    state and the saved state, not accumulated keystrokes — so typing and erasing the same words
    makes no version; characters outside the common prefix and suffix per text block and title;
    a fixed weight (`NON_TEXT_BLOCK_WEIGHT`, to calibrate) per non-text block added, removed or
    changed, and once for a reorder. The tests assert only its properties (zero exactly for equal
    states, symmetric, never shrinking as a change grows) and that the service decides with it;
    any other measure with those properties passes them.
20. **The threshold's default** (`DefaultCommitPolicy::DEFAULT_THRESHOLD = 500`, about a short
    paragraph) is a placeholder **to calibrate**; no test depends on its value.
21. **The hook receives every text field on every ingesting write**, not only the fields that
    changed — so a field can never escape ingest (G10). Cost: see weakness 14.

*Added 2026-10-15 (G20–G23):*

22. **A copy's provenance is `Copy { of, from }`** — the source id and the source's provenance as
    it was (nested for a copy of a copy). The alternative, the source's provenance unchanged (what
    the step-3 host helper did), loses that the document is a copy. `from` keeps a copied web page
    saying where it came from.
23. **A copy has the source's trust, including an adopted web page's owned** (her decision made it
    hers). The step-3 helper turned that case into undetermined only because `create` refuses
    *web + owned*; with a *copy of* provenance the bound of G23 does not apply. A copy records **no
    trust decision of its own** (not asserted either way; on a paired device it is created with the
    sender's trust, as any synced document).
24. **Placeholders are copied as placeholders**: a soft-deleted record linked to the source and
    present in its text is linked to the copy too, so its token shows as a placeholder there, as in
    the source. Linking only live records would turn those tokens into raw `[pii:…]` text in the
    copy.
25. **Duplicate's ingest runs as a create** (`WritePath::Create`; a new path variant would change a
    public enum other crates match on). Only "a request for the copy" is asserted.
26. **A read-only source is refused** (newer format, unreadable), as the step-3 helper did — a
    newer-format document could be copied byte for byte, but the copy would be a document this
    version wrote and cannot read.
27. ~~Candidates for the thread and folder gestures are undetermined and external~~ — **decided
    by Céline 2026-10-15: thread, import-folder and date gestures take undetermined documents
    only**; an explicit selection of ids takes undetermined and external (an external document is
    adopted only when she picks it). Tests: `g21_a_thread_gesture_lists_its_undetermined_documents_only`,
    `g21_a_folder_gesture_leaves_out_an_external_document_of_that_folder`,
    `g22_thread_and_folder_gestures_leave_external_documents_fenced`,
    `g22_an_explicit_selection_including_an_external_document_adopts_it`.
28. **Import folder matching**: path components after `\` → `/` and dropping a trailing separator;
    **case-sensitive**; prefix by components (`/in/corpus` ≠ `/in/corpusX`). Windows paths that
    differ only in case or drive-letter case would not match. Non-path import provenances
    (`command line`, `sample data`, `skill:<name>`, `shared from another app`) match no folder.
    Copies of imported documents are not part of the import.
29. **Dates are strictly before** the chosen instant; created and modified are the document's
    record fields (an import's original file times).
30. **Apply uses the preview's selection, never recomputes** (`g22_a_document_arriving_after_the_preview_*`).
    A previewed document that stopped being a candidate is skipped with a reason (already owned,
    trash, purged, gone); one that the user lowered between preview and apply *is* adopted (the
    gesture is her later decision).
31. **Adoption makes no version** — trust is not content; only "content, title, thread, times and
    provenance unchanged" is asserted, not the version count.
32. **All documents of one apply share one decision time** (the clock at the apply).
33. **A bulk gesture cannot lower** — lowering stays per document (`set_trust`); nothing found
    that needs a bulk lowering.

*Added 2026-10-15 (step 4 tests):*

34. **5 minutes** is the stamp bound (C-WIRE [choice]); tests use 4 and 6 minutes around it
    (`MAX_STAMP_AHEAD_SECS ± 60 s`), and 30 minutes at the p2p level.
35. **Every write is stamped** — saves, the assistant, trash, restore from trash, trust decisions
    (`g25_every_local_write_gets_a_greater_stamp_*`). A trust decision's stamp is reported
    separately (`meta.decision_stamp`).
36. **"Concurrent" means the remote's previous stamp is not the local stamp while the local state
    changed since.** A remote state equal to the local one is never concurrent (G19's no-op).
37. **The lower concurrent state is stored as a version with the peer's title**, so History shows
    which edit it was (`g26_..._kept_as_a_version_only` checks the title).
38. **A purge marker for a copy that was adopted (trust changed) still purges it** — the
    fingerprint ignores trust (`g27_an_identical_copy_is_purged_wherever_it_sits`, case
    "adopted").
39. **The backup carries purge markers** (G30); `build_snapshot` does not yet (no field) — the p2p
    backup test does not check markers, the service test does.
40. **Alerts are one per document** (`dismiss_alert(id)` takes a document id); a second detection on
    a document with an open alert — replaces it or adds one? Not tested.
41. **The device name recorded on decisions and stamps is the sync identity** (the peer id string)
    in the p2p tests; the service takes whatever the host gives (`ServiceDeps.device`).

## Contract weaknesses found while writing the tests

1. **Purge versus sync: the tombstone cannot simply disappear.** The brief listed "sync
   tombstone" among what purge removes. If nothing remains, a paired device still holding the
   document live sends it back (ch. 05) and the purge is undone. The contract keeps a
   **purge marker without content** instead — **decided by Céline 2026-10-11**. Still open, in
   C-WIRE: where it is stored (no table has it), how it travels, what the peer does with it
   (trash? purge?). And it remembers forever that *some* document with that id existed — a
   contentless but real residue.
2. **Migration must be deterministic, and the editor's provisional model is not.** It gives
   migrated blocks random ids. Two devices migrating the same legacy document would write
   different bytes, hash differently, and — with the timeline position deliberately kept — tie
   on modification time: a sync conflict for every migrated document. The format API adds
   `BlockId::derived(seed, index)` and `DocumentV2::read_seeded`.
3. **A v1-writing peer and a v2 device never converge.** Whatever one stores, the other's hash
   differs; equal times give endless conflicts. Shell and APK have to move to v2 together (or
   sync must compare a normalised form). Not solvable inside C-DOC.
4. **`SourceRef` spans are byte offsets into a flat body**; v2 has blocks, and now titles,
   captions and alt texts hold tokens too. The offsets are meaningless after the first edit. The
   tests use `source_id` only. C-PII should redefine a source as (document, field, offset) — the
   field being the API's `TextField` — or drop spans.
5. **The editor links a pasted token at the paste, the contract at the save.** C-TEXT's
   `PiiDisplay::link_to_document` is called when the paste happens; ch. 09 and G14 link when
   the target is saved — a paste deleted before saving must link nothing
   (`g14_a_pasted_token_not_in_the_saved_text_*`). The editor's `SaveRequest` should carry the
   pasted tokens (with their source document) and the host hold display-only links until then.
   **Decided (main session, 2026-10-11):** links happen at the save, as G14 says; the editor's
   `SaveRequest` carrying pasted tokens is a later C-TEXT change.
6. ~~Titles, captions and alt texts are not ingested~~ — **decided by Céline 2026-10-11: they
   are** (G10). What it leaves open: `SourceRef` has no notion of a field (title, caption), so a
   record found in a title has a source reference that points nowhere precise — see weakness 4.
7. **Trust tri-state, provenance and the purge marker have no storage** (`is_owned: bool`, no
   provenance field, no marker table). The implementer must extend `sovereign-db` — and
   regenerate `tests/common/faulty.rs` (see below).
8. **Whole-or-nothing writes need the store's help.** `EncryptedGraphDB::update_document` writes
   content, then the time, then the title, in separate calls; a crash in between leaves new
   content under the old title (`g04_a_crash_anywhere_in_a_save_*` will catch it). Same root as
   DB-M7 (no transactions). `create_document` inserts a blanked row first, so a crash leaves an
   empty row the service must hide (`g04_a_crash_anywhere_in_a_create_*`).
9. **The editor has no notion of a refused save.** C-TEXT knows "a failed save keeps it and says
   so" and "remote change"; G11 adds `Conflict` (another writer — a skill, the assistant —
   saved first). The host has to turn a conflict into the remote-change flow.
10. ~~Duplicating a document breaks its tokens~~ — **closed 2026-10-15 by G20** (Céline:
    duplicate becomes a document-service operation that links the copy under G14's conditions).
11. **"Keep mine" on a sync conflict** (mobile, `pairing.rs:563`) is a write that changes only
    the modification time — G11 makes an identical save a no-op. Needs its own verb (C-WIRE).
12. **Backup restore** (`sovereign-p2p/src/backup.rs`) inserts whole rows, including preserved
    raw bodies; neither `create` nor `apply_remote` covers it (trust? ingest? versions?). Open.
13. ~~Hook failure versus "before content settles"~~ — **decided by Céline 2026-10-11**:
    nothing typed is lost; the document is marked not scanned.
14. **Ingesting every text field on every autosave is costly.** With saves every couple of
    seconds and NER at ingest (C-PII), the whole document goes through the pipeline each time.
    The contract permits a hook that remembers what it already scanned; C-PII has to make that
    cheap, or ingest has to move to version boundaries — which would let unscanned text sit
    between versions (masked at read time, ch. 09).
15. **Trust decisions are ordered by device clocks.** "The later decision wins" uses the time
    each device recorded; a device whose clock runs ahead wins every disagreement. Sync already
    bounds future skew for snapshots (ch. 05); the decision's time needs the same bound, or a
    Lamport stamp like the row tables use. C-WIRE.
16. **The measure is an upper bound for scattered edits.** Characters outside the common prefix
    and suffix count everything between the first and last edit in a block, so two one-letter
    fixes at the two ends of a long paragraph measure as the whole paragraph. Harmless for a
    threshold (it errs toward a version), but it is not "characters inserted + deleted". A real
    diff is the alternative, at a cost per save.
17. **No version by time means a long session can leave its states only in memory.** With a high
    threshold and no Ctrl+S, the restore point is the end of the session; a crash of the shell
    loses the editor's in-memory undo but not the saved state (each autosave is stored), so what
    is lost is only the history between versions — consistent with the decision, stated so it is
    known.

*Found 2026-10-15, writing G20–G23:*

18. **A copy's provenance has no wire form.** `Provenance::Copy` nests a provenance; storage
    today is a kind and one detail string, and sync carries provenance by C-WIRE's added fields.
    Until both carry it, a copy synced to a paired device arrives with some other provenance.
    And on the peer, G12's creation rule (web never created owned without a decision) does not
    see a `Copy { from: Web }` as web: an owned copy of an adopted page lands owned there —
    consistent with G20, but it is the one way content of web origin is created owned without a
    decision record on the receiving device.
19. **The skill provenance gap.** Step 3 records what a skill writes as `Import { "skill:<name>" }`
    because the contract has no skill provenance; G23 now makes it undetermined by force (an
    import cannot be owned), which matches ch. 04 for a skill reading an external file but is
    wrong for a skill that writes from the user's own text (a journal entry: ch. 04 says owned).
    A `Provenance::Skill { name }` would let the host say which; not added here.
20. **Import folders depend on how hosts write the path.** The provenance detail is free text
    (absolute path from the CLI, canonical path from the Tauri import, `shared from another app`);
    folder matching is only as good as the consistency of those strings, and is case-sensitive
    (Uncertain 28). A per-import identifier recorded at import would name "one import" exactly
    (ch. 04 says *documents whose provenance names that import's folder*, which is what is
    specified); the bulk import has no import id today.
21. **A preview is a list the caller holds.** `apply_adoption` takes the preview back; a caller
    could build a preview by hand with any ids. The apply re-checks candidacy, so the worst case
    is adopting non-owned documents the user did not see — the same power `set_trust` already
    has. Binding a preview to an id the service issued would close it.
22. **Previews do not expire.** A preview applied a week later adopts what it listed then (still
    candidates). The ch. 04 rule is satisfied (selected at the preview, never a standing rule),
    but the user's look at the list is a week old; a host may want a time limit.
23. **Duplicate and the pending raw body** (step 3's workaround for `body_raw_*` on create)
    apply to copies too; the hook still cannot store a raw body with the row. Same open item as
    the create path.

*Found 2026-10-15, writing the step-4 tests:*

24. **Two row semantics of C-WIRE 15 cannot be expressed through the store API.** A contact's
    addresses can only be added (`add_contact_address`), never removed or replaced, and no
    `GraphDB` method changes a conversation's title. Their convergence over sync is untestable until
    the store has those setters; only the restorable-table list is tested.
25. **Stamps for rows written before step 4.** Legacy rows (and every document written by builds
    before step 4) have no stamp. The tests assume the service gives such a row a stamp on read
    (`stored_state(...).meta.stamp` is `Some` for every state, `current_stamp`) without writing it;
    the contract does not say how two never-stamped copies are ordered (C-WIRE 3 makes them equal
    when their content is equal; when it differs, only the wall clock is left).
26. **Ingest on a create from the wire.** C-WIRE 5 needs the receiver's tokenized text to travel
    back; the outcome flag `ingest_changed_text` says so, but who sends it back (the next round's
    plan, by stamp) is left to the sync layer. A receiver whose ingest changes the text must stamp
    that change as its own write, or the two devices disagree with equal stamps.
27. **Three or more devices.** All the two-device tests pass a "concurrent" verdict through
    `prev_stamp`; with a relaying third device a previous stamp can be unknown locally (the
    draft's accepted false alarm). Not tested; a three-device test belongs with the version-vector
    question if it is ever reopened.
28. **The alert surface is the host's.** The service stores and lists alerts; the status-bar notice,
    the *Alerts* window and the mobile banner are UI, untested here.
29. **The old-build refusal is decided on the manifest, and on envelopes without a wire state.** A
    peer that sends a wire version but an envelope without `sealed_wire` (a bug, or a downgrade
    attempt) is treated as an older build for that envelope (refused). Stated in the test, not in
    the spec.
30. **The gate test's first version was blind** — it cut `sync_service.rs` at a `#[cfg(test)]` on a
    single import (line 10) and found none of the nine writes it was written to find; it now ends
    production code only at a `#[cfg(test)]` that opens a module. Kept as a record: a gate that
    passes against the code it exists to catch is the first thing to suspect.

## Changing tests/

Nothing in `tests/` may be changed by the service's implementer, with **one mechanical
exception**: `tests/common/faulty.rs` is generated from the `GraphDB` trait. If the build adds
methods to `GraphDB` (it will — weakness 7), regenerate it from the repository root:

```
python crates/sovereign-docs/tests/common/gen_faulty.py \
    crates/sovereign-db/src/traits.rs crates/sovereign-docs/tests/common/faulty.rs
```

and commit the regenerated file alone, saying so. Any other change to `tests/` goes back to the
suite's author or to Céline.
