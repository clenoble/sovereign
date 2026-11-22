//! Contract C-LOG, the host lot on the desktop (spec ch. 08, DG3, DG4 and DG6; the Fable maille,
//! by delegation of Céline, 2026-11-18, on what Export gives, which quarantine the choices act on,
//! where the window opens and its words). Written before the code by its test author: whoever
//! builds the lot makes these pass without editing this file; a test that looks wrong is
//! reported, not worked around.
//!
//! A child of `app`, like `clog_login_tests`: each test is one launch of this binary in a child
//! process, on a temporary profile of its own (`SOVEREIGN_DATA_DIR`), because the log lives under
//! the process's data directory. Before the `App` starts, the launch writes the persona's log
//! under the session's log key and breaks it; the login (faked: `AuthWork::unlock` hands back a
//! session over an in-memory store and the test account key) checks it and quarantines it, as
//! LG5 has it.
//!
//! The log: three turns. The first holds a word of its own, a direction-changing character, a
//! control character and a URL; the second and third hold a marker. The second and third lines are
//! swapped, so the chain breaks at entry 2 of 3: one entry still verifies, two are counted.
//!
//! Everything is read and pressed the way she reaches it: through the accessibility tree, the
//! spoken names and the buttons' click action. The one seam is where Export finds her Downloads
//! folder, `App::log_export_downloads`: a temporary one, or none.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use accesskit::{Action, ActionRequest, NodeId, Role as AkRole, TreeId};
use sovereign_ai::docs_host::testing;
use sovereign_ai::orchestrator::OrchestratorStatePaths;
use sovereign_ai::session_log::{quarantine_unverifiable, SessionLog};
use sovereign_core::auth::PersonaKind;

use super::first_run_tests::{
    first_frame, frames_until, is_button, launch, login_settled, profile, read_tree, shut_runtime, submit_login,
    CHILD_VAR, DURESS, PRIMARY,
};
use super::App;
use crate::auth_work::{AuthWork, Session};

fn module() -> &'static str {
    module_path!()
}

// ── the words decided (the Fable maille, by delegation of Céline, 2026-11-18) ─

const SHOW: &str = "Show what remains readable";
const KEEP: &str = "Keep it";
const EXPORT: &str = "Export for an expert";
const DELETE_IT: &str = "Delete it";
const DELETE_QUESTION: &str = "Delete this quarantined log? It cannot be brought back.";
const DELETED: &str = "The quarantined log was deleted.";
const EXPORT_QUESTION_START: &str = "Export the quarantined log to ";
const EXPORT_QUESTION_FILES: &str = "Its files leave as they are on disk.";
const EXPORT_QUESTION_PLAIN: &str =
    "Sealed entries stay sealed; any line written before encryption can be read by whoever has the folder.";
const EXPORTED_START: &str = "The quarantined log was exported to ";
const EXPORTED_KEPT: &str = "It is still kept here.";
const NAME_TAKEN: &str = "A folder of that name already exists in Downloads. Nothing was exported.";
const NO_DOWNLOADS: &str = "No Downloads folder was found. Nothing was exported.";
/// The count of what is not shown, for two (the Fable maille, by delegation of Céline,
/// 2026-11-18).
const COUNT_TWO: &str = "2 entries are not shown: from the break on, they no longer verify.";
/// The action gate's own buttons (`app.rs`, the action modal): Export is a Transmit action.
const APPROVE: &str = "Approve";
const REJECT: &str = "Reject";

// ── the fixture ───────────────────────────────────────────────────────────

/// The words of a log's first entry, and the marker of its broken part, for one persona.
struct Words {
    readable: String,
    broken: String,
}

impl Words {
    fn new(tag: &str) -> Words {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        Words { readable: format!("READABLE{tag}{n}"), broken: format!("BROKEN{tag}{n}") }
    }
}

fn paths(persona: PersonaKind) -> OrchestratorStatePaths {
    OrchestratorStatePaths::for_persona(&sovereign_core::sovereign_dir(), persona)
}

fn log_key() -> [u8; 32] {
    testing::account_key().derive_session_log_key()
}

