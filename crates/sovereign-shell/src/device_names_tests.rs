//! v0.0.12 items 14 and 19 in the native shell — devices have names (N1–N6),
//! and History says them (R1, R2).
//!
//! Contract: `doc/plans/V0012-items14-19-devices.md` (as amended at the
//! laptop's 0139 read: 64 grapheme clusters, the duress consequence, the
//! short id from the end, the host-name prefill decided by Céline). Written
//! before the code, by another author: whoever implements items 14 and 19
//! makes these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`, like `first_run_tests`), so
//! it drives `App`'s own paths on a real, headless `App`, and reuses
//! `first_run_tests`' child launches, frames and tree reading.
//!
//! # The seams this file needs (it does not compile until they exist)
//!
//! 1. **The name's rules** — a module `crate::device_name`
//!    (`src/device_name.rs`, declared in `main.rs`):
//!
//!    ```ignore
//!    /// A name as typed at setup (N2): trimmed, at most 64 grapheme
//!    /// clusters, never cut inside one. `None` for a blank or
//!    /// whitespace-only entry, which means the default.
//!    pub(crate) fn clean(input: &str) -> Option<String>;
//!    ```
//!
//! 2. **The rename field** — a module `crate::device_rename`
//!    (`src/device_rename.rs`), the shape of `lane_rename`:
//!
//!    ```ignore
//!    /// What a Rename action renames.
//!    #[derive(Clone, Debug, PartialEq, Eq)]
//!    pub(crate) enum RenameTarget {
//!        /// This device's own name (N3).
//!        ThisDevice,
//!        /// A paired device's label on this device (N5), by its peer id.
//!        Paired(String),
//!    }
//!    /// The open field: the editor's single-line field.
//!    pub(crate) struct DeviceRename { … }
//!    impl DeviceRename {
//!        /// The name as typed (committed text).
//!        pub(crate) fn name(&self) -> String;
//!        /// The message a refused entry shows (a blank one), if any.
//!        pub(crate) fn message(&self) -> Option<&str>;
//!    }
//!    ```
//!
//! 3. **On `App`**:
//!    - a field `pub(crate) device_rename: Option<crate::device_rename::DeviceRename>`
//!      — the open rename field, if any;
//!    - `App::open_device_rename(&mut self, target: RenameTarget)` — what a
//!      Rename action of Settings → Devices does (a click on it): opens the
//!      field holding the target's current name;
//!    - `App::device_rename_key(&mut self, key: &winit::keyboard::Key, text: Option<&str>, mods: sovereign_editor::input::Modifiers)`
//!      — the keyboard while the field is open, as `window_event` routes it:
//!      Enter saves and closes, Escape cancels and closes, a blank entry is
//!      refused with a message and the field stays open. Its return value is
//!      not read here.
//!
//! 4. **A node-less P2P handle** — on `crate::p2p::P2pHandle`:
//!
//!    ```ignore
//!    /// A handle with no node behind it, for tests: this node's peer id
//!    /// `local_peer_id`, the name it was started with `device_name`, no
//!    /// listen address, a generated account key, the paired-device store
//!    /// `paired` (saved under `store_key` when it changes); every command
//!    /// it sends goes to the returned receiver (capacity at least 64).
//!    #[cfg(test)]
//!    pub(crate) fn fake(
//!        local_peer_id: &str,
//!        device_name: &str,
//!        paired: sovereign_p2p::pairing::PairingManager,
//!        store_key: [u8; 32],
//!    ) -> (P2pHandle, tokio::sync::mpsc::Receiver<sovereign_p2p::P2pCommand>);
//!    ```
//!
//! 5. **History's device** — a module `crate::history_device`
//!    (`src/history_device.rs`), and two methods on `crate::panels::HistoryPanel`:
//!
//!    ```ignore
//!    /// R1: the text a History row shows and announces for a version's
//!    /// stored `message`: "Synced from {id}" / "Received from {id}" with the
//!    /// device's name (from `names`, peer id → name), "this device" for
//!    /// `own_id`, or "…" and the id's last 8 characters for an id not in
//!    /// `names`; any other message as it is.
//!    pub(crate) fn shown_message(message: &str, names: &HashMap<String, String>, own_id: &str) -> String;
//!    /// R2: `text` on one line no wider than `width` (logical px) when
//!    /// shaped by `shaper` at `font_px`: itself when it fits, else a prefix
//!    /// of it followed by "…".
//!    pub(crate) fn cut_to_width(shaper: &mut crate::text::TextShaper, text: &str, width: f64, font_px: f32) -> String;
//!
//!    impl HistoryPanel {
//!        /// After `ensure_shaped` at `row.width()`: the rectangles row `i`'s
//!        /// message line and time line occupy when the row is drawn at
//!        /// `row` (from their shaped layouts, as `draw_history` places them).
//!        pub(crate) fn row_line_rects(&self, i: usize, row: Rect) -> (Rect, Rect);
//!        /// The text row `i`'s message line draws, as last shaped.
//!        pub(crate) fn drawn_message(&self, i: usize) -> String;
//!    }
//!    ```
//!
//! Everything else is today's API: `App::build_devices_panel` (its
//! `device_name` is "this device's name"), `App::device_names`,
//! `App::load_alerts_panel`, `App::open_history`, `App::open_pairing`,
//! `App::open_guardian_enroll`, `App::focus_or_toggle(WindowKind::Devices)`,
//! `App::build_a11y_tree`, the wizard (`OnboardingWizard`: its second field
//! on the nickname step is reached with `focus = 1`), the join form
//! (`AuthForm::join()`, its fifth field `fields[4]`).
//!
//! # How the tests run
//!
//! - What writes the workspace (this device's name, a setup, a login) runs in
//!   **child processes** on fresh temporary profiles (`first_run_tests::launch`,
//!   which runs this file's `child_entry` with `FIRST_RUN_CHILD=<scenario>`).
//!   A scenario `name_is:<name>` is a fresh launch that reads this device's
//!   name before any login.
//! - What only reads, or writes a paired-device store in a temporary
//!   directory, runs in this process on an `App` unlocked over an in-memory
//!   store (the unlock sequence of `lot1_tests`), with the node-less handle.
//! - The History rows come from the document service: a version received from
//!   a peer is written by `apply_remote` (its "Synced from {id}" / "Received
//!   from {id}" messages); alerts by `raise_alert`; a waiting document by
//!   `apply_wire` then the thread's deletion, as `orphans_tests` does.
//!
//! **Readings chosen** (the contract leaves them open; reported with the
//! tests):
//! - "This device's name" is what Settings → Devices shows for this device
//!   (`build_devices_panel().device_name`), before and after a login.
//! - The OS host name is read independently of the shell: `COMPUTERNAME` on
//!   Windows, `hostname` elsewhere; compared ignoring ASCII case.
//! - "In a file of its own": the config file is not written by a rename, and
//!   another data directory does not see the name.
//! - The join's name is checked as saved once the join's handshake succeeds
//!   (the submit is real; the handshake's success is the pairing result the
//!   app receives, sent by the test; the login it ends in is faked). That the
//!   join **sends** it in `PairHello` is a minimal source check of `try_join`
//!   (the call to `accept_pairing` reads the form's fifth field).
//! - "Refused with a message": the field stays open, and a message is shown
//!   on the field (`DeviceRename::message`) or as the app's notice.
//! - "Under it" (N2's plain words, N4's sentence): after the field / the line
//!   naming this device, in the accessibility tree's order.
//! - R1's "other messages" are tested with messages that do not begin with
//!   "Synced from " or "Received from "; "Before sync from {id}" (a prior
//!   version's message) is not tested either way.
//! - R2: the message line's rectangle lies within the row's width, above the
//!   time line's and not intersecting it, and is one line high (no taller than
//!   a short message's line in the same panel).
//! - N1b: the document service's device identity is read as it writes, with
//!   today's API: the `device` of a document's stored state and of its
//!   stamp (`DocumentService::stored_state`). "Unlock again" is the unlock's
//!   `install_document_service` run anew on the same store, and a later
//!   launch on a new store.
//! - A rename (N3, N5) follows N2's trim and 64-cluster limit (settled by the
//!   coordinator, 2026-10-24); tested on a paired device's rename.
//! - The update notice (`note_older_peer`) takes its name from the node's
//!   event, which a node-less handle does not emit: left to the manual check.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use accesskit::Role as AkRole;
use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, IncomingAlert, RemoteDocument, Stamp, Trust, WireMeta};
use sovereign_editor::host::ClipboardPrivacy;
use sovereign_editor::input::Modifiers;
use sovereign_p2p::pairing::{PairedDevice, PairingManager};
use sovereign_p2p::P2pCommand;
use vello::kurbo::Rect;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use super::editor_v2::{self, SystemClipboard};
use super::first_run_tests::{
    fill_wizard, first_frame, frames_until, launch, login_settled, press_finish, profile, read_tree, setup_settled,
    shut_runtime, submit_login, Read, CHILD_VAR, DURESS, PRIMARY,
};
use super::{App, WindowContent, WindowKind};
use crate::auth_work::{AuthWork, Session};
use crate::device_rename::RenameTarget;
use crate::onboarding::{WizAction, STEP_CANARY, STEP_NICKNAME};
use crate::panels::HIST_ROW_H;

