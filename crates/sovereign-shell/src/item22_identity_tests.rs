//! v0.0.12 item 22 in the native shell — a device's identity in its stamps is
//! its libp2p peer id, not its display name (D1, D2, D4, D5, D6).
//!
//! Contract: `doc/plans/V0012-item22-device-identity.md` (finding: claude-coord
//! `from-windows/0142-device-identity-finding.md`). Written before the code,
//! by another author: whoever implements item 22 makes these pass **without
//! editing this file**; a test that looks wrong is reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`), like `device_names_tests`,
//! whose fixtures it reuses (`unlocked`'s shape, the node-less P2P handle,
//! the rename field, History's lines) and `first_run_tests`' child launches.
//!
//! **What "the persona's peer id" is here:** the id
//! `sovereign_p2p::identity::derive_keypair(device_key)` gives for the
//! persona's device key (`App::device_key`, set by every unlock — login,
//! setup, recovery — before `install_document_service`), as a string
//! (`PeerId::to_string`, `12D3KooW…`): the id the P2P node runs under
//! (`P2pHandle::local_peer_id`).
//!
//! **How the identity is read:** as the service writes it, with today's API —
//! a new document's stored state (`DocumentService::stored_state`): its
//! stamp's `device`, its `author_device`, its `device`; a trust decision's
//! `device`; a thread's stamp.
//!
//! **No new API is needed by this file.** What it needs of the code: the
//! document service the shell installs takes its identity from
//! `App::device_key`, not from `p2p_config.device_name`; History and the
//! orphans list name a device as D5 says.
//!
//! **Readings chosen** (reported with the tests):
//! - D5's "this device" is the same words as R1's (`history_device`), lower
//!   case: the orphans line reads "Last written on this device.".
//! - D5 on History: besides R1's "Synced from" / "Received from" (which carry
//!   the sending peer's id), History names a device from a **stamp** in one
//!   place, a concurrent edit kept as a version ("Concurrent edit from
//!   {author}", the stamp's author). D5 is read as covering it:
//!   CONTRACT-QUESTION, see `d5_history_names_the_author_of_a_concurrent_edit_kept_as_a_version`.
//! - D5's "shown as that name": a stamp's device that is not a peer id in
//!   `device_names()` and not this device's is shown as it is when it is not
//!   a peer id (a display name from before the upgrade); R1's short form
//!   ("…" and the last 8 characters) stays for a forgotten peer id.
//!   CONTRACT-QUESTION, see `d5_a_display_name_from_before_the_upgrade_is_shown_as_that_name`.

use std::path::PathBuf;
use std::sync::Arc;

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{IncomingAlert, Stamp, Trust, WireApplied, WireMeta};

use super::device_names_tests::{
    docs, ensure_test_data_dir, history_lines, install_node, last8, open_devices, peer_id, remote, rename, this_name,
    MemClipboard, Unlocked, ELLIPSIS,
};
use super::editor_v2;
use super::first_run_tests::{first_frame, frames_until, launch, login_settled, profile, submit_login, CHILD_VAR, DURESS, PRIMARY};
use super::App;
use crate::device_rename::RenameTarget;

/// The display name every desktop had by default until items 14 + 19
/// (`P2pConfig::default`): what v0.0.11 wrote in every stamp.
const DISPLAY_NAME: &str = "Sovereign Device";

/// D5: what a stamp of this device's own peer id is shown as (R1's words).
const THIS_DEVICE: &str = "this device";

fn module() -> &'static str {
    module_path!()
}

// ===================================================================== fixtures

/// The persona's peer id for a device key, as P2P derives it.
fn peer_of(dk: &DeviceKey) -> String {
    sovereign_p2p::identity::derive_keypair(dk).expect("derive the keypair").public().to_peer_id().to_string()
}

