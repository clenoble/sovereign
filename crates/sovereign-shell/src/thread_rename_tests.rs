//! Renaming a thread in its lane label — contract R1–R8 (`doc/plans/THREAD-name.md`,
//! *Renaming a thread — decided 2026-10-18*, Céline: "dans l'étiquette ou via
//! l'orchestrator"; spec ch. 03, *Thread deletion and the fallback lane*, and
//! ch. 12). The rename goes through the one stamped thread-write path
//! (`doc/plans/THREAD-sync.md` S1), so the later rename wins across devices.
//!
//! Written before the code, by another author: whoever builds it makes these
//! pass without editing this file. List, red reasons and the readings chosen:
//! `crates/sovereign-shell/TESTS-thread-rename.md`. Drawing, real keyboard
//! focus, a real input method and a screen reader are checked by hand
//! (`THREAD_CHECKLIST.md`, §R).
//!
//! Seams: the label field (`lane_rename::LaneRename`), the double-click
//! (`lane_rename::LabelClicks`) over the label rects
//! (`canvas::lane_label_rect`, `lane_rename::label_at`), the renamable labels
//! (`lane_rename::lane_labels`) and their AccessKit nodes
//! (`lane_rename::lane_label_nodes`), the save (`lane_rename::save_thread_rename`)
//! through the seam to S1 (`thread_writes::ThreadWrites`; S1 is built on
//! another branch, so here a recording fake stands for it, and a stand-in over
//! the store where a test needs the name stored). `App` needs a window and a
//! GPU: what only `App` does (routing the double-click, saving, reloading,
//! announcing) is checked by reading its source, as `thread_name_tests.rs` does.
//!
//! R7, R8 and P4 were added on 2026-10-18 (plan, main 3dc6e8e), answering
//! the test author's P1, P2 and P4.
//!
//! R3 ("the editor's single-line field (N4, N7, T9)") is tested through the
//! shared field: a few checks that the rename field routes each kind of input
//! to it, not the field's own suites again.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::{Clipboard, ClipboardContent, ClipboardWrite};
use sovereign_editor::input::{Command, ImeEvent, InputEvent, Modifiers};
use sovereign_editor::Fonts;
use vello::kurbo::{Point, Rect};
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::canvas::{lane_label_rect, load_workspace, AXIS_H, FALLBACK_LANE_NAME, LANES, LANE_H};
use crate::lane_rename::{
    label_at, lane_label_nodes, lane_labels, save_thread_rename, LabelClicks, LaneLabel, LaneRename, RenameOutcome,
    RenameStep, FIELD_LABEL, LABEL_BUTTON_PREFIX,
};
use crate::thread_writes::{ThreadWrites, WriteFuture};

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");
/// The window (logical pixels).
const W: f64 = 1280.0;
const H: f64 = 800.0;
/// The thread the field renames in the model tests.
const TID: &str = "thread:t1";
/// A label as drawn for a short name, lane 1 at zoom 1, no scroll.
fn label() -> Rect {
    lane_label_rect(AXIS_H + LANE_H, 60.0)
}

// ------------------------------------------------------------------ fixtures

/// A clipboard in memory: what it holds, and every write.
#[derive(Clone, Default)]
struct MemClip {
    held: Rc<RefCell<Option<ClipboardContent>>>,
    writes: Rc<RefCell<Vec<ClipboardWrite>>>,
}
impl Clipboard for MemClip {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.held.borrow().clone()
    }
    fn write(&mut self, item: ClipboardWrite) {
        *self.held.borrow_mut() = Some(ClipboardContent { text: item.text.clone(), sovereign: item.sovereign.clone() });
        self.writes.borrow_mut().push(item);
    }
}
impl MemClip {
    fn set_text(&self, text: &str) {
        *self.held.borrow_mut() = Some(ClipboardContent { text: text.to_string(), sovereign: None });
    }
    fn text(&self) -> Option<String> {
        self.held.borrow().as_ref().map(|c| c.text.clone())
    }
}

/// The field as the double-click opens it on thread [`TID`] named `name`, laid
/// out over [`label`] (as the first frame does).
fn open(name: &str) -> (LaneRename, MemClip) {
    let clip = MemClip::default();
    let mut r = LaneRename::open(TID, name, Fonts::Only(vec![TEST_FONT.to_vec()]), Box::new(clip.clone()));
    r.layout(label(), W, H);
    (r, clip)
}

fn type_str(r: &mut LaneRename, s: &str) -> Vec<RenameStep> {
    s.chars()
        .map(|c| {
            let t = c.to_string();
            if c == ' ' {
                r.key(&WKey::Named(WNamed::Space), Some(" "), Modifiers::NONE)
            } else {
                r.key(&WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE)
            }
        })
        .collect()
}

fn enter(r: &mut LaneRename) -> RenameStep {
    r.key(&WKey::Named(WNamed::Enter), Some("\r"), Modifiers::NONE)
}