/// Three turns under the session's log key, the second and third lines swapped.
fn broken_log(persona: PersonaKind, w: &Words) {
    let p = paths(persona);
    {
        let mut log = SessionLog::open_encrypted(&p.dir, log_key(), Some(&p.beacon)).unwrap();
        log.log_user_input("chat", &format!("{} \u{202E}reversed\u{7}bell https://link.example/x", w.readable), "chat");
        log.log_user_input("chat", &format!("{} second", w.broken), "chat");
        log.log_user_input("chat", &format!("{} third", w.broken), "chat");
    }
    let file = p.dir.join("session_log.jsonl");
    let mut lines: Vec<String> =
        std::fs::read_to_string(&file).unwrap().lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    assert_eq!(lines.len(), 3, "fixture: three entries");
    lines.swap(1, 2);
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
}

/// Quarantines the persona's log now, as a login before this one did.
fn quarantined_now(persona: PersonaKind) {
    let p = paths(persona);
    let q = quarantine_unverifiable(&p.dir, log_key(), Some(&p.beacon)).expect("the check runs");
    assert!(q.is_some(), "fixture: the log is quarantined");
}

fn quarantines(persona: PersonaKind) -> Vec<PathBuf> {
    let dir = paths(persona).dir;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("quarantine-"))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The files under `root`, by file name, with their bytes.
