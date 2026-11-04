//! v0.0.12 item 9, T4 — "Restore into…" offers every thread
//! (`doc/plans/V0012-item9-act-on-what-was-shown.md`; review of v0.0.11, R20).
//!
//! Written before the code, by another author: whoever builds item 9 makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`): a real headless `App` (the
//! `lanes_tests` harness) with 30 threads, a document in the trash whose
//! thread was deleted, and the chooser opened as Settings → Trash opens it
//! (`trash_row_action(RestoreOne | RestoreGroup)`). What the chooser offers
//! and which thread is chosen are read from the App's accessibility tree
//! (`build_a11y_tree`): the contract makes it "an accessible list: a list
//! role, each thread an option named by the thread's name, the selection
//! exposed".
//!
//! **Seams this file names** (they do not exist at `c7e571b`, so this file
//! does not compile until they are built):
//!
//! - `App::trash_modal_key(&mut self, key: &winit::keyboard::Key)` — a key
//!   while a trash modal is open (today only Escape acts, inline in the event
//!   loop): Up / Down / Page Up / Page Down / Home / End move the chosen
//!   thread, Enter restores into it, Escape cancels.
//! - `App::trash_modal_scroll(&mut self, lines: i64)` — the wheel over the
//!   open trash modal, `lines` rows (positive: up, as
//!   `orphans_ui::ThreadChooser::scroll`).
//! - `TrashModal::visible_threads(&self) -> Vec<String>` — the ids of the
//!   threads whose rows are drawn now, top to bottom (empty for the Empty
//!   trash confirmation).
//!
//! Not assumed: how many rows show at once (only that 30 do not all show, so
//! the list scrolls), how far Page Up / Page Down move (more than one row, in
//! range), the chooser's type.

use accesskit::Role;
use winit::keyboard::{Key, NamedKey};

use super::lanes_tests::{lanes, tname, H, L, W};
use crate::trash_ui::{modal_hit, ModalHit, TrashRowAction};
use sovereign_ai::docs_host::new_user_document;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use vello::kurbo::Point;

const N: usize = 30;

/// 30 live threads (`Thread 000`…), then "Gone": created, given `docs`
/// documents, and deleted — its documents are in the trash, needing a thread.
fn world(tag: &str, docs: usize) -> (L, String, Vec<String>) {
    let mut l = lanes(tag, N);
    let store = l.store.clone();
    let gone = l.app.rt.block_on(store.create_thread(Thread::new("Gone".into(), String::new()))).unwrap().id_string().unwrap();
    let svc = l.docs();
    let ids: Vec<String> = (0..docs)
        .map(|i| l.app.rt.block_on(svc.create(new_user_document(format!("Doc {i}"), &gone, "text"))).unwrap().id)
        .collect();
    l.app.rt.block_on(sovereign_docs::delete_thread_to_trash(&svc, store.as_ref(), &gone)).expect("thread deleted");
    l.app.load_workspace_now();
    assert_eq!(l.app.lanes.thread_ids(), l.threads.as_slice(), "setup: 30 lanes, in creation order");
    l.app.refresh_trash();
    (l, gone, ids)
}

fn open_one(l: &mut L, id: &str) {
    l.app.trash_row_action(TrashRowAction::RestoreOne(id.to_string()));
    assert!(l.app.trash_modal.is_some(), "the chooser opens for a document whose thread was deleted");
}

/// What the chooser exposes: its options' names in order, the selected one,
/// and whether the focus is on it.
struct Exposed {
    names: Vec<String>,
    selected: Vec<usize>,
    focus_on_selected: bool,
}

fn exposed(l: &mut L) -> Exposed {
    let tree = l.app.build_a11y_tree();
    let node = |id: accesskit::NodeId| tree.nodes.iter().find(|(n, _)| *n == id).map(|(_, x)| x);
    let lists: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::ListBox)
        .filter(|(_, n)| {
            n.children().iter().any(|c| node(*c).is_some_and(|o| o.label().is_some_and(|s| s.starts_with("Thread "))))
        })
        .collect();
    assert_eq!(lists.len(), 1, "one list of threads (a list role) while the chooser is open");
    let options: Vec<(accesskit::NodeId, &accesskit::Node)> =
        lists[0].1.children().iter().map(|c| (*c, node(*c).expect("each option is in the tree"))).collect();
    for (_, o) in &options {
        assert_eq!(o.role(), Role::ListBoxOption, "each thread an option");
    }
    let names = options.iter().map(|(_, o)| o.label().unwrap_or_default().to_string()).collect();
    let selected: Vec<usize> =
        options.iter().enumerate().filter(|(_, (_, o))| o.is_selected() == Some(true)).map(|(i, _)| i).collect();
    let focus_on_selected = selected.len() == 1 && options[selected[0]].0 == tree.focus;
    Exposed { names, selected, focus_on_selected }
}

