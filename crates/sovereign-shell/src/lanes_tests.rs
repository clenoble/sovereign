//! v0.0.12 lanes lot — items 15 (a move goes through the stamped path), 16
//! (one lane per thread, C-LANE) and 1b (raw ids in the shell). Contract:
//! `doc/plans/V0012-lot-lanes.md` (guarantees M1–M5, N1–N8, I1–I4); spec
//! ch. 12, *Y — thread lanes*.
//!
//! Written before the code, by another author: whoever builds the lot makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! This file uses only what exists today, plus one test seam (`App::test_view`,
//! the window size a headless `App` reports from `win_size`). What needs the
//! API the lot adds (the wheel, the zoom keys, a resize, the status line's
//! text, the colours, the drop target, the accessibility focus) is in
//! `lanes_api_tests.rs`.
//!
//! A child module of `app` (declared in `app.rs`), so it drives `App`'s own
//! paths — the workspace load, "New document", the drop of a dragged card, the
//! context menu, home and fit, a frame, the accessibility tree — on a real,
//! headless `App` (`App::new()` needs no window and no GPU). Stores: a real
//! encrypted store over an in-memory SurrealDB (`docs_host::testing`), unlocked
//! as `try_login` does; nothing touches a real workspace.
//!
//! Geometry the tests rely on, as the canvas draws today: lane `i` is the band
//! `[i·LANE_H, (i+1)·LANE_H)` in world Y (cards are drawn at `lane · LANE_H`);
//! the canvas area runs from `AXIS_H` (below the time ruler) to
//! `h − BOTTOM_CHROME` (above the taskbar).

use std::sync::{Arc, OnceLock};

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{raw_to_thing, thing_to_raw, RelationType, Thread};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, RemoteDocument, Stamp, Trust, WireMeta};
use sovereign_editor::input::Modifiers;
use sovereign_editor::{Position, Selection};
use winit::keyboard::Key as WKey;

use super::editor_v2::{self, route_key, SystemClipboard};
use super::{App, WindowContent};
use crate::canvas::{AXIS_H, BOTTOM_CHROME, FALLBACK_LANE_NAME, LANE_H};

/// The window (logical pixels) every test's `App` reports.
pub(super) const W: f64 = 1280.0;
pub(super) const H: f64 = 800.0;

// ======================================================================= fixtures

