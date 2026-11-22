//! v0.0.13 lot L5, B1 — **C-MERGE in the shell** (spec ch. 03, *Merging
//! keeps the source thread — contract (C-MERGE)*, v2.0.109): MG2 (the ended
//! lane, its end date, its mark, its cards) and MG3 (« New document », the
//! drop, the two thread choosers, the thread count).
//!
//! Fixtures: `lanes_tests.rs` (a shell unlocked on an encrypted in-memory
//! store, threads `Thread 000`, `Thread 001`, … created one second apart).
//! The merge is the document service's (`thread_merge(target, source)`), as
//! the assistant runs it; the shell offers no merge gesture (Céline,
//! 2026-11-17: « Assistant seul »).
//!
//! **The API this file names** (« types and signatures » commit; the body is
//! the implementer's): `LaneMap::ended_at(&self, lane) -> Option<DateTime<Utc>>`
//! — the end date of the thread a lane shows, where its band stops. The rest
//! is the shell's existing test surface (`drop_target_thread`,
//! `drop_card_on_lane`, `new_document`, the context menu's rows, the notice,
//! `build_a11y_tree`, `status_text`, `open_thread_chooser`, `orphan_start`).
//!
//! The words are the contract's, compared whole (the Fable maille, by
//! delegation of Céline, 2026-11-17). Dates are the device's local day
//! (`chrono::Local`), as MG2 says.
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file. A test that looks wrong is reported, not worked around.

use sovereign_ai::docs_host::new_user_document;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{Edit, EditContent, Origin};

use super::lanes_tests::{labelled, lane_y, lanes, show_lane, tname, L, W};
use crate::canvas::x_of_ts;

/// « merged into '{target}' on {YYYY-MM-DD} »
const MARK: &str = "merged into '{target}' on {date}";
/// « '{title}' is now in '{target}' ('{name}' was merged into it). »
const MOVED: &str = "'{title}' is now in '{target}' ('{name}' was merged into it).";
/// When the thread it was merged into has ended too (the Fable maille, by
/// delegation of Céline, 2026-11-17): « '{title}' is now in '{live end}'
/// ('{name}' was merged into '{target}', which has ended too). »
const MOVED_CHAIN: &str = "'{title}' is now in '{live end}' ('{name}' was merged into '{target}', which has ended too).";
/// « '{name}' has ended (merged into '{target}'), and no live thread takes
/// its place. Nothing was changed. »
const NO_LIVE_END: &str =
    "'{name}' has ended (merged into '{target}'), and no live thread takes its place. Nothing was changed.";

fn fill(template: &str, parts: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in parts {
        s = s.replace(k, v);
    }
    s
}

fn local_day(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()
}

impl L {
    fn row(&self, id: &str) -> Thread {
        self.app.rt.block_on(self.store.get_thread(id)).expect("the thread record")
    }

    /// Merges `threads[source]` into `threads[into]` through the service, as
    /// the assistant does; reloads the canvas. Checks it is ended.
    fn merge(&mut self, source: usize, into: usize) {
        let docs = self.docs();
        self.app.rt.block_on(docs.thread_merge(&self.threads[into], &self.threads[source])).expect("merged");
        let t = self.row(&self.threads[source]);
        assert_eq!(
            t.merged_into.as_deref(),
            Some(self.threads[into].as_str()),
            "MG1: {} is ended, merged into {}",
            tname(source),
            tname(into)
        );
        self.app.load_workspace_now();
    }

    /// A document created in `threads[i]` through the service; the canvas
    /// reloaded.
    fn doc(&mut self, i: usize, title: &str) -> String {
        let docs = self.docs();
        let id = self.app.rt.block_on(docs.create(new_user_document(title, &self.threads[i], "text"))).unwrap().id;
        self.app.load_workspace_now();
        id
    }

    fn thread_of_doc(&self, id: &str) -> String {
        self.app.rt.block_on(self.docs().read(id)).expect("readable").thread_id
    }

    /// The lane of `threads[i]`, found by its thread (not by its label, which
    /// may carry the mark).
    fn lane_of(&self, i: usize) -> usize {
        self.app.lanes.lane_of_thread(&self.threads[i]).unwrap_or_else(|| panic!("{} has a lane", tname(i)))
    }

