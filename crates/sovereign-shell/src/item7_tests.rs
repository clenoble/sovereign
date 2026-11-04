//! v0.0.12 item 7 in the native shell (`doc/plans/V0012-item7-conversion.md`,
//! V2 and V5; review of v0.0.11, findings 11 and 12). Written before the
//! code, by another author: whoever implements item 7 makes these pass without
//! editing this file. A child of `app` (declared in `app.rs`) for the same
//! reason as `editor_default_tests`: it drives the unlock
//! (`install_document_service`), the Alerts window and the editor's
//! "changed elsewhere" path (`v2_after_sync`) on a real `App`.
//!
//! - **V2 — Listed, in the Alerts window**: one alert, "Documents that could
//!   not be read", lists each by a short id and its last modified date (no
//!   title), with one line on what it means (kept as it was; tried again
//!   after the next update). It stays while any remain. Each persona lists its
//!   own. Read through the window's accessibility lines (what a screen reader
//!   is given, and what the window shows).
//! - **V5 — No "changed elsewhere" from the open document's own conversion**:
//!   a conversion whose plain text equals what the editor holds is taken
//!   silently; a real change still raises the warning.
//!
//! The guarantees needing the API the item adds (a version change empties the
//! list; the shell records its own build version) are in `item7_api_tests.rs`.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB, built here as
//! `docs_host::testing::encrypted_memory_store` builds it but keeping the raw
//! store, to make a row undecryptable at rest (as DB-M1's test does). Nothing
//! touches a real workspace or the user's clipboard.

use std::sync::{Arc, OnceLock};

use chrono::{DateTime, TimeZone, Utc};
use sovereign_ai::docs_host::testing;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::ClipboardPrivacy;
use sovereign_editor::input::Modifiers;
use sovereign_editor::{NoticeKind, Position, Selection};
use winit::keyboard::Key as WKey;

use super::editor_v2::{self, route_key, EditorPane, SystemClipboard};
use super::{App, WindowContent, WindowKind};
use sovereign_core::auth::PersonaKind;

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything (DB-M1's test).
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// The alert's title (contract V2).
const HEADING: &str = "documents that could not be read";

/// What must never appear in the alert: the undecryptable rows' titles (which
/// still decrypt: only the body is corrupted) and bodies.
const MARKER: &str = "zebra";

// ======================================================================= fixtures

fn ensure_test_data_dir() {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-item7-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

#[derive(Default)]
struct MemClipboard(Option<String>);
impl SystemClipboard for MemClipboard {
    fn write_text(&mut self, text: &str, _privacy: ClipboardPrivacy) -> Result<(), String> {
        self.0 = Some(text.to_string());
        Ok(())
    }
    fn read_text(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn change_count(&self) -> Option<u64> {
        None
    }
    fn clear(&mut self) -> Result<(), String> {
        self.0 = None;
        Ok(())
    }
}

/// `testing::encrypted_memory_store`, keeping the raw store.
pub(super) async fn store_with_raw(tag: &str) -> (Arc<SurrealGraphDB>, Arc<EncryptedGraphDB>) {
    use sovereign_crypto::device_key::DeviceKey;
    use sovereign_crypto::index_key::IndexKey;
    use sovereign_crypto::kek::Kek;
    use sovereign_crypto::key_db::KeyDatabase;
    use sovereign_crypto::master_key::MasterKey;
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store"));
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("item7-shell-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let device_key = DeviceKey::derive(&mk, "test-device").expect("device key");
    let edb = Arc::new(EncryptedGraphDB::new(
        raw.clone() as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(device_key),
    ));
    (raw, edb)
}

/// The last modified date of the undecryptable rows (noon UTC: the same date
/// in every time zone a display might use).
pub(super) fn when() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 2, 12, 0, 0).unwrap()
}

/// A row as writers before the service left it, modified at [`when`].
pub(super) async fn legacy_row(store: &EncryptedGraphDB, thread: &str, title: &str, content: &str) -> String {
    let mut d = Document::new(title.into(), thread.into(), true);
    d.content = content.into();
    d.created_at = when();
    d.modified_at = when();
    store.create_document(d).await.unwrap().id_string().unwrap()
}

/// Makes the row undecryptable at rest; returns its sealed body (to give it
/// back).
pub(super) async fn make_undecryptable(raw: &SurrealGraphDB, store: &EncryptedGraphDB, id: &str) -> (String, String) {
    let row = raw.get_document(id).await.expect("the row at rest");
    let nonce = row.encryption_nonce.clone().expect("sealed at rest");
    raw.set_document_content_encryption(id, JUNK_CT, JUNK_NONCE).await.expect("corrupt at rest");
    assert!(store.get_document(id).await.is_err(), "fixture: the row no longer decrypts");
    (row.content, nonce)
}

pub(super) async fn new_thread(store: &EncryptedGraphDB) -> String {
    store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap()
}

/// The unlock sequence of `App::try_login` on `store` (no auth store, no
/// P2P), waiting for the one-time conversion it starts, as
/// `editor_default_tests::unlock_into` does.
pub(super) fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>, persona: PersonaKind) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(persona);
    app.install_document_service(store);
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
}

