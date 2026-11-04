//! v0.0.12 item 8, K3 for the shell's own single-line fields
//! (`doc/plans/V0012-item8-clipboard-labels.md`): "A copy made by a shell field
//! is recorded where the editors' copies are, so K1 and K2 cover it. A copy
//! made after another one replaces it, whichever made it."
//!
//! Written before the code, by another author: whoever builds item 8 makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around. A child module of `app` (declared in
//! `app.rs`), as `clipboard_forget_tests` is.
//!
//! **Seam this file needs (it does not compile until it exists):**
//!
//! - `crate::app::editor_v2::shared_clipboard() -> Rc<RefCell<ClipState>>` —
//!   this thread's shared clipboard state (the `CLIPBOARD` thread-local, the
//!   one `use_test_system_clipboard` replaces), reachable **outside tests**:
//!   it is where every editor's copy and every shell field's copy is recorded,
//!   and what the store change and the quit forget.
//!
//! The pairing code the Devices window copies (`crate::clip::set_text`, K1 as
//! settled in 8775982) is driven through that function, under the same seam.
//!
//! The fields themselves are driven through the clipboard the shell gives them
//! today, `crate::clip::FieldClipboard::default()` (the title prompt, the chat
//! composer and search, the renames, the device name). The seam keeps this
//! file from running against today's `FieldClipboard`, which writes to the
//! **real** OS clipboard: a test never touches the user's clipboard.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use sovereign_ai::docs_host::testing;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::{Clipboard, ClipboardPrivacy, ClipboardWrite};

use super::editor_v2::{self, shared_clipboard, SystemClipboard};
use super::App;
use crate::clip::FieldClipboard;

// ======================================================================= the fake system clipboard

#[derive(Default)]
struct Sys {
    text: Option<String>,
    count: u64,
    has_count: bool,
    clears: usize,
    /// The privacy flags of every write, in order (K5).
    privacy: Vec<ClipboardPrivacy>,
}

#[derive(Clone)]
struct SysHandle(Rc<RefCell<Sys>>);

impl SysHandle {
    fn text(&self) -> Option<String> {
        self.0.borrow().text.clone()
    }
    fn clears(&self) -> usize {
        self.0.borrow().clears
    }
    fn other_app_copies(&self, text: &str) {
        let mut s = self.0.borrow_mut();
        s.text = Some(text.into());
        s.count += 1;
    }
}

impl SystemClipboard for SysHandle {
    fn write_text(&mut self, text: &str, privacy: ClipboardPrivacy) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        s.privacy.push(privacy);
        s.text = Some(text.to_string());
        s.count += 1;
        Ok(())
    }
    fn read_text(&mut self) -> Option<String> {
        self.0.borrow().text.clone()
    }
    fn change_count(&self) -> Option<u64> {
        let s = self.0.borrow();
        s.has_count.then_some(s.count)
    }
    fn clear(&mut self) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        s.text = None;
        s.count += 1;
        s.clears += 1;
        Ok(())
    }
}

/// This thread's shared clipboard state now talks to a fake system clipboard.
fn fake(has_count: bool) -> SysHandle {
    let sys = SysHandle(Rc::new(RefCell::new(Sys { has_count, ..Default::default() })));
    editor_v2::use_test_system_clipboard(Box::new(sys.clone()));
    sys
}

const ALL_PRIVATE: ClipboardPrivacy =
    ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };

fn item(text: &str, sovereign: Option<&str>, clear_after_s: Option<u64>) -> ClipboardWrite {
    ClipboardWrite {
        text: text.into(),
        sovereign: sovereign.map(String::from),
        privacy: ALL_PRIVATE,
        clear_after: clear_after_s.map(Duration::from_secs),
    }
}

/// A shell field copies `text` (a field's copies carry no payload and no
/// delayed clear).
fn field_copies(text: &str) {
    FieldClipboard::default().write(item(text, None, None));
}

// ======================================================================= K3