fn by_name(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

/// The login, faked: `unlock` opens `persona`'s session over an in-memory store, with the test
/// account key, whatever the password.
struct FakeUnlock {
    session: Session,
}

impl FakeUnlock {
    fn new(app: &App, persona: PersonaKind) -> Arc<FakeUnlock> {
        let db = app.rt.block_on(testing::encrypted_memory_store("clog-hosts"));
        let mk = sovereign_crypto::master_key::MasterKey::generate();
        Arc::new(FakeUnlock {
            session: (
                persona,
                db,
                testing::account_key(),
                Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&mk, "clog-hosts").unwrap()),
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

/// A login as `persona`, after which the log has been checked and quarantined.
fn logged_in(persona: PersonaKind) -> App {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let before = quarantines(persona).len();
    let mut app = App::new();
    first_frame(&mut app);
    app.auth_work = FakeUnlock::new(&app, persona);
    submit_login(&mut app, if persona == PersonaKind::Duress { DURESS } else { PRIMARY });
    assert!(frames_until(&mut app, 60, login_settled), "the login ends");
    assert!(!app.locked, "precondition: the session opens: {:?}", app.auth_form.error);
    assert!(
        frames_until(&mut app, 15, |_| quarantines(persona).len() > before),
        "precondition: the broken log is quarantined at the login (LG5)"
    );
    app.load_workspace_now();
    frames(&mut app, 3);
    app
}

fn end(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.build_scene(super::first_run_tests::W, super::first_run_tests::H);
    }
}

// ── reading and pressing, as she does ─────────────────────────────────────

/// Every spoken name in the tree.
fn spoken(app: &mut App) -> Vec<String> {
    frames(app, 2);
    read_tree(app).all.into_iter().map(|s| s.spoken).collect()
}

fn heard(app: &mut App) -> String {
    spoken(app).join("\n")
}

/// The button whose spoken name starts with `label` (« Export for an expert… » starts with
/// « Export for an expert »).
fn button(app: &mut App, label: &str) -> Option<NodeId> {
    frames(app, 2);
    read_tree(app).all.into_iter().find(|s| is_button(s.role) && s.spoken.starts_with(label)).map(|s| s.id)
}

/// The button whose spoken name is exactly `label`.
fn exact_button(app: &mut App, label: &str) -> Option<NodeId> {
    frames(app, 2);
    read_tree(app).all.into_iter().find(|s| is_button(s.role) && s.spoken == label).map(|s| s.id)
}

fn press(app: &mut App, id: NodeId) {
    app.a11y_action(ActionRequest { action: Action::Click, target_tree: TreeId::ROOT, target_node: id, data: None });
    frames(app, 3);
}

fn press_label(app: &mut App, label: &str) {
    let all = spoken(app);
    let id = button(app, label).unwrap_or_else(|| panic!("a button « {label} » is offered; spoken: {all:#?}"));
    press(app, id);
}

fn open_the_window(app: &mut App) {
    press_label(app, SHOW);
}

// ── the launches ──────────────────────────────────────────────────────────

/// DG3: the control below the notice opens the window. It shows the entry before the break as
/// plain text, the direction-changing and control characters as visible marks, the URL as no
/// link; it counts the two entries from the break on and shows neither. The window is not a
/// document: the graph and the workspace context the assistant receives are the same before and
/// after it opens, and hold none of its words.
fn the_window() {
    let w = Words::new("window");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let db = app.db.clone().expect("a store after login");
    let counts = |app: &App| {
        app.rt.block_on(async {
            (db.list_documents(None).await.unwrap().len(), db.list_threads().await.unwrap().len())
        })
    };
    let before = counts(&app);
    assert!(!heard(&mut app).contains(&w.readable), "precondition: nothing of the log is shown before she asks");

    open_the_window(&mut app);
    let tree = {
        frames(&mut app, 2);
        read_tree(&mut app)
    };
    let entry = tree.all.iter().find(|s| s.spoken.contains(&w.readable));
    let entry = entry.unwrap_or_else(|| panic!("DG3: the entry before the break is shown: {:#?}", tree.all));
    assert!(entry.spoken.contains("[U+202E]"), "DG3: the direction-changing character is a visible mark: {:?}", entry.spoken);
    assert!(entry.spoken.contains("[U+0007]"), "DG3: the control character is a visible mark: {:?}", entry.spoken);
    for s in &tree.all {
        assert!(!s.spoken.contains('\u{202E}') && !s.spoken.contains('\u{7}'), "DG3: no character is applied: {:?}", s.spoken);
        assert!(!s.spoken.contains(&w.broken), "DG3: nothing from the break on is shown: {:?}", s.spoken);
        assert!(
            !(s.role == AkRole::Link && s.spoken.contains("link.example")),
            "DG3: no live link in the window: {:?}",
            s.spoken
        );
    }
    let counted = tree.all.iter().any(|s| {
        !s.spoken.contains(&w.readable)
            && s.spoken.split(|c: char| !c.is_alphanumeric()).any(|word| word == "2")
    });
    assert!(counted, "DG3: the two entries from the break on are counted: {:#?}", tree.all);
    assert!(
        tree.all.iter().any(|s| s.spoken.contains(COUNT_TWO)),
        "DG3: the count is said « {COUNT_TWO} »: {:#?}",
        tree.all
    );
    for label in [KEEP, EXPORT, DELETE_IT] {
        assert!(tree.all.iter().any(|s| is_button(s.role) && s.spoken.starts_with(label)), "DG4: « {label} » is offered");
    }

    assert_eq!(counts(&app), before, "DG3: the window is not a document: the graph is unchanged");
    let context = app.rt.block_on(async {
        let ctx = sovereign_ai::llm::context::gather_workspace_context(db.as_ref()).await;
        sovereign_ai::llm::context::format_workspace_context(&ctx)
    });
    assert!(!context.contains(&w.readable), "DG3: the window's content is not in what the assistant receives:\n{context}");
    end(app);
}

/// DG4, Keep: the window closes and the quarantine keeps every byte.
fn keep() {
    let w = Words::new("keep");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let q = quarantines(PersonaKind::Primary);
    let before = by_name(&q[0]);
    open_the_window(&mut app);
    assert!(heard(&mut app).contains(&w.readable), "precondition: the window shows the entry");
    press_label(&mut app, KEEP);
    assert!(!heard(&mut app).contains(&w.readable), "Keep closes the window");
    assert_eq!(quarantines(PersonaKind::Primary), q, "Keep: the quarantine is still there");
    assert_eq!(by_name(&q[0]), before, "Keep: it keeps every byte");
    end(app);
}

/// A temporary Downloads folder of its own, empty.
fn downloads(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("sov-clog-hosts-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// The export's folder names for the minute before, this minute and the next, in local time:
/// « Sovereign quarantined log YYYY-MM-DD HHMM » (the Fable maille, by delegation of Céline,
/// 2026-11-18). The minute before is for a name read after the export formed it, the minute turning
/// in between under load; the next one is for a name taken before the export forms it.
fn folder_names_now() -> [String; 3] {
    let now = chrono::Local::now();
    let name = |t: chrono::DateTime<chrono::Local>| format!("Sovereign quarantined log {}", t.format("%Y-%m-%d %H%M"));
    [name(now - chrono::Duration::minutes(1)), name(now), name(now + chrono::Duration::minutes(1))]
}

/// Export up to the gate's question: nothing written yet; the question asked in the decided words,
/// naming the whole path of a new folder in Downloads; the gate's buttons. Returns that path.
fn export_until_the_question(app: &mut App, base: &Path) -> PathBuf {
    app.log_export_downloads(Some(base.to_path_buf()));
    press_label(app, EXPORT);
    frames(app, 3);
    let said = heard(app);
    for part in [EXPORT_QUESTION_START, EXPORT_QUESTION_FILES, EXPORT_QUESTION_PLAIN] {
        assert!(said.contains(part), "DG4: the gate's question says « {part} »:\n{said}");
    }
    let names = folder_names_now();
    let folder = names
        .iter()
        .map(|n| base.join(n))
        .find(|p| said.contains(&p.display().to_string()))
        .unwrap_or_else(|| panic!("the question names the whole path of a new folder in Downloads, one of {names:?}:\n{said}"));
    assert!(by_name(base).is_empty() && !folder.exists(), "DG4: nothing is written before her answer (a Transmit action)");
    assert!(button(app, APPROVE).is_some() && button(app, REJECT).is_some(), "the gate's buttons are offered:\n{said}");
    folder
}

/// DG4, Export, approved: a new folder in Downloads holds the quarantine's files as they are, and
/// the diagnosis's text file; the quarantine keeps every byte; a notice names the path.
fn export_approved() {
    let w = Words::new("export");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let q = quarantines(PersonaKind::Primary);
    let quarantined = by_name(&q[0]);
    let base = downloads("approved");
    open_the_window(&mut app);
    let folder = export_until_the_question(&mut app, &base);

    press_label(&mut app, APPROVE);
    assert!(frames_until(&mut app, 10, |_| by_name(&folder).len() > quarantined.len()), "the export is written");
    let exported = by_name(&folder);
    for (name, bytes) in &quarantined {
        assert!(exported.iter().any(|(n, b)| n == name && b == bytes), "{name} is exported as it is on disk");
    }
    assert_eq!(exported.len(), quarantined.len() + 1, "and one text file beside them: {:?}", exported.iter().map(|(n, _)| n).collect::<Vec<_>>());
    assert_eq!(by_name(&base).len(), exported.len(), "nothing else is written in Downloads");
    assert_eq!(by_name(&q[0]), quarantined, "the quarantine keeps every byte");
    let said = heard(&mut app);
    assert!(
        said.contains(EXPORTED_START) && said.contains(&folder.display().to_string()) && said.contains(EXPORTED_KEPT),
        "a notice says « {EXPORTED_START}{{folder}}. {EXPORTED_KEPT} »:\n{said}"
    );
    let _ = std::fs::remove_dir_all(&base);
    end(app);
}

/// DG4, Export, rejected at the gate: nothing is written.
fn export_rejected() {
    let w = Words::new("reject");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let q = quarantines(PersonaKind::Primary);
    let quarantined = by_name(&q[0]);
    let base = downloads("rejected");
    open_the_window(&mut app);
    let folder = export_until_the_question(&mut app, &base);
    press_label(&mut app, REJECT);
    frames(&mut app, 10);
    assert!(!folder.exists() && by_name(&base).is_empty(), "a rejected export writes nothing");
    assert_eq!(by_name(&q[0]), quarantined, "the quarantine keeps every byte");
    let _ = std::fs::remove_dir_all(&base);
    end(app);
}

/// Presses « Approve » if the gate asks, then lets the export run.
fn approve_if_asked(app: &mut App) {
    if let Some(id) = button(app, APPROVE) {
        press(app, id);
    }
    frames(app, 10);
}

/// DG4, Export, the folder's name already taken in Downloads (every name `folder_names_now` gives,
/// so the minute cannot turn under the test): nothing is written, and she is told so.
fn export_name_taken() {
    let w = Words::new("taken");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let base = downloads("taken");
    for n in folder_names_now() {
        std::fs::create_dir_all(base.join(n)).unwrap();
    }
    app.log_export_downloads(Some(base.clone()));
    open_the_window(&mut app);
    press_label(&mut app, EXPORT);
    approve_if_asked(&mut app);
    assert!(by_name(&base).is_empty(), "nothing is written into a folder that exists");
    let said = heard(&mut app);
    assert!(said.contains(NAME_TAKEN), "she is told « {NAME_TAKEN} »:\n{said}");
    let _ = std::fs::remove_dir_all(&base);
    end(app);
}

/// DG4, Export, no Downloads folder found: nothing is written, and she is told so.
fn export_no_downloads() {
    let w = Words::new("nodownloads");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let q = quarantines(PersonaKind::Primary);
    let quarantined = by_name(&q[0]);
    app.log_export_downloads(None);
    open_the_window(&mut app);
    press_label(&mut app, EXPORT);
    approve_if_asked(&mut app);
    let said = heard(&mut app);
    assert!(said.contains(NO_DOWNLOADS), "she is told « {NO_DOWNLOADS} »:\n{said}");
    assert_eq!(by_name(&q[0]), quarantined, "the quarantine keeps every byte");
    end(app);
}

/// DG4, Delete: it asks first, in the decided words, the focus on « Cancel »; « Cancel » removes
/// nothing; « Delete » removes the quarantine and only it, closes the window and says so.
fn delete() {
    let w = Words::new("delete");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let p = paths(PersonaKind::Primary);
    let q = quarantines(PersonaKind::Primary);
    assert_eq!(q.len(), 1, "fixture: one quarantine");
    open_the_window(&mut app);

    press_label(&mut app, DELETE_IT);
    let tree = {
        frames(&mut app, 2);
        read_tree(&mut app)
    };
    let said: String = tree.all.iter().map(|s| s.spoken.clone()).collect::<Vec<_>>().join("\n");
    assert!(said.contains(DELETE_QUESTION), "DG4: Delete asks to confirm, « {DELETE_QUESTION} »:\n{said}");
    let cancel = exact_button(&mut app, "Cancel").expect("« Cancel » is offered");
    assert!(exact_button(&mut app, "Delete").is_some(), "« Delete » is offered");
    assert_eq!(tree.focus, cancel, "the focus is on « Cancel »");
    assert!(q[0].exists(), "nothing is removed when she chooses Delete");

    press(&mut app, cancel);
    assert!(q[0].exists(), "« Cancel » removes nothing");

    press_label(&mut app, DELETE_IT);
    let delete = exact_button(&mut app, "Delete").expect("« Delete » is offered again");
    // Everything of the persona's log but the quarantine: the live log if there is one yet, and
    // the record beside it.
    let rest = |q: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        let mut v: Vec<(PathBuf, Vec<u8>)> = std::fs::read_dir(&p.dir)
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|f| f.is_file()).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .chain(std::iter::once(p.beacon.clone()).filter(|b| b.exists()))
            .filter(|f| !f.starts_with(q))
            .map(|f| {
                let b = std::fs::read(&f).unwrap_or_default();
                (f, b)
            })
            .collect();
        v.sort();
        v
    };
    let before = rest(&q[0]);
    press(&mut app, delete);
    assert!(frames_until(&mut app, 10, |_| !q[0].exists()), "« Delete » removes the quarantine");
    assert_eq!(rest(&q[0]), before, "the live log and its record keep every byte");
    let said = heard(&mut app);
    assert!(said.contains(DELETED), "a notice says « {DELETED} »:\n{said}");
    assert!(!said.contains(&w.readable), "the window closes after a deletion");
    end(app);
}

/// DG4 and DG1, several quarantines: after the most recent is deleted, the earlier one remains,
/// keeps every byte, and the control offers it next.
fn delete_then_the_next() {
    let p = paths(PersonaKind::Primary);
    {
        let mut log = SessionLog::open_encrypted(&p.dir, [0x53; 32], Some(&p.beacon)).unwrap();
        log.log_user_input("chat", "a turn under another key", "chat");
    }
    quarantined_now(PersonaKind::Primary);
    let earlier = quarantines(PersonaKind::Primary);
    assert_eq!(earlier.len(), 1, "fixture: the earlier quarantine");
    let earlier_bytes = by_name(&earlier[0]);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let w = Words::new("next");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    assert_eq!(quarantines(PersonaKind::Primary).len(), 2, "fixture: two quarantines");

    open_the_window(&mut app);
    assert!(heard(&mut app).contains(&w.readable), "the window is of the most recent quarantine");
    press_label(&mut app, DELETE_IT);
    let delete = exact_button(&mut app, "Delete").expect("« Delete » is offered");
    press(&mut app, delete);
    assert!(frames_until(&mut app, 10, |_| quarantines(PersonaKind::Primary).len() == 1), "the most recent is deleted");
    assert_eq!(quarantines(PersonaKind::Primary), earlier, "the earlier one remains");
    assert_eq!(by_name(&earlier[0]), earlier_bytes, "and keeps every byte");
    assert!(button(&mut app, SHOW).is_some(), "the control offers the earlier quarantine next");
    end(app);
}

/// DG6: in a duress session, the window shows the duress persona's quarantine, and Delete
/// reaches it only; the primary's quarantine keeps every byte and none of its words is shown.
fn duress() {
    let wp = Words::new("primary");
    broken_log(PersonaKind::Primary, &wp);
    quarantined_now(PersonaKind::Primary);
    let primary_q = quarantines(PersonaKind::Primary);
    let primary_bytes = by_name(&primary_q[0]);
    let wd = Words::new("duress");
    broken_log(PersonaKind::Duress, &wd);
    let mut app = logged_in(PersonaKind::Duress);

    open_the_window(&mut app);
    let said = heard(&mut app);
    assert!(said.contains(&wd.readable), "DG6: the duress persona's own entry is shown:\n{said}");
    assert!(!said.contains(&wp.readable), "DG6: nothing of the primary's log is shown:\n{said}");
    press_label(&mut app, DELETE_IT);
    let delete = exact_button(&mut app, "Delete").expect("« Delete » is offered");
    press(&mut app, delete);
    assert!(frames_until(&mut app, 10, |_| quarantines(PersonaKind::Duress).is_empty()), "the duress quarantine is deleted");
    assert_eq!(quarantines(PersonaKind::Primary), primary_q, "DG6: the primary's quarantine is still there");
    assert_eq!(by_name(&primary_q[0]), primary_bytes, "DG6: and keeps every byte");
    end(app);
}

// ── the window scrolls, a long entry wraps; one Downloads for both personas ───

/// `n` turns that still verify (each holding `tag` and its number), then two that break: the
/// second-to-last and last lines swapped. Returns the texts of the turns that still verify.
fn many_turns_then_broken(persona: PersonaKind, tag: &str, n: usize, long_last: bool) -> Vec<String> {
    let p = paths(persona);
    let mut texts: Vec<String> = (0..n).map(|i| format!("{tag} turn {i}")).collect();
    if long_last {
        let words: Vec<String> = (0..600).map(|i| format!("w{i}")).collect();
        texts[n - 1] = format!("{tag} LONGSTART {} LONGEND", words.join(" "));
    }
    {
        let mut log = SessionLog::open_encrypted(&p.dir, log_key(), Some(&p.beacon)).unwrap();
        for t in &texts {
            log.log_user_input("chat", t, "chat");
        }
        log.log_user_input("chat", "BROKEN one", "chat");
        log.log_user_input("chat", "BROKEN two", "chat");
    }
    let file = p.dir.join("session_log.jsonl");
    let mut lines: Vec<String> =
        std::fs::read_to_string(&file).unwrap().lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    let k = lines.len();
    lines.swap(k - 2, k - 1);
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
    texts
}

/// The bounds the last frame gave the node of `role` whose spoken name contains `needle`.
fn bounds_of(app: &mut App, role: AkRole, needle: &str) -> accesskit::Rect {
    frames(app, 2);
    let up = app.build_a11y_tree();
    let found = up.nodes.iter().find(|(id, n)| {
        n.role() == role && crate::a11y_spoken::spoken_name(&up.nodes, *id).contains(needle)
    });
    let (_, n) = found.unwrap_or_else(|| panic!("a {role:?} node says « {needle} »"));
    n.bounds().unwrap_or_else(|| panic!("the {role:?} « {needle} » has bounds after a frame"))
}

/// Whether the line holding `needle` is drawn inside the window's view: within the dialog, above its
/// buttons.
fn in_view(app: &mut App, needle: &str) -> bool {
    let dialog = bounds_of(app, AkRole::Dialog, "");
    let keep = bounds_of(app, AkRole::Button, KEEP);
    let line = bounds_of(app, AkRole::Label, needle);
    line.y0 >= dialog.y0 && line.y1 <= keep.y0
}

/// DG3: with more entries than the window holds, the last one is reached by End, by Page Down and
/// by the wheel; the first one is in view when the window opens. Read from the bounds the window's
/// nodes carry.
fn scrolls() {
    use winit::keyboard::{Key, NamedKey};
    let texts = many_turns_then_broken(PersonaKind::Primary, "SCROLL", 60, false);
    let mut app = logged_in(PersonaKind::Primary);
    // A headless app has no window: its size, for the scroll to be kept within the lines.
    app.test_view = Some((super::first_run_tests::W, super::first_run_tests::H));
    open_the_window(&mut app);
    let (first, last) = (texts[0].clone(), texts[texts.len() - 1].clone());
    assert!(in_view(&mut app, &first), "the first entry is in view when the window opens");
    assert!(!in_view(&mut app, &last), "precondition: more entries than the window holds");

    app.log_window_key(&Key::Named(NamedKey::End));
    assert!(in_view(&mut app, &last), "End reaches the last entry");
    app.log_window_key(&Key::Named(NamedKey::Home));
    assert!(in_view(&mut app, &first), "Home goes back to the first");

    for _ in 0..200 {
        app.log_window_key(&Key::Named(NamedKey::PageDown));
    }
    assert!(in_view(&mut app, &last), "Page Down reaches the last entry");
    app.log_window_key(&Key::Named(NamedKey::Home));
    for _ in 0..500 {
        app.log_window_wheel(-1.0);
    }
    assert!(in_view(&mut app, &last), "the wheel reaches the last entry");
    end(app);
}

/// DG3: one long entry is shown whole, wrapped to the window's width, never cut at its edge: its
/// node says every word of it, and it is taller than a short entry's, within the dialog's width.
fn long_entry() {
    let texts = many_turns_then_broken(PersonaKind::Primary, "WRAP", 3, true);
    let mut app = logged_in(PersonaKind::Primary);
    open_the_window(&mut app);
    let dialog = bounds_of(&mut app, AkRole::Dialog, "");
    let short = bounds_of(&mut app, AkRole::Label, &texts[0]);
    let long = bounds_of(&mut app, AkRole::Label, "LONGSTART");
    let up = app.build_a11y_tree();
    let said: String = up
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == AkRole::Label)
        .map(|(id, _)| crate::a11y_spoken::spoken_name(&up.nodes, *id))
        .find(|t| t.contains("LONGSTART"))
        .unwrap_or_default();
    for word in texts[2].split_whitespace() {
        assert!(said.contains(word), "« {word} » of the long entry is said, not cut");
    }
    assert!(long.height() > short.height() * 1.5, "the long entry wraps over several rows: {long:?} against {short:?}");
    assert!(long.x1 <= dialog.x1, "it wraps within the window's width: {long:?} in {dialog:?}");
    end(app);
}

/// One Downloads folder for both personas (the Fable maille, by delegation of Céline, 2026-11-19):
/// a duress session's export goes where the primary's goes: a new folder directly in
/// Downloads, under the one name « Sovereign quarantined log YYYY-MM-DD HHMM », with no persona's
/// name in it or in its place.
fn duress_export_folder() {
    let wd = Words::new("duressexport");
    broken_log(PersonaKind::Duress, &wd);
    let mut app = logged_in(PersonaKind::Duress);
    let base = downloads("duress");
    open_the_window(&mut app);
    let folder = export_until_the_question(&mut app, &base);
    press_label(&mut app, APPROVE);
    assert!(frames_until(&mut app, 10, |_| folder.exists()), "the export is written");
    let made: Vec<PathBuf> = std::fs::read_dir(&base).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(made, vec![folder.clone()], "one folder, directly in Downloads, as for the primary");
    let name = folder.file_name().unwrap().to_string_lossy().to_lowercase();
    for persona in ["duress", "primary", "persona"] {
        assert!(!name.contains(persona), "no persona's name in the folder's name: {name}");
    }
    assert!(folder_names_now().iter().any(|n| n.to_lowercase() == name), "the one name: {name}");
    let _ = std::fs::remove_dir_all(&base);
    end(app);
}

/// The buttons « Show what remains readable » offered now.
fn controls(app: &mut App) -> Vec<NodeId> {
    frames(app, 2);
    read_tree(app).all.into_iter().filter(|s| is_button(s.role) && s.spoken.starts_with(SHOW)).map(|s| s.id).collect()
}

/// A chat turn as the orchestrator ends it: its events through the app's own channel, `diagnosed`
/// saying whether the turn's diagnosis tool found a quarantine.
fn a_turn(app: &mut App, diagnosed: bool, words: &str) {
    use sovereign_core::interfaces::{ChatEnd, ChatOutcome, OrchestratorEvent};
    if diagnosed {
        app.orch_tx.send(OrchestratorEvent::QuarantinedLogDiagnosed).unwrap();
    }
    app.orch_tx.send(OrchestratorEvent::ChatResponse { text: words.into() }).unwrap();
    app.orch_tx
        .send(OrchestratorEvent::ChatTurnEnded { outcome: ChatOutcome { writes: vec![], end: ChatEnd::Completed } })
        .unwrap();
    frames(app, 5);
}

/// DG3, the control below the turn (the Fable maille, by delegation of Céline, 2026-11-18): a turn
/// whose diagnosis tool found the quarantine ends with one more « Show what remains readable »,
/// and it opens the window; a turn without the event adds none.
fn the_turns_control() {
    let w = Words::new("turn");
    broken_log(PersonaKind::Primary, &w);
    let mut app = logged_in(PersonaKind::Primary);
    let before = controls(&mut app);

    a_turn(&mut app, false, "An answer that used no diagnosis.");
    assert_eq!(controls(&mut app).len(), before.len(), "a turn without the event adds no control");

    a_turn(&mut app, true, "Here is what the check found.");
    let after = controls(&mut app);
    assert_eq!(after.len(), before.len() + 1, "the turn whose tool found the quarantine ends with the control");
    let new = *after.iter().find(|id| !before.contains(id)).expect("the turn's control is a node of its own");
    press(&mut app, new);
    assert!(heard(&mut app).contains(&w.readable), "the turn's control opens the window");
    end(app);
}

/// One launch on the profile of `SOVEREIGN_DATA_DIR` (set by the parent). Ignored in ordinary
/// runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "the_window" => the_window(),
        "keep" => keep(),
        "export_approved" => export_approved(),
        "export_rejected" => export_rejected(),
        "export_name_taken" => export_name_taken(),
        "export_no_downloads" => export_no_downloads(),
        "delete" => delete(),
        "delete_then_the_next" => delete_then_the_next(),
        "duress" => duress(),
        "the_turns_control" => the_turns_control(),
        "scrolls" => scrolls(),
        "long_entry" => long_entry(),
        "duress_export_folder" => duress_export_folder(),
        other => panic!("unknown scenario {other}"),
    }
}