/// A device key from a fixed passphrase (the same in every launch), as an
/// unlock derives one.
pub(super) fn fixed_device_key(tag: &str) -> Arc<DeviceKey> {
    let mk = MasterKey::from_passphrase(format!("item22-{tag}").as_bytes(), b"item22-salt").unwrap();
    Arc::new(DeviceKey::derive(&mk, tag).unwrap())
}

/// An `App` unlocked over an in-memory store with the unlock sequence of
/// `App::unlock_done` up to the document service (persona, keys, then
/// `install_document_service`), with one thread. `device_names_tests::unlocked`
/// does the same without a device key; an unlock always has one.
pub(super) fn unlocked_as(tag: &str, dk: Arc<DeviceKey>, persona: sovereign_core::auth::PersonaKind) -> Unlocked {
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
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(persona);
    app.account_key = Some(testing::account_key());
    app.device_key = Some(dk);
    app.kek = Some(Arc::new(sovereign_crypto::kek::Kek::generate()));
    app.install_document_service(store.clone());
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
    Unlocked { app, store, thread }
}

/// What the installed service writes as its device, for a document it
/// creates now in `thread`: (stamp's device, author_device, state's device).
pub(super) fn written_by(app: &App, thread: &str, title: &str) -> (String, (String, Option<String>, String)) {
    let d = docs(app);
    let id = app.rt.block_on(d.create(new_user_document(title, thread, "text"))).expect("create").id;
    let st = app.rt.block_on(d.stored_state(&id)).expect("the stored state reads");
    let stamp = st.meta.stamp.expect("a new document is stamped").device;
    (id, (stamp, st.meta.author_device, st.document.device))
}

fn assert_identity(written: &(String, Option<String>, String), expected: &str, why: &str) {
    assert_eq!(written.0, expected, "{why}: the new stamp's device is the persona's peer id");
    assert_eq!(written.1.as_deref(), Some(expected), "{why}: the local stamping's author is the peer id");
    assert_eq!(written.2, expected, "{why}: the state's device is the peer id");
}

// ======================================================================== D1

// D1: the shell's document service writes this persona's peer id — the one
// P2P derives from the persona's device key — in every new stamp, local
// stamping's author, `author_device`, trust decision and thread stamp. Never
// the configured display name, never this device's name (items 14 + 19).
#[test]
fn d1_the_shells_document_service_writes_the_personas_peer_id_never_the_display_name() {
    let dk = fixed_device_key("d1-shell");
    let expected = peer_of(&dk);
    let mut u = unlocked_as("item22-d1", dk, sovereign_core::auth::PersonaKind::Primary);
    let display = u.app.p2p_config.device_name.clone();
    let this = this_name(&mut u.app);
    assert_ne!(expected, display, "precondition: a peer id is not the display name");

    let (id, written) = written_by(&u.app, &u.thread, "D1");
    assert_identity(&written, &expected, "D1 (a creation)");
    assert_ne!(written.0, display, "D1: never the configured display name ({display:?})");
    assert_ne!(written.0, this, "D1: never this device's name ({this:?})");

    let d = docs(&u.app);
    // Local writes after the creation.
    u.app.rt.block_on(d.trash(&id)).unwrap();
    u.app.rt.block_on(d.restore_from_trash(&id)).unwrap();
    let st = u.app.rt.block_on(d.stored_state(&id)).unwrap();
    assert_eq!(st.meta.stamp.map(|s| s.device).as_deref(), Some(expected.as_str()), "D1: a later local write's stamp");
    // A trust decision.
    u.app.rt.block_on(d.set_trust(&id, Trust::External)).unwrap();
    let st = u.app.rt.block_on(d.stored_state(&id)).unwrap();
    assert_eq!(
        st.document.trust_decision.map(|t| t.device).as_deref(),
        Some(expected.as_str()),
        "D1: a trust decision's device"
    );
    // A thread write.
    let t = u.app.rt.block_on(d.thread_create("D1 thread", "")).unwrap();
    let stamp = u.app.rt.block_on(u.store.get_thread(&t)).unwrap().stamp.expect("a thread write is stamped");
    assert!(stamp.ends_with(&format!(":{expected}")), "D1: a thread stamp's device is the peer id: {stamp}");
    std::mem::forget(u);
}