fn esc(r: &mut LaneRename) -> RenameStep {
    r.key(&WKey::Named(WNamed::Escape), Some("\u{1b}"), Modifiers::NONE)
}

fn named(r: &mut LaneRename, k: WNamed) -> RenameStep {
    r.key(&WKey::Named(k), None, Modifiers::NONE)
}

/// Ctrl + a letter, with the control character winit gives as its text.
fn ctrl(r: &mut LaneRename, c: char) -> RenameStep {
    let ctl = char::from_u32((c as u32) & 0x1f).unwrap().to_string();
    r.key(&WKey::Character(c.to_string().as_str().into()), Some(&ctl), Modifiers::CTRL)
}

fn save(name: &str) -> RenameStep {
    RenameStep::Save { thread_id: TID.to_string(), name: name.to_string() }
}

fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap()
}

fn as_dyn(store: &Arc<EncryptedGraphDB>) -> Arc<dyn GraphDB> {
    store.clone()
}

/// S1 as the tests see it: every rename asked, and the answer to give.
#[derive(Default)]
struct RecordingWrites {
    calls: RefCell<Vec<(String, String)>>,
    fail: Option<String>,
}
impl ThreadWrites for RecordingWrites {
    fn rename_thread<'a>(&'a self, thread_id: &'a str, name: &'a str) -> WriteFuture<'a> {
        self.calls.borrow_mut().push((thread_id.to_string(), name.to_string()));
        let answer = match &self.fail {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        };
        Box::pin(async move { answer })
    }
}

/// A stand-in for S1 that stores the name (unstamped: only for checking what
/// the canvas then shows; the stamp is S1's own suite).
struct StoreStandIn(Arc<dyn GraphDB>);
impl ThreadWrites for StoreStandIn {
    fn rename_thread<'a>(&'a self, thread_id: &'a str, name: &'a str) -> WriteFuture<'a> {
        Box::pin(async move { self.0.update_thread(thread_id, Some(name), None).await.map(|_| ()).map_err(|e| e.to_string()) })
    }
}

/// Threads named `names`, created one second apart (the canvas orders lanes by
/// creation), returning their ids in order.
async fn threads(store: &EncryptedGraphDB, names: &[&str]) -> Vec<String> {
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let mut ids = vec![];
    for (i, n) in names.iter().enumerate() {
        let mut t = Thread::new((*n).to_string(), String::new());
        t.created_at = base + chrono::Duration::seconds(i as i64);
        ids.push(store.create_thread(t).await.unwrap().id_string().unwrap());
    }
    ids
}

/// A store holding one thread named `name`: the store, as `dyn GraphDB`, the id.
async fn one_thread(tag: &str, name: &str) -> (Arc<EncryptedGraphDB>, Arc<dyn GraphDB>, String) {
    let store = testing::encrypted_memory_store(tag).await;
    let db = as_dyn(&store);
    let id = threads(&store, &[name]).await.remove(0);
    (store, db, id)
}

/// The canvas's lane names, as the workspace load gives them.
async fn lane_names(db: &Arc<dyn GraphDB>) -> Vec<String> {
    let mut shaper = crate::text::TextShaper::new();
    let (_, _, _, names, _) = load_workspace(db, &mut shaper).await.expect("a workspace with threads loads");
    names
}

/// The production part of a shell source file (before its trailing test
/// module, if any).
fn production(file: &str, test_marker: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    match src.find(test_marker) {
        Some(at) => src[..at].to_string(),
        None => src,
    }
}

fn app_rs() -> String {
    production("app.rs", "#[cfg(test)]\nmod taskbar_tests")
}

/// Code without `//` comments, so a comment naming a call does not count.
fn code(src: &str) -> String {
    src.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// The body of `fn <name>` in `src` (to the next 4-space indented `fn`).
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let at = src.find(&sig).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &src[at + sig.len()..];
    let end = rest.find("\n    fn ").into_iter().chain(rest.find("\n    pub(crate) fn ")).min().unwrap_or(rest.len());
    &rest[..end]
}

/// The method of `src` (4-space indented `fn`) that contains byte `at`.
fn enclosing_fn(src: &str, at: usize) -> &str {
    let head = &src[..at];
    let start = head.rfind("\n    fn ").into_iter().chain(head.rfind("\n    pub(crate) fn ")).max().expect("inside a method");
    let rest = &src[start + 1..];
    let end = rest[1..].find("\n    fn ").into_iter().chain(rest[1..].find("\n    pub(crate) fn ")).min().map(|e| e + 1).unwrap_or(rest.len());
    &rest[..end]
}

fn a11y(r: &mut LaneRename) -> accesskit_consumer::Tree {
    accesskit_consumer::Tree::new(r.accessibility_tree(), true)
}

// ======================================================================= R1

