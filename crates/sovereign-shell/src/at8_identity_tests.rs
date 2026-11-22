//! C-HOST AT8 in the shell — **hosts refer by identity too** (spec ch. 08,
//! *Action targets*, AT8, v2.0.78: "A menu, a list or a drag in any host
//! carries the object's id, never its index in a list that a reload can
//! reorder"; "Each one goes through a shared function that takes an id, tested
//! without a window (D16)").
//!
//! The three host paths the spec lists, through the seams the implementer
//! added (`47aff85b`):
//! - a lane's rename: `lane_rename::label_thread_at`, `LabelClicksById`,
//!   `label_of_thread`;
//! - a document window: `card_menu::card_position`;
//! - the Skills menu: `doc_writes::menu_skill_names`, `skill_named`,
//!   `CardMenu::click_skill_named`.
//!
//! Each test does what a host does across a reload: it takes what a click or a
//! menu row carries, reorders or reloads the list, and checks that the same
//! click or row still reaches the same object. No window.
//!
//! Written by the LH test author (PR-0011); the implementer does not edit this
//! file. Green against `fae2d23`: each test names the mutation that turns it
//! red (run on 2026-11-14, see the commit message).

use std::time::{Duration, Instant};

use vello::kurbo::{Point, Rect};

use crate::app::build_skill_registry;
use crate::card_menu::{card_position, CardMenu};
use crate::doc_writes::{menu_skill_names, skill_named};
use crate::lane_rename::{label_of_thread, label_thread_at, LabelClicksById, LaneLabel};

const A: &str = "thread:aaaa";
const B: &str = "thread:bbbb";
const C: &str = "thread:cccc";

fn top() -> Rect {
    Rect::new(0.0, 100.0, 160.0, 120.0)
}

fn bottom() -> Rect {
    Rect::new(0.0, 220.0, 160.0, 240.0)
}