// D1: the identity is the device key's, whatever the display name says: two
// unlocks with the same configuration and different device keys write
// different identities, each its own key's peer id.
#[test]
fn d1_the_identity_follows_the_device_key_not_the_configuration() {
    let (dk1, dk2) = (fixed_device_key("d1-one"), fixed_device_key("d1-two"));
    let (p1, p2) = (peer_of(&dk1), peer_of(&dk2));
    assert_ne!(p1, p2, "precondition: two device keys, two peer ids");
    let u1 = unlocked_as("item22-d1-one", dk1, sovereign_core::auth::PersonaKind::Primary);
    let (_, w1) = written_by(&u1.app, &u1.thread, "One");
    let display = u1.app.p2p_config.device_name.clone();
    std::mem::forget(u1);
    let u2 = unlocked_as("item22-d1-two", dk2, sovereign_core::auth::PersonaKind::Primary);
    assert_eq!(u2.app.p2p_config.device_name, display, "precondition: the same configured name");
    let (_, w2) = written_by(&u2.app, &u2.thread, "Two");
    std::mem::forget(u2);
    assert_identity(&w1, &p1, "D1 (first key)");
    assert_identity(&w2, &p2, "D1 (second key)");
}

/// The body of `fn <name>` in `src` (from its opening brace to the matching
/// closing one; braces in strings and comments are not special-cased, which
/// the scanned function does not need).
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("`fn {name}` is in the source"));
    let open = at + src[at..].find('{').expect("a body");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("`fn {name}`'s body closes");
}