// K3: "A copy made by a shell field is recorded where the editors' copies
// are" — it lands through the shared state (on the fake, not the OS), and
// the forgetting clears it (K1, K2).
#[test]
fn k3_a_field_copy_is_recorded_with_the_editors_copies_and_forgotten() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        assert!(Rc::ptr_eq(&shared_clipboard(), &editor_v2::clipboard_for_tests()), "one shared state");
        field_copies("zebra field text");
        assert_eq!(sys.text().as_deref(), Some("zebra field text"), "has_count={has_count}: through the shared state");
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: the field's copy is cleared");
        assert_eq!(sys.clears(), 1);
    }
}

// K3: a field copy after an editor copy replaces it — the editor copy's
// payload, source and pending clear go with it; the field's (ordinary) copy
// is not cleared during the session, and is cleared by the forgetting.
#[test]
fn k3_a_field_copy_replaces_an_editor_copy() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        shared_clipboard().borrow_mut().write_from(
            item("revealed 4111", Some("editor-payload"), Some(30)),
            Duration::ZERO,
            Some("document:a".into()),
        );
        field_copies("from the field");
        {
            let st = shared_clipboard();
            let mut st = st.borrow_mut();
            let c = st.read().expect("the clipboard holds the field's copy");
            assert_eq!(c.text, "from the field", "has_count={has_count}");
            assert_eq!(c.sovereign, None, "the editor copy's payload is gone");
            assert_eq!(st.source_document(), None, "and its source document");
            st.tick(Duration::from_secs(1_000_000));
        }
        assert_eq!(sys.text().as_deref(), Some("from the field"), "has_count={has_count}: no 30-s clear of the field's copy");
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: forgotten");
    }
}

// K3: an editor copy after a field copy replaces it; the forgetting clears
// the clipboard once.
#[test]
fn k3_an_editor_copy_replaces_a_field_copy() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        field_copies("from the field");
        shared_clipboard().borrow_mut().write_from(item("from the editor", Some("p"), None), Duration::ZERO, Some("document:a".into()));
        {
            let st = shared_clipboard();
            let mut st = st.borrow_mut();
            let c = st.read().expect("the editor's copy");
            assert_eq!(c.text, "from the editor");
            assert_eq!(c.sovereign.as_deref(), Some("p"), "the editor's own payload, as today");
        }
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text(), None, "has_count={has_count}");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: once");
    }
}

// K1 for a field: another application copying after the field's copy is left.
#[test]
fn k3_another_applications_copy_after_a_field_copy_is_never_cleared() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        field_copies("from the field");
        sys.other_app_copies("someone else's text");
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text().as_deref(), Some("someone else's text"), "has_count={has_count}");
        assert_eq!(sys.clears(), 0);
    }
}

// Unchanged ("Pasting inside Sovereign"; `FieldClipboard`'s rule): a field
// never gets a Sovereign payload back, even from the editors' copy it now
// shares a record with — a token copied from a document pastes into a field
// as the text it showed.
#[test]
fn unchanged_a_field_never_reads_a_payload() {
    let _sys = fake(true);
    shared_clipboard().borrow_mut().write_from(item("Mail \u{2039}email\u{203a}", Some("payload"), None), Duration::ZERO, Some("document:a".into()));
    let c = FieldClipboard::default().read().expect("the field reads the clipboard");
    assert_eq!(c.text, "Mail \u{2039}email\u{203a}");
    assert_eq!(c.sovereign, None);
}