// R1: "A **double-click** on a lane's label opens an inline field": the
// second press, close in time and place, on the same label.
#[test]
fn r1_a_double_click_on_a_label_opens_it() {
    let mut clicks = LabelClicks::default();
    let p = Point::new(20.0, 130.0);
    let t0 = Instant::now();
    assert_eq!(clicks.press(Some(1), p, t0), None, "the first press opens nothing");
    assert_eq!(clicks.press(Some(1), p, t0 + Duration::from_millis(150)), Some(1), "the second press opens lane 1's label");
}

// R1: a single click, a slow second click, a second click on another label or
// off any label open nothing.
#[test]
fn r1_only_a_double_click_on_one_label_opens() {
    let p = Point::new(20.0, 130.0);
    let t0 = Instant::now();

    let mut c = LabelClicks::default();
    assert_eq!(c.press(Some(1), p, t0), None, "a single click");
    assert_eq!(c.press(Some(1), p, t0 + Duration::from_secs(2)), None, "two clicks seconds apart are not a double-click");

    let mut c = LabelClicks::default();
    c.press(Some(1), p, t0);
    assert_eq!(c.press(Some(2), p, t0 + Duration::from_millis(150)), None, "a click on one label then another");

    let mut c = LabelClicks::default();
    assert_eq!(c.press(None, p, t0), None);
    assert_eq!(c.press(None, p, t0 + Duration::from_millis(150)), None, "a double-click off the labels opens nothing");

    let mut c = LabelClicks::default();
    c.press(None, p, t0);
    assert_eq!(c.press(Some(1), p, t0 + Duration::from_millis(150)), None, "a click off the labels then on one is one click on it");
}

// R1, hit-testing: the label under the point, by the same rects as drawn.
#[test]
fn r1_label_at_finds_the_label_under_the_point() {
    let rects: Vec<(usize, Rect)> =
        (0..3).map(|lane| (lane, lane_label_rect(AXIS_H + lane as f64 * LANE_H, 40.0 + 20.0 * lane as f64))).collect();
    for (lane, r) in &rects {
        assert_eq!(label_at(center(*r), &rects), Some(*lane), "the centre of lane {lane}'s label");
    }
    assert_eq!(label_at(Point::new(W / 2.0, AXIS_H + LANE_H * 0.5), &rects), None, "the middle of a lane is not its label");
    assert_eq!(label_at(Point::new(W / 2.0, 2.0), &[]), None);
}

// R1: the label is the pill at the lane's left edge, wide enough for its text,
// inside its lane at zoom 1 — what hit-testing and drawing share.
#[test]
fn r1_the_label_rect_is_a_pill_at_the_lane_left_edge() {
    let top = AXIS_H + 2.0 * LANE_H;
    let r = lane_label_rect(top, 80.0);
    assert!(r.x0 >= 0.0 && r.x0 < 24.0, "at the canvas's left edge: {r:?}");
    assert!(r.width() > 80.0, "wider than its text: {r:?}");
    assert!(r.height() > 0.0);
    assert!(r.y0 >= top && r.y1 <= top + LANE_H, "inside its lane: {r:?} in {top}..{}", top + LANE_H);
    let wider = lane_label_rect(top, 160.0);
    assert_eq!(wider.x0, r.x0, "a longer name keeps the left edge");
    assert!(wider.width() > r.width(), "and makes the label wider");
}

// R1: a lane scrolled partly under the time ruler keeps its label below the
// ruler (the pill is sticky), where it can be double-clicked.
#[test]
fn r1_the_label_stays_below_the_time_ruler() {
    let r = lane_label_rect(AXIS_H - LANE_H * 0.5, 60.0);
    assert!(r.y0 >= AXIS_H, "below the ruler: {r:?}");
}

// R1 in the shell: the label is drawn and hit-tested with the one rect, and a
// double-click on it opens the field.
#[test]
fn r1_the_shell_draws_and_hits_the_same_label() {
    let prod = code(&app_rs());
    let scene = fn_body(&prod, "build_scene");
    assert!(scene.contains("lane_label_rect("), "the labels are drawn at lane_label_rect");
    let squashed: String = scene.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(!squashed.contains("RoundedRect::new(6.0,py"), "no second, inline pill geometry");
    assert!(prod.contains("LabelClicks"), "the shell recognises the double-click with LabelClicks");
    assert!(prod.contains("label_at("), "and hit-tests the labels with label_at");
    assert!(prod.contains("LaneRename::open("), "a double-click opens the rename field");
}

// R1: "… with the current name selected": the field holds the name, all of it
// selected, so typing replaces it.
#[test]
fn r1_the_field_opens_with_the_current_name_selected() {
    let (mut r, _c) = open("Ideas");
    assert_eq!(r.name(), "Ideas", "the field holds the current name");
    let sel = r.field().selection();
    let (a, f) = (sel.anchor.offset, sel.focus.offset);
    assert_eq!((a.min(f), a.max(f)), (0, "Ideas".len()), "the whole name is selected: {sel:?}");
    type_str(&mut r, "Plans");
    assert_eq!(r.name(), "Plans", "typing replaces the selected name");
}