/// The argument lists of every `build_document_service(` call in `src`
/// (balanced parentheses).
fn service_calls(src: &str) -> Vec<String> {
    let mut out = vec![];
    let key = "build_document_service(";
    let mut from = 0;
    while let Some(i) = src[from..].find(key) {
        let open = from + i + key.len() - 1;
        let mut depth = 0usize;
        let mut end = open;
        for (j, c) in src[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + j;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(src[open..=end].to_string());
        from = end + 1;
    }
    out
}

fn shell_src(file: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join(file);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

// D1 (source): the unlock's document service is not built from a display-name
// configuration field. `install_document_service` names neither
// `device_name` (the config's, `p2p_config.device_name`, or this device's,
// items 14 + 19); no production `build_document_service(` call of the shell
// passes one. (A scan on top of the behaviour tests above: it catches a
// fallback to the name that only some unlock path would take.)
#[test]
fn d1_the_shell_never_builds_the_document_service_from_a_display_name() {
    let app = shell_src("app.rs");
    let body = fn_body(&app, "install_document_service");
    assert!(
        !body.contains("device_name"),
        "D1: install_document_service must not take the identity from a display name:\n{body}"
    );
    assert!(body.contains("build_document_service"), "precondition: the body builds the service");
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut seen = 0;
    for entry in walk(&dir) {
        let name = entry.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if name.ends_with("_tests.rs") || !name.ends_with(".rs") {
            continue;
        }
        let src = std::fs::read_to_string(&entry).unwrap();
        for call in service_calls(&src) {
            seen += 1;
            assert!(
                !call.contains("device_name"),
                "D1: {}: a document service built from a display name: build_document_service{call}",
                entry.display()
            );
        }
    }
    assert!(seen > 0, "precondition: the scan sees the shell's calls");
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// ================================================================== D1 + D6, real logins

fn identity_file(persona: &str) -> PathBuf {
    sovereign_core::sovereign_dir().join(format!("item22-identity-{persona}.txt"))
}

/// A real login with `password` on this launch's profile (an `auth.store`
/// written by `store_only`), then a document and a thread written through the
/// service the login installed. The identity written is the peer id of the
/// device key the login derived; when the P2P node came up, it is the node's
/// own id. Recorded for the next launch.
fn identity_login(password: &str, persona: sovereign_core::auth::PersonaKind, tag: &str) -> String {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, password);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the login opens: {:?}", app.auth_form.error);
    assert_eq!(app.persona, Some(persona), "precondition: the persona");
    let dk = app.device_key.clone().expect("an unlock sets the device key");
    let expected = peer_of(&dk);
    let d = app.docs.clone().expect("the login installs the document service");
    let thread = app.rt.block_on(d.thread_create("Item 22", "")).expect("a thread");
    let (_, written) = written_by(&app, &thread, "Item 22");
    assert_identity(&written, &expected, &format!("D1 ({tag} login)"));
    assert_ne!(written.0, app.p2p_config.device_name, "D1: never the configured display name");
    assert_ne!(written.0, app.device_name, "D1: never this device's name");
    if let Some(h) = &app.p2p {
        assert_eq!(h.local_peer_id, expected, "D1: the same peer id P2P uses");
    }
    std::fs::write(identity_file(tag), &expected).unwrap();
    std::mem::forget(app);
    expected
}

/// One launch on the profile of `SOVEREIGN_DATA_DIR` (set by the parent).
/// Ignored in ordinary runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "store_only" => {
            crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
        }
        "login_primary" => {
            identity_login(PRIMARY, sovereign_core::auth::PersonaKind::Primary, "primary");
        }
        "login_primary_again" => {
            let before = std::fs::read_to_string(identity_file("primary")).expect("the primary's identity recorded");
            let now = identity_login(PRIMARY, sovereign_core::auth::PersonaKind::Primary, "primary-again");
            assert_eq!(now, before, "D1: the identity is stable across launches (the same device key)");
        }
        "login_duress" => {
            let primary = std::fs::read_to_string(identity_file("primary")).expect("the primary's identity recorded");
            let duress = identity_login(DURESS, sovereign_core::auth::PersonaKind::Duress, "duress");
            assert_ne!(duress, primary, "D6: the duress persona's identity is its own, not the primary's");
        }
        "rename_keeps_identity" => rename_keeps_identity(),
        "identity_after_rename_next_launch" => identity_after_rename_next_launch(),
        other => panic!("unknown scenario {other}"),
    }
}

// D1 + D6: real logins on one profile — the primary's service writes the
// primary's peer id (the node's, when it runs), the same at the next launch;
// the duress persona's writes its own peer id, derived from its own device
// key, different from the primary's.
#[test]
fn d1_d6_each_personas_real_login_writes_its_own_peer_id() {
    let p = profile();
    launch(module(), "store_only", p.path());
    launch(module(), "login_primary", p.path());
    launch(module(), "login_primary_again", p.path());
    launch(module(), "login_duress", p.path());
}

// ======================================================================== D2