fn centre(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

fn label(lane: usize, thread_id: &str, name: &str) -> LaneLabel {
    LaneLabel { lane, thread_id: thread_id.into(), name: name.into() }
}

// ---------------------------------------------------------- lane rename --

/// AT8: a press on a lane label names the thread whose label is drawn there in
/// the same frame. After a reload that swaps the lanes, the same place names
/// the other thread, and each thread is found where its label now is.
/// Mutation: `label_thread_at` returns the first rect's thread → red.
#[test]
fn at8_a_label_press_names_the_thread_drawn_there_after_a_reorder() {
    let before = vec![(A.to_string(), top()), (B.to_string(), bottom())];
    assert_eq!(label_thread_at(centre(top()), &before).as_deref(), Some(A));
    assert_eq!(label_thread_at(centre(bottom()), &before).as_deref(), Some(B));

    let after = vec![(B.to_string(), top()), (A.to_string(), bottom())];
    assert_eq!(label_thread_at(centre(top()), &after).as_deref(), Some(B), "the label drawn there now");
    assert_eq!(label_thread_at(centre(bottom()), &after).as_deref(), Some(A), "A, where its label is now");
    assert_eq!(label_thread_at(Point::new(400.0, 400.0), &after), None, "not on a label");
}

/// AT8: a double-click is two presses on the same thread's label. A first
/// press on A's label, then a reload that puts B's label under the cursor,
/// then a second press at the same place: that is not a double-click on B
/// (by position, it would have opened B's rename). Two presses on B are.
/// Mutation: `LabelClicksById::press` ignores which thread the previous press
/// hit → red.
#[test]
fn at8_a_double_click_across_a_reorder_never_opens_another_threads_rename() {
    let p = centre(top());
    let t0 = Instant::now();
    let mut clicks = LabelClicksById::default();
    assert_eq!(clicks.press(Some(A), p, t0), None, "one press");
    // A reload: B's label is now where A's was.
    assert_eq!(
        clicks.press(Some(B), p, t0 + Duration::from_millis(60)),
        None,
        "a press on A then one on B is no double-click on B"
    );
    assert_eq!(
        clicks.press(Some(B), p, t0 + Duration::from_millis(120)),
        Some(B.to_string()),
        "two presses on B's label"
    );

    let mut clicks = LabelClicksById::default();
    clicks.press(Some(A), p, t0);
    assert_eq!(clicks.press(Some(A), p, t0 + Duration::from_millis(60)), Some(A.to_string()), "two presses on A's label");
}

/// AT8: the rename opens on the label of the thread that was double-clicked,
/// as the labels are now: after a reload that reorders the lanes, the label
/// found is that thread's (its new lane, its name); a thread gone has none.
/// Mutation: `label_of_thread` returns the first label → red.
#[test]
fn at8_the_rename_opens_on_the_clicked_threads_label_after_a_reload() {
    let before = vec![label(0, A, "Garden"), label(1, B, "Kitchen")];
    assert_eq!(label_of_thread(&before, B).map(|l| (l.lane, l.name.as_str())), Some((1, "Kitchen")));

    let after = vec![label(0, C, "Attic"), label(1, B, "Kitchen"), label(2, A, "Garden")];
    let found = label_of_thread(&after, A).expect("A still has a label");
    assert_eq!((found.lane, found.thread_id.as_str(), found.name.as_str()), (2, A, "Garden"), "A, in its new lane");
    let gone = vec![label(0, C, "Attic"), label(1, B, "Kitchen")];
    assert!(label_of_thread(&gone, A).is_none(), "a thread removed by the reload has no label to rename");
}

// ------------------------------------------------------- document window --

/// AT8: what opens a document window carries the document's id; the position
/// is resolved when it runs, in the cards as loaded then. A reload that
/// reorders the cards gives the new position of the same document; one that
/// removes it gives none (nothing opens, never the card now at the old index).
/// Mutation: `card_position` returns the first card's position → red.
#[test]
fn at8_a_document_window_opens_the_same_document_after_a_reload() {
    let before = ["document:a", "document:b", "document:c"];
    assert_eq!(card_position(before.iter().copied(), "document:b"), Some(1));

    let after = ["document:c", "document:a", "document:b"];
    let at = card_position(after.iter().copied(), "document:b").expect("still loaded");
    assert_eq!(after[at], "document:b", "the same document, at its new position");
    assert_eq!(at, 2);

    let removed = ["document:c", "document:a"];
    assert_eq!(card_position(removed.iter().copied(), "document:b"), None, "removed: nothing opens");
}

// ------------------------------------------------------------ Skills menu --

/// AT8: each Skills menu row carries the name of the skill it shows, and the
/// label shown is that skill's own (its first action's label, or its name).
/// Mutation: `menu_skill_names` reads the skill one place further in the
/// registry (`get(i + 1)`) → red.
#[test]
fn at8_each_skills_menu_row_carries_the_skill_it_shows() {
    let registry = build_skill_registry();
    let rows = menu_skill_names(&registry);
    assert!(!rows.is_empty());
    for (name, label) in &rows {
        let skill = registry.all_skills().iter().find(|s| s.name() == name).unwrap_or_else(|| panic!("row names {name}"));
        let own = skill.actions().into_iter().next().map(|(_, l)| l).unwrap_or_else(|| skill.name().to_string());
        assert_eq!(&own, label, "row {name}: the label shown is that skill's");
    }
}

/// AT8: a Skills menu row taken from one registry runs the skill it names in
/// another registry that holds the skills in another order (a reload), and the
/// card menu hands on the document and that name.
/// Mutation: `skill_named` returns the registry's first skill → red.
#[test]
fn at8_a_skills_menu_row_runs_the_skill_it_names_after_a_reload() {
    use sovereign_skills::skills;
    let first = build_skill_registry();
    let (name, _) = menu_skill_names(&first)
        .into_iter()
        .find(|(n, _)| n == "word-count")
        .expect("the Word Count row");

    let mut reloaded = sovereign_skills::SkillRegistry::new();
    reloaded.register(Box::new(skills::find_replace::FindReplaceSkill));
    reloaded.register(Box::new(skills::text_editor::TextEditorSkill));
    reloaded.register(Box::new(skills::word_count::WordCountSkill));
    assert_ne!(reloaded.all_skills()[0].name(), "word-count", "the reload holds the skills in another order");
    let skill = skill_named(&reloaded, &name).expect("still registered");
    assert_eq!(skill.name(), "word-count", "the row runs the skill it names");
    assert!(skill_named(&reloaded, "no-such-skill").is_none(), "a name no skill has finds none");

    let cards = vec!["document:a".to_string(), "document:b".to_string()];
    let mut menu = CardMenu::open_on(&cards, 1).expect("menu on card 1");
    menu.open_skills();
    assert_eq!(menu.click_skill_named(&name), ("document:b".to_string(), "word-count".to_string()));
}

// ------------------------------------------------- the call sites in App --

/// `app.rs`'s production part, as code lines (comments dropped, whitespace
/// removed, numbered), split at the development switches: the functions
/// compiled only with `debug_assertions` or `dev-switches` (`run_shot` and the
/// probes after it), which end the production part before its test modules.
fn app_code() -> (Vec<(usize, String)>, Vec<(usize, String)>) {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs")).unwrap();
    let prod_end = src.find("#[cfg(test)]\nmod taskbar_tests").unwrap_or(src.len());
    let dev_start = src
        .find("#[cfg(any(debug_assertions, feature = \"dev-switches\"))]\npub(crate) fn run_shot")
        .expect("the development switches start at run_shot");
    assert!(dev_start < prod_end);
    let lines = |from: usize, to: usize| -> Vec<(usize, String)> {
        let first = src[..from].lines().count() + 1;
        src[from..to]
            .lines()
            .enumerate()
            .map(|(i, l)| (first + i, l.split("//").next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect()))
            .collect()
    };
    (lines(0, dev_start), lines(dev_start, prod_end))
}

/// The calls of `name(` in `code` (not a longer name ending in it, not its
/// definition): (line number, squashed line, index in `code`).
fn calls<'a>(code: &'a [(usize, String)], name: &str) -> Vec<(usize, &'a str, usize)> {
    let pat = format!("{name}(");
    let mut out = vec![];
    for (i, (n, l)) in code.iter().enumerate() {
        let mut from = 0;
        while let Some(rel) = l[from..].find(&pat) {
            let at = from + rel;
            from = at + pat.len();
            let before = l[..at].chars().last();
            let ident_before = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let definition = l[..at].ends_with("fn");
            if !ident_before && !definition {
                out.push((*n, l.as_str(), i));
            }
        }
    }
    out
}