// R1: the field takes the keyboard at once (it is focused).
#[test]
fn r1_the_field_is_focused() {
    let (mut r, _c) = open("Ideas");
    let tree = a11y(&mut r);
    let focus = tree.state().focus().expect("the field has the focus");
    assert_eq!(focus.role(), accesskit::Role::TextInput);
}

// R1: "an inline field over it": over the label, inside the window.
#[test]
fn r1_the_field_lies_over_the_label() {
    let clip = MemClip::default();
    let mut r = LaneRename::open(TID, "Ideas", Fonts::Only(vec![TEST_FONT.to_vec()]), Box::new(clip));
    let l = label();
    let f = r.layout(l, W, H);
    assert!(f.area() > 0.0);
    assert!(f.contains(center(l)), "the field covers the label: {f:?} over {l:?}");
    assert!(Rect::new(0.0, 0.0, W, H).contains_rect(f), "inside the window: {f:?}");
}

// R1: the save renames the thread whose label was double-clicked — the field
// carries its id, whatever lane that thread is drawn in afterwards.
#[test]
fn r1_the_save_names_the_thread_opened() {
    let (mut r, _c) = open("Ideas");
    assert_eq!(r.thread_id(), TID);
    type_str(&mut r, "Plans");
    assert_eq!(enter(&mut r), save("Plans"));
}

// R1: the renamable labels are the lanes' labels: the same lane, the thread
// drawn there, the name shown (checked against the canvas's workspace load).
#[test]
fn r1_labels_are_the_threads_the_lanes_show() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r1-labels").await;
        let db = as_dyn(&store);
        let ids = threads(&store, &["Ideas", "Notes", "Plans"]).await;
        let labels = lane_labels(&db.list_threads().await.unwrap());
        assert_eq!(labels.len(), 3, "one label per thread: {labels:?}");
        let names = lane_names(&db).await;
        for (i, l) in labels.iter().enumerate() {
            assert_eq!(l.thread_id, ids[i], "lane {} holds thread {i}", l.lane);
            assert_eq!(names[l.lane], l.name, "the label's name is the one drawn in lane {}", l.lane);
        }
        let mut lanes: Vec<_> = labels.iter().map(|l| l.lane).collect();
        lanes.sort_unstable();
        lanes.dedup();
        assert_eq!(lanes.len(), 3, "one label per lane");
    });
}

// R1 with lanes shared (more threads than lanes: thread i is drawn in lane
// i mod LANES): a lane's label renames the thread whose name it shows.
#[test]
fn r1_a_shared_lane_renames_the_thread_its_label_shows() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r1-shared").await;
        let db = as_dyn(&store);
        let names: Vec<String> = (0..=LANES).map(|i| format!("Thread {i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let ids = threads(&store, &refs).await;
        let labels = lane_labels(&db.list_threads().await.unwrap());
        let drawn = lane_names(&db).await;
        assert_eq!(labels.len(), LANES, "one label per lane, not per thread: {labels:?}");
        for l in &labels {
            assert_eq!(drawn[l.lane], l.name, "lane {}: the label renames the thread it shows", l.lane);
            let i = ids.iter().position(|id| *id == l.thread_id).expect("a thread of the workspace");
            assert_eq!(names[i], l.name, "the id and the name are the same thread's");
        }
    });
}

// ======================================================================= R2

// R2: "**Enter** saves the trimmed name".
#[test]
fn r2_enter_saves_the_trimmed_name() {
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "  Plans for spring  ");
    assert_eq!(enter(&mut r), save("Plans for spring"));
}

// R2: "(one line)": a pasted line break becomes a space.
#[test]
fn r2_a_pasted_newline_becomes_a_space() {
    let (mut r, clip) = open("Ideas");
    clip.set_text("Line one\nLine two");
    r.command(Command::Paste);
    assert!(!r.name().contains(['\n', '\r']), "no line break in the name: {:?}", r.name());
    assert_eq!(enter(&mut r), save("Line one Line two"));
}

// R2: "**Esc** … cancels", whatever was typed.
#[test]
fn r2_esc_cancels() {
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "Plans");
    assert_eq!(esc(&mut r), RenameStep::Cancel);
}

// R2: "… or a click outside cancels"; a click in the field keeps it open.
#[test]
fn r2_a_click_outside_cancels() {
    let l = label();
    for q in [Point::new(W - 1.0, H - 1.0), Point::new(W / 2.0, H / 2.0), Point::new(1.0, H - 1.0)] {
        let (mut r, _c) = open("Ideas");
        let f = r.layout(l, W, H);
        assert!(!f.contains(q), "test point {q:?} is outside the field");
        type_str(&mut r, "Plans");
        assert_eq!(r.click(q), RenameStep::Cancel, "a click at {q:?} cancels");
    }
    let (mut r, _c) = open("Ideas");
    let f = r.layout(l, W, H);
    assert_eq!(r.click(center(f)), RenameStep::Open, "a click in the field keeps it open");
    assert_eq!(r.name(), "Ideas", "and keeps the name");
}