// D2 (the hosts' half): two desktops configured alike ("Sovereign Device" by
// default) write the same document: their stamps differ by identity, and
// each one's state reaches the other — never ignored as already known. (The
// same millisecond with counter 0 is forced in `sovereign-docs`'
// `item22_device_identity.rs`, with an injected clock the shell does not
// have; here the stamps must differ in their device whatever their time.)
#[test]
fn d2_two_desktops_with_the_same_display_name_never_write_the_same_identity() {
    let (dka, dkb) = (fixed_device_key("d2-a"), fixed_device_key("d2-b"));
    let (pa, pb) = (peer_of(&dka), peer_of(&dkb));
    let a = unlocked_as("item22-d2-a", dka, sovereign_core::auth::PersonaKind::Primary);
    let b = unlocked_as("item22-d2-b", dkb, sovereign_core::auth::PersonaKind::Primary);
    assert_eq!(a.app.p2p_config.device_name, b.app.p2p_config.device_name, "precondition: the same display name");
    let (da, db) = (docs(&a.app), docs(&b.app));

    // A shared document: created on a, sent to b.
    let id = a.app.rt.block_on(da.create(new_user_document("Shared", &a.thread, "shared"))).unwrap().id;
    let st = a.app.rt.block_on(da.stored_state(&id)).unwrap();
    let mut doc = st.document;
    doc.thread_id = b.thread.clone();
    let out = b.app.rt.block_on(db.apply_wire(doc, st.meta)).unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: shared, {out:?}");

    // Both edit it.
    for (u, d, text) in [(&a, &da, "edit on a"), (&b, &db, "edit on b")] {
        let base = u.app.rt.block_on(d.read(&id)).unwrap().revision;
        u.app
            .rt
            .block_on(d.save(&id, sovereign_docs::Edit {
                base,
                title: None,
                content: Some(sovereign_docs::EditContent::PlainText(text.into())),
                origin: sovereign_docs::Origin::Assistant,
                explicit: false,
                pasted_tokens: vec![],
            }))
            .unwrap();
    }
    let sa = a.app.rt.block_on(da.stored_state(&id)).unwrap();
    let sb = b.app.rt.block_on(db.stored_state(&id)).unwrap();
    let (stamp_a, stamp_b) = (sa.meta.stamp.clone().unwrap(), sb.meta.stamp.clone().unwrap());
    assert_eq!(stamp_a.device, pa, "D2: a's stamp names a's peer id");
    assert_eq!(stamp_b.device, pb, "D2: b's stamp names b's peer id");
    assert_ne!(stamp_a.device, stamp_b.device, "D2: two devices never share an identity");
    assert_ne!(stamp_a, stamp_b, "D2: the stamps differ");

    let mut doc = sa.document;
    doc.thread_id = b.thread.clone();
    let out = b.app.rt.block_on(db.apply_wire(doc, sa.meta)).unwrap();
    assert_ne!(out.applied, WireApplied::Ignored, "D2: a's state is not ignored as known on b");
    assert!(out.concurrent, "D2: a concurrent edit");
    std::mem::forget(a);
    std::mem::forget(b);
}

// ======================================================================== D4

fn rename_record() -> PathBuf {
    sovereign_core::sovereign_dir().join("item22-d4-identity.txt")
}

/// D4: the identity is the peer id; renaming this device (items 14 + 19)
/// leaves it unchanged, right after the rename and after the next unlock.
fn rename_keeps_identity() {
    let dk = fixed_device_key("d4");
    let expected = peer_of(&dk);
    let mut u = unlocked_as("item22-d4", dk.clone(), sovereign_core::auth::PersonaKind::Primary);
    let (_, before) = written_by(&u.app, &u.thread, "Before");
    assert_identity(&before, &expected, "D4 (before the rename)");

    open_devices(&mut u.app);
    rename(&mut u.app, RenameTarget::ThisDevice, "Atelier Nord");
    assert_eq!(this_name(&mut u.app), "Atelier Nord", "precondition: renamed");
    let (_, after) = written_by(&u.app, &u.thread, "Right after");
    assert_identity(&after, &expected, "D4 (right after the rename)");

    // Unlock again: the document service is installed anew.
    u.app.install_document_service(u.store.clone());
    if let Some(task) = u.app.migration_task.take() {
        let _ = u.app.rt.block_on(task);
    }
    let (_, next) = written_by(&u.app, &u.thread, "Next unlock");
    assert_identity(&next, &expected, "D4 (after the next unlock)");
    std::fs::write(rename_record(), &expected).unwrap();
    std::mem::forget(u);
}

/// D4, a launch later: the renamed device's service still writes its peer id.
fn identity_after_rename_next_launch() {
    let expected = std::fs::read_to_string(rename_record()).expect("the identity recorded before");
    let dk = fixed_device_key("d4");
    let mut u = unlocked_as("item22-d4-next", dk, sovereign_core::auth::PersonaKind::Primary);
    assert_eq!(this_name(&mut u.app), "Atelier Nord", "precondition: the rename was kept");
    let (_, written) = written_by(&u.app, &u.thread, "Next launch");
    assert_identity(&written, &expected, "D4 (next launch)");
    std::mem::forget(u);
}