fn chosen(l: &mut L) -> usize {
    let e = exposed(l);
    assert_eq!(e.selected.len(), 1, "exactly one thread selected: {:?}", e.selected);
    assert!(e.focus_on_selected, "the focus is on the selected thread");
    e.selected[0]
}

fn visible(l: &L) -> Vec<String> {
    l.app.trash_modal.as_ref().expect("the chooser is open").visible_threads()
}

fn key(l: &mut L, k: NamedKey) {
    l.app.trash_modal_key(&Key::Named(k));
}

/// After every move, the chosen thread is among the rows drawn.
fn chosen_in_view(l: &mut L) -> usize {
    let i = chosen(l);
    let v = visible(l);
    assert!(v.contains(&l.threads[i]), "the chosen thread {i} is drawn: {v:?}");
    i
}

// T4 — "Every live thread is in the chooser, in the lanes' order … It is an
// accessible list: a list role, each thread an option named by the thread's
// name, the selection exposed."
#[test]
fn t4_every_live_thread_is_offered_in_lane_order_as_named_options() {
    let (mut l, _gone, ids) = world("i9-t4-all", 1);
    open_one(&mut l, &ids[0]);
    let e = exposed(&mut l);
    let lane_names: Vec<String> = (0..N).map(tname).collect();
    assert_eq!(e.names, lane_names, "all 30 live threads, in the lanes' order (never the deleted one)");
    assert_eq!(e.selected.len(), 1, "the selection exposed");
    assert!(e.focus_on_selected);
    let v = visible(&l);
    assert!(!v.is_empty() && v.len() < N, "30 threads do not all show at once: the list scrolls ({} shown)", v.len());
    for t in &v {
        assert!(l.threads.contains(t), "a drawn row is a live thread");
    }
}

// T4 — "the list scrolls (… keyboard Up/Down/Page/Home/End)".
#[test]
fn t4_the_keyboard_reaches_every_thread_and_keeps_it_in_view() {
    let (mut l, _gone, ids) = world("i9-t4-keys", 1);
    open_one(&mut l, &ids[0]);
    key(&mut l, NamedKey::Home);
    assert_eq!(chosen_in_view(&mut l), 0, "Home: the first thread");
    for want in 1..N {
        key(&mut l, NamedKey::ArrowDown);
        assert_eq!(chosen_in_view(&mut l), want, "Down moves one thread");
    }
    key(&mut l, NamedKey::ArrowDown);
    assert_eq!(chosen_in_view(&mut l), N - 1, "stays on the last");
    key(&mut l, NamedKey::ArrowUp);
    assert_eq!(chosen_in_view(&mut l), N - 2, "Up moves one thread");

    key(&mut l, NamedKey::Home);
    assert_eq!(chosen_in_view(&mut l), 0);
    key(&mut l, NamedKey::ArrowUp);
    assert_eq!(chosen_in_view(&mut l), 0, "stays on the first");
    key(&mut l, NamedKey::PageDown);
    let p = chosen_in_view(&mut l);
    assert!(p > 1 && p < N, "Page Down moves more than one thread: {p}");
    key(&mut l, NamedKey::PageUp);
    assert_eq!(chosen_in_view(&mut l), 0, "Page Up back to the first");
    key(&mut l, NamedKey::End);
    assert_eq!(chosen_in_view(&mut l), N - 1, "End: the last thread");
    key(&mut l, NamedKey::PageDown);
    assert_eq!(chosen_in_view(&mut l), N - 1, "Page Down stays in range");
    assert!(l.app.trash_modal.is_some(), "moving keeps the chooser open");
}

// T4 — "the list scrolls (wheel …)".
#[test]
fn t4_the_wheel_scrolls_the_list_and_keeps_the_choice_in_view() {
    let (mut l, _gone, ids) = world("i9-t4-wheel", 1);
    open_one(&mut l, &ids[0]);
    key(&mut l, NamedKey::Home);
    assert_eq!(visible(&l)[0], l.threads[0], "from the top");
    l.app.trash_modal_scroll(-5);
    let v = visible(&l);
    assert_ne!(v[0], l.threads[0], "the wheel scrolled down: {v:?}");
    chosen_in_view(&mut l);
    l.app.trash_modal_scroll(-1000);
    assert!(visible(&l).contains(&l.threads[N - 1]), "scrolled to the end, the last thread shows");
    chosen_in_view(&mut l);
    l.app.trash_modal_scroll(1000);
    assert_eq!(visible(&l)[0], l.threads[0], "back to the top");
    chosen_in_view(&mut l);
}