    fn mark(&self, source: usize, into: usize) -> String {
        let ended = self.row(&self.threads[source]).ended_at.expect("an end date");
        fill(MARK, &[("{target}", &tname(into)), ("{date}", &local_day(ended))])
    }
}

/// The production part of `app.rs`, and the body of its function `name` (up
/// to the next method).
fn app_fn_body(name: &str) -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs")).expect("app.rs");
    let start = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} in app.rs"));
    let rest = &src[start..];
    let end = ["\n    fn ", "\n    pub(crate) fn ", "\n    pub fn "]
        .iter()
        .filter_map(|m| rest[1..].find(m))
        .min()
        .unwrap_or(rest.len() - 1);
    rest[..end + 1].to_string()
}

// ------------------------------------------------------------------ MG2 --

/// MG2: « The source's lane stops at its end date … The end date is the merge
/// stamp's time. » The ended lane's end date is its thread's `ended_at`; a
/// lane not ended has none; the canvas draws the lane bands from it.
///
/// Red today: the merge deletes the source, so it has no lane (and
/// `LaneMap::ended_at` is not built).
#[test]
fn mg2_an_ended_lane_ends_at_its_end_date() {
    let mut l = lanes("b1-mg2-end", 3);
    l.doc(0, "Seeds");
    l.merge(0, 1);
    let ended = l.row(&l.threads[0]).ended_at.expect("an end date");
    let (g, k) = (l.lane_of(0), l.lane_of(1));
    assert_eq!(l.app.lanes.ended_at(g), Some(ended), "the ended lane ends at its thread's end date");
    assert_eq!(l.app.lanes.ended_at(k), None, "the lane merged into does not end");
    assert!(app_fn_body("build_scene").contains(".ended_at("), "the lane bands are drawn from the end date");
}

/// MG2: « Their cards are drawn no later than the end date, even when edited
/// after the merge. » Seeds, edited more than a second after the merge,
/// stays in the ended lane, and its card is not drawn after the end date.
///
/// Red today: the merge moves Seeds to the other thread and deletes the
/// source.
#[test]
fn mg2_cards_of_an_ended_thread_are_drawn_no_later_than_its_end_date() {
    let mut l = lanes("b1-mg2-cards", 2);
    let seeds = l.doc(0, "Seeds");
    l.merge(0, 1);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let docs = l.docs();
    let base = l.app.rt.block_on(docs.read(&seeds)).unwrap().revision;
    let edit = Edit {
        base,
        title: None,
        content: Some(EditContent::PlainText("edited after the merge".into())),
        origin: Origin::Assistant,
        explicit: false,
        pasted_tokens: vec![],
    };
    l.app.rt.block_on(docs.save(&seeds, edit)).expect("an edit after the merge");
    l.app.load_workspace_now();
    let ended = l.row(&l.threads[0]).ended_at.expect("an end date");
    let modified = l.app.rt.block_on(l.store.get_document(&seeds)).unwrap().modified_at;
    assert!(modified.timestamp() > ended.timestamp(), "setup: edited after the end date");
    let card = l.card(&seeds);
    assert_eq!(card.lane, l.lane_of(0), "Seeds stays in the ended lane");
    let end_x = x_of_ts(ended.timestamp(), l.app.time_ref);
    assert!(card.x <= end_x, "the card is drawn no later than the end date: x {} > {end_x}", card.x);
}

/// MG2: « shows its mark, « merged into '{target}' on {YYYY-MM-DD} », which is
/// also its accessibility label, after its name. » The ended lane's label is
/// spoken with its name, then its mark.
///
/// Red today: the merge deletes the source; no lane, no mark.
#[test]
fn mg2_the_ended_lanes_accessibility_label_carries_its_mark_after_its_name() {
    let mut l = lanes("b1-mg2-mark", 2);
    l.merge(0, 1);
    let mark = l.mark(0, 1);
    let tree = l.app.build_a11y_tree();
    let labels: Vec<String> = labelled(&tree).into_iter().map(|(_, s)| s).collect();
    let name = tname(0);
    let with_mark: Vec<&String> =
        labels.iter().filter(|s| s.ends_with(&mark) && s.find(&name).is_some_and(|i| i < s.len() - mark.len())).collect();
    assert_eq!(with_mark.len(), 1, "one label: {name}, then {mark:?}; labels: {labels:?}");
    assert_eq!(labels.iter().filter(|s| s.contains("merged into")).count(), 1, "only the ended lane has a mark: {labels:?}");
}

