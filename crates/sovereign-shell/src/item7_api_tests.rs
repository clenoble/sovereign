//! v0.0.12 item 7 in the native shell — the guarantees that need the API the
//! item adds (`doc/plans/V0012-item7-conversion.md`, V2 with V3). Written
//! before the code, by another author: whoever implements item 7 makes these
//! pass without editing this file. **It does not compile until that API
//! exists** (`DocumentService::with_build_version`, named in
//! `sovereign-docs/tests/item7_conversion_api.rs`); the rest of the shell's
//! item-7 tests are in `item7_tests.rs`, whose fixtures this file uses.
//!
//! - **V3 in the shell**: the unlock records the shell's own build version
//!   (`env!("CARGO_PKG_VERSION")` of `sovereign-shell`), not a constant of
//!   the document crate.
//! - **V2 "stays while any remain"**, the other half: once a version change
//!   has converted every listed row, the alert is gone.

use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_core::auth::PersonaKind;
use sovereign_db::traits::GraphDB;
use sovereign_docs::unlock_migration::RECORD_KEY;
use sovereign_docs::run_unlock_migration;

use super::item7_tests::{alerts_text, legacy_row, make_undecryptable, names_short_id, new_app, new_thread, store_with_raw, unlock_into};

/// V3: the shell's unlock runs the conversion under the shell's build
/// version, and the completion record keeps it.
#[test]
fn v3_the_shells_unlock_records_its_own_build_version() {
    let mut app = new_app();
    let (raw, store) = app.rt.block_on(store_with_raw("v3-shell-version"));
    app.rt.block_on(async {
        let t = new_thread(&store).await;
        let bad = legacy_row(&store, &t, "Bad", "zebra").await;
        make_undecryptable(&raw, &store, &bad).await;
    });
    unlock_into(&mut app, store.clone(), PersonaKind::Primary);
    let rec = app.rt.block_on(store.get_store_record(RECORD_KEY)).unwrap().expect("the completion record");
    assert!(
        rec.contains(env!("CARGO_PKG_VERSION")),
        "the record keeps the shell's build version ({}): {rec}",
        env!("CARGO_PKG_VERSION")
    );
}

/// V2 with V3: the alert lists a row while it cannot be read — through an
/// unlock of the shell (another build than the one that first listed it, the
/// row still undecryptable) — and is gone once a build that can read it has
/// converted it.
#[test]
fn v2_the_alert_is_gone_once_a_version_change_has_converted_every_row() {
    let mut app = new_app();
    let (raw, store) = app.rt.block_on(store_with_raw("v2-gone"));
    let (bad, sealed) = app.rt.block_on(async {
        let t = new_thread(&store).await;
        let bad = legacy_row(&store, &t, "Zebra title", "zebra body").await;
        let sealed = make_undecryptable(&raw, &store, &bad).await;
        (bad, sealed)
    });
    // An older build completed the conversion and listed the row.
    let older = build_document_service(store.clone(), Some(testing::account_key()), "older".into())
        .with_build_version("0.0.0-item7-older");
    app.rt.block_on(run_unlock_migration(&older)).expect("the older build's conversion");

    // The shell (another build): the row still cannot be read, so it stays.
    unlock_into(&mut app, store.clone(), PersonaKind::Primary);
    let text = alerts_text(&mut app);
    assert!(text.to_lowercase().contains("documents that could not be read") && names_short_id(&text, &bad), "{text}");

    // A newer build can read it: it converts it, and the list is empty.
    app.rt.block_on(raw.set_document_content_encryption(&bad, &sealed.0, &sealed.1)).unwrap();
    let newer = Arc::new(
        build_document_service(store.clone(), Some(testing::account_key()), "newer".into())
            .with_build_version("99.0.0-item7-newer"),
    );
    app.rt.block_on(run_unlock_migration(&newer)).expect("the newer build's retry");
    let text = alerts_text(&mut app);
    assert!(
        !text.to_lowercase().contains("documents that could not be read"),
        "none remain: the alert is gone (V2):\n{text}"
    );
}