// T4 — "Enter restores into the selected thread."
#[test]
fn t4_enter_restores_into_the_selected_thread() {
    let (mut l, _gone, ids) = world("i9-t4-enter", 1);
    open_one(&mut l, &ids[0]);
    key(&mut l, NamedKey::Home);
    for _ in 0..20 {
        key(&mut l, NamedKey::ArrowDown);
    }
    assert_eq!(chosen(&mut l), 20);
    key(&mut l, NamedKey::Enter);
    assert!(l.app.trash_modal.is_none(), "the chooser closes");
    let docs = l.docs();
    let v = l.app.rt.block_on(docs.read(&ids[0])).unwrap();
    assert!(v.trashed_at.is_none(), "restored");
    assert_eq!(v.thread_id, l.threads[20], "into the 21st thread, beyond the first rows");
}

// T4 — "Escape cancels."
#[test]
fn t4_escape_cancels_and_restores_nothing() {
    let (mut l, gone, ids) = world("i9-t4-esc", 1);
    open_one(&mut l, &ids[0]);
    key(&mut l, NamedKey::End);
    key(&mut l, NamedKey::Escape);
    assert!(l.app.trash_modal.is_none(), "the chooser closes");
    let docs = l.docs();
    let v = l.app.rt.block_on(docs.read(&ids[0])).unwrap();
    assert!(v.trashed_at.is_some(), "still in the trash");
    assert_eq!(v.thread_id, gone);
}

// T4 — a click lands on the row drawn there, scrolled or not.
#[test]
fn t4_a_click_on_a_scrolled_row_restores_into_the_thread_it_shows() {
    let (mut l, _gone, ids) = world("i9-t4-click", 1);
    open_one(&mut l, &ids[0]);
    key(&mut l, NamedKey::End);
    let top = visible(&l)[0].clone();
    assert_ne!(top, l.threads[0], "setup: scrolled");
    // The first point, from the top, that the modal says is a thread row.
    let modal = l.app.trash_modal.as_ref().unwrap();
    let mut p = None;
    'scan: for yi in 0..(H as usize / 2) {
        for xi in 0..(W as usize / 4) {
            let q = Point::new(xi as f64 * 4.0, yi as f64 * 2.0);
            if matches!(modal_hit(modal, W, H, q), ModalHit::Thread(_)) {
                p = Some(q);
                break 'scan;
            }
        }
    }
    let p = p.expect("a thread row to click");
    l.app.handle_trash_modal_click(Point::new(p.x + 20.0, p.y + 8.0), W, H);
    let docs = l.docs();
    let v = l.app.rt.block_on(docs.read(&ids[0])).unwrap();
    assert!(v.trashed_at.is_none(), "restored");
    assert_eq!(v.thread_id, top, "into the thread the top row showed");
}

// T4 — "The same rule for 'Restore all into a thread…'."
#[test]
fn t4_restore_all_offers_every_thread_and_enter_restores_the_group_into_the_selected() {
    let (mut l, gone, ids) = world("i9-t4-group", 2);
    l.app.trash_row_action(TrashRowAction::RestoreGroup(gone.clone()));
    assert!(l.app.trash_modal.is_some(), "the chooser opens for the group");
    let e = exposed(&mut l);
    assert_eq!(e.names, (0..N).map(tname).collect::<Vec<_>>(), "every live thread, in the lanes' order");
    assert!(visible(&l).len() < N, "the list scrolls");
    key(&mut l, NamedKey::End);
    assert_eq!(chosen_in_view(&mut l), N - 1);
    key(&mut l, NamedKey::Enter);
    assert!(l.app.trash_modal.is_none());
    let docs = l.docs();
    for id in &ids {
        let v = l.app.rt.block_on(docs.read(id)).unwrap();
        assert!(v.trashed_at.is_none(), "{id} restored");
        assert_eq!(v.thread_id, l.threads[N - 1], "{id} into the last thread");
    }
}

// T4 — Escape on "Restore all" cancels too.
#[test]
fn t4_escape_on_restore_all_restores_nothing() {
    let (mut l, gone, ids) = world("i9-t4-group-esc", 2);
    l.app.trash_row_action(TrashRowAction::RestoreGroup(gone.clone()));
    key(&mut l, NamedKey::PageDown);
    key(&mut l, NamedKey::Escape);
    assert!(l.app.trash_modal.is_none());
    let docs = l.docs();
    for id in &ids {
        assert!(l.app.rt.block_on(docs.read(id)).unwrap().trashed_at.is_some(), "{id} still in the trash");
    }
}