// ------------------------------------------------------------------ MG3 --

/// MG3: « No picker, menu, drop or assistant offers it for a document. » The
/// canvas menu over the ended lane has no « New document », and asking for one
/// there creates nothing; a live lane still offers it.
///
/// Red today: the merge deletes the source (no lane).
#[test]
fn mg3_new_document_is_not_offered_on_an_ended_lane() {
    let mut l = lanes("b1-mg3-new", 2);
    l.merge(0, 1);
    let g = l.lane_of(0);
    show_lane(&mut l.app, g);
    l.app.cursor = (W * 0.5, lane_y(&l.app, g));
    l.app.open_context_menu();
    let rows: Vec<String> = l.app.ctx_menu_rows().into_iter().map(|(label, _, _)| label).collect();
    assert!(!rows.is_empty(), "the canvas menu opened over the ended lane");
    assert!(!rows.iter().any(|r| r == "New document"), "no « New document » on an ended lane: {rows:?}");
    let before = l.app.rt.block_on(l.docs().list(None)).unwrap().len();
    l.app.new_document(g);
    assert_eq!(l.app.rt.block_on(l.docs().list(None)).unwrap().len(), before, "nothing created");

    l.app.windows.clear();
    let k = l.lane_of(1);
    show_lane(&mut l.app, k);
    l.app.cursor = (W * 0.5, lane_y(&l.app, k));
    l.app.open_context_menu();
    let rows: Vec<String> = l.app.ctx_menu_rows().into_iter().map(|(label, _, _)| label).collect();
    assert!(rows.iter().any(|r| r == "New document"), "a live lane offers it: {rows:?}");
}

/// MG3: « A drop on its lane … go[es] to its live end … a drop in the shell
/// says it in a notice, since the document did not go where it was dropped »:
/// « Moved »: « '{title}' is now in '{target}' ('{name}' was merged into it). »
/// The drop highlight shows over the ended lane, for its live end.
///
/// Red today: the merge deletes the source (no lane).
#[test]
fn mg3_a_drop_on_an_ended_lane_goes_to_its_live_end_with_a_notice() {
    let mut l = lanes("b1-mg3-drop", 3);
    let pans = l.doc(2, "Pans");
    l.merge(0, 1);
    let g = l.lane_of(0);
    show_lane(&mut l.app, g);
    let y = lane_y(&l.app, g);
    assert_eq!(l.app.drop_target_thread(y), Some(l.threads[1].clone()), "over the ended lane, the drop goes to its live end");
    l.app.drop_card_on_lane(&pans, y);
    assert_eq!(l.thread_of_doc(&pans), l.threads[1], "Pans is in the live end");
    let said = l.app.notice.as_ref().map(|n| n.said.clone());
    assert_eq!(
        said,
        Some((
            "Moved".to_string(),
            fill(MOVED, &[("{title}", "Pans"), ("{target}", &tname(1)), ("{name}", &tname(0))])
        )),
        "the notice says where it went"
    );
}

/// MG3, a chain (b1-tests-v2; the Fable maille, by delegation of Céline,
/// 2026-11-17): Thread 000 into Thread 001, then Thread 001 into Thread 002.
/// A drop on Thread 000's lane goes to the live end, Thread 002, and the
/// notice names both: « '{title}' is now in '{live end}' ('{name}' was merged
/// into '{target}', which has ended too). »
///
/// Red today: the merge deletes the source (no lane).
#[test]
fn mg3_a_drop_on_a_lane_whose_target_has_ended_too_goes_to_the_live_end() {
    let mut l = lanes("b1-mg3-drop-chain", 4);
    let pans = l.doc(3, "Pans");
    l.merge(0, 1);
    l.merge(1, 2);
    let a = l.lane_of(0);
    show_lane(&mut l.app, a);
    let y = lane_y(&l.app, a);
    assert_eq!(l.app.drop_target_thread(y), Some(l.threads[2].clone()), "over Thread 000's lane, the drop goes to the live end");
    l.app.drop_card_on_lane(&pans, y);
    assert_eq!(l.thread_of_doc(&pans), l.threads[2], "Pans is in the live end");
    let said = l.app.notice.as_ref().map(|n| n.said.clone());
    assert_eq!(
        said,
        Some((
            "Moved".to_string(),
            fill(MOVED_CHAIN, &[("{title}", "Pans"), ("{live end}", &tname(2)), ("{name}", &tname(0)), ("{target}", &tname(1))])
        )),
        "the notice names the live end and the thread it was merged into"
    );
}

