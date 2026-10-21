# rc2 tests — written before the code (red)

Spec: v2.0.46 (`5e7c4e0`), ch. 07 *Auth store* "Created once, never replaced by account
creation"; ch. 12 *Onboarding and the auth gate* "While the account is being created" and
"Text fields of the auth gate and the wizard are readable in both themes". Extended by Céline the
same day (`7506579`): **every** first write of `auth.store` is exclusive (also the desktop's
fresh and paired creation, and the phone's backup recovery), backup recovery joins the same
claim, and the search window's field joins the contrast rule. Whoever implements rc2 makes these
pass **without editing them**.

## 1. `auth.store` is written by an exclusive create — `crates/sovereign-crypto/tests/auth_store_save_new.rs`

Spec (ch. 07): "The first write of `auth.store` refuses an existing file (an exclusive create,
not a replace)"; "a refused request changes no file".

| Test | Asserts |
|---|---|
| `save_new_on_an_absent_path_writes_a_store_that_loads_and_authenticates` | `save_new` on an absent path writes; `AuthStore::load` reads it back; both personas authenticate; same KEK |
| `save_new_on_an_existing_file_errors_and_leaves_it_byte_identical` | on an existing store: `Err`, bytes unchanged, no new file in the directory (no temp left), the first account still opens |
| `save_new_refuses_any_existing_file_not_only_a_store` | an existing non-store file is not overwritten either |

Red: `error[E0599]: no method named `save_new` found for struct `AuthStore`` (4 occurrences).

## 2. Single-flight claim — `crates/sovereign-app/src/onboarding_claim_tests.rs`

Registered from `src/lib.rs` (`#[cfg(test)] #[path = "onboarding_claim_tests.rs"] mod onboarding_claim_tests;`)
because `onboarding_claim.rs` does not exist yet. API: `crate::onboarding_claim::{claim, ClaimRefused, OnboardingClaim}`.

Spec (ch. 07): "a request that arrives while one is in progress is refused ("Setup is already in
progress") and one that arrives after one has completed is refused ("Already onboarded — log in
instead"); a refused request changes no file."

| Test | Asserts |
|---|---|
| `the_claim_guard_is_send` | `OnboardingClaim: Send` (held across `.await` in a Tauri async command) |
| `a_second_claim_while_the_first_is_held_is_refused_in_progress` | `Err(InProgress)` |
| `after_the_guard_drops_without_an_auth_store_a_new_claim_succeeds` | a failed onboarding can be retried |
| `an_existing_auth_store_is_refused_already_onboarded` | `crypto/auth.store` present → `Err(AlreadyOnboarded)` |
| `after_a_completed_onboarding_a_new_claim_is_refused_already_onboarded` | claim, write auth.store, drop → `Err(AlreadyOnboarded)` |
| `a_refused_claim_changes_no_file` | both refusals leave every file under the profile byte-identical |
| `concurrent_claims_on_one_fresh_profile_let_exactly_one_through` | 8 threads behind a `Barrier`, guards held until all tried: exactly 1 `Ok`, 7 `InProgress` |
| `claims_on_different_profile_dirs_do_not_block_each_other` | two temp dirs, both `Ok` |
| `an_onboarding_and_a_backup_recovery_on_one_profile_exclude_each_other` | (`7506579`: "one claim excluding the other") a claim held by either flow refuses the other `InProgress` — the claim has no kind, it is keyed by the profile directory |
| `the_refusals_display_the_spec_sentences` | `Display`: "Setup is already in progress", "Already onboarded — log in instead" |

Scope: claims within **one process** only; nothing is asserted about two processes.

Red: `error[E0432]: unresolved import `crate::onboarding_claim``.

## 3. The three phone commands take the claim first and write with `save_new` — `crates/sovereign-app/src/onboarding_order_tests.rs`

Spec (ch. 07): "The claim is taken **before** the key derivation, so the check and the write
cannot be separated by it"; "a refused request changes no file"; "**Every** first write …
refuses an existing file … and the backup recovery that rebuilds an account"; "Account creation
and backup recovery are single-flight on the phone, one claim excluding the other" (`7506579`).

Source scan of `complete_onboarding` (`src/tauri_commands/auth.rs`),
`complete_onboarding_paired` (`src/tauri_commands/pairing.rs`) and `recovery_finalize`
(`src/tauri_commands/recovery.rs`, the backup recovery, ch. 06 Feature 2); bodies by brace
matching, comments blanked, strings and char literals skipped.