// R2: closing by any other path changes nothing.
#[test]
fn r2_closing_cancels() {
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "Plans");
    assert_eq!(r.close(), RenameStep::Cancel);
}

// R2: "an empty name changes nothing and closes" — in the field (Enter closes
// without saving) and at the save (no write).
#[test]
fn r2_an_empty_name_changes_nothing_and_closes() {
    for typed in ["", "   "] {
        let (mut r, _c) = open("Ideas");
        named(&mut r, WNamed::Backspace); // the selected name goes
        assert_eq!(r.name(), "", "Backspace removes the selected name");
        type_str(&mut r, typed);
        assert_eq!(enter(&mut r), RenameStep::Cancel, "{typed:?}: Enter closes, saves nothing");
    }
    let writes = RecordingWrites::default();
    rt().block_on(async {
        let (_store, db, id) = one_thread("r2-empty", "Ideas").await;
        for name in ["", "   ", " \n \r\n "] {
            assert_eq!(save_thread_rename(&*db, &writes, &id, name).await, Ok(RenameOutcome::Unchanged), "{name:?} changes nothing");
        }
    });
    assert!(writes.calls.borrow().is_empty(), "no write: {:?}", writes.calls.borrow());
}

// R2 + S1: the save goes through the one stamped thread-write path, once, with
// the trimmed one-line name, for the thread opened.
#[test]
fn r2_the_save_goes_through_the_stamped_write_path() {
    let writes = RecordingWrites::default();
    let (saved, id) = rt().block_on(async {
        let (_store, db, id) = one_thread("r2-write", "Ideas").await;
        (save_thread_rename(&*db, &writes, &id, "  Plans\nnext  ").await, id)
    });
    assert_eq!(saved, Ok(RenameOutcome::Written));
    assert_eq!(*writes.calls.borrow(), vec![(id, "Plans next".to_string())]);
}

// R2: a write that fails is reported (the shell shows it), not taken for done.
#[test]
fn r2_a_failed_save_is_reported() {
    let writes = RecordingWrites { fail: Some("store closed".into()), ..Default::default() };
    let saved = rt().block_on(async {
        let (_store, db, id) = one_thread("r2-fail", "Ideas").await;
        save_thread_rename(&*db, &writes, &id, "Plans").await
    });
    assert!(matches!(&saved, Err(e) if e.contains("store closed")), "the error comes back: {saved:?}");
    assert_eq!(writes.calls.borrow().len(), 1, "one attempt, no retry behind the user's back");
}

// R2: "The label shows the new name": once the name is stored, the same lane's
// label shows it, for the same thread.
#[test]
fn r2_the_label_shows_the_new_name() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r2-label").await;
        let db = as_dyn(&store);
        let ids = threads(&store, &["Ideas", "Notes"]).await;
        let before = lane_labels(&db.list_threads().await.unwrap());
        let notes = before.iter().find(|l| l.thread_id == ids[1]).expect("Notes has a label").clone();
        assert_eq!(save_thread_rename(&*db, &StoreStandIn(db.clone()), &ids[1], " Journal ").await, Ok(RenameOutcome::Written));
        let after = lane_labels(&db.list_threads().await.unwrap());
        assert!(
            after.contains(&LaneLabel { lane: notes.lane, thread_id: ids[1].clone(), name: "Journal".into() }),
            "the same lane's label shows the new name: {after:?}"
        );
        assert_eq!(lane_names(&db).await[notes.lane], "Journal", "and the canvas draws it");
    });
}

// R2 in the shell: the field's Save is written through `save_thread_rename`
// over the shell's binding of S1, at one place, and the canvas is reloaded
// there so the label shows the new name at once.
#[test]
fn r2_the_shell_saves_through_the_seam_then_reloads() {
    let prod = code(&app_rs());
    assert!(prod.contains("RenameStep::Save"), "the shell acts on the field's Save");
    let calls: Vec<usize> = prod.match_indices("save_thread_rename(").map(|(i, _)| i).collect();
    assert_eq!(calls.len(), 1, "one place saves a rename");
    let f = enclosing_fn(&prod, calls[0]);
    assert!(f.contains("StampedThreadWrites"), "through the shell's binding of the stamped write path: {f}");
    assert!(f.contains("load_workspace_now"), "then reloads, so the label shows the new name: {f}");
}

// R2 + S1: neither the shell nor the rename writes a thread's name around the
// stamped path (the binding to S1 lives in `thread_writes.rs`).
#[test]
fn r2_no_rename_around_the_stamped_path() {
    for file in ["app.rs", "lane_rename.rs"] {
        let src = code(&production(file, "#[cfg(test)]\nmod "));
        assert!(!src.contains("update_thread("), "{file} calls the store's update_thread: an unstamped rename");
    }
}