#[test]
fn dg3_the_window_shows_what_still_verifies_as_plain_text_and_counts_the_rest() {
    launch(module(), "the_window", profile().path());
}

#[test]
fn dg4_keep_closes_the_window_and_changes_nothing() {
    launch(module(), "keep", profile().path());
}

#[test]
fn dg4_export_asks_at_the_gate_then_gives_the_files_as_they_are() {
    launch(module(), "export_approved", profile().path());
}

#[test]
fn dg4_an_export_rejected_at_the_gate_writes_nothing() {
    launch(module(), "export_rejected", profile().path());
}

#[test]
fn dg4_export_never_writes_into_a_folder_that_exists() {
    launch(module(), "export_name_taken", profile().path());
}

#[test]
fn dg4_export_with_no_downloads_folder_writes_nothing_and_says_so() {
    launch(module(), "export_no_downloads", profile().path());
}

#[test]
fn dg4_delete_asks_to_confirm_then_removes_the_quarantine_only() {
    launch(module(), "delete", profile().path());
}

#[test]
fn dg4_after_a_deletion_the_earlier_quarantine_is_offered_next() {
    launch(module(), "delete_then_the_next", profile().path());
}

#[test]
fn dg6_a_duress_session_reaches_its_own_quarantine_only() {
    launch(module(), "duress", profile().path());
}

#[test]
fn dg3_a_turn_whose_tool_found_the_quarantine_ends_with_the_control() {
    launch(module(), "the_turns_control", profile().path());
}

#[test]
fn dg3_the_window_scrolls_to_the_last_entry_by_keys_and_wheel() {
    launch(module(), "scrolls", profile().path());
}

#[test]
fn dg3_a_long_entry_is_shown_whole_wrapped() {
    launch(module(), "long_entry", profile().path());
}

#[test]
fn a_duress_sessions_export_goes_into_the_one_downloads_folder() {
    launch(module(), "duress_export_folder", profile().path());
}