pub(super) fn new_app() -> App {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    App::new()
}

/// What the Alerts window says (its accessibility lines): opened if it is
/// not, re-read (`refresh_alerts`) if it is.
pub(super) fn alerts_text(app: &mut App) -> String {
    if app.windows.iter().any(|w| matches!(w.content, WindowContent::Alerts(_))) {
        app.refresh_alerts();
    } else {
        app.focus_or_toggle(WindowKind::Alerts);
    }
    let ap = app
        .windows
        .iter()
        .find_map(|w| match &w.content {
            WindowContent::Alerts(ap) => Some(ap),
            _ => None,
        })
        .expect("the Alerts window is open (unlocked)");
    ap.a11y_lines().join("\n")
}

/// Whether `text` names the row `id` by a short id: some run of 6
/// consecutive characters of its key. **[choice of the test]** the short
/// id's form (prefix, suffix, ellipsis) is the implementer's.
pub(super) fn names_short_id(text: &str, id: &str) -> bool {
    let key: Vec<char> = id.rsplit(':').next().unwrap_or(id).chars().collect();
    key.windows(6.min(key.len())).any(|w| text.contains(&w.iter().collect::<String>()))
}

/// Whether `text` gives the date [`when`]. CONTRACT-QUESTION: the contract
/// says "its last modified date" without a form; accepted here are the forms
/// the shell already uses (`%Y-%m-%d`, the trash list) and the usual written
/// ones.
pub(super) fn gives_the_date(text: &str) -> bool {
    ["2025-03-02", "2 Mar 2025", "Mar 2, 2025", "2 March 2025", "March 2, 2025", "02.03.2025"]
        .iter()
        .any(|f| text.contains(f))
}

/// The lines of the alert (from its title to the end of the window's lines).
fn alert_part(text: &str) -> Option<String> {
    let low = text.to_lowercase();
    low.find(HEADING).map(|i| text[i..].to_string())
}

// ======================================================================= V2