/// AT8 at the call sites (Fable 0040): `App` cannot be built without a window,
/// so this reads `app.rs`'s production part, as
/// `step5_tests::s5_4_app_menu_and_drag_carry_no_card_index` does. Outside the
/// development switches:
/// - a lane-label press keeps the thread's id (the labels are kept with their
///   thread id, the double-click is counted by thread id, the rename opens by
///   thread id, and `label_at`'s lane is only ever asked "is there a label");
/// - the Skills row's action carries the skill's name, built from
///   `menu_skill_names`;
/// - `open_doc_window(` is called only with a position resolved from the
///   document's id in the same arm (`card_index_of` / `card_position`), never
///   with a stored index;
/// - `run_skill(` (by registry index) and `click_skill(` have no caller.
///
/// Mutations at call sites (run on 2026-11-14): the menu's Open calls
/// `self.open_doc_window(0)` → red; the RunSkill handler calls
/// `self.run_skill(&id, 0)` → red.
#[test]
fn at8_app_call_sites_carry_ids_not_positions() {
    let (prod, dev) = app_code();
    let joined: String = prod.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
    let mut wrong = vec![];

    // Lane rename.
    for needed in [
        "label_threads:Vec<(String,Rect)>",
        "label_clicks:crate::lane_rename::LabelClicksById",
        "fnopen_lane_rename(&mutself,thread_id:&str)",
    ] {
        if !joined.contains(needed) {
            wrong.push(format!("lane rename: `{needed}` is missing"));
        }
    }
    for (n, l) in &prod {
        if let Some(at) = l.find("lane_rename::LabelClicks") {
            if !l[at..].starts_with("lane_rename::LabelClicksById") {
                wrong.push(format!("app.rs:{n}: a double-click counted by lane position"));
            }
        }
    }
    for (n, l, _) in calls(&prod, "label_at") {
        let ok = l.contains("label_at(p,&self.label_rects).is_some()") || l.contains("label_at(p,&self.label_rects).is_none()");
        if !ok {
            wrong.push(format!("app.rs:{n}: label_at's lane used for more than \"is there a label\": {l}"));
        }
    }
    if calls(&prod, "label_thread_at").is_empty() {
        wrong.push("lane rename: no press resolves a thread id (label_thread_at)".into());
    }

    // Skills menu.
    if !joined.contains("RunSkill{skill:String}") {
        wrong.push("Skills menu: the RunSkill action does not carry a name".into());
    }
    if calls(&prod, "menu_skill_names").is_empty() {
        wrong.push("Skills menu: the rows are not built from menu_skill_names".into());
    }
    for (n, _, _) in calls(&prod, "menu_skill_entries") {
        wrong.push(format!("app.rs:{n}: Skills rows from registry indices (menu_skill_entries)"));
    }

    // Document windows: a position only as resolved from the id, in the same arm.
    for (n, l, i) in calls(&prod, "open_doc_window") {
        let arg = l.split("open_doc_window(").nth(1).and_then(|r| r.split(')').next()).unwrap_or("");
        let bound_here = !arg.is_empty() && l.contains(&format!("Some({arg})=>"));
        let resolved = prod[i.saturating_sub(2)..i].iter().chain(std::iter::once(&prod[i])).any(|(_, p)| {
            p.contains("card_index_of(") || p.contains("card_position(")
        });
        if !(bound_here && resolved) {
            wrong.push(format!("app.rs:{n}: open_doc_window not on a position resolved from the id: {l}"));
        }
    }

    // Entries by position: no caller outside the development switches.
    for name in ["run_skill", "click_skill"] {
        for (n, l, _) in calls(&prod, name) {
            wrong.push(format!("app.rs:{n}: {name}( called outside the development switches: {l}"));
        }
    }
    assert!(wrong.is_empty(), "AT8 at the call sites:\n{}", wrong.join("\n"));

    // The development switch that still takes an index is where it is said to be.
    assert!(!calls(&dev, "run_skill").is_empty(), "SHELL_RUN_SKILL's run_skill( moved: re-read this test");
}