| Test | Asserts |
|---|---|
| `complete_onboarding_claims_before_deriving_and_writes_auth_store_exclusively` | see the rules below |
| `complete_onboarding_paired_claims_before_deriving_and_writes_auth_store_exclusively` | same |
| `recovery_finalize_claims_before_deriving_and_writes_auth_store_exclusively` | same (`7506579`) |
| `the_scanner_tells_a_bad_body_from_a_good_one` | the scanner flags bad bodies (derivation before the claim, `.save(` of auth.store, `let _ =`, a claim not on the profile dir, a store bound to another name) and passes a good one |

Rules: (1) `onboarding_claim::claim(` is called on the profile directory (its argument names
`profile_dir`, so onboarding and recovery share one claim), bound to a named guard
(`let _claim = …`, not `let _ =` nor a bare statement), not `drop(…)`ped before auth.store is
written; (2) it comes before the first of `AuthStore::create`, `create_with_imported_account_key(`,
`MasterKey::derive(`, `pair_with_source(`, `passphrase_matches(` (the derivations — the
recovery's passphrase check derives a key) **and** before the first of `std::fs::write(`,
`create_dir_all(`, `.save(`, `.save_new(`, `load_or_create_device_id(` (so a refused request has
written nothing — e.g. the profile in `complete_onboarding`, the salt and device id in the paired
path); (3) auth.store is written with `.save_new(`, never `.save(` (on `auth_store` or any name
bound from `AuthStore::create…`, or with an `auth.store` path).

Not checked: `access_recovery_finalize` (ch. 06 Feature 1) replaces the forgotten `auth.store`
by design (`recovery_store::install_recovered_auth_store`); it is not a first write.

Red: while `onboarding_claim` is missing, the lib test does not compile (the E0432 of §2). With a
throw-away `onboarding_claim` in place, the scan prints, for each of the three commands: "no
call to `onboarding_claim::claim(`", "`auth_store.save(&crypto_dir.join("auth.store"))` replaces
auth.store — the first write must be `.save_new(`" (auth.rs:528, pairing.rs:312,
recovery.rs:325), and "auth.store is not written with `.save_new(`".

## 3b. The desktop's first writes are exclusive — `crates/sovereign-shell/src/auth_store_write_tests.rs`

Registered in `src/main.rs`. Spec (ch. 07, `7506579`): "**Every** first write of `auth.store`
refuses an existing file … on the phone and on the desktop". No claim on the desktop (it creates
on its window thread, ch. 12).

| Test | Asserts |
|---|---|
| `the_desktop_wizard_writes_auth_store_exclusively` | `crypto.rs` `create_auth_store`: auth.store written with `.save_new(`, never `.save(` |
| `the_desktop_paired_onboarding_writes_auth_store_exclusively` | `p2p.rs` `accept_pairing` (imported account key): same |
| `the_scanner_tells_a_replace_from_an_exclusive_create` | the scanner on known bodies (a method chain over two lines included) |

Red: the shell test binary does not compile while `field_fill` is missing (§5). With it present:
"`store.save(&auth_store_path())` replaces auth.store" (crypto.rs `create_auth_store`) and
"`auth_store.save(&crate::crypto::auth_store_path())` replaces auth.store" (p2p.rs
`accept_pairing`), each with "auth.store is not written with `.save_new(`".

## 4. The mobile wizard while the account is being created — `frontend/src/lib/components/OnboardingWizard.svelte.test.ts`

Spec (ch. 12): "a spinner and a line saying what is happening — and every button that could
start or leave account creation ("Get started", "Skip", "Back") is disabled until it ends; if it
fails, the error is shown on the screen, not only in a log."

Backend mocked at `invoke` (`$lib/test/tauri`); `device.isMobile = true`; crypto on.

| Test | Asserts | Red |
|---|---|---|
| first device: `starts one creation however many times "Get Started" is pressed` | `complete_onboarding` called once after two clicks | called 2 times |
| first device: `disables "Get Started", "Back" and "Skip this step" and shows that it is working` | all three disabled; role `status` or "Creating your account" present | "Get Started" not disabled |
| first device: `does not start a second creation from "Skip this step" on the last step` | Skip, Skip, Get Started → one call; buttons locked | called 3 times |
| first device: `shows the error on the screen when creation fails` | the rejection text ("Setup is already in progress") is in the DOM | text not found |
| first device: `gives the buttons back after a failed creation, with the error shown` | locked while pending; after the rejection the error ("Already onboarded — log in instead") is shown and "Get Started", "Back", "Skip this step" are enabled again (settled by the coordinator, 2026-10-19: otherwise the user is stuck) | "Get Started" not disabled while creating |
| paired: `starts one pairing, disables "Get Started" and "Back", and shows that it is working` | one call; Get Started and Back disabled; role `status`, "Creating your account" or the existing "Connecting to …" line | "Back" not disabled |