/// V2: "One alert, 'Documents that could not be read', lists each by a short
/// id and its last modified date (no title: none can be read), with one line
/// on what it means: the document is kept as it was, and Sovereign will try
/// again after its next update." One alert for two rows, each listed; no
/// title, no text; the readable document is not listed.
#[test]
fn v2_one_alert_lists_each_undecryptable_document_by_short_id_and_date() {
    let mut app = new_app();
    let (raw, store) = app.rt.block_on(store_with_raw("v2-list"));
    let (ok, bad, bad2) = app.rt.block_on(async {
        let t = new_thread(&store).await;
        let ok = legacy_row(&store, &t, "Readable plain", "# fine\n\nreadable").await;
        let bad = legacy_row(&store, &t, "Zebra secret title", "zebra-secret-body").await;
        let bad2 = legacy_row(&store, &t, "Zebra second title", r#"{"body":"zebra-second-body","images":[]}"#).await;
        make_undecryptable(&raw, &store, &bad).await;
        make_undecryptable(&raw, &store, &bad2).await;
        (ok, bad, bad2)
    });
    unlock_into(&mut app, store, PersonaKind::Primary);

    let text = alerts_text(&mut app);
    let low = text.to_lowercase();
    assert_eq!(low.matches(HEADING).count(), 1, "one alert, \"Documents that could not be read\" (V2):\n{text}");
    let part = alert_part(&text).expect("the alert");
    assert!(names_short_id(&part, &bad) && names_short_id(&part, &bad2), "each listed by a short id:\n{text}");
    assert!(!names_short_id(&part, &ok), "the readable document is not listed:\n{text}");
    assert!(gives_the_date(&part), "with its last modified date (2025-03-02):\n{text}");
    assert!(!low.contains(MARKER), "no title, no text:\n{text}");
    let part_low = part.to_lowercase();
    assert!(
        part_low.contains("kept") && part_low.contains("update"),
        "one line on what it means: kept as it was, tried again after the next update:\n{text}"
    );
}

/// V2: "The alert stays while any remain." Re-reading the alerts (any
/// refresh) and a later unlock of the same build keep it: the same build
/// retries nothing (V3), so the rows remain.
#[test]
fn v2_the_alert_stays_while_any_remain() {
    let mut app = new_app();
    let (raw, store) = app.rt.block_on(store_with_raw("v2-stays"));
    let bad = app.rt.block_on(async {
        let t = new_thread(&store).await;
        let bad = legacy_row(&store, &t, "Zebra title", "zebra body").await;
        make_undecryptable(&raw, &store, &bad).await;
        bad
    });
    unlock_into(&mut app, store.clone(), PersonaKind::Primary);
    assert!(names_short_id(&alert_part(&alerts_text(&mut app)).expect("listed"), &bad));

    app.refresh_alerts();
    let text = alerts_text(&mut app);
    assert!(alert_part(&text).is_some_and(|p| names_short_id(&p, &bad)), "after a refresh:\n{text}");

    // A later unlock (a new service over the same store, same build).
    unlock_into(&mut app, store, PersonaKind::Primary);
    let text = alerts_text(&mut app);
    assert!(alert_part(&text).is_some_and(|p| names_short_id(&p, &bad)), "after a later unlock:\n{text}");
}

/// V2: "It is the same on both personas' screens (each lists its own)." The
/// primary lists its row, then — the duress persona unlocked in the same
/// window manager, over its own store — the duress screen lists its own, the
/// same alert, and nothing of the other's.
#[test]
fn v2_each_persona_lists_its_own() {
    let mut app = new_app();
    let (raw_p, store_p) = app.rt.block_on(store_with_raw("v2-primary"));
    let (raw_d, store_d) = app.rt.block_on(store_with_raw("v2-duress"));
    let (mine_p, mine_d) = app.rt.block_on(async {
        let tp = new_thread(&store_p).await;
        let td = new_thread(&store_d).await;
        let p = legacy_row(&store_p, &tp, "Zebra primary", "zebra p").await;
        let d = legacy_row(&store_d, &td, "Zebra decoy", "zebra d").await;
        make_undecryptable(&raw_p, &store_p, &p).await;
        make_undecryptable(&raw_d, &store_d, &d).await;
        (p, d)
    });

    unlock_into(&mut app, store_p, PersonaKind::Primary);
    let primary = alerts_text(&mut app);
    let pp = alert_part(&primary).expect("the primary's alert");
    assert!(names_short_id(&pp, &mine_p), "the primary lists its own:\n{primary}");
    assert!(!names_short_id(&pp, &mine_d), "and not the other persona's:\n{primary}");

    unlock_into(&mut app, store_d, PersonaKind::Duress);
    let duress = alerts_text(&mut app);
    let dp = alert_part(&duress).expect("the same alert on the duress persona's screen");
    assert!(names_short_id(&dp, &mine_d), "the duress persona lists its own:\n{duress}");
    assert!(!names_short_id(&dp, &mine_p), "and not the primary's:\n{duress}");
    assert_eq!(duress.to_lowercase().matches(HEADING).count(), 1);
}

/// V2, the other side: a store with no undecryptable row shows no such alert.
#[test]
fn v2_no_undecryptable_row_no_alert() {
    let mut app = new_app();
    let (_raw, store) = app.rt.block_on(store_with_raw("v2-none"));
    app.rt.block_on(async {
        let t = new_thread(&store).await;
        legacy_row(&store, &t, "Fine", "# fine").await;
    });
    unlock_into(&mut app, store, PersonaKind::Primary);
    let text = alerts_text(&mut app);
    assert!(!text.to_lowercase().contains(HEADING), "{text}");
}

// ======================================================================= V5

/// A shell unlocked on an empty store (the conversion has completed on it),
/// then one legacy document written raw — what the conversion would convert
/// — opened in the editor.
struct Open {
    app: App,
    store: Arc<EncryptedGraphDB>,
    id: String,
}

const LEGACY_TEXT: &str = "zebra legacy text";

fn open_legacy(tag: &str) -> Open {
    let mut app = new_app();
    let (_raw, store) = app.rt.block_on(store_with_raw(tag));
    let thread = app.rt.block_on(new_thread(&store));
    unlock_into(&mut app, store.clone(), PersonaKind::Primary);
    let id = app.rt.block_on(legacy_row(&store, &thread, "Open legacy", LEGACY_TEXT));
    app.load_workspace_now();
    let ci = app.cards.iter().position(|c| c.id == id).expect("its card");
    app.open_doc_window(ci);
    let mut o = Open { app, store, id };
    assert_eq!(o.pane().editor.document().plain_text(), LEGACY_TEXT, "fixture: the editor holds it");
    o
}

impl Open {
    fn pane(&mut self) -> &mut EditorPane {
        let id = self.id.clone();
        let wi = self
            .app
            .windows
            .iter()
            .position(|w| matches!(&w.content, WindowContent::Doc(d) if d.doc_id == id))
            .expect("the window");
        match &mut self.app.windows[wi].content {
            WindowContent::Doc(d) => d.editor_pane_mut().expect("the editor"),
            _ => unreachable!(),
        }
    }

    /// The conversion of this document, as the one-time conversion does it
    /// (`DocumentService::migrate_all`: a later unlock's resume, or a retry,
    /// converts a row exactly so).
    fn convert(&mut self) {
        let docs = self.app.docs.clone().expect("unlocked");
        let report = self.app.rt.block_on(docs.migrate_all()).expect("the conversion");
        assert_eq!(report.migrated, vec![self.id.clone()], "fixture: the open document is converted");
        let stored = self.app.rt.block_on(self.store.get_document(&self.id)).unwrap().content;
        assert!(stored.contains("\"sovereign.doc\""), "fixture: stored in format v2 now");
    }

    fn remote_change_notices(&mut self) -> Vec<String> {
        self.pane()
            .editor
            .take_notices()
            .into_iter()
            .filter(|n| n.kind == NoticeKind::RemoteChange)
            .map(|n| n.message)
            .collect()
    }

    fn type_at_end(&mut self, s: &str) {
        let p = self.pane();
        p.set_focus(true);
        let b = &p.editor.document().blocks()[0];
        let at = Position { block: b.id(), offset: b.text().unwrap_or("").len() };
        p.editor.set_selection(Selection::caret(at));
        for c in s.chars() {
            let t = c.to_string();
            route_key(&mut p.editor, &WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }

    fn save(&mut self) {
        self.pane().editor.command(sovereign_editor::input::Command::Save);
        for _ in 0..3 {
            self.app.v2_frame();
        }
    }

    fn stored_text(&self) -> String {
        let docs = self.app.docs.clone().unwrap();
        self.app.rt.block_on(docs.read(&self.id)).unwrap().plain_text()
    }
}

/// V5: "A conversion of the open document whose plain text equals what the
/// editor holds is not announced as a change from elsewhere. The editor takes
/// the converted form silently." The conversion lands; the next reload's
/// pass over the open editors (`v2_after_sync`, run after a sync or the
/// assistant's writes) raises no "changed elsewhere"; the editor shows the
/// same text; and its next save is based on the converted form — no conflict,
/// so no "changed on …" either.
#[test]
fn v5_the_open_documents_own_conversion_raises_no_changed_elsewhere() {
    let mut o = open_legacy("v5-silent");
    o.convert();
    o.app.v2_after_sync("a paired device");
    let notices = o.remote_change_notices();
    assert!(notices.is_empty(), "V5: its own conversion is not a change from elsewhere: {notices:?}");
    assert_eq!(o.pane().editor.document().plain_text(), LEGACY_TEXT, "the same text");

    o.app.v2_after_sync("this device (by the assistant)");
    assert!(o.remote_change_notices().is_empty(), "nor on the assistant's reload path");

    o.type_at_end(" and more");
    o.save();
    assert_eq!(o.stored_text(), format!("{LEGACY_TEXT} and more"), "the save lands");
    let status = o.pane().status_text();
    assert!(
        !status.contains("changed on"),
        "taken silently: the next save is based on the converted form, not a conflict: {status}"
    );
}

/// V5: "a real change still raises the warning" — after the silent
/// conversion, another device's change of the text is announced and shown.
#[test]
fn v5_a_real_change_after_the_conversion_still_raises_the_warning() {
    let mut o = open_legacy("v5-real");
    o.convert();
    o.app.v2_after_sync("a paired device");
    o.remote_change_notices();

    let raw = o.app.rt.block_on(o.store.get_document(&o.id)).unwrap().content;
    let theirs = raw.replace(LEGACY_TEXT, "zebra from a peer");
    assert_ne!(theirs, raw, "fixture: the converted text holds the legacy text");
    o.app.rt.block_on(o.store.update_document(&o.id, None, Some(&theirs))).unwrap();
    o.app.v2_after_sync("a paired device");
    let notices = o.remote_change_notices();
    assert!(!notices.is_empty(), "a real change raises the warning");
    assert!(notices.iter().all(|m| m.contains("changed on")), "{notices:?}");
    assert_eq!(o.pane().editor.document().plain_text(), "zebra from a peer", "and is shown");
}

/// V5, the case the contract leaves open. CONTRACT-QUESTION: "whose plain
/// text equals what the editor holds" — with edits not yet saved, the editor
/// holds two texts: what it opened (its base) and what is typed. Best
/// reading, asserted here: the conversion is compared with the editor's base
/// (it changed the stored form, not the state): no warning, the typed text
/// is kept, and the next save is based on the converted form (no conflict, no
/// "changed on …").
#[test]
fn v5_contract_question_unsaved_edits_and_the_conversion() {
    let mut o = open_legacy("v5-unsaved");
    o.type_at_end(" typed");
    o.convert();
    o.app.v2_after_sync("a paired device");
    let notices = o.remote_change_notices();
    assert!(notices.is_empty(), "its own conversion is not a change from elsewhere: {notices:?}");
    assert_eq!(o.pane().editor.document().plain_text(), format!("{LEGACY_TEXT} typed"), "the typed text is kept");
    o.save();
    assert_eq!(o.stored_text(), format!("{LEGACY_TEXT} typed"));
    let status = o.pane().status_text();
    assert!(!status.contains("changed on"), "no conflict with its own conversion: {status}");
}