// ======================================================================= words

/// N2: the setup field's label, and N6: the rename field's name for this device.
const THIS_DEVICES_NAME: &str = "This device's name";
/// N2: the plain words under the setup field.
const SEEN_BY: &str = "Your other devices and your guardians see this name.";
/// N4: under this device's name in Settings → Devices.
const KEEP_LEARNED: &str =
    "Devices already paired with this one keep the name they learned until you pair them again.";
/// N6: the Rename buttons.
const RENAME_THIS: &str = "Rename this device";
fn rename_button(name: &str) -> String {
    format!("Rename {name}")
}
/// N6: a paired device's rename field.
fn name_of(name: &str) -> String {
    format!("Name of {name}")
}
pub(super) const ELLIPSIS: char = '\u{2026}';

/// The name pairing teaches by default, on both sides of BACKLOG §12.
pub(super) const LEARNED: &str = "Sovereign Device";

// ===================================================================== fixtures

fn module() -> &'static str {
    module_path!()
}

/// A libp2p peer id, as the node prints it (`12D3KooW…`).
pub(super) fn peer_id() -> String {
    sovereign_p2p::libp2p::identity::Keypair::generate_ed25519().public().to_peer_id().to_string()
}

pub(super) fn last8(id: &str) -> &str {
    &id[id.len() - 8..]
}

/// The config's device name, as `App::new` loads the config.
fn config_name() -> String {
    sovereign_core::config::AppConfig::load_or_default(None).p2p.device_name
}