// D4: renaming this device never changes the service's identity, which is the
// peer id (items 14 + 19's N1b, `device_names_tests::
// n1b_a_rename_never_changes_the_document_services_device_identity`, checks
// "unchanged"; this checks "unchanged, and the peer id" — the reason N1b holds).
#[test]
fn d4_a_rename_never_changes_the_identity_which_is_the_peer_id() {
    let p = profile();
    launch(module(), "rename_keeps_identity", p.path());
    launch(module(), "identity_after_rename_next_launch", p.path());
}

// ======================================================================== D5

/// A document whose current state's stamp names `device`, waiting in a
/// thread deleted afterwards (as `n5_r1` builds one). Returns its id.
fn waiting_from(u: &mut Unlocked, id: &str, device: &str) -> String {
    let doomed = u
        .app
        .rt
        .block_on(u.store.create_thread(Thread::new(format!("Old drafts {device}"), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    let d = docs(&u.app);
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: chrono::Utc::now().timestamp_millis(), counter: 0, device: device.into() }),
        ..WireMeta::default()
    };
    let out = u.app.rt.block_on(d.apply_wire(remote(id, &doomed, "Waiting", "w", device, 0), meta)).expect("applied");
    assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: the waiting document arrives, {out:?}");
    u.app.rt.block_on(u.store.soft_delete_thread(&doomed)).unwrap();
    id.to_string()
}

fn orphan_label(u: &mut Unlocked, id: &str) -> String {
    u.app.load_workspace_now();
    let ap = u.app.load_alerts_panel().expect("the alerts load");
    ap.orphans()
        .lines()
        .iter()
        .find(|l| l.id == id)
        .unwrap_or_else(|| panic!("{id} is listed among the documents whose thread was deleted"))
        .device
        .clone()
}

// D5: the orphans list names a document written here — its stamp names this
// persona's own peer id — "this device"; one from a paired device by its
// label from `device_names()` (as 14 + 19's `n5_r1` checks after a rename);
// one whose stamp predates the upgrade by the display name it carries. Never
// a raw peer id.
#[test]
fn d5_the_orphans_list_names_this_device_a_paired_device_and_an_old_name() {
    let dk = fixed_device_key("d5-orphans");
    let own = peer_of(&dk);
    let mut u = unlocked_as("item22-d5-orphans", dk, sovereign_core::auth::PersonaKind::Primary);
    let dir = tempfile::tempdir().unwrap();
    let a = peer_id();
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&a, "Studio laptop")], dir.path());

    // Written here, in a thread deleted afterwards.
    let doomed = u
        .app
        .rt
        .block_on(u.store.create_thread(Thread::new("Here, deleted".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    let d = docs(&u.app);
    let mine = u.app.rt.block_on(d.create(new_user_document("Mine", &doomed, "m"))).unwrap().id;
    let stamp = u.app.rt.block_on(d.stored_state(&mine)).unwrap().meta.stamp.unwrap();
    assert_eq!(stamp.device, own, "precondition (D1): written here under this persona's peer id");
    u.app.rt.block_on(u.store.soft_delete_thread(&doomed)).unwrap();

    let from_a = waiting_from(&mut u, "document:item22d5a", &a);
    let old = waiting_from(&mut u, "document:item22d5old", DISPLAY_NAME);

    assert_eq!(orphan_label(&mut u, &mine), THIS_DEVICE, "D5: this persona's own peer id is \"this device\"");
    assert_eq!(orphan_label(&mut u, &from_a), "Studio laptop", "D5: a paired device's label from device_names()");
    assert_eq!(orphan_label(&mut u, &old), DISPLAY_NAME, "D5: a stamp from before the upgrade, as its name");
    for id in [&mine, &from_a, &old] {
        let label = orphan_label(&mut u, id);
        assert!(!label.contains(&own) && !label.contains(&a), "D5: no raw peer id shown: {label}");
    }
    std::mem::forget(u);
}

// D5: an alert names its device by `device_names()`; an alert whose device is
// a display name from before the upgrade is shown as that name.
#[test]
fn d5_an_alert_from_an_old_display_name_is_shown_as_that_name() {
    let dk = fixed_device_key("d5-alerts");
    let own = peer_of(&dk);
    let mut u = unlocked_as("item22-d5-alerts", dk, sovereign_core::auth::PersonaKind::Primary);
    let dir = tempfile::tempdir().unwrap();
    let a = peer_id();
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&a, "Studio laptop")], dir.path());
    let d = docs(&u.app);
    let mut ids = vec![];
    for (title, device) in [("From a", a.as_str()), ("From an old name", DISPLAY_NAME)] {
        let id = u.app.rt.block_on(d.create(new_user_document(title, &u.thread, "x"))).unwrap().id;
        u.app
            .rt
            .block_on(d.raise_alert(IncomingAlert {
                id: id.clone(),
                device: device.into(),
                finding: "injected instructions in the new content".into(),
                previous_version: None,
            }))
            .unwrap();
        ids.push((id, device));
    }
    let ap = u.app.load_alerts_panel().expect("the alerts load");
    let label = |id: &str| ap.rows.iter().find(|r| r.doc_id == id).expect("the alert is listed").device.clone();
    assert_eq!(label(&ids[0].0), "Studio laptop", "D5: a paired device's label");
    assert_eq!(label(&ids[1].0), DISPLAY_NAME, "D5: an old display name, as that name");
    std::mem::forget(u);
}