// ======================================================================= K1 through the App

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
        let d = std::env::temp_dir().join(format!("sov-item8f-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
}

// K1: "That covers a copy … from a shell field": a field's copy made while a
// store is open is cleared when the store is replaced.
#[test]
fn k1_a_field_copy_is_cleared_at_a_store_change() {
    ensure_test_data_dir();
    let sys = fake(false);
    let mut app = App::new();
    let a = app.rt.block_on(testing::encrypted_memory_store("item8-field-a"));
    unlock_into(&mut app, a);
    field_copies("zebra typed in the composer");
    assert_eq!(sys.text().as_deref(), Some("zebra typed in the composer"));
    let b = app.rt.block_on(testing::encrypted_memory_store("item8-field-b"));
    unlock_into(&mut app, b);
    assert_eq!(sys.text(), None, "the field's copy does not outlive the store");
}

// ======================================================================= the pairing code (K1, 8775982)

// K1 as settled (8775982): "the pairing code the Devices window copies
// (`app.rs`, `crate::clip::set_text`)" is a copy this process made: it lands
// through the shared state, and the forgetting (store change, quit) clears it.
#[test]
fn k1_the_pairing_code_copy_is_recorded_and_forgotten() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        assert!(crate::clip::set_text("PAIR-7Q2X-ZEBRA"), "has_count={has_count}: the copy is said to succeed");
        assert_eq!(sys.text().as_deref(), Some("PAIR-7Q2X-ZEBRA"), "has_count={has_count}: through the shared state");
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: the pairing code is cleared");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: once");
    }
}

// K1: another application's copy made after the pairing code is never cleared.
#[test]
fn k1_another_applications_copy_after_the_pairing_code_is_never_cleared() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        assert!(crate::clip::set_text("PAIR-7Q2X-ZEBRA"));
        sys.other_app_copies("someone else's text");
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text().as_deref(), Some("someone else's text"), "has_count={has_count}");
        assert_eq!(sys.clears(), 0, "has_count={has_count}");
    }
}

// K3: the pairing code and an editor copy replace each other, either way;
// the forgetting clears whichever is current, once.
#[test]
fn k3_the_pairing_code_and_an_editor_copy_replace_each_other() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        shared_clipboard().borrow_mut().write_from(item("editor text", Some("p"), Some(30)), Duration::ZERO, Some("document:a".into()));
        assert!(crate::clip::set_text("PAIR-7Q2X-ZEBRA"));
        {
            let st = shared_clipboard();
            let mut st = st.borrow_mut();
            let c = st.read().expect("the pairing code");
            assert_eq!(c.text, "PAIR-7Q2X-ZEBRA", "has_count={has_count}");
            assert_eq!(c.sovereign, None, "the editor copy's payload is gone");
            assert_eq!(st.source_document(), None);
        }
        shared_clipboard().borrow_mut().write_from(item("later editor text", None, None), Duration::ZERO, Some("document:b".into()));
        shared_clipboard().borrow_mut().forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: the current copy is cleared");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: once");
    }
}

// K1 through the App: the pairing code copied while a store is open does not
// outlive the store change.
#[test]
fn k1_the_pairing_code_is_cleared_at_a_store_change() {
    ensure_test_data_dir();
    let sys = fake(false);
    let mut app = App::new();
    let a = app.rt.block_on(testing::encrypted_memory_store("item8-pair-a"));
    unlock_into(&mut app, a);
    assert!(crate::clip::set_text("PAIR-7Q2X-ZEBRA"));
    assert_eq!(sys.text().as_deref(), Some("PAIR-7Q2X-ZEBRA"));
    let b = app.rt.block_on(testing::encrypted_memory_store("item8-pair-b"));
    unlock_into(&mut app, b);
    assert_eq!(sys.text(), None, "the pairing code does not outlive the store");
}

// ======================================================================= K5

// K5 (d6ffb28): "The pairing code copied by the Devices window
// (`crate::clip::set_text`) is written with the same privacy flags as an
// editor copy (`ALL_EXCLUDED`: not in the clipboard history, not
// cloud-synced, not monitored)." The flags the system clipboard receives for
// that write are the editor's — `ALL_PRIVATE` here has `ALL_EXCLUDED`'s
// values (`sovereign-editor/src/editor.rs`).
#[test]
fn k5_the_pairing_code_is_written_with_the_editors_privacy_flags() {
    for has_count in [true, false] {
        let sys = fake(has_count);
        assert!(crate::clip::set_text("PAIR-7Q2X-ZEBRA"));
        let written = sys.0.borrow().privacy.clone();
        assert_eq!(written, vec![ALL_PRIVATE], "has_count={has_count}: the pairing code is written private");
    }
}