/// The data directory every `App` of this module reads: `SOVEREIGN_DATA_DIR`
/// when the run sets it (it should), otherwise a fresh temporary directory.
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
        let d = std::env::temp_dir().join(format!("sov-lanes-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// An in-memory system clipboard (a document window's editor reaches it).
#[derive(Default)]
struct MemClipboard(Option<String>);
impl SystemClipboard for MemClipboard {
    fn write_text(&mut self, text: &str, _privacy: sovereign_editor::host::ClipboardPrivacy) -> Result<(), String> {
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

/// A thread's name in the fixtures: "Thread 007".
pub(super) fn tname(i: usize) -> String {
    format!("Thread {i:03}")
}

/// A shell unlocked on an encrypted in-memory store holding `threads` (in
/// creation order: `threads[i]` was created `i` seconds after `threads[0]`).
pub(super) struct L {
    pub(super) app: App,
    pub(super) store: Arc<EncryptedGraphDB>,
    pub(super) threads: Vec<String>,
}

/// Threads named `names`, created one second apart (the canvas orders lanes
/// by creation), returning their ids in order.
pub(super) async fn make_threads(store: &EncryptedGraphDB, names: &[String]) -> Vec<String> {
    let base = chrono::Utc::now() - chrono::Duration::hours(2);
    let mut ids = Vec::with_capacity(names.len());
    for (i, n) in names.iter().enumerate() {
        let mut t = Thread::new(n.clone(), String::new());
        t.created_at = base + chrono::Duration::seconds(i as i64);
        ids.push(store.create_thread(t).await.unwrap().id_string().unwrap());
    }
    ids
}

/// The unlock sequence of `App::try_login`, on `store` (no auth store, no P2P).
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

/// A shell with `n` threads named `Thread 000`… and no document.
pub(super) fn lanes(tag: &str, n: usize) -> L {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    let mut app = App::new();
    app.test_view = Some((W, H));
    let store = app.rt.block_on(testing::encrypted_memory_store(tag));
    let names: Vec<String> = (0..n).map(tname).collect();
    let threads = app.rt.block_on(make_threads(&store, &names));
    unlock_into(&mut app, store.clone());
    L { app, store, threads }
}

impl L {
    pub(super) fn docs(&self) -> Arc<DocumentService> {
        self.app.docs.clone().expect("the workspace is unlocked")
    }

    /// A document created through the service in thread `threads[i]`; the
    /// canvas reloaded. Returns its id (raw).
    pub(super) fn doc_in(&mut self, i: usize, title: &str) -> String {
        let docs = self.docs();
        let id = self.app.rt.block_on(docs.create(new_user_document(title, &self.threads[i], "text"))).unwrap().id;
        self.app.load_workspace_now();
        id
    }

    pub(super) fn thread_of(&self, id: &str) -> String {
        let docs = self.docs();
        self.app.rt.block_on(docs.read(id)).expect("readable").thread_id
    }

    /// The stamp of `id`'s current state, and the stamp it replaced.
    pub(super) fn stamps(&self, id: &str) -> (Option<Stamp>, Option<Stamp>) {
        let docs = self.docs();
        let st = self.app.rt.block_on(docs.stored_state(id)).expect("stored state");
        (st.meta.stamp, st.meta.prev_stamp)
    }

    /// The canvas card of document `id` (by its raw id: I1).
    pub(super) fn card(&self, id: &str) -> &crate::canvas::Card {
        self.app.cards.iter().find(|c| c.id == id).unwrap_or_else(|| {
            let ids: Vec<&str> = self.app.cards.iter().map(|c| c.id.as_str()).collect();
            panic!("no card with the raw id {id}; cards: {ids:?}")
        })
    }

    /// The lane whose label shows `name`.
    pub(super) fn lane_named(&self, name: &str) -> usize {
        self.app
            .lane_names
            .iter()
            .position(|n| n == name)
            .unwrap_or_else(|| panic!("no lane labelled {name:?} among {} lanes", self.app.lane_names.len()))
    }

    /// The thread ids of every live document, by title.
    pub(super) fn documents_titled(&self, title: &str) -> Vec<String> {
        let docs = self.docs();
        self.app.rt.block_on(docs.list(None)).unwrap().into_iter().filter(|d| d.title == title).map(|d| d.id).collect()
    }
}

/// The screen y of lane `lane`'s middle, with the camera as it is.
pub(super) fn lane_y(app: &App, lane: usize) -> f64 {
    app.cam.w2s_y((lane as f64 + 0.5) * LANE_H)
}

/// The canvas area's top and bottom (screen y), for a window `h` high.
pub(super) fn canvas_band(h: f64) -> (f64, f64) {
    (AXIS_H, h - BOTTOM_CHROME)
}

/// Scrolls the camera (zoom 1) so lane `lane` sits in the middle of the canvas.
pub(super) fn show_lane(app: &mut App, lane: usize) {
    let (top, bottom) = canvas_band(H);
    app.cam.zoom = 1.0;
    app.cam.offset_y = (top + bottom) / 2.0 - (lane as f64 + 0.5) * LANE_H;
}

/// Every name of an accessibility update, with its node id: what Narrator
/// receives (`crate::a11y_spoken::spoken_name`; a `Label` is spoken by its
/// value), nodes without one left out. Amended for V0012-rc2-labels-spoken
/// (2026-11-01, its test author): it read the label property, which Narrator
/// does not read for a `Label` (the canvas summary).
pub(super) fn labelled(update: &accesskit::TreeUpdate) -> Vec<(accesskit::NodeId, String)> {
    update
        .nodes
        .iter()
        .map(|(id, _)| (*id, crate::a11y_spoken::spoken_name(&update.nodes, *id)))
        .filter(|(_, s)| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------- source reading

/// `src` with `//` comments removed (strings kept: a format string's `{id}`
/// counts).
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|l| {
            // A `//` inside a string literal is rare in this code; cutting there
            // only removes text, it never adds a match.
            match l.find("//") {
                Some(at) => &l[..at],
                None => l,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `src` up to its first inline test module (`#[cfg(test)]`, then `mod x {`).
fn before_inline_test_module(src: &str) -> &str {
    let mut from = 0;
    while let Some(rel) = src[from..].find("#[cfg(test)]") {
        let at = from + rel;
        let rest = &src[at + "#[cfg(test)]".len()..];
        let mut lines = rest.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("#["));
        if let Some(next) = lines.next() {
            if next.starts_with("mod ") && next.contains('{') {
                return &src[..at];
            }
        }
        from = at + 1;
    }
    src
}

/// `(file, production text)` of every source file of the shell: the test
/// files (`*_tests.rs`) left out, inline test modules cut, comments removed.
pub(super) fn shell_production() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![root.clone()];
    let mut out = vec![];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, strip_line_comments(before_inline_test_module(&src))));
        }
    }
    out.sort();
    assert!(out.len() >= 25, "only {} shell source files read", out.len());
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `ident` occurs in `line` as a whole identifier.
fn has_ident(line: &str, ident: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = line[from..].find(ident) {
        let at = from + rel;
        let before = line[..at].chars().next_back();
        let after = line[at + ident.len()..].chars().next();
        if !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char) {
            return true;
        }
        from = at + ident.len();
    }
    false
}

// ============================================================================ N1

/// N1: "A source scan fails on `LANES`, `% LANES` or a lane loop bounded by a
/// constant in production code."
///
/// CONTRACT-QUESTION (N1): `avatar_hue_index` (app.rs) uses `LANES` as the
/// size of the colour palette for contact avatars, not as a lane count. Read
/// literally, the scan forbids the name everywhere; the implementer renames
/// that use (the palette's size) rather than this test excepting it.
#[test]
fn n1_no_fixed_lane_count_in_the_shells_production_code() {
    let mut found = vec![];
    for (file, prod) in shell_production() {
        for (i, line) in prod.lines().enumerate() {
            if has_ident(line, "LANES") {
                found.push(format!("{file}:{}: LANES: {}", i + 1, line.trim()));
            }
            // `for <something with lane> in 0..<CONST or literal>`
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("for ") {
                if let Some(in_at) = rest.find(" in ") {
                    let pat = &rest[..in_at];
                    let range = rest[in_at + 4..].trim_start();
                    if pat.to_lowercase().contains("lane") {
                        if let Some(bound) = range.strip_prefix("0..").map(|b| b.trim_start_matches('=').trim_start()) {
                            let b: String = bound.chars().take_while(|c| is_ident_char(*c)).collect();
                            let constant = !b.is_empty()
                                && (b.chars().all(|c| c.is_ascii_digit())
                                    || b.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'));
                            if constant {
                                found.push(format!("{file}:{}: a lane loop bounded by {b}: {}", i + 1, line.trim()));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(found.is_empty(), "a fixed lane count in the shell (N1):\n{}", found.join("\n"));
}

/// N1: one mapping, read by every use — the lane a card is drawn in, the
/// label that lane shows, the renamable label's lane, "New document" there and
/// a drop there all name the same thread. 15 threads: thread 13 used to share
/// lane 1 with thread 1.
#[test]
fn n1_drawing_labels_new_document_and_drop_agree_on_one_lane() {
    let mut l = lanes("n1-agree", 15);
    let id = l.doc_in(13, "In thirteen");
    let lane = l.card(&id).lane;
    assert_eq!(l.app.lane_names[lane], tname(13), "the card's lane shows its thread's name");
    let label = l.app.lane_labels.iter().find(|x| x.thread_id == l.threads[13]).expect("thread 13 has a label");
    assert_eq!(label.lane, lane, "the renamable label is on the card's lane");
    assert_eq!(label.name, tname(13));

    l.app.new_document(lane);
    let made = l.documents_titled("Untitled");
    assert_eq!(made.len(), 1, "one new document");
    assert_eq!(l.thread_of(&made[0]), l.threads[13], "\"New document\" on the lane goes to the thread it shows");

    let other = l.doc_in(1, "In one");
    show_lane(&mut l.app, lane);
    let y = lane_y(&l.app, lane);
    l.app.drop_card_on_lane(&other, y);
    assert_eq!(l.thread_of(&other), l.threads[13], "a drop on the lane goes to the thread it shows");
}

// ============================================================================ N2

/// N2: "Lanes follow the live threads in creation order … With 103 threads
/// there are 103 lanes, each labelled by its own thread."
#[test]
fn n2_103_threads_are_103_lanes_each_labelled_by_its_own_thread() {
    let l = lanes("n2-103", 103);
    assert_eq!(l.app.lane_names.len(), 103, "one lane per live thread, no Unfiled lane (no document needs it)");
    for i in 0..103 {
        assert_eq!(l.app.lane_names[i], tname(i), "lane {i} shows thread {i}, in creation order");
    }
    assert_eq!(l.app.lane_labels.len(), 103, "every lane has its renamable label");
    for i in 0..103 {
        let label = l.app.lane_labels.iter().find(|x| x.lane == i).unwrap_or_else(|| panic!("lane {i} has a label"));
        assert_eq!((label.thread_id.as_str(), label.name.as_str()), (l.threads[i].as_str(), tname(i).as_str()), "lane {i}");
    }
}

/// N2: a card is drawn in its own thread's lane — thread 12 and thread 0 no
/// longer share lane 0, nor thread 102 and thread 6.
#[test]
fn n2_each_card_sits_in_its_own_threads_lane() {
    let mut l = lanes("n2-cards", 103);
    for i in [0usize, 6, 12, 50, 102] {
        let id = l.doc_in(i, &format!("Doc of {i}"));
        let lane = l.card(&id).lane;
        assert_eq!(l.app.lane_names.get(lane).map(String::as_str), Some(tname(i).as_str()), "the card of thread {i}");
    }
}

/// N2 (decks, rewritten from the deck tests where lanes stood for threads):
/// two documents of the same moment in two threads that used to share a lane
/// (0 and 12) are in two lanes, so never in one deck.
#[test]
fn n2_two_threads_never_share_a_deck() {
    let mut l = lanes("n2-decks", 13);
    let a = l.doc_in(0, "A");
    let b = l.doc_in(12, "B");
    let ia = l.app.cards.iter().position(|c| c.id == a).unwrap();
    let ib = l.app.cards.iter().position(|c| c.id == b).unwrap();
    assert_ne!(l.app.cards[ia].lane, l.app.cards[ib].lane, "two threads, two lanes");
    let roles = crate::canvas::compute_deck_roles(&l.app.cards, 0.01);
    assert_eq!(roles[ia].count, 1, "A is not decked with B");
    assert_eq!(roles[ib].count, 1, "B is not decked with A");
}

/// N2: "A new document … goes to the thread whose name the lane shows" — on
/// lane 102, and on lane 12 (which used to give thread 0).
#[test]
fn n2_new_document_goes_to_the_thread_the_lane_shows() {
    let mut l = lanes("n2-new", 103);
    for i in [102usize, 12] {
        let before = l.documents_titled("Untitled");
        let lane = l.lane_named(&tname(i));
        l.app.new_document(lane);
        let made: Vec<String> = l.documents_titled("Untitled").into_iter().filter(|d| !before.contains(d)).collect();
        assert_eq!(made.len(), 1, "one document created on lane {lane}");
        assert_eq!(l.thread_of(&made[0]), l.threads[i], "in thread {i}, the one lane {lane} shows");
    }
}

/// N2 + M2 (the shell's drag): "a dropped card goes to the thread whose name
/// the lane shows", and the move is the document service's — stamped,
/// replacing the state it moved, so it travels.
#[test]
fn n2_a_dropped_card_goes_to_the_thread_the_lane_shows_through_the_stamped_move() {
    let mut l = lanes("n2-drop", 103);
    let id = l.doc_in(0, "Traveller");
    let (before, _) = l.stamps(&id);
    let lane = l.lane_named(&tname(102));
    show_lane(&mut l.app, lane);
    let y = lane_y(&l.app, lane);
    l.app.drop_card_on_lane(&id, y);
    assert_eq!(l.thread_of(&id), l.threads[102], "moved to thread 102");
    let (after, prev) = l.stamps(&id);
    assert!(after > before, "a stamped move (M1): {before:?} -> {after:?}");
    assert_eq!(prev, before, "replacing the state it moved");
    assert_eq!(l.app.lane_names.get(l.card(&id).lane).map(String::as_str), Some(tname(102).as_str()), "and drawn there");
}

/// N2: "A card can move to any other thread" — including between two threads
/// that used to share a lane (0 and 12: the drop was refused on "its own lane").
#[test]
fn n2_a_card_moves_between_threads_that_used_to_share_a_lane() {
    let mut l = lanes("n2-shared", 13);
    let id = l.doc_in(0, "Zero");
    let lane = l.lane_named(&tname(12));
    show_lane(&mut l.app, lane);
    let y = lane_y(&l.app, lane);
    l.app.drop_card_on_lane(&id, y);
    assert_eq!(l.thread_of(&id), l.threads[12]);
}

// ============================================================================ N3

/// Thread `Gone` deleted on another device while its document `orphan` is
/// live here: drawn in the Unfiled lane. `n` live threads besides.
fn with_an_orphan(tag: &str, n: usize) -> (L, String) {
    let mut l = lanes(tag, n);
    let gone = l.app.rt.block_on(make_threads(&l.store, &["Gone".to_string()])).remove(0);
    let docs = l.docs();
    let orphan = l.app.rt.block_on(docs.create(new_user_document("Orphan", &gone, "left"))).unwrap().id;
    // The deletion arriving from the other device is the row sync's store
    // call (as in `orphans_tests.rs`).
    l.app.rt.block_on(l.store.soft_delete_thread(&gone)).unwrap();
    l.app.load_workspace_now();
    (l, orphan)
}

/// N3: "The Unfiled lane exists only while a document's thread cannot be
/// resolved, after the threads' lanes."
#[test]
fn n3_no_unfiled_lane_while_every_document_has_its_thread() {
    let mut l = lanes("n3-none", 3);
    for i in 0..3 {
        l.doc_in(i, &format!("D{i}"));
    }
    assert_eq!(l.app.lane_names, (0..3).map(tname).collect::<Vec<_>>(), "three lanes, no Unfiled");
}

/// N3: with a document whose thread was deleted, the Unfiled lane comes after
/// every thread's lane — with 13 live threads, too (it used to merge into lane
/// 11 from 12 threads on).
#[test]
fn n3_the_unfiled_lane_comes_after_the_threads_lanes_while_needed() {
    let (l, orphan) = with_an_orphan("n3-after", 13);
    assert_eq!(l.app.lane_names.len(), 14, "13 thread lanes and Unfiled: {:?}", l.app.lane_names);
    for i in 0..13 {
        assert_eq!(l.app.lane_names[i], tname(i));
    }
    assert_eq!(l.app.lane_names[13], FALLBACK_LANE_NAME, "Unfiled, last");
    assert_eq!(l.card(&orphan).lane, 13, "the orphan is drawn there");
}

/// N3: on the Unfiled lane "New document" is not offered — not in the
/// context menu, and the gesture creates nothing (it used to create in thread
/// 0, review finding 21).
#[test]
fn n3_new_document_is_not_offered_on_the_unfiled_lane() {
    let (mut l, _orphan) = with_an_orphan("n3-new", 2);
    let unfiled = l.lane_named(FALLBACK_LANE_NAME);
    show_lane(&mut l.app, unfiled);
    l.app.cursor = (W * 0.5, lane_y(&l.app, unfiled));
    l.app.open_context_menu();
    let rows: Vec<String> = l.app.ctx_menu_rows().into_iter().map(|(label, _, _)| label).collect();
    assert!(!rows.is_empty(), "the canvas menu opened over the Unfiled lane");
    assert!(!rows.iter().any(|r| r == "New document"), "no \"New document\" on Unfiled: {rows:?}");

    let before = l.app.rt.block_on(l.docs().list(None)).unwrap().len();
    l.app.new_document(unfiled);
    let after = l.app.rt.block_on(l.docs().list(None)).unwrap().len();
    assert_eq!(after, before, "nothing created from the Unfiled lane");

    // A thread's lane still offers it.
    l.app.windows.clear();
    show_lane(&mut l.app, 0);
    l.app.cursor = (W * 0.5, lane_y(&l.app, 0));
    l.app.open_context_menu();
    let rows: Vec<String> = l.app.ctx_menu_rows().into_iter().map(|(label, _, _)| label).collect();
    assert!(rows.iter().any(|r| r == "New document"), "a thread's lane offers it: {rows:?}");
}

/// N3: "a dropped card does nothing" on the Unfiled lane.
#[test]
fn n3_a_card_dropped_on_the_unfiled_lane_does_nothing() {
    let (mut l, _orphan) = with_an_orphan("n3-drop", 2);
    let id = l.doc_in(0, "Stays");
    let before = l.stamps(&id);
    let unfiled = l.lane_named(FALLBACK_LANE_NAME);
    show_lane(&mut l.app, unfiled);
    let y = lane_y(&l.app, unfiled);
    l.app.drop_card_on_lane(&id, y);
    assert_eq!(l.thread_of(&id), l.threads[0], "not moved");
    assert_eq!(l.stamps(&id), before, "nothing written");
}

/// N3: "a card dragged from it to a thread's lane moves to that thread (M1)";
/// then no document needs Unfiled, and the lane goes.
#[test]
fn n3_a_card_dragged_from_unfiled_to_a_threads_lane_moves_there() {
    let (mut l, orphan) = with_an_orphan("n3-out", 2);
    let (before, _) = l.stamps(&orphan);
    let lane = l.lane_named(&tname(1));
    show_lane(&mut l.app, lane);
    let y = lane_y(&l.app, lane);
    l.app.drop_card_on_lane(&orphan, y);
    assert_eq!(l.thread_of(&orphan), l.threads[1], "moved to thread 1");
    let (after, prev) = l.stamps(&orphan);
    assert!(after > before, "through the stamped move");
    assert_eq!(prev, before);
    assert!(!l.app.lane_names.iter().any(|n| n == FALLBACK_LANE_NAME), "Unfiled is no longer needed: {:?}", l.app.lane_names);
}

// ============================================================================ N4

/// The stop (N4, "[choice]"): the first lane never below the canvas top, the
/// last never above the canvas bottom; when every lane fits, the whole block
/// lies in the canvas. "The top" and "the bottom": the canvas area between
/// the time ruler and the taskbar (settled with the coordinator, 2026-10-24;
/// the stop applies to every vertical movement).
pub(super) fn assert_stop(app: &App, lanes: usize, h: f64, what: &str) {
    let (top, bottom) = canvas_band(h);
    let first = app.cam.offset_y;
    let last = app.cam.offset_y + lanes as f64 * LANE_H * app.cam.zoom;
    let eps = 0.5;
    if last - first <= bottom - top + eps {
        assert!(
            first >= top - eps && last <= bottom + eps,
            "{what}: every lane fits, so every lane is in the canvas: lanes {first:.1}..{last:.1}, canvas {top}..{bottom}"
        );
    } else {
        assert!(first <= top + eps, "{what}: the first lane is not below the canvas top: {first:.1} > {top}");
        assert!(last >= bottom - eps, "{what}: the last lane is not above the canvas bottom: {last:.1} < {bottom}");
    }
}

/// N6 + N4: "Home and fit frame the lanes that exist" — with 30 threads home
/// shows all 30 (it used to fit 12).
#[test]
fn n6_home_frames_every_lane_that_exists() {
    let mut l = lanes("n6-home", 30);
    l.app.frame_now();
    let (top, bottom) = canvas_band(H);
    let first = l.app.cam.offset_y;
    let last = first + 30.0 * LANE_H * l.app.cam.zoom;
    assert!(first >= top - 0.5 && last <= bottom + 0.5, "all 30 lanes in view after home: {first:.1}..{last:.1} in {top}..{bottom}");
}

/// N4: "The stop is applied again after a zoom (… home, fit)": home and fit
/// leave the lanes within the stop, with many lanes (103) and with few (3).
#[test]
fn n4_home_and_fit_respect_the_stop() {
    for n in [103usize, 3] {
        let mut l = lanes(&format!("n4-home-fit-{n}"), n);
        for i in 0..n.min(5) {
            l.doc_in(i, &format!("D{i}"));
        }
        l.app.frame_now();
        assert_stop(&l.app, n, H, &format!("{n} lanes, after home"));
        l.app.frame_fit();
        assert_stop(&l.app, n, H, &format!("{n} lanes, after fit"));
    }
}

// ============================================================================ N5

/// N5: "The accessibility summary says "{n} documents across {m} thread
/// lanes" with the real m."
#[test]
fn n5_the_accessibility_summary_says_the_real_lane_count() {
    let mut l = lanes("n5-summary", 103);
    l.doc_in(0, "One");
    l.doc_in(102, "Two");
    let tree = l.app.build_a11y_tree();
    let labels: Vec<String> = labelled(&tree).into_iter().map(|(_, s)| s).collect();
    assert!(
        labels.iter().any(|s| s.ends_with("2 documents across 103 thread lanes")),
        "the canvas is announced with its 103 lanes: {:?}",
        labels.iter().filter(|s| s.contains("thread lanes")).collect::<Vec<_>>()
    );
}

/// N5, settled reading (2026-10-24): the summary's "{m} thread lanes" counts
/// threads; the Unfiled lane, drawn as well, is not one of them.
#[test]
fn n5_the_accessibility_summary_does_not_count_the_unfiled_lane() {
    let (mut l, _orphan) = with_an_orphan("n5-summary-unfiled", 5);
    assert!(l.app.lane_names.iter().any(|n| n == FALLBACK_LANE_NAME), "setup: Unfiled is drawn");
    let tree = l.app.build_a11y_tree();
    let labels: Vec<String> = labelled(&tree).into_iter().map(|(_, s)| s).collect();
    let n = l.app.cards.len();
    assert!(
        labels.iter().any(|s| s.ends_with(&format!("{n} documents across 5 thread lanes"))),
        "5 thread lanes, Unfiled not counted: {:?}",
        labels.iter().filter(|s| s.contains("thread lanes")).collect::<Vec<_>>()
    );
}

/// N5 + N8: every lane's label is in the accessibility tree, visible or not
/// ("The accessibility tree is not culled"): 103 buttons, with the camera on
/// the first few lanes.
#[test]
fn n8_every_lane_label_is_in_the_accessibility_tree_even_off_screen() {
    let mut l = lanes("n8-a11y", 103);
    show_lane(&mut l.app, 2);
    l.app.build_scene(W, H);
    let tree = l.app.build_a11y_tree();
    let labels: Vec<String> = labelled(&tree).into_iter().map(|(_, s)| s).collect();
    for i in 0..103 {
        let want = format!("{}{}", crate::lane_rename::LABEL_BUTTON_PREFIX, tname(i));
        assert!(labels.contains(&want), "{want:?} is in the tree");
    }
}

/// N5: "Each lane's label button keeps a stable accessibility id while its
/// thread exists (keyed by the thread, not the lane index)": thread 50's
/// button keeps its id when thread 10 is deleted (its lane becomes 49).
#[test]
fn n5_a_label_buttons_accessibility_id_is_keyed_by_its_thread() {
    let mut l = lanes("n5-ids", 103);
    let want = format!("{}{}", crate::lane_rename::LABEL_BUTTON_PREFIX, tname(50));
    let id_of = |app: &mut App| -> accesskit::NodeId {
        let tree = app.build_a11y_tree();
        let found: Vec<accesskit::NodeId> = labelled(&tree).into_iter().filter(|(_, s)| *s == want).map(|(id, _)| id).collect();
        assert_eq!(found.len(), 1, "one button {want:?}");
        found[0]
    };
    let before = id_of(&mut l.app);
    let tree = l.app.build_a11y_tree();
    let mut ids: Vec<accesskit::NodeId> = labelled(&tree)
        .into_iter()
        .filter(|(_, s)| s.starts_with(crate::lane_rename::LABEL_BUTTON_PREFIX))
        .map(|(id, _)| id)
        .collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "every label button has its own id");

    l.app.rt.block_on(l.store.soft_delete_thread(&l.threads[10])).unwrap();
    l.app.load_workspace_now();
    assert_eq!(l.lane_named(&tname(50)), 49, "setup: thread 50 moved up one lane");
    assert_eq!(id_of(&mut l.app), before, "its button kept its id");
}

// ============================================================================ N8

/// A shell with `n` threads and one document in each of lanes 60..=66, the
/// camera on lane 63 at zoom 1, one frame laid out.
fn wide(tag: &str, n: usize) -> L {
    let mut l = lanes(tag, n);
    let docs = l.docs();
    for i in 60..=66 {
        l.app.rt.block_on(docs.create(new_user_document(format!("D{i}"), &l.threads[i], "x"))).unwrap();
    }
    l.app.load_workspace_now();
    show_lane(&mut l.app, 63);
    l.app.cam.offset_x = 200.0 - l.app.cards.iter().map(|c| c.x).fold(f64::INFINITY, f64::min);
    l.app.build_scene(W, H);
    l
}

/// The lanes with at least one pixel between `top` and `bottom` (screen y).
fn lanes_between(app: &App, n: usize, top: f64, bottom: f64) -> Vec<usize> {
    (0..n)
        .filter(|&i| {
            let y0 = app.cam.w2s_y(i as f64 * LANE_H);
            let y1 = app.cam.w2s_y((i + 1) as f64 * LANE_H);
            y1 > top && y0 < bottom
        })
        .collect()
}

/// N8: "With 100+ lanes, the bands, labels and cards outside the visible rows
/// are not drawn … A test draws a workspace of 120 threads headless and checks
/// the frame's work is bounded by the visible lanes." Measured three ways:
/// the labels laid out this frame are every lane visible in the canvas, and
/// none outside the window (a lane behind the taskbar may be laid out); the
/// cards drawn are those of the lanes in the window; and the scene of 240
/// threads, at the same place, holds about as many paths as the scene of 120
/// — fewer than one more per extra lane (a band alone is two fills, a label a
/// pill and its text; the allowance is for a minimap row per lane, N6's "at
/// least a hairline").
#[test]
fn n8_drawing_is_bounded_by_the_visible_lanes() {
    let l = wide("n8-120", 120);
    let (top, bottom) = canvas_band(H);
    let seen = lanes_between(&l.app, 120, top, bottom);
    let in_window = lanes_between(&l.app, 120, 0.0, H);
    assert!(seen.len() >= 5 && seen.len() <= 9, "setup: a handful of lanes in view: {seen:?}");
    assert!(seen.contains(&63));
    let mut labelled_lanes: Vec<usize> = l.app.label_rects.iter().map(|(lane, _)| *lane).collect();
    labelled_lanes.sort_unstable();
    labelled_lanes.dedup();
    for lane in &seen {
        assert!(labelled_lanes.contains(lane), "lane {lane} is visible: its label is laid out ({labelled_lanes:?})");
    }
    for lane in &labelled_lanes {
        assert!(in_window.contains(lane), "lane {lane}'s label is laid out off the window ({in_window:?})");
    }
    let in_view = l.app.cards.iter().filter(|c| in_window.contains(&c.lane)).count();
    assert!(l.app.last_visible <= in_view, "no card drawn outside the window ({} > {in_view})", l.app.last_visible);
    let visible_cards = l.app.cards.iter().filter(|c| seen.contains(&c.lane)).count();
    assert!(l.app.last_visible >= visible_cards, "every visible lane's card is drawn ({} < {visible_cards})", l.app.last_visible);

    let paths_120 = l.app.scene.encoding().n_paths;
    let l2 = wide("n8-240", 240);
    assert_eq!(lanes_between(&l2.app, 240, top, bottom), seen, "setup: the same lanes in view");
    let paths_240 = l2.app.scene.encoding().n_paths;
    assert!(
        paths_240 <= paths_120 + 120 + 16,
        "120 more lanes off screen cost {} more paths ({paths_120} -> {paths_240}): they are drawn",
        paths_240.saturating_sub(paths_120)
    );
}

// ============================================================================ I1

/// A document written on the phone, with key `id`, in thread `thread_id`,
/// arriving by sync (its id generated by the peer's store).
fn arrive(l: &mut L, id: &str, thread_id: &str, title: &str) {
    let content = serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": "from the phone"}]})
    .to_string();
    let remote = RemoteDocument {
        id: id.into(),
        title: title.into(),
        content,
        thread_id: thread_id.into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: chrono::Utc::now(),
        deleted_at: None,
        device: "peer-phone".into(),
        trust_decision: None,
    };
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: chrono::Utc::now().timestamp_millis(), counter: 0, device: "peer-phone".into() }),
        ..WireMeta::default()
    };
    let docs = l.docs();
    l.app.rt.block_on(docs.apply_wire(remote, meta)).expect("applied");
    l.app.load_workspace_now();
}

/// A key the `Display` of a `Thing` quotes (`document:⟨phone-a1⟩`).
const QUOTED_DOC: &str = "document:phone-a1";

/// I1: "Every id the shell holds … is the raw form": a card's id is the raw
/// id, for a key whose display form differs (a hyphen is quoted).
#[test]
fn i1_a_cards_id_is_the_raw_id() {
    let mut l = lanes("i1-card", 1);
    let t = l.threads[0].clone();
    arrive(&mut l, QUOTED_DOC, &t, "From the phone");
    assert_ne!(
        raw_to_thing(QUOTED_DOC).unwrap().to_string(),
        QUOTED_DOC,
        "setup: the display form of this id differs from the raw form"
    );
    let card = l.card(QUOTED_DOC);
    assert_eq!(l.app.lane_names[card.lane], tname(0));
}

/// I1: lane keys are raw — a thread whose key the display form quotes gets its
/// own lane, and its documents are drawn in it (not in Unfiled).
#[test]
fn i1_a_thread_with_a_quoted_key_gets_its_lane_and_its_documents() {
    let mut l = lanes("i1-thread", 1);
    let mut t = Thread::new("Quoted lane".into(), String::new());
    t.id = raw_to_thing("thread:my-lane");
    t.created_at = chrono::Utc::now() - chrono::Duration::minutes(30);
    assert!(l.app.rt.block_on(l.store.create_thread_with_id(t)).unwrap(), "setup: the thread");
    let docs = l.docs();
    let id = l.app.rt.block_on(docs.create(new_user_document("In the quoted lane", "thread:my-lane", "x"))).unwrap().id;
    l.app.load_workspace_now();
    assert!(!l.app.lane_names.iter().any(|n| n == FALLBACK_LANE_NAME), "no Unfiled lane: {:?}", l.app.lane_names);
    let lane = l.card(&id).lane;
    assert_eq!(l.app.lane_names[lane], "Quoted lane", "drawn in its thread's lane");
    let label = l.app.lane_labels.iter().find(|x| x.lane == lane).expect("a renamable label");
    assert_eq!(label.thread_id, "thread:my-lane", "the label names the thread by its raw id");
}

/// I1: edge ends are raw — a relationship between two documents with quoted
/// keys is drawn.
#[test]
fn i1_links_join_documents_by_raw_id() {
    let mut l = lanes("i1-links", 1);
    let t = l.threads[0].clone();
    arrive(&mut l, QUOTED_DOC, &t, "A");
    arrive(&mut l, "document:phone-b2", &t, "B");
    l.app.rt.block_on(l.store.create_relationship(QUOTED_DOC, "document:phone-b2", RelationType::References, 1.0)).unwrap();
    l.app.load_workspace_now();
    let a = l.app.cards.iter().position(|c| c.id == QUOTED_DOC).expect("card A by raw id");
    let b = l.app.cards.iter().position(|c| c.id == "document:phone-b2").expect("card B by raw id");
    assert!(
        l.app.links.iter().any(|&(x, y)| (x as usize, y as usize) == (a, b) || (x as usize, y as usize) == (b, a)),
        "the link is drawn: {:?}",
        l.app.links
    );
}

/// I1: a window's document id is the raw id.
#[test]
fn i1_a_document_windows_id_is_the_raw_id() {
    let mut l = lanes("i1-window", 1);
    let t = l.threads[0].clone();
    arrive(&mut l, QUOTED_DOC, &t, "From the phone");
    let ci = l.app.cards.iter().position(|c| c.id == QUOTED_DOC).expect("the card, by raw id");
    l.app.open_doc_window(ci);
    let ids: Vec<String> = l
        .app
        .windows
        .iter()
        .filter_map(|w| match &w.content {
            WindowContent::Doc(d) => Some(d.doc_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec![QUOTED_DOC.to_string()], "the window holds the raw id");
}

// ============================================================================ I2

/// The names a line binds to a `Thing` (`if let Some(id) = &t.id`, `let
/// Some(id) = t.id.as_ref()`, `if let (Some(a), Some(b)) = (&r.in_, &r.out)`);
/// `squashed` is the line with its whitespace removed.
fn thing_bindings(squashed: &str) -> Vec<String> {
    let ends_on_thing = |e: &str| {
        let e = e.trim_start_matches('&');
        let e = e.strip_suffix(".as_ref()").unwrap_or(e);
        [".id", ".in_", ".out"].iter().any(|f| e.ends_with(f))
    };
    let rhs_end = |r: &str| r.find(|c| c == '{' || c == ';').map(|k| r[..k].to_string()).unwrap_or_else(|| r.to_string());
    let mut out = vec![];
    // `letSome(NAME)=RHS`
    let mut from = 0;
    while let Some(rel) = squashed[from..].find("letSome(") {
        let at = from + rel + "letSome(".len();
        from = at;
        let name: String = squashed[at..].chars().take_while(|c| is_ident_char(*c)).collect();
        if let Some(rhs) = squashed[at + name.len()..].strip_prefix(")=") {
            let rhs = rhs_end(rhs);
            let rhs = rhs.strip_suffix("else").unwrap_or(&rhs);
            if !name.is_empty() && ends_on_thing(rhs) {
                out.push(name);
            }
        }
    }
    // `let(Some(A),Some(B))=(R1,R2)`
    let mut from = 0;
    while let Some(rel) = squashed[from..].find("let(Some(") {
        let at = from + rel + "let(".len();
        from = at;
        let Some(eq) = squashed[at..].find(")=(") else { continue };
        let lhs = &squashed[at..at + eq];
        let rhs = rhs_end(&squashed[at + eq + 3..]);
        let rhs = rhs.trim_end_matches(')');
        let names: Vec<String> =
            lhs.split(',').map(|p| p.trim_start_matches("Some(").trim_end_matches(')').to_string()).collect();
        let exprs: Vec<&str> = rhs.split(',').collect();
        for (n, e) in names.iter().zip(exprs) {
            if !n.is_empty() && n.chars().all(is_ident_char) && ends_on_thing(e) {
                out.push(n.clone());
            }
        }
    }
    out
}

/// Whether `later` turns `name` into text: `name.to_string()` (as a whole
/// identifier), `format!("…", name)` or `"…{name}…"` in a `format!`.
fn turns_into_text(later: &str, name: &str) -> bool {
    let call = format!("{name}.to_string()");
    let mut from = 0;
    while let Some(rel) = later[from..].find(&call) {
        let at = from + rel;
        if !later[..at].chars().next_back().is_some_and(is_ident_char) {
            return true;
        }
        from = at + call.len();
    }
    later.contains("format!(") && (later.contains(&format!("\",{name})")) || later.contains(&format!("{{{name}}}")))
}

/// The `Thing` → text conversions on a line (whitespace removed).
fn thing_to_text_on(squashed: &str) -> Vec<&'static str> {
    let mut hits = vec![];
    for f in [".id", ".in_", ".out"] {
        let mut from = 0;
        while let Some(rel) = squashed[from..].find(&format!("{f}.as_ref().map(")) {
            let at = from + rel + f.len() + ".as_ref().map(".len();
            from = at;
            let rest = &squashed[at..];
            if rest.starts_with("ToString::to_string)") || rest.starts_with("std::string::ToString::to_string)") {
                hits.push("Thing.to_string()");
                continue;
            }
            if let Some(r) = rest.strip_prefix('|') {
                let v: String = r.chars().take_while(|c| is_ident_char(*c)).collect();
                let body = &r[v.len()..];
                if !v.is_empty()
                    && (body.starts_with(&format!("|{v}.to_string())")) || body.starts_with(&format!("|format!(\"{{}}\",{v})")))
                {
                    hits.push("Thing.to_string()");
                }
            }
        }
    }
    let mut from = 0;
    while let Some(rel) = squashed[from..].find("raw_to_thing(") {
        let open = from + rel + "raw_to_thing".len();
        from = open + 1;
        let mut depth = 0usize;
        let mut close = None;
        for (k, c) in squashed[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { continue };
        let rest = &squashed[close + 1..];
        let rest = rest.strip_prefix(".unwrap()").or_else(|| rest.strip_prefix(".expect(")).unwrap_or(rest);
        if rest.starts_with(".to_string()") || rest.starts_with(".map(ToString::to_string)") {
            hits.push("raw_to_thing(..) to text");
        } else if let Some(r) = rest.strip_prefix(".map(|") {
            let v: String = r.chars().take_while(|c| is_ident_char(*c)).collect();
            if !v.is_empty() && r[v.len()..].starts_with(&format!("|{v}.to_string())")) {
                hits.push("raw_to_thing(..).map(|t| t.to_string())");
            }
        }
    }
    hits
}

/// I2: "A source scan fails on a `Thing` turned into text with `to_string()` /
/// `format!("{}")` in the shell's production code" (and I1's "the conversion
/// at app.rs:3357 goes": `raw_to_thing(..).map(|t| t.to_string())`). The
/// shell turns a `Thing` into its raw form (`thing_to_raw`, `id_string()`).
///
/// What the scan reads: `X.id` / `X.in_` / `X.out` mapped to text through
/// `as_ref().map(|t| t.to_string())`; a name bound to one of them
/// (`Some(id) = &t.id`, `(Some(a), Some(b)) = (&r.in_, &r.out)`) then turned
/// into text in the next lines (`id.to_string()`, `format!("{}", id)`,
/// `"{id}"`); and `raw_to_thing(..)` turned back into text.
#[test]
fn i2_no_thing_is_turned_into_text_in_the_shell() {
    let mut found = vec![];
    for (file, prod) in shell_production() {
        let lines: Vec<String> = prod.lines().map(|l| l.chars().filter(|c| !c.is_whitespace()).collect()).collect();
        for (i, sq) in lines.iter().enumerate() {
            for hit in thing_to_text_on(sq) {
                found.push(format!("{file}:{}: {hit}", i + 1));
            }
            for name in thing_bindings(sq) {
                for (j, later) in lines.iter().enumerate().skip(i).take(8) {
                    if turns_into_text(later, &name) {
                        found.push(format!("{file}:{}: `{name}` (a Thing, bound at line {}) turned into text", j + 1, i + 1));
                    }
                }
            }
        }
    }
    found.sort();
    found.dedup();
    assert!(found.is_empty(), "a Thing turned into text in the shell (I2; use thing_to_raw / id_string):\n{}", found.join("\n"));
}

// ============================================================================ I3

/// I3: "Nothing stored changes … A test pins it: a document, a thread and a
/// relationship created through the service have `Display` equal to raw, so a
/// store written by v0.0.11 reads the same."
///
/// CONTRACT-QUESTION (I3): the document service writes no relationship; the
/// relationship is created as the shell and the assistant create one, through
/// the store (`create_relationship`), with its generated id.
#[test]
fn i3_display_equals_raw_for_ids_created_through_the_service() {
    let l = lanes("i3-pin", 0);
    let docs = l.docs();
    let tid = l.app.rt.block_on(docs.thread_create("Pinned form", "")).unwrap();
    let thread = l.app.rt.block_on(l.store.get_thread(&tid)).unwrap();
    let t = thread.id.expect("the thread's id");
    assert_eq!(t.to_string(), thing_to_raw(&t), "a thread's id: Display = raw");
    assert_eq!(thing_to_raw(&t), tid);

    let a = l.app.rt.block_on(docs.create(new_user_document("A", &tid, "a"))).unwrap().id;
    let b = l.app.rt.block_on(docs.create(new_user_document("B", &tid, "b"))).unwrap().id;
    for id in [&a, &b] {
        let d = l.app.rt.block_on(l.store.get_document(id)).unwrap();
        let th = d.id.expect("the document's id");
        assert_eq!(th.to_string(), thing_to_raw(&th), "a document's id: Display = raw");
        assert_eq!(&thing_to_raw(&th), id);
    }
    let rel = l.app.rt.block_on(l.store.create_relationship(&a, &b, RelationType::References, 0.5)).unwrap();
    for (what, th) in [("id", rel.id.clone()), ("in", rel.in_.clone()), ("out", rel.out.clone())] {
        let th = th.unwrap_or_else(|| panic!("the relationship's {what}"));
        assert_eq!(th.to_string(), thing_to_raw(&th), "a relationship's {what}: Display = raw");
    }
}

// ============================================================================ I4

/// I4: "The orphans' guard (lot 1, BACKLOG §13) compares raw with raw": a
/// document whose thread was deleted, with a key the display form quotes,
/// open with unsaved edits, blocks its own trash from the section.
#[test]
fn i4_the_orphans_guard_finds_an_unsaved_window_by_raw_id() {
    let mut l = lanes("i4-guard", 1);
    let old = l.app.rt.block_on(make_threads(&l.store, &["Old".to_string()])).remove(0);
    l.app.rt.block_on(sovereign_docs::delete_thread_to_trash(&l.docs(), l.store.as_ref(), &old)).expect("deleted here");
    arrive(&mut l, QUOTED_DOC, &old, "Waiting");
    let waiting = l.app.rt.block_on(l.docs().orphans_of_deleted_threads()).unwrap();
    assert!(waiting.iter().any(|o| o.id == QUOTED_DOC), "setup: it waits for a decision (raw id {QUOTED_DOC})");

    let ci = l.app.cards.iter().position(|c| c.id == QUOTED_DOC).expect("its card, by raw id");
    l.app.open_doc_window(ci);
    let wi = l
        .app
        .windows
        .iter()
        .position(|w| matches!(&w.content, WindowContent::Doc(_)))
        .expect("its window opened");
    {
        let p = match &mut l.app.windows[wi].content {
            WindowContent::Doc(d) => d.editor_pane_mut().expect("the editor"),
            _ => unreachable!(),
        };
        p.set_focus(true);
        let b = &p.editor.document().blocks()[0];
        let at = Position { block: b.id(), offset: b.text().unwrap_or("").len() };
        p.editor.set_selection(Selection::caret(at));
        for c in " typed".chars() {
            let t = c.to_string();
            route_key(&mut p.editor, &WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }
    let dirty = matches!(&l.app.windows[wi].content, WindowContent::Doc(d) if d.has_unsaved_edits());
    assert!(dirty, "setup: unsaved edits in the window");
    let blocked = l.app.orphan_trash_blocked(&crate::orphans_ui::OrphanTarget::One(QUOTED_DOC.to_string()));
    assert!(blocked.is_some(), "the guard finds the window by the raw id and blocks the trash");
}