Also checked green against a throw-away wizard change (reverted).

## 5. Field text contrast in the shell — `crates/sovereign-shell/src/field_contrast_tests.rs`

Registered in `src/main.rs`. Seam: `crate::theme::field_fill(&Palette) -> vello::peniko::Color`.

Spec (ch. 12): "their text is drawn at **4.5:1 or more** against the field's fill (WCAG 1.4.3),
the fill being the palette's input colour."

| Test | Asserts |
|---|---|
| `the_metric_is_wcag_contrast` | the contrast function: 21:1, 1:1, rc1's dark field < 1.5:1 |
| `field_text_reaches_4_5_to_1_in_the_dark_theme` / `…_light_theme` | WCAG contrast of `p.text` on `field_fill(p)` (composited over `p.panel` if translucent) ≥ 4.5 |
| `the_field_fill_is_the_palettes_input_colour` | `field_fill(p) == p.input` (the spec names the colour) |
| `the_wizard_fields_are_filled_with_field_fill` | `onboarding.rs` `draw_fields`: a `scene.fill(…)` uses `field_fill(`; no `on_accent` in the body |
| `the_auth_gate_fields_are_filled_with_field_fill` | `panels.rs` `draw_auth`: same |
| `the_search_field_is_filled_with_field_fill` | `panels.rs` `draw_search`: same (`7506579`: "the search window … The search box had the same fill") |

Red: `error[E0432]: unresolved import `crate::theme::field_fill``. With `field_fill` present and
the sites unchanged, the three scans print "no `scene.fill(…)` uses `field_fill(`" and "uses
`on_accent` … `scene.fill(Fill::NonZero, Affine::IDENTITY, pal().on_accent, None, &frr);`"
(`&input_rr` for `draw_search`).

## 6. The laptop's second position on `2e8e8ff` (Q1–Q3)

Source: claude-coord `from-laptop/0108-rc2-second-position.md`; contracts settled by the
coordinator (2026-10-19). Written against the implementer's `2e8e8ff`.

### Q1 — how `save_new` published

`crates/sovereign-crypto/tests/write_private_new_mode.rs`. Contract: `pub enum NewWrite { Linked,
InPlace }` in `sovereign_crypto::fs_private` (`Debug, Clone, Copy, PartialEq, Eq`);
`write_private_new -> io::Result<NewWrite>`; `AuthStore::save_new -> CryptoResult<NewWrite>`.

| Test | Asserts |
|---|---|
| `new_write_has_the_two_modes_and_its_traits` | the derives; `{:?}` gives "Linked" / "InPlace" |
| `a_fresh_write_private_new_reports_linked_where_hard_links_work` | a fresh write on NTFS / ext4 returns `Linked` (would fail on a filesystem without hard links: the mode is the fact) |
| `a_refused_write_private_new_reports_no_mode` | an existing file → `Err(AlreadyExists)`, file unchanged |
| `save_new_reports_the_mode_it_published_with` | `save_new` returns `Linked` |

`auth_store_save_new.rs` is unchanged: none of its tests compares with `Ok(())`.

Red: `error[E0432]: unresolved import 'sovereign_crypto::fs_private::NewWrite'`.

`crates/sovereign-app/src/rc2_second_position_tests.rs`,
`q1_every_save_new_in_the_phone_commands_logs_its_mode`: every `.save_new(` in
`tauri_commands/{auth,pairing,recovery}.rs` is bound (`let mode = ….save_new(…)…?;`) and the
name is then logged by a `tracing` macro with `?` formatting (`tracing::info!("… {mode:?}")`).
Red: "the mode `save_new` returns is not bound to a name" at auth.rs:533, pairing.rs:313,
recovery.rs:331.

### Q2 — the salt file

`crates/sovereign-app/src/salt_restore_tests.rs`, registered from `setup.rs` (a child module:
`load_or_create_salt(dir)` is private; `dir` is the crypto directory, signature unchanged).