// D5: History names the device of a concurrent edit kept as a version — a
// message the service writes from the stamp's author ("Concurrent edit from
// {author}") — by its label from `device_names()`, and an author from before
// the upgrade by its name; never a raw peer id.
//
// CONTRACT-QUESTION: D5 lists History among the places "where a device is
// named from a stamp". R1 (items 14 + 19) maps "Synced from {id}" and
// "Received from {id}", whose id is the sending peer's (P2P's `sender_id`);
// the only History message whose device comes from a stamp is "Concurrent
// edit from {author}" (`wire.rs`, `keep_as_version`). Today it shows the
// display name; after item 22 it would show a raw peer id unless History maps
// it too. Read here as: History maps that form the same way.
#[test]
fn d5_history_names_the_author_of_a_concurrent_edit_kept_as_a_version() {
    let dk = fixed_device_key("d5-history");
    let own = peer_of(&dk);
    let mut u = unlocked_as("item22-d5-history", dk, sovereign_core::auth::PersonaKind::Primary);
    let dir = tempfile::tempdir().unwrap();
    let a = peer_id();
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&a, "Studio laptop")], dir.path());
    let d = docs(&u.app);
    let id = u.app.rt.block_on(d.create(new_user_document("Notes", &u.thread, "local"))).unwrap().id;
    let local = u.app.rt.block_on(d.stored_state(&id)).unwrap().meta.stamp.unwrap();

    // Two concurrent states with lower stamps than the local one: kept as
    // versions, not current.
    for (k, author) in [(1i64, a.as_str()), (2, DISPLAY_NAME)] {
        let meta = WireMeta {
            stamp: Some(Stamp { wall_ms: local.wall_ms - 1_000 * k, counter: 0, device: author.into() }),
            prev_stamp: Some(Stamp { wall_ms: local.wall_ms - 10_000, counter: 0, device: author.into() }),
            ..WireMeta::default()
        };
        let out = u
            .app
            .rt
            .block_on(d.apply_wire(remote(&id, &u.thread, "Notes", &format!("from {k}"), author, 0), meta))
            .unwrap();
        assert_eq!(out.applied, WireApplied::StoredAsVersion, "precondition: kept as a version ({author})");
    }
    let stored: Vec<String> = u.app.rt.block_on(d.versions(&id)).unwrap().into_iter().map(|v| v.message).collect();
    assert!(stored.contains(&format!("Concurrent edit from {a}")), "precondition: the stored message: {stored:?}");

    let lines = history_lines(&mut u.app, &id).join("\n");
    assert!(lines.contains("Concurrent edit from Studio laptop"), "D5: the paired device's label\n{lines}");
    assert!(lines.contains(&format!("Concurrent edit from {DISPLAY_NAME}")), "D5: an old author, as its name\n{lines}");
    assert!(!lines.contains(a.as_str()), "D5: no raw peer id in History\n{lines}");
    std::mem::forget(u);
}