// ======================================================================= R3

// R3: "The field is the editor's single-line field".
#[test]
fn r3_the_field_is_the_editors_single_line_field() {
    let (mut r, _c) = open("Ideas");
    assert_eq!(r.field().field_text(), r.name(), "the name is the field's text");
    assert_eq!(r.field().document().blocks().len(), 1, "one line");
    let own = r.field_mut().accessibility_tree();
    assert!(own.nodes.iter().any(|(_, n)| n.role() == accesskit::Role::TextInput), "a single-line text input");
    assert!(!own.nodes.iter().any(|(_, n)| n.role() == accesskit::Role::MultilineTextInput));
}

// R3 (N4, title T6): a key's text reaches the field once; Ctrl shortcuts type
// nothing.
#[test]
fn r3_text_is_sent_once() {
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "a b");
    assert_eq!(r.name(), "a b");
    ctrl(&mut r, 'z');
    assert!(!r.name().contains('\u{1a}'), "Ctrl+Z types no control character");
}

// R3 (N4): copy, cut and paste go through the clipboard the shell gives.
#[test]
fn r3_copy_cut_paste_go_through_the_clipboard() {
    let (mut r, clip) = open("Ideas");
    ctrl(&mut r, 'c');
    assert_eq!(clip.text().as_deref(), Some("Ideas"), "Ctrl+C copies the selected name");
    let w = clip.writes.borrow().last().cloned().expect("a clipboard write");
    assert!(w.privacy.exclude_from_history && w.privacy.exclude_from_cloud_sync, "marked as an editor copy: {:?}", w.privacy);
    ctrl(&mut r, 'x');
    assert_eq!(r.name(), "", "Ctrl+X cuts");
    ctrl(&mut r, 'v');
    assert_eq!(r.name(), "Ideas", "Ctrl+V pastes");
}

// R3 (N7): "Enter … while an input-method composition is showing ends the
// composition and does nothing else": no save of the name minus what is being
// composed; the next Enter saves.
#[test]
fn r3_enter_while_composing_does_not_save() {
    let (mut r, _c) = open("Ideas");
    named(&mut r, WNamed::End);
    type_str(&mut r, " ");
    r.event(InputEvent::Ime(ImeEvent::Enabled));
    r.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
    assert_eq!(enter(&mut r), RenameStep::Open, "Enter during a composition does not save");
    r.event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    r.event(InputEvent::Ime(ImeEvent::Commit("日本".into())));
    assert_eq!(enter(&mut r), save("Ideas 日本"), "the committed name is saved");
}

// R3 (N7): "… the next Esc cancels": the first Esc only ends the composition.
#[test]
fn r3_esc_while_composing_ends_the_composition_first() {
    let (mut r, _c) = open("Ideas");
    named(&mut r, WNamed::End);
    r.event(InputEvent::Ime(ImeEvent::Enabled));
    r.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
    assert_eq!(esc(&mut r), RenameStep::Open, "the first Esc ends the composition, not the field");
    assert_eq!(r.name(), "Ideas", "nothing of the composition is committed");
    assert_eq!(esc(&mut r), RenameStep::Cancel, "the next Esc cancels");
}

// R3 (pointer): a click at the field's left edge places the caret before the
// first character (and ends the selection).
#[test]
fn r3_a_click_in_the_field_places_the_caret() {
    let (mut r, _c) = open("Ideas");
    let f = r.layout(label(), W, H);
    assert_eq!(r.click(Point::new(f.x0 + 1.0, (f.y0 + f.y1) / 2.0)), RenameStep::Open);
    let sel = r.field().selection();
    assert_eq!((sel.anchor.offset, sel.focus.offset), (0, 0), "a caret at the start: {sel:?}");
}

// R3 (T9): a name wider than the field scrolls inside it; the caret stays
// within the field's width.
#[test]
fn r3_a_long_name_scrolls_inside_the_field() {
    let long = "A rather long thread name that goes on ".repeat(8);
    let (mut r, _c) = open(&long);
    let width = r.layout(label(), W, H).width();
    let visible = |r: &mut LaneRename, what: &str| {
        let c = r.field_mut().caret_rect().expect("a caret");
        assert!(c.x0 >= -0.5 && c.x1 <= width + 0.5, "{what}: the caret {c:?} stays within the field's width {width}");
    };
    named(&mut r, WNamed::End);
    visible(&mut r, "after End");
    named(&mut r, WNamed::Home);
    visible(&mut r, "after Home");
}

// ======================================================================= R4