/// The OS host name, read without the shell's code.
fn os_host_name() -> String {
    let h = if cfg!(windows) {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    } else {
        std::process::Command::new("hostname")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    assert!(!h.trim().is_empty(), "precondition: the OS host name can be read");
    h.trim().to_string()
}

/// `n` grapheme clusters, known by construction, of several kinds (a
/// decomposed accent, a ZWJ family, a flag, ASCII, an emoji with a skin tone,
/// a precomposed letter). Cluster 63 (the 64th) is the family and cluster 64
/// the flag, so a cut at 64 falls between two multi-code-point clusters.
fn clusters(n: usize) -> Vec<String> {
    const KINDS: [&str; 6] = [
        "e\u{301}",
        "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1E8}\u{1F1ED}",
        "a",
        "\u{1F44D}\u{1F3FD}",
        "\u{00DF}",
    ];
    (0..n).map(|i| KINDS[(i + 4) % KINDS.len()].to_string()).collect()
}

fn long_name_typed() -> String {
    format!("   {}  ", clusters(70).concat())
}

fn long_name_saved() -> String {
    clusters(64).concat()
}

/// This device's name, as Settings → Devices shows it.
pub(super) fn this_name(app: &mut App) -> String {
    app.build_devices_panel().device_name
}

pub(super) fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-device-names-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

#[derive(Default)]
pub(super) struct MemClipboard(Option<String>);
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

/// An `App` unlocked over an in-memory store (the unlock sequence of
/// `App::try_login`, as `lot1_tests` runs it), with one thread.
pub(super) struct Unlocked {
    pub(super) app: App,
    pub(super) store: Arc<EncryptedGraphDB>,
    pub(super) thread: String,
}

pub(super) fn unlocked(tag: &str) -> Unlocked {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    let mut app = App::new();
    let store = app.rt.block_on(testing::encrypted_memory_store(tag));
    let thread = app
        .rt
        .block_on(store.create_thread(Thread::new("Lane".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store.clone());
    // The unlock's background migration ends before the test writes (the
    // race `lot1_tests::wait_for_unlock_migration` describes).
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
    Unlocked { app, store, thread }
}

/// The node-less handle, installed: this node is `own`, started as `name`,
/// paired with `paired` (peer id, learned name) in a store under `dir`.
pub(super) struct Node {
    rx: tokio::sync::mpsc::Receiver<P2pCommand>,
    store_path: PathBuf,
    store_key: [u8; 32],
}

pub(super) fn install_node(app: &mut App, own: &str, name: &str, paired: &[(&str, &str)], dir: &Path) -> Node {
    let store_path = dir.join("paired_devices.json");
    let store_key = [9u8; 32];
    let mut pm = PairingManager::new(store_path.clone());
    for (i, (id, learned)) in paired.iter().enumerate() {
        pm.add_device(PairedDevice::with_key((*id).into(), (*learned).into(), [i as u8 + 1; 32]));
    }
    let (h, rx) = crate::p2p::P2pHandle::fake(own, name, pm, store_key);
    app.p2p = Some(h);
    Node { rx, store_path, store_key }
}

impl Node {
    fn commands(&mut self) -> Vec<P2pCommand> {
        let mut v = Vec::new();
        while let Ok(c) = self.rx.try_recv() {
            v.push(c);
        }
        v
    }
    /// The paired-device store as saved on disk.
    fn saved_names(&self) -> HashMap<String, String> {
        PairingManager::load(&self.store_path, &self.store_key)
            .expect("the paired-device store loads")
            .list_devices()
            .iter()
            .map(|d| (d.peer_id.clone(), d.device_name.clone()))
            .collect()
    }
}

pub(super) fn open_devices(app: &mut App) {
    if !app.windows.iter().any(|w| matches!(&w.content, WindowContent::Devices(_))) {
        app.focus_or_toggle(WindowKind::Devices);
    }
    assert!(
        matches!(app.windows.last().map(|w| &w.content), Some(WindowContent::Devices(_))),
        "Settings → Devices is open on top"
    );
}

// ---- the rename field's keyboard

fn key(app: &mut App, k: WKey, text: Option<&str>, mods: Modifiers) {
    let _ = app.device_rename_key(&k, text, mods);
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        let t = c.to_string();
        if c == ' ' {
            key(app, WKey::Named(WNamed::Space), Some(" "), Modifiers::NONE);
        } else {
            key(app, WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }
}

fn select_all(app: &mut App) {
    key(app, WKey::Character("a".into()), Some("\u{1}"), Modifiers::CTRL);
}

fn enter(app: &mut App) {
    key(app, WKey::Named(WNamed::Enter), Some("\r"), Modifiers::NONE);
}

fn escape(app: &mut App) {
    key(app, WKey::Named(WNamed::Escape), Some("\u{1b}"), Modifiers::NONE);
}

fn backspace(app: &mut App) {
    key(app, WKey::Named(WNamed::Backspace), None, Modifiers::NONE);
}

/// Opens the field on `target`, replaces its text with `name`, and presses Enter.
pub(super) fn rename(app: &mut App, target: RenameTarget, name: &str) {
    app.open_device_rename(target.clone());
    assert!(app.device_rename.is_some(), "the Rename action opens the field ({target:?})");
    select_all(app);
    type_str(app, name);
    enter(app);
    assert!(app.device_rename.is_none(), "Enter saves and closes the field ({target:?} → {name:?})");
}

// ---- the tree

fn index_of(t: &Read, pred: impl Fn(&super::first_run_tests::Seen) -> bool) -> Option<usize> {
    t.order.iter().position(pred)
}

fn field_named<'a>(t: &'a Read, name: &str) -> Option<&'a super::first_run_tests::Seen> {
    t.order.iter().find(|s| super::first_run_tests::is_field(s.role) && s.name() == name)
}

// ---- documents

fn v2_text(text: &str) -> String {
    serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": text}]})
    .to_string()
}

pub(super) fn remote(id: &str, thread: &str, title: &str, text: &str, device: &str, minutes: i64) -> RemoteDocument {
    RemoteDocument {
        id: id.into(),
        title: title.into(),
        content: v2_text(text),
        thread_id: thread.into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: chrono::Utc::now() + chrono::Duration::minutes(minutes),
        deleted_at: None,
        device: device.into(),
        trust_decision: None,
    }
}

pub(super) fn docs(app: &App) -> Arc<DocumentService> {
    app.docs.clone().expect("the workspace is unlocked")
}

/// A document created here, then synced from each of `from` in turn: one
/// "Synced from {id}" version each. Returns its id.
fn synced_doc(u: &mut Unlocked, title: &str, from: &[&str]) -> String {
    let d = docs(&u.app);
    let id = u.app.rt.block_on(d.create(new_user_document(title, &u.thread, "local text"))).unwrap().id;
    for (i, dev) in from.iter().enumerate() {
        let r = remote(&id, &u.thread, title, &format!("from {i}"), dev, i as i64 + 1);
        u.app.rt.block_on(d.apply_remote(r)).expect("the remote state applies");
    }
    let msgs = stored_messages(&u.app, &id);
    for dev in from {
        assert!(
            msgs.iter().any(|m| m == &format!("Synced from {dev}")),
            "precondition: a version \"Synced from {dev}\" is stored: {msgs:?}"
        );
    }
    u.app.load_workspace_now();
    id
}

/// A shown line of R1's two forms ("Before sync from {id}", a prior
/// version's message, is not one of them).
fn names_a_device(line: &str) -> bool {
    line.contains("Synced from ") || line.contains("Received from ")
}

pub(super) fn stored_messages(app: &App, id: &str) -> Vec<String> {
    app.rt.block_on(docs(app).versions(id)).unwrap().into_iter().map(|v| v.message).collect()
}

/// Opens `id`'s History (closing one already open) and returns its window.
fn history(app: &mut App, id: &str) -> usize {
    app.windows.retain(|w| !matches!(&w.content, WindowContent::History(_)));
    app.open_history(id, "Notes");
    app.windows
        .iter()
        .position(|w| matches!(&w.content, WindowContent::History(h) if h.doc_id == id))
        .expect("the History window opens")
}

pub(super) fn history_lines(app: &mut App, id: &str) -> Vec<String> {
    let wi = history(app, id);
    match &app.windows[wi].content {
        WindowContent::History(hp) => hp.a11y_lines(),
        _ => unreachable!(),
    }
}

// ================================================================== the fake login

/// The login that ends a join, faked (a real one needs the other device):
/// `unlock` opens a session over an in-memory store.
struct FakeUnlock {
    session: Session,
}

impl FakeUnlock {
    fn new(app: &App) -> Arc<FakeUnlock> {
        let db = app.rt.block_on(testing::encrypted_memory_store("device-names-join"));
        let mk = sovereign_crypto::master_key::MasterKey::generate();
        Arc::new(FakeUnlock {
            session: (
                sovereign_core::auth::PersonaKind::Primary,
                db,
                testing::account_key(),
                Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&mk, "device-names-join").unwrap()),
                Arc::new(sovereign_crypto::kek::Kek::generate()),
            ),
        })
    }
}

impl AuthWork for FakeUnlock {
    fn create_store(&self, _primary: &[u8], _duress: &[u8]) -> anyhow::Result<sovereign_crypto::auth::AuthStore> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
    fn install(&self, _store: &sovereign_crypto::auth::AuthStore, _password: &[u8]) -> anyhow::Result<Session> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
    fn unlock(&self, _password: &[u8]) -> anyhow::Result<Session> {
        Ok(self.session.clone())
    }
    fn recover(&self, _new_password: &[u8]) -> anyhow::Result<Session> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
}

// ============================================================ child launches