| Test | Asserts | Red |
|---|---|---|
| `a_missing_salt_beside_an_auth_store_is_restored_from_the_store` | store present, salt missing → returns `AuthStore::load(..).salt` (a `pub` field) and the salt file equals it | "the returned salt must be the account's (auth.store's), not a new one" (a new salt is minted) |
| `with_neither_salt_nor_store_a_fresh_salt_is_minted_and_kept` | 32 random bytes, written, read back the same; no auth.store created | passes today (guard) |
| `an_existing_salt_file_is_read_as_is` | an existing salt file is returned unchanged | passes today (guard) |

`rc2_second_position_tests.rs`, `q2_the_salt_is_written_with_write_private_not_std_fs_write`: in
`tauri_commands/{auth,pairing,recovery}.rs` and the shell's `p2p.rs` the salt is written with
`write_private(` and no `fs::write(` names the salt; `setup.rs` and the shell's `crypto.rs` have
no `fs::write(` of the salt either. Red: "`fs::write(crypto_dir.join("salt"), …)` writes the salt"
at auth.rs:541, pairing.rs:315, recovery.rs:334, shell p2p.rs:700, each with "the salt is not
written with `write_private(`". `the_q1_q2_scanners_tell_bad_from_good` checks both scanners.

The shell's half — `crates/sovereign-shell/src/salt_restore_tests.rs`, registered in `src/main.rs`.
Seam settled by the coordinator (2026-10-19): `crate::crypto::load_or_create_salt_in(crypto_dir:
&Path) -> anyhow::Result<Vec<u8>>`, same rule as the app; `load_or_create_salt()` becomes a
wrapper over `crypto_dir()`.

| Test | Asserts |
|---|---|
| `a_missing_salt_beside_an_auth_store_is_restored_from_the_store` | store present, salt missing → the store's salt, returned and written back |
| `with_neither_salt_nor_store_a_fresh_salt_is_minted_and_kept` | as today: 64 hex characters (32 random bytes as hex), written, read back the same; no auth.store created |
| `an_existing_salt_file_is_read_as_is` | an existing salt file is returned unchanged |
| `load_or_create_salt_goes_through_the_seam` | source scan of `crypto.rs`: `load_or_create_salt` calls `load_or_create_salt_in(`; the seam writes with `write_private(`, not `fs::write(` |

Red: `error[E0432]: unresolved import 'crate::crypto::load_or_create_salt_in'` (the shell test
binary does not compile).

### Q3 — backup recovery vs Argon2id accounts

`crates/sovereign-p2p/tests/backup_recovery_argon2.rs`.

| Test | Asserts | Result on `2e8e8ff` |
|---|---|---|
| `control_the_harness_reaches_the_tag_comparison` | the recovery state built (manifest + salt) reaches the comparison and refuses a wrong passphrase — neutral on the KDF | passes |
| `backup_recovery_recognises_the_passphrase_of_an_account_made_by_onboarding` (`#[ignore = "backup recovery vs Argon2id accounts: decision pending (Céline), rc2 log"]`) | account by `AuthStore::create`, tag from the session's `account_key.derive_backup_tag()`, `passphrase_matches(right passphrase)` → `true` | **FAILED** with `-- --ignored`: `passphrase_matches` returns `false` for the right passphrase |

Cost: one `AuthStore::create` + one authentication at the only KDF parameters the crate exposes
(Argon2id 64 MiB, t=3): about 12 s in this debug build.

## How to run (Windows, warm `C:/cargo-target`)

- `cargo test --workspace --test auth_store_save_new --locked --target-dir C:/cargo-target -j 4`
  — `-p sovereign-crypto` alone selects a feature set the warm target has not built, and Smart
  App Control blocked the fresh `num-traits` build script (os error 4551); `--workspace` unifies
  features as the warm builds did.
- `cargo test --workspace --test write_private_new_mode --locked --target-dir C:/cargo-target -j 4`
- `cargo test -p sovereign-app --lib --locked --target-dir C:/cargo-target -j 4 -- onboarding rc2_second salt_restore`
  (default features; the first rc2 tests also compile with `--no-default-features --features mobile`).
- `cargo test --workspace --test backup_recovery_argon2 --locked --target-dir C:/cargo-target -j 4 -- --ignored`
  (Q3, ignored by default).
- `cargo test -p sovereign-shell --locked --target-dir C:/cargo-target -j 4 -- field_contrast auth_store_write salt_restore`
- in `frontend/`: `npx vitest run src/lib/components/OnboardingWizard.svelte.test.ts`

Each Rust suite was checked to go green against a throw-away implementation (not committed),
so the tests are satisfiable as written.