// R4: "Each lane label is an AccessKit button named "Rename thread <name>"".
#[test]
fn r4_each_label_is_a_rename_button() {
    assert_eq!(LABEL_BUTTON_PREFIX, "Rename thread ");
    let labels = vec![
        LaneLabel { lane: 0, thread_id: "thread:a".into(), name: "Ideas".into() },
        LaneLabel { lane: 3, thread_id: "thread:b".into(), name: "Notes for spring".into() },
    ];
    let nodes = lane_label_nodes(&labels);
    assert_eq!(nodes.len(), labels.len(), "one node per label");
    for (n, l) in nodes.iter().zip(&labels) {
        assert_eq!(n.role(), accesskit::Role::Button, "{}: a button", l.name);
        assert_eq!(n.label(), Some(format!("Rename thread {}", l.name).as_str()));
    }
}

// R4: "while open, the field is a text input labelled "Thread name"", holding
// the focus, its value the name.
#[test]
fn r4_the_open_field_is_a_text_input_labelled_thread_name() {
    assert_eq!(FIELD_LABEL, "Thread name");
    let (mut r, _c) = open("Ideas");
    let tree = a11y(&mut r);
    let focus = tree.state().focus().expect("focus");
    assert_eq!(focus.role(), accesskit::Role::TextInput);
    assert_eq!(focus.label().as_deref(), Some(FIELD_LABEL));
    let value = if focus.supports_text_ranges() { focus.document_range().text() } else { focus.value().unwrap_or_default() };
    assert_eq!(value.trim_end_matches('\n'), "Ideas", "the announced value is the name");
}

// R4 in the shell: the labels' buttons and the open field are part of what the
// shell announces.
#[test]
fn r4_the_shell_announces_the_labels_and_the_field() {
    let prod = code(&app_rs());
    let body = fn_body(&prod, "build_a11y_tree");
    assert!(body.contains("lane_label_nodes("), "build_a11y_tree announces the lane labels as buttons");
    assert!(body.contains("lane_rename"), "build_a11y_tree includes the open rename field");
}

// ======================================================================= R5

// R5: "Duplicates allowed": a name another thread has is saved as typed.
#[test]
fn r5_a_name_already_used_is_saved() {
    let (mut r, _c) = open("Notes");
    type_str(&mut r, "Welcome");
    assert_eq!(enter(&mut r), save("Welcome"));
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r5-dup").await;
        let db = as_dyn(&store);
        let ids = threads(&store, &["Ideas", "Notes"]).await;
        assert_eq!(save_thread_rename(&*db, &StoreStandIn(db.clone()), &ids[1], "Ideas").await, Ok(RenameOutcome::Written));
        let named: Vec<_> = db.list_threads().await.unwrap().into_iter().filter(|t| t.name == "Ideas").collect();
        assert_eq!(named.len(), 2, "both threads named Ideas");
    });
}

// R5: the rename checks no existing names.
#[test]
fn r5_the_rename_checks_no_existing_names() {
    let src = code(&production("lane_rename.rs", "#[cfg(test)]\nmod "));
    assert!(!src.contains("list_threads") && !src.contains("find_thread_by_name"), "the rename checks no existing names");
}

// ======================================================================= R6

// R6: "Not for a deleted thread (there is no label to click)": a deleted
// thread has no label and no button; nor has the fallback lane, where its
// live document is drawn and which holds no thread.
#[test]
fn r6_no_label_for_a_deleted_thread_nor_the_fallback_lane() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r6-deleted").await;
        let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
        let db = as_dyn(&store);
        let ids = threads(&store, &["Keep", "Gone"]).await;
        docs.create(new_user_document("Left behind", &ids[1], "text")).await.unwrap();
        // Deleted on another device: the thread goes, its document here stays
        // live (S5-12) and is drawn in the fallback lane.
        store.soft_delete_thread(&ids[1]).await.unwrap();

        let drawn = lane_names(&db).await;
        assert!(drawn.iter().any(|n| n == FALLBACK_LANE_NAME), "the fallback lane is drawn: {drawn:?}");
        let labels = lane_labels(&db.list_threads().await.unwrap());
        assert_eq!(labels.len(), 1, "only Keep can be renamed: {labels:?}");
        assert_eq!((labels[0].thread_id.as_str(), labels[0].name.as_str()), (ids[0].as_str(), "Keep"));
        let announced: Vec<String> =
            lane_label_nodes(&labels).iter().filter_map(|n| n.label().map(str::to_string)).collect();
        assert!(
            !announced.iter().any(|l| l.contains("Gone") || l.contains(FALLBACK_LANE_NAME)),
            "no button for the deleted thread or the fallback lane: {announced:?}"
        );
    });
}