/// One launch on the profile of `SOVEREIGN_DATA_DIR` (set by the parent).
/// Ignored in ordinary runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    if let Some(expected) = scenario.strip_prefix("name_is:") {
        return name_is(expected);
    }
    match scenario.as_str() {
        "name_is_config" => name_is(&config_name()),
        "name_is_long" => name_is(&long_name_saved()),
        "this_device_keys" => this_device_keys(),
        "offers_after_rename" => offers_after_rename(),
        "setup_prefill" => setup_prefill(),
        "setup_named" => setup_with_name("  Studio PC  "),
        "setup_blank" => setup_with_name("   "),
        "join_long_name" => join_with_name(&long_name_typed()),
        "store_only" => {
            crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
        }
        "duress_renames" => duress_renames(),
        "primary_sees_shared" => primary_sees_shared(),
        "identity_rename" => identity_rename(),
        "identity_next_launch" => identity_next_launch(),
        other => panic!("unknown scenario {other}"),
    }
}

/// A fresh launch: this device's name, before any login.
fn name_is(expected: &str) {
    let mut app = App::new();
    let got = this_name(&mut app);
    assert_eq!(got, expected, "this device's name, read at start (N1)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn config_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/default.toml")
}

fn file_hash(p: &Path) -> Option<Vec<u8>> {
    std::fs::read(p).ok()
}

/// N3 on this device: the field opens holding the current name; Escape and a
/// blank entry change nothing; Enter saves at once (N1); the config file is
/// not written.
fn this_device_keys() {
    let config_before = file_hash(&config_file());
    let mut u = unlocked("n3-keys");
    let app = &mut u.app;
    open_devices(app);
    let before = this_name(app);
    assert_eq!(before, config_name(), "precondition: a fresh workspace shows the config's name (N1)");

    app.open_device_rename(RenameTarget::ThisDevice);
    let field = app.device_rename.as_ref().expect("\"Rename this device\" opens the field (N3)");
    assert_eq!(field.name(), before, "the field opens holding this device's name");

    // Escape cancels.
    select_all(app);
    type_str(app, "Discarded name");
    escape(app);
    assert!(app.device_rename.is_none(), "Escape closes the field (N3)");
    assert_eq!(this_name(app), before, "Escape changes nothing (N3)");

    // A blank entry is refused with a message; the field stays open.
    app.open_device_rename(RenameTarget::ThisDevice);
    select_all(app);
    backspace(app);
    type_str(app, "   ");
    enter(app);
    let field = app.device_rename.as_ref().expect("a blank entry is refused: the field stays open (N3)");
    let said = field.message().map(str::to_string).filter(|m| !m.trim().is_empty());
    assert!(
        said.is_some() || app.notice.is_some(),
        "a blank entry is refused **with a message** (N3): none on the field, no notice"
    );
    assert_eq!(this_name(app), before, "a refused entry changes nothing (N3)");
    escape(app);
    assert!(app.device_rename.is_none());

    // Enter saves, at once.
    rename(app, RenameTarget::ThisDevice, "Atelier Nord");
    assert_eq!(this_name(app), "Atelier Nord", "the new name is saved at once (N3, N1)");
    assert_eq!(file_hash(&config_file()), config_before, "the rename does not write the config file (N1)");
    shut_runtime(&mut u.app);
    std::mem::forget(u);
}