/// MG3: « With no live end (… a deleted end), the request or the drop is
/// refused and said. Nothing is changed. » The thread merged into is deleted;
/// a drop on the ended lane moves nothing, says the contract's sentence, and
/// no drop highlight shows there.
///
/// Red today: the merge deletes the source (no lane).
#[test]
fn mg3_a_drop_on_an_ended_lane_with_no_live_end_is_refused_and_said() {
    let mut l = lanes("b1-mg3-drop-none", 3);
    let pans = l.doc(2, "Pans");
    l.merge(0, 1);
    let docs = l.docs();
    l.app
        .rt
        .block_on(sovereign_docs::delete_thread_to_trash(&docs, l.store.as_ref(), &l.threads[1]))
        .expect("the live end deleted");
    l.app.load_workspace_now();
    let before = l.app.rt.block_on(docs.stored_state(&pans)).unwrap().meta.stamp;
    let g = l.lane_of(0);
    show_lane(&mut l.app, g);
    let y = lane_y(&l.app, g);
    assert_eq!(l.app.drop_target_thread(y), None, "no drop target where a drop does nothing");
    l.app.drop_card_on_lane(&pans, y);
    assert_eq!(l.thread_of_doc(&pans), l.threads[2], "Pans is not moved");
    assert_eq!(l.app.rt.block_on(docs.stored_state(&pans)).unwrap().meta.stamp, before, "nothing written");
    let body = l.app.notice.as_ref().map(|n| n.said.1.clone());
    assert_eq!(
        body,
        Some(fill(NO_LIVE_END, &[("{name}", &tname(0)), ("{target}", &tname(1))])),
        "the refusal is said"
    );
}

/// MG3: no picker offers an ended thread. The trash's « restore into… »
/// chooser and the orphans' « Move to… » chooser list every live thread but
/// the ended one.
///
/// Red today: the merge deletes the source, so it is missing for the wrong
/// reason; the setup's check fails first.
#[test]
fn mg3_the_thread_choosers_never_offer_an_ended_thread() {
    let mut l = lanes("b1-mg3-choosers", 3);
    let pans = l.doc(2, "Pans");
    l.merge(0, 1);
    assert!(l.row(&l.threads[0]).deleted_at.is_none(), "setup: the ended thread is live");

    l.app.open_thread_chooser(crate::trash_ui::RestoreTarget::One(pans.clone()), "Restore Pans into\u{2026}".into());
    let offered: Vec<String> = match &l.app.trash_modal {
        Some(crate::trash_ui::TrashModal::ChooseThread { threads, .. }) => threads.iter().map(|(id, _)| id.clone()).collect(),
        _ => panic!("the trash's thread chooser opened"),
    };
    assert_eq!(offered, vec![l.threads[1].clone(), l.threads[2].clone()], "the trash's chooser: the live threads, not the ended one");
    l.app.trash_modal = None;

    l.app.orphan_start(crate::orphans_ui::OrphanTarget::One(pans), crate::orphans::OrphanAction::MoveTo, "Pans".into());
    let mut offered: Vec<String> =
        l.app.orphan_modal.as_ref().expect("the orphans' chooser opened").threads.iter().map(|(id, _)| id.clone()).collect();
    offered.sort();
    let mut live = vec![l.threads[1].clone(), l.threads[2].clone()];
    live.sort();
    assert_eq!(offered, live, "the orphans' chooser: the live threads, not the ended one");
}

/// MG3: « it counts among the threads, as its lane is drawn ». Three threads,
/// one merged into another: the status bar counts three.
///
/// Red today: the merge deletes the source: two.
#[test]
fn mg3_an_ended_thread_counts_among_the_threads() {
    let mut l = lanes("b1-mg3-count", 3);
    l.merge(0, 1);
    let text = l.app.status_text();
    assert!(text.contains("3 threads"), "the status bar counts the ended thread: {text}");
}