// R6: "… nor a marker": a deleted row (as the sync manifest lists them; a
// marker is a deleted row reduced to its id, THREAD-sync S6) never gets a
// label, whichever list the labels are built from.
#[test]
fn r6_a_deleted_row_or_a_marker_never_gets_a_label() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r6-rows").await;
        let db = as_dyn(&store);
        threads(&store, &["Live", "Deleted", "Marker"]).await;
        let mut rows = db.list_threads().await.unwrap();
        let deleted_at = chrono::Utc::now().to_rfc3339();
        rows[1].deleted_at = Some(deleted_at.clone());
        rows[2].deleted_at = Some(deleted_at);
        rows[2].name = String::new();
        rows[2].description = String::new();
        let labels = lane_labels(&rows);
        assert_eq!(labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec!["Live"], "{labels:?}");
    });
}

// ======================================================================= R7

// R7: "If the thread is deleted … while the field is open, Enter saves nothing"
// and "a rename never brings a thread back from this device": a thread deleted
// here meanwhile (by the assistant, or by a deletion that arrived through sync)
// is not written, and stays deleted.
#[test]
fn r7_a_thread_deleted_meanwhile_saves_nothing() {
    let writes = RecordingWrites::default();
    rt().block_on(async {
        let store = testing::encrypted_memory_store("r7-deleted").await;
        let db = as_dyn(&store);
        let ids = threads(&store, &["Keep", "Gone"]).await;
        // The field opened on "Gone"; then the thread was deleted.
        store.soft_delete_thread(&ids[1]).await.unwrap();
        assert_eq!(save_thread_rename(&*db, &writes, &ids[1], "Journal").await, Ok(RenameOutcome::ThreadGone));
        let live: Vec<_> = db.list_threads().await.unwrap().into_iter().filter_map(|t| t.id_string()).collect();
        assert_eq!(live, vec![ids[0].clone()], "the deleted thread stays deleted");
    });
    assert!(writes.calls.borrow().is_empty(), "no write reached the stamped path: {:?}", writes.calls.borrow());
}

// R7: a thread missing here altogether (gone, or never received) is not
// written either.
#[test]
fn r7_a_missing_thread_saves_nothing() {
    let writes = RecordingWrites::default();
    rt().block_on(async {
        let (_store, db, _id) = one_thread("r7-missing", "Keep").await;
        assert_eq!(save_thread_rename(&*db, &writes, "thread:nowhere", "Journal").await, Ok(RenameOutcome::ThreadGone));
    });
    assert!(writes.calls.borrow().is_empty(), "no write: {:?}", writes.calls.borrow());
}

// R7 in the shell: "… and the field closes with a notice": where the save is
// made, a thread gone meanwhile shows a notice.
#[test]
fn r7_the_shell_tells_the_thread_is_gone() {
    let prod = code(&app_rs());
    let at = prod.find("save_thread_rename(").expect("the shell saves a rename");
    let f = enclosing_fn(&prod, at);
    assert!(f.contains("RenameOutcome::ThreadGone"), "the shell acts on a thread gone meanwhile: {f}");
    assert!(f.contains("set_notice"), "with a notice: {f}");
}

// ======================================================================= R8

// R8: "Enter with the same name (after trimming) closes the field without a
// write" — compared with the name the field opened with, so a rename made
// elsewhere meanwhile is not overridden by a name the user did not change.
#[test]
fn r8_enter_with_the_unchanged_name_closes_without_saving() {
    let (mut r, _c) = open("Ideas");
    assert_eq!(enter(&mut r), RenameStep::Cancel, "the name as opened");
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "  Ideas  ");
    assert_eq!(enter(&mut r), RenameStep::Cancel, "retyped, the same once trimmed");
    let (mut r, _c) = open("Ideas");
    type_str(&mut r, "ideas");
    assert_eq!(enter(&mut r), save("ideas"), "a change of case is a change");
}

// R8 at the save: the name the thread already has (after trimming) writes
// nothing through the stamped path.
#[test]
fn r8_the_name_the_thread_has_writes_nothing() {
    let writes = RecordingWrites::default();
    rt().block_on(async {
        let (_store, db, id) = one_thread("r8-same", "Ideas").await;
        for name in ["Ideas", "  Ideas ", "Ideas\n"] {
            assert_eq!(save_thread_rename(&*db, &writes, &id, name).await, Ok(RenameOutcome::Unchanged), "{name:?}");
        }
    });
    assert!(writes.calls.borrow().is_empty(), "no write: {:?}", writes.calls.borrow());
}

// ======================================================================= P4

// P4 (THREAD-name, noted 2026-10-18): "The label buttons are present in the
// tree whether or not a window is open" — the shell builds them outside the
// branch that announces the top window or the empty canvas.
#[test]
fn p4_the_label_buttons_do_not_depend_on_an_open_window() {
    let prod = code(&app_rs());
    let body = fn_body(&prod, "build_a11y_tree");
    let call = body.find("lane_label_nodes(").expect("build_a11y_tree announces the lane labels");
    let branch = body.find("self.windows.last()").expect("the top-window branch");
    let after = body.find("let mut nodes").expect("the end of the window / canvas branch");
    assert!(!(branch..after).contains(&call), "the label buttons are built inside the window / canvas branch");
}