/// N3: after a rename, the next pairing offer and the guardian offer carry
/// the new name, without a restart.
fn offers_after_rename() {
    let mut u = unlocked("n3-offers");
    let dir = tempfile::tempdir().unwrap();
    let own = peer_id();
    let start_name = this_name(&mut u.app);
    let mut node = install_node(&mut u.app, &own, &start_name, &[], dir.path());
    // A pairing offer releases the MasterKey salt: the profile needs one.
    std::fs::create_dir_all(crate::crypto::crypto_dir()).unwrap();
    std::fs::write(crate::crypto::crypto_dir().join("salt"), [7u8; 32]).unwrap();

    open_devices(&mut u.app);
    rename(&mut u.app, RenameTarget::ThisDevice, "Atelier Nord");
    node.commands();

    // The pairing offer: its QR / code, and what the node is armed with.
    u.app.open_pairing();
    let code = match &u.app.pairing_modal {
        Some(m) => m.code.clone(),
        None => panic!("the pairing offer is armed (notice: {:?})", u.app.notice.as_ref().map(|n| n.said.clone())),
    };
    let offer = sovereign_p2p::PairingOffer::decode(&code).expect("the offer decodes");
    assert_eq!(offer.source_device_name, "Atelier Nord", "the pairing offer names this device by its new name (N3)");
    let armed: Vec<String> = node
        .commands()
        .into_iter()
        .filter_map(|c| match c {
            P2pCommand::SetPairingOffer { offer } => Some(offer.device_name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(armed, vec!["Atelier Nord".to_string()], "the node is armed with the new name (N3)");

    // The guardian offer (the primary's real one).
    u.app.pairing_modal = None;
    u.app.kek = Some(Arc::new(sovereign_crypto::kek::Kek::generate()));
    u.app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    u.app.open_guardian_enroll();
    assert!(
        u.app.guardian_modal.is_some(),
        "the guardian offer is armed (notice: {:?})",
        u.app.notice.as_ref().map(|n| n.said.clone())
    );
    let labels: Vec<String> = node
        .commands()
        .into_iter()
        .filter_map(|c| match c {
            P2pCommand::SetGuardianOffer { offer } => Some(offer.owner_label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, vec!["Atelier Nord".to_string()], "the guardian offer names this device by its new name (N3)");
    shut_runtime(&mut u.app);
    std::mem::forget(u);
}

/// N2: the setup fields, prefilled with the host name.
fn setup_prefill() {
    let host = os_host_name();
    let mut app = App::new();
    first_frame(&mut app);
    assert!(app.locked && app.wizard.is_some(), "precondition: a fresh profile shows the setup wizard");
    app.wizard.as_mut().unwrap().step = STEP_NICKNAME;
    app.wizard.as_mut().unwrap().focus = 0;
    let t = read_tree(&mut app);
    assert_eq!(
        t.field_names(),
        vec!["Nickname".to_string(), THIS_DEVICES_NAME.to_string()],
        "the nickname step has a second field, \"This device's name\" (N2)\n{}",
        t.dump()
    );
    let f = field_named(&t, THIS_DEVICES_NAME).unwrap();
    assert_eq!(f.role, AkRole::TextInput, "an editable text field (N2, N6)");
    let v = f.value.clone().unwrap_or_default();
    assert!(v.eq_ignore_ascii_case(&host), "the field is prefilled with the computer's name {host:?}, holds {v:?} (N2)");
    let at_field = index_of(&t, |s| super::first_run_tests::is_field(s.role) && s.name() == THIS_DEVICES_NAME).unwrap();
    let at_words = index_of(&t, |s| s.name() == SEEN_BY);
    assert!(
        at_words.is_some_and(|i| i > at_field),
        "under the field, in plain words: {SEEN_BY:?} (N2)\n{}",
        t.dump()
    );

    // The join form: the same field, fifth.
    app.handle_wiz_action(WizAction::ChoosePair);
    assert!(app.auth_form.joining, "precondition: the join form is shown");
    assert_eq!(app.auth_form.labels.get(4).copied(), Some(THIS_DEVICES_NAME), "the join form's fifth field (N2)");
    let t = read_tree(&mut app);
    let names = t.field_names();
    assert_eq!(names.len(), 5, "the join form has five fields (N2): {names:?}");
    assert_eq!(names[4], THIS_DEVICES_NAME, "named by its visible label (N6)\n{}", t.dump());
    let f = field_named(&t, THIS_DEVICES_NAME).unwrap();
    assert_eq!(f.role, AkRole::TextInput, "an editable text field (N2)");
    let v = f.value.clone().unwrap_or_default();
    assert!(v.eq_ignore_ascii_case(&host), "the join form's field is prefilled with {host:?}, holds {v:?} (N2)");
    let at_field = index_of(&t, |s| super::first_run_tests::is_field(s.role) && s.name() == THIS_DEVICES_NAME).unwrap();
    assert!(
        index_of(&t, |s| s.name() == SEEN_BY).is_some_and(|i| i > at_field),
        "under the join form's field, in plain words: {SEEN_BY:?} (N2)\n{}",
        t.dump()
    );
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// A real first-device setup with `typed` in the device-name field.
fn setup_with_name(typed: &str) {
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    {
        let w = app.wizard.as_mut().unwrap();
        w.step = STEP_NICKNAME;
        w.focus = 1;
        for _ in 0..200 {
            w.backspace();
        }
        w.insert_str(typed);
        w.step = STEP_CANARY;
        w.focus = 0;
    }
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(!app.locked, "setup unlocks: {:?}", app.wizard.as_ref().and_then(|w| w.error.clone()));
    frames_until(&mut app, 1, |_| false);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// A join with `typed` in the join form's fifth field: the submit is real
/// (`auth_submit` → `try_join`, which validates and starts the handshake); the
/// handshake's success is the pairing result the app receives; the login it
/// ends in is faked.
fn join_with_name(typed: &str) {
    let mut app = App::new();
    first_frame(&mut app);
    app.handle_wiz_action(WizAction::ChoosePair);
    assert!(app.auth_form.joining && app.auth_form.fields.len() == 5, "the join form has five fields (N2)");
    // An offer from a device that never answers (a documentation address), so
    // the real handshake waits while the test answers in its place.
    let offer = sovereign_p2p::PairingOffer::new(peer_id(), "Source".into(), vec!["/ip4/192.0.2.1/udp/9/quic-v1".into()], 600)
        .encode()
        .unwrap();
    app.auth_form.fields[0] = offer;
    app.auth_form.fields[1] = "ABCDE-FGHJK".into();
    app.auth_form.fields[2] = PRIMARY.into();
    app.auth_form.fields[3] = DURESS.into();
    app.auth_form.fields[4] = typed.into();
    app.auth_submit();
    assert!(app.auth_form.busy, "the join starts (no validation error: {:?})", app.auth_form.error);
    app.auth_work = FakeUnlock::new(&app);
    app.pair_tx.send(Ok(())).unwrap();
    assert!(frames_until(&mut app, 30, login_settled), "the join's login ends");
    assert!(!app.locked, "the joined account opens: {:?}", app.auth_form.error);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// The duress persona renames the device.
fn duress_renames() {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, DURESS);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the duress persona opens: {:?}", app.auth_form.error);
    assert_eq!(app.persona, Some(sovereign_core::auth::PersonaKind::Duress));
    open_devices(&mut app);
    rename(&mut app, RenameTarget::ThisDevice, "Shared desk");
    assert_eq!(this_name(&mut app), "Shared desk");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// The primary reads the name the duress persona gave, before and after its login.
fn primary_sees_shared() {
    let mut app = App::new();
    assert_eq!(this_name(&mut app), "Shared desk", "the name is the device's, read at start (N1)");
    first_frame(&mut app);
    submit_login(&mut app, PRIMARY);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the primary opens: {:?}", app.auth_form.error);
    assert_eq!(app.persona, Some(sovereign_core::auth::PersonaKind::Primary));
    assert_eq!(this_name(&mut app), "Shared desk", "both personas read the same name (N1 [choice])");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ================================================================== N1

#[test]
fn n1_a_fresh_workspace_uses_the_configs_name() {
    let p = profile();
    launch(module(), "name_is_config", p.path());
}

#[test]
fn n1_the_name_is_stored_in_the_workspace_read_at_start_and_overrides_the_config() {
    let p = profile();
    launch(module(), "this_device_keys", p.path());
    // "Atelier Nord" differs from the config's name (checked in the launch).
    launch(module(), "name_is:Atelier Nord", p.path());
    launch(module(), "name_is:Atelier Nord", p.path());
    // Another workspace (data directory) has its own.
    let other = profile();
    launch(module(), "name_is_config", other.path());
}

#[test]
fn n1_both_personas_read_and_change_the_same_name() {
    let p = profile();
    launch(module(), "store_only", p.path());
    launch(module(), "duress_renames", p.path());
    launch(module(), "primary_sees_shared", p.path());
}

// ================================================================== N1b

/// The document service's device identity as it writes: the `device` of a
/// document's stored state and of the stamp of the write that produced it
/// (today's API, `DocumentService::stored_state`).
fn service_identity(app: &App, id: &str) -> (String, Option<String>) {
    let st = app.rt.block_on(docs(app).stored_state(id)).expect("the stored state reads");
    (st.document.device, st.meta.stamp.map(|s| s.device))
}

/// Where `identity_rename` leaves the identity for the next launch (the
/// test's own file, in the profile; nothing of the app's).
fn identity_file() -> PathBuf {
    sovereign_core::sovereign_dir().join("n1b-test-identity.txt")
}

fn new_doc(u: &Unlocked, title: &str) -> String {
    u.app.rt.block_on(docs(&u.app).create(new_user_document(title, &u.thread, "text"))).unwrap().id
}

/// N1b: the identity before a rename; a rename; the unlock again (the
/// unlock's document-service installation, on the same store): what is
/// stored and what is written next keep the identity.
fn identity_rename() {
    let mut u = unlocked("n1b-rename");
    let before_doc = new_doc(&u, "Before");
    let before = service_identity(&u.app, &before_doc);
    assert!(!before.0.is_empty(), "precondition: the service writes a device identity");
    std::fs::write(identity_file(), serde_json::to_string(&before).unwrap()).unwrap();

    open_devices(&mut u.app);
    rename(&mut u.app, RenameTarget::ThisDevice, "Atelier Nord");
    assert_eq!(this_name(&mut u.app), "Atelier Nord", "precondition: renamed");
    // Written right after the rename, before any new unlock.
    let after_doc = new_doc(&u, "Right after");
    assert_eq!(service_identity(&u.app, &after_doc), before, "a rename does not change the service's device identity (N1b)");

    // Unlock again: the document service is installed anew.
    u.app.install_document_service(u.store.clone());
    if let Some(task) = u.app.migration_task.take() {
        let _ = u.app.rt.block_on(task);
    }
    let next_doc = new_doc(&u, "Next unlock");
    assert_eq!(service_identity(&u.app, &next_doc), before, "after the next unlock, the identity is unchanged (N1b)");
    assert_eq!(service_identity(&u.app, &before_doc), before, "what is already stored keeps its identity (N1b)");
    shut_runtime(&mut u.app);
    std::mem::forget(u);
}

/// N1b, a launch later: the renamed device's service writes the identity it
/// wrote before the rename.
fn identity_next_launch() {
    let before: (String, Option<String>) =
        serde_json::from_str(&std::fs::read_to_string(identity_file()).expect("the identity recorded before")).unwrap();
    let mut u = unlocked("n1b-next-launch");
    assert_eq!(this_name(&mut u.app), "Atelier Nord", "precondition: the rename was kept");
    let id = new_doc(&u, "Next launch");
    assert_eq!(service_identity(&u.app, &id), before, "at the next launch, the identity is unchanged (N1b)");
    shut_runtime(&mut u.app);
    std::mem::forget(u);
}

#[test]
fn n1b_a_rename_never_changes_the_document_services_device_identity() {
    let p = profile();
    launch(module(), "identity_rename", p.path());
    launch(module(), "identity_next_launch", p.path());
}

// ================================================================== N2

#[test]
fn n2_the_setup_fields_ask_for_the_name_prefilled_with_the_host_name() {
    let p = profile();
    launch(module(), "setup_prefill", p.path());
}

#[test]
fn n2_the_wizards_name_is_saved_trimmed_with_the_setup() {
    let p = profile();
    launch(module(), "setup_named", p.path());
    launch(module(), "name_is:Studio PC", p.path());
}

#[test]
fn n2_a_blank_name_at_setup_means_the_default() {
    let p = profile();
    launch(module(), "setup_blank", p.path());
    launch(module(), "name_is_config", p.path());
}

#[test]
fn n2_the_joins_name_is_saved_trimmed_to_64_grapheme_clusters() {
    let p = profile();
    launch(module(), "join_long_name", p.path());
    launch(module(), "name_is_long", p.path());
}

#[test]
fn n2_clean_trims_and_blank_means_the_default() {
    assert_eq!(crate::device_name::clean(""), None);
    assert_eq!(crate::device_name::clean("   "), None);
    assert_eq!(crate::device_name::clean(" \t \n "), None);
    assert_eq!(crate::device_name::clean("  Studio PC  ").as_deref(), Some("Studio PC"));
    assert_eq!(crate::device_name::clean("Bureau").as_deref(), Some("Bureau"));
}

#[test]
fn n2_clean_keeps_64_grapheme_clusters_and_never_cuts_inside_one() {
    let exactly = clusters(64).concat();
    assert_eq!(crate::device_name::clean(&exactly).as_deref(), Some(exactly.as_str()), "64 clusters are kept whole");
    let one_more = clusters(65).concat();
    assert_eq!(crate::device_name::clean(&one_more).as_deref(), Some(exactly.as_str()), "the 65th cluster goes, whole");
    assert_eq!(
        crate::device_name::clean(&long_name_typed()).as_deref(),
        Some(long_name_saved().as_str()),
        "trimmed, then the first 64 clusters"
    );
    let accents = "e\u{301}".repeat(70);
    assert_eq!(
        crate::device_name::clean(&accents).as_deref(),
        Some("e\u{301}".repeat(64).as_str()),
        "a decomposed accent is one cluster: 64 of them, each with its accent"
    );
    let flags = "\u{1F1E8}\u{1F1ED}".repeat(65);
    assert_eq!(
        crate::device_name::clean(&flags).as_deref(),
        Some("\u{1F1E8}\u{1F1ED}".repeat(64).as_str()),
        "a flag is one cluster: never half a flag"
    );
}

/// The join sends the form's name in `PairHello`: `try_join` hands
/// `accept_pairing` (whose name goes into `PairHello`) a name read from the
/// form's fifth field. A minimal source check at the call site.
#[test]
fn n2_the_join_sends_the_forms_name() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs")).unwrap();
    let src = match src.find("#[cfg(test)]\nmod taskbar_tests") {
        Some(at) => &src[..at],
        None => &src[..],
    };
    let code = crate::field_contrast_tests::blank_line_comments(src);
    let body = crate::field_contrast_tests::fn_body(&code, "try_join", "src/app.rs");
    assert!(body.contains("accept_pairing("), "try_join starts the handshake with accept_pairing");
    let squeezed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        squeezed.contains("get(4)") || squeezed.contains("[4]"),
        "try_join reads the join form's fifth field (the name it sends, N2):\n{body}"
    );
}

// ================================================================== N3, N5, N6 (in this process)

#[test]
fn n3_this_devices_rename_opens_cancels_refuses_and_saves() {
    let p = profile();
    launch(module(), "this_device_keys", p.path());
}

#[test]
fn n3_pairing_and_the_guardian_offer_use_the_new_name_without_a_restart() {
    let p = profile();
    launch(module(), "offers_after_rename", p.path());
}

#[test]
fn n4_n6_settings_devices_says_names_are_learned_and_names_its_rename_actions() {
    let mut u = unlocked("n6-devices");
    let dir = tempfile::tempdir().unwrap();
    let (own, a, b) = (peer_id(), peer_id(), peer_id());
    let this = this_name(&mut u.app);
    // Paired names apart from this device's, so "under this device's name"
    // cannot match a paired row.
    let _node = install_node(&mut u.app, &own, &this, &[(&a, "Laptop"), (&b, "Phone")], dir.path());
    open_devices(&mut u.app);
    let t = read_tree(&mut u.app);

    // N4, under this device's name.
    let at_name = index_of(&t, |s| !super::first_run_tests::is_button(s.role) && s.name().contains(&this))
        .unwrap_or_else(|| panic!("this device's name {this:?} is in the tree\n{}", t.dump()));
    let at_words = index_of(&t, |s| s.name() == KEEP_LEARNED);
    assert!(at_words.is_some_and(|i| i > at_name), "under this device's name: {KEEP_LEARNED:?} (N4)\n{}", t.dump());

    // N6, the Rename buttons.
    let buttons = t.button_names();
    for want in [RENAME_THIS.to_string(), rename_button("Laptop"), rename_button("Phone")] {
        assert!(buttons.contains(&want), "a button named {want:?} (N6): {buttons:?}\n{}", t.dump());
    }

    // N6, the fields.
    u.app.open_device_rename(RenameTarget::ThisDevice);
    let t = read_tree(&mut u.app);
    assert!(field_named(&t, THIS_DEVICES_NAME).is_some(), "the open field is a text input named {THIS_DEVICES_NAME:?} (N6)\n{}", t.dump());
    escape(&mut u.app);
    assert!(u.app.device_rename.is_none());
    u.app.open_device_rename(RenameTarget::Paired(b.clone()));
    let t = read_tree(&mut u.app);
    assert!(
        field_named(&t, &name_of("Phone")).is_some(),
        "a paired device's open field is a text input named {:?} (N6)\n{}",
        name_of("Phone"),
        t.dump()
    );
    escape(&mut u.app);
}

#[test]
fn n5_a_paired_devices_rename_opens_cancels_refuses_and_saves_here_only() {
    let mut u = unlocked("n5-rename");
    let dir = tempfile::tempdir().unwrap();
    let (own, a, b) = (peer_id(), peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let node = install_node(&mut u.app, &own, &this, &[(&a, LEARNED), (&b, LEARNED)], dir.path());
    let app = &mut u.app;
    open_devices(app);

    app.open_device_rename(RenameTarget::Paired(a.clone()));
    let field = app.device_rename.as_ref().expect("the row's Rename opens the field (N5)");
    assert_eq!(field.name(), LEARNED, "holding the name this device learned");

    // Escape cancels.
    select_all(app);
    type_str(app, "Discarded");
    escape(app);
    assert!(app.device_rename.is_none(), "Escape closes the field (N5)");
    assert_eq!(app.device_names().get(&a).map(String::as_str), Some(LEARNED), "Escape changes nothing (N5)");

    // A blank entry is refused with a message.
    app.open_device_rename(RenameTarget::Paired(a.clone()));
    select_all(app);
    backspace(app);
    enter(app);
    let field = app.device_rename.as_ref().expect("a blank entry is refused: the field stays open (N5)");
    let said = field.message().map(str::to_string).filter(|m| !m.trim().is_empty());
    assert!(said.is_some() || app.notice.is_some(), "a blank entry is refused with a message (N5)");
    assert_eq!(app.device_names().get(&a).map(String::as_str), Some(LEARNED), "a refused entry changes nothing");
    escape(app);

    // Enter saves, in this device's paired list.
    rename(app, RenameTarget::Paired(a.clone()), "Studio laptop");
    let names = app.device_names();
    assert_eq!(names.get(&a).map(String::as_str), Some("Studio laptop"), "device_names() uses the new label (N5)");
    assert_eq!(names.get(&b).map(String::as_str), Some(LEARNED), "the other device keeps its name");
    let rows: Vec<(String, String)> = app.build_devices_panel().rows.into_iter().map(|r| (r.peer_id, r.name)).collect();
    assert!(rows.contains(&(a.clone(), "Studio laptop".to_string())), "the device list uses it (N5): {rows:?}");
    assert_eq!(
        node.saved_names().get(&a).map(String::as_str),
        Some("Studio laptop"),
        "the label replaces the stored name in this device's paired list (N5)"
    );
    assert_eq!(this_name(app), this, "this device's own name is unchanged");

    // A rename follows N3's rules, so N2's: trimmed, at most 64 grapheme
    // clusters, never cut inside one (settled 2026-10-24 by the coordinator).
    rename(app, RenameTarget::Paired(b.clone()), &long_name_typed());
    assert_eq!(
        app.device_names().get(&b),
        Some(&long_name_saved()),
        "a rename is trimmed and kept to 64 grapheme clusters (N5 → N3 → N2)"
    );
}

#[test]
fn n5_r1_a_local_rename_changes_what_history_alerts_and_orphans_show() {
    let mut u = unlocked("n5-shown");
    let dir = tempfile::tempdir().unwrap();
    let (own, a, b, gone) = (peer_id(), peer_id(), peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&a, LEARNED), (&b, LEARNED)], dir.path());

    let synced = synced_doc(&mut u, "Notes", &[&a, &gone, &own]);
    // A document this device did not have: "Received from {a}".
    let received = "document:receiveda1".to_string();
    let d = docs(&u.app);
    u.app.rt.block_on(d.apply_remote(remote(&received, &u.thread, "Arrived", "new", &a, 1))).unwrap();
    assert!(stored_messages(&u.app, &received).contains(&format!("Received from {a}")), "precondition");
    // An alert raised by a write from `a`.
    u.app
        .rt
        .block_on(d.raise_alert(IncomingAlert {
            id: synced.clone(),
            device: a.clone(),
            finding: "injected instructions in the new content".into(),
            previous_version: None,
        }))
        .unwrap();
    // A document written on `a`, waiting in a thread deleted there.
    let doomed = u
        .app
        .rt
        .block_on(u.store.create_thread(Thread::new("Old drafts".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    let waiting = remote("document:waitinga1", &doomed, "Waiting", "w", &a, 0);
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: chrono::Utc::now().timestamp_millis(), counter: 0, device: a.clone() }),
        ..WireMeta::default()
    };
    u.app.rt.block_on(d.apply_wire(waiting, meta)).expect("applied");
    u.app.rt.block_on(u.store.soft_delete_thread(&doomed)).unwrap();
    u.app.load_workspace_now();

    // Before: the learned name, "this device", the short id.
    let lines = history_lines(&mut u.app, &synced).join("\n");
    assert!(lines.contains(&format!("Synced from {LEARNED}")), "R1: the paired device's name\n{lines}");
    assert!(lines.contains("Synced from this device"), "R1: \"this device\" for this device's own id\n{lines}");
    assert!(lines.contains(&format!("Synced from {ELLIPSIS}{}", last8(&gone))), "R1: a forgotten device's short id\n{lines}");
    for l in lines.lines().filter(|l| names_a_device(l)) {
        for id in [&a, &gone, &own] {
            assert!(!l.contains(id.as_str()), "R1: no peer id in place of a name ({id}): {l}");
        }
    }

    // Rename `a` here.
    open_devices(&mut u.app);
    rename(&mut u.app, RenameTarget::Paired(a.clone()), "Studio laptop");

    let lines = history_lines(&mut u.app, &synced).join("\n");
    assert!(lines.contains("Synced from Studio laptop"), "History uses the new label (N5, R1)\n{lines}");
    assert!(!lines.contains(&format!("Synced from {LEARNED}")), "not the learned name any more\n{lines}");
    let lines = history_lines(&mut u.app, &received).join("\n");
    assert!(lines.contains("Received from Studio laptop"), "\"Received from\" too (R1)\n{lines}");

    let ap = u.app.load_alerts_panel().expect("the alerts load");
    let alert = ap.rows.iter().find(|r| r.doc_id == synced).expect("the alert is listed");
    assert_eq!(alert.device, "Studio laptop", "the alert uses the new label (N5)");
    let line = ap.orphans().lines().iter().find(|l| l.id == "document:waitinga1").expect("the waiting document is listed");
    assert_eq!(line.device, "Studio laptop", "the waiting document uses the new label (N5)");

    // The stored messages are unchanged (R1: display only).
    let msgs = stored_messages(&u.app, &synced);
    for id in [&a, &gone, &own] {
        assert!(msgs.contains(&format!("Synced from {id}")), "the stored message keeps the id: {msgs:?}");
    }
}

// ================================================================== R1 (pure)

#[test]
fn r1_shown_message_names_the_device() {
    let (own, a, b, gone) = (peer_id(), peer_id(), peer_id(), peer_id());
    let names = HashMap::from([(a.clone(), "Studio laptop".to_string()), (b.clone(), LEARNED.to_string())]);
    let shown = |m: &str| crate::history_device::shown_message(m, &names, &own);

    assert_eq!(shown(&format!("Synced from {a}")), "Synced from Studio laptop");
    assert_eq!(shown(&format!("Received from {a}")), "Received from Studio laptop");
    assert_eq!(shown(&format!("Synced from {b}")), format!("Synced from {LEARNED}"));
    assert_eq!(shown(&format!("Synced from {own}")), "Synced from this device");
    assert_eq!(shown(&format!("Received from {own}")), "Received from this device");
    assert_eq!(shown(&format!("Synced from {gone}")), format!("Synced from {ELLIPSIS}{}", last8(&gone)));
    assert_eq!(shown(&format!("Received from {gone}")), format!("Received from {ELLIPSIS}{}", last8(&gone)));
}

#[test]
fn r1_two_forgotten_devices_read_apart() {
    let own = peer_id();
    let names = HashMap::new();
    let (x, y) = (peer_id(), peer_id());
    assert!(x.starts_with("12D3KooW") && y.starts_with("12D3KooW"), "precondition: libp2p ids share their first 8");
    let sx = crate::history_device::shown_message(&format!("Synced from {x}"), &names, &own);
    let sy = crate::history_device::shown_message(&format!("Synced from {y}"), &names, &own);
    assert_ne!(sx, sy, "the short form is the id's end, which differs");
    assert!(!sx.contains("12D3KooW"), "not the id's start: {sx}");
}

#[test]
fn r1_other_messages_are_shown_as_they_are() {
    let (own, a) = (peer_id(), peer_id());
    let names = HashMap::from([(a.clone(), "Studio laptop".to_string())]);
    for m in [
        "Autosaved",
        "Restored version of 2026-10-01 12:00",
        "Summarised by the skill Digest",
        "(no message)",
        "Synced manually",
        "",
    ] {
        assert_eq!(crate::history_device::shown_message(m, &names, &own), m, "{m:?} is shown as it is");
    }
}

// ================================================================== R2

/// The History row's message size today (`HistoryPanel::ensure_shaped`).
const MESSAGE_PX: f32 = 14.0;

#[test]
fn r2_cut_to_width_fits_one_line_with_an_ellipsis() {
    let mut shaper = crate::text::TextShaper::new();
    let id = peer_id();
    let texts = [
        format!("Synced from {}", "Workstation of the third-floor laboratory, left of the window ".repeat(2)),
        format!("Received from {id}"),
        format!("Synced from {ELLIPSIS}{}", last8(&id)),
        "Autosaved".to_string(),
        format!("Synced from {}", "W".repeat(64)),
    ];
    for text in &texts {
        let full_w = shaper.shape(text, 1.0e6, MESSAGE_PX).width() as f64;
        for width in [60.0, 100.0, 160.0, 240.0, 400.0, 640.0, 1200.0] {
            let cut = crate::history_device::cut_to_width(&mut shaper, text, width, MESSAGE_PX);
            let lay = shaper.shape(&cut, 1.0e6, MESSAGE_PX);
            assert!(lay.len() <= 1, "one line: {cut:?}");
            assert!(lay.width() as f64 <= width + 0.5, "shaped width {} ≤ {width}: {cut:?}", lay.width());
            if full_w <= width {
                assert_eq!(&cut, text, "a message that fits is not cut (width {width})");
            } else {
                let kept = cut.strip_suffix(ELLIPSIS).unwrap_or_else(|| panic!("a cut ends with \"…\": {cut:?}"));
                assert!(text.starts_with(kept), "a cut keeps the message's start: {cut:?} of {text:?}");
            }
        }
    }
}

#[test]
fn r2_a_rows_message_line_never_overlaps_its_time_line_and_keeps_its_full_text() {
    let mut u = unlocked("r2-rows");
    let dir = tempfile::tempdir().unwrap();
    let (own, a, gone) = (peer_id(), peer_id(), peer_id());
    let long = "Workstation of the third-floor laboratory, left of the window, by the old printer".repeat(2);
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&a, &long)], dir.path());
    let id = synced_doc(&mut u, "Notes", &[&a, &gone]);
    let wi = history(&mut u.app, &id);
    let mut shaper = crate::text::TextShaper::new();
    let WindowContent::History(hp) = &mut u.app.windows[wi].content else { unreachable!() };

    let full = format!("Synced from {long}");
    let lines = hp.a11y_lines();
    assert!(lines.iter().any(|l| l.contains(&full)), "the full message stays in the row's accessible text (R2)\n{lines:?}");
    let n = hp.commits.len();
    assert!(n >= 3, "precondition: the creation and two synced versions");

    for width in [240.0, 320.0, 480.0, 640.0, 900.0, 1400.0] {
        hp.ensure_shaped(&mut shaper, width);
        let rows: Vec<Rect> = (0..n).map(|i| Rect::new(0.0, 10.0 + i as f64 * HIST_ROW_H, width, 10.0 + (i + 1) as f64 * HIST_ROW_H)).collect();
        let rects: Vec<(Rect, Rect)> = (0..n).map(|i| hp.row_line_rects(i, rows[i])).collect();
        let one_line_h = (0..n)
            .min_by_key(|&i| hp.drawn_message(i).chars().count())
            .map(|i| rects[i].0.height())
            .unwrap();
        for i in 0..n {
            let (msg, time) = rects[i];
            let row = rows[i];
            let drawn = hp.drawn_message(i);
            assert!(msg.x0 >= row.x0 - 0.5 && msg.x1 <= row.x1 + 0.5, "row {i} at {width}: the message {msg:?} within the row {row:?} ({drawn:?})");
            let overlap = msg.intersect(time);
            assert!(
                overlap.width() <= 0.0 || overlap.height() <= 0.0,
                "row {i} at {width}: the message {msg:?} and the time line {time:?} intersect ({drawn:?})"
            );
            assert!(msg.y1 <= time.y0 + 0.5, "row {i} at {width}: the time line is under the message");
            assert!(msg.height() <= one_line_h + 0.5, "row {i} at {width}: the message is one line ({drawn:?}, {msg:?})");
            if let Some(kept) = drawn.strip_suffix(ELLIPSIS) {
                assert!(lines.iter().any(|l| l.contains(kept)), "row {i}: the drawn line {drawn:?} is the start of its message");
            }
            if names_a_device(&drawn) {
                assert!(!drawn.contains(a.as_str()) && !drawn.contains(gone.as_str()), "row {i}: no peer id drawn ({drawn:?})");
            }
        }
    }
    let lines = hp.a11y_lines();
    assert!(lines.iter().any(|l| l.contains(&full)), "after shaping at any width, the accessible text is whole (R2)");
    assert!(lines.iter().any(|l| l.contains(&format!("Synced from {ELLIPSIS}{}", last8(&gone)))), "the forgotten device's short id");
}