// D5 (pure): History's naming keeps a display name from before the upgrade as
// it is; R1's short form stays for a forgotten peer id; "this device" for this
// persona's own id; the concurrent-edit form is named like R1's two.
//
// CONTRACT-QUESTION: R1 shows "any id not in `names`" as "…" and its last 8
// characters. D5 says a stamp whose device is a display name "is shown as
// that name". Read here: a device string that is not a libp2p peer id is a
// name and is shown as it is; a peer id not in `names` keeps R1's short form.
#[test]
fn d5_a_display_name_from_before_the_upgrade_is_shown_as_that_name() {
    let (own, a, gone) = (peer_id(), peer_id(), peer_id());
    let names = std::collections::HashMap::from([(a.clone(), "Studio laptop".to_string())]);
    let shown = |m: &str| crate::history_device::shown_message(m, &names, &own);

    assert_eq!(shown(&format!("Synced from {DISPLAY_NAME}")), format!("Synced from {DISPLAY_NAME}"), "D5: a name, as it is");
    assert_eq!(shown(&format!("Concurrent edit from {DISPLAY_NAME}")), format!("Concurrent edit from {DISPLAY_NAME}"));
    assert_eq!(shown(&format!("Concurrent edit from {a}")), "Concurrent edit from Studio laptop", "D5: a paired device's label");
    assert_eq!(shown(&format!("Concurrent edit from {own}")), format!("Concurrent edit from {THIS_DEVICE}"), "D5: this device");
    assert_eq!(
        shown(&format!("Concurrent edit from {gone}")),
        format!("Concurrent edit from {ELLIPSIS}{}", last8(&gone)),
        "D5: a forgotten peer id keeps R1's short form"
    );
    assert_eq!(shown(&format!("Synced from {gone}")), format!("Synced from {ELLIPSIS}{}", last8(&gone)), "R1 unchanged");
}

// ======================================================================== D6

// D6: the duress persona's identity is its own peer id, derived from its own
// device key, as its P2P identity is: different from the primary's, nothing
// shared between them. (The real logins are in
// `d1_d6_each_personas_real_login_writes_its_own_peer_id`.)
#[test]
fn d6_the_duress_personas_service_writes_its_own_peer_id() {
    let (dkp, dkd) = (fixed_device_key("d6-primary"), fixed_device_key("d6-duress"));
    let (pp, pd) = (peer_of(&dkp), peer_of(&dkd));
    let p = unlocked_as("item22-d6-p", dkp, sovereign_core::auth::PersonaKind::Primary);
    let (_, wp) = written_by(&p.app, &p.thread, "Primary");
    std::mem::forget(p);
    let d = unlocked_as("item22-d6-d", dkd, sovereign_core::auth::PersonaKind::Duress);
    let (_, wd) = written_by(&d.app, &d.thread, "Duress");
    std::mem::forget(d);
    assert_identity(&wp, &pp, "D6 (primary)");
    assert_identity(&wd, &pd, "D6 (duress: its own device key's peer id)");
    assert_ne!(wp.0, wd.0, "D6: the two personas' identities differ");
}
