//! Title editing — contract T1–T7 (`doc/plans/TITLE-edit.md`, decided by
//! Céline 2026-10-17; spec ch. 12, document window, *Title*). The editor's
//! half: the title field lives in the document editor, beside the body, so it
//! shares its saves, its unsaved state, its input routing and its
//! accessibility tree. The shell's half (the save through the document
//! service, the card, the window) is in `sovereign-shell/src/title_tests.rs`.
//!
//! Written before the code, by another author than the implementer: whoever
//! builds the title field makes these pass without editing this file. See
//! `crates/sovereign-shell/TESTS-title.md` for the list and the red reasons.
//!
//! T1 was revised after Céline's manual run (same day, main 6cd4787): the
//! title is part of the page (it scrolls with the body), 1.1 × the body size,
//! semi-bold — section *T1 (revised)* below.
//!
//! Invariants only. Where the contract leaves a choice open the test accepts
//! every reading that satisfies the sentence (e.g. an unchanged title may be
//! sent again or omitted; a pasted CR LF may become one or two spaces).

mod common;
use common::*;

use std::time::Duration;

use sovereign_editor::input::{ImeEvent, InputEvent, Modifiers, NamedKey, PointerButton, PointerEvent};
use sovereign_editor::{CloseRequest, Editor, EventResult, NoticeKind, OpenDocument, OpenMode, EditorPart, SaveStatus, Trust};
use unicode_segmentation::UnicodeSegmentation;

const BODY: &str = "Body text here";

/// Opens a document titled `title` with body `content`, focused; the caret is
/// wherever the editor puts it (the body, for a document opened this way).
fn open_t(title: &str, content: &str, mode: OpenMode) -> (Editor, Fakes) {
    let (host, f) = host();
    let doc = OpenDocument { title: title.into(), content: content.into(), trust: Trust::Owned, mode };
    let mut ed = Editor::open(doc, config(), host).expect("open");
    ed.set_viewport(2000.0, 1000.0, 1.0);
    ed.handle_event(InputEvent::Focus(true));
    (ed, f)
}

/// A document titled `title` (body [`BODY`]) with the caret in its title,
/// all of the title selected — how a new document starts (T3).
fn on_title(title: &str) -> (Editor, Fakes) {
    let (mut ed, f) = open_t(title, &doc1(BODY), OpenMode::Edit);
    ed.focus_title();
    (ed, f)
}

fn body(ed: &Editor) -> String {
    all_text(ed.document())
}

fn sel_range(ed: &Editor) -> (usize, usize) {
    let (a, b) = ed.title_selection();
    (a.min(b), a.max(b))
}

/// Polls with the host clock moving in 100 ms steps until the editor asks for
/// a save; returns how long that took (`None` after 60 s).
fn time_to_autosave(ed: &mut Editor, f: &Fakes) -> Option<Duration> {
    let before = save_count(f);
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited <= Duration::from_secs(60) {
        ed.poll();
        if save_count(f) > before {
            return Some(waited);
        }
        advance(f, step);
        waited += step;
    }
    None
}

/// Lets the host clock run 10 minutes in 100 ms steps, polling.
fn let_time_pass(ed: &mut Editor, f: &Fakes) {
    for _ in 0..6000 {
        advance(f, Duration::from_millis(100));
        ed.poll();
    }
}

// ------------------------------------------------------------- accessibility

fn collect(node: accesskit_consumer::Node<'_>, out: &mut Vec<accesskit_consumer::NodeId>) {
    out.push(node.id());
    for c in node.children() {
        collect(c, out);
    }
}

/// Every node of the tree reachable from its root.
fn all_ids(tree: &accesskit_consumer::Tree) -> Vec<accesskit_consumer::NodeId> {
    let mut out = vec![];
    collect(tree.state().root(), &mut out);
    out
}

/// The nodes a platform adapter would announce as "Title".
fn labelled_title(tree: &accesskit_consumer::Tree) -> Vec<accesskit_consumer::NodeId> {
    let state = tree.state();
    all_ids(tree)
        .into_iter()
        .filter(|id| state.node_by_id(*id).and_then(|n| n.label()).as_deref() == Some("Title"))
        .collect()
}

/// The one title field: a node labelled "Title", which must be unique.
fn title_field(tree: &accesskit_consumer::Tree) -> accesskit_consumer::NodeId {
    let found = labelled_title(tree);
    assert_eq!(found.len(), 1, "exactly one node is announced as \"Title\" (found {})", found.len());
    found[0]
}

// =================================================================== T1

// T1: "The document window shows the title as a single-line editable field
// above the body". Single-line: an AccessKit TextInput (not a multiline one),
// editable (not read-only), holding the title.
#[test]
fn t1_the_title_is_a_single_line_editable_field() {
    let (mut ed, _f) = open_t("Quarterly plan", &doc1(BODY), OpenMode::Edit);
    assert_eq!(ed.title(), "Quarterly plan");
    let tree = a11y_tree(&mut ed);
    let id = title_field(&tree);
    let state = tree.state();
    let n = state.node_by_id(id).unwrap();
    assert_eq!(n.role(), accesskit::Role::TextInput, "a single-line field");
    assert!(!n.is_read_only(), "editable");
    assert_eq!(n.value().as_deref(), Some("Quarterly plan"));
    // The body is still the multiline editor.
    assert!(
        all_ids(&tree).iter().any(|i| state.node_by_id(*i).unwrap().role() == accesskit::Role::MultilineTextInput),
        "the body is still there"
    );
}

// T1: "… above the body" — the title's caret is drawn above every caret of the
// body's first line.
#[test]
fn t1_the_title_is_above_the_body() {
    let (mut ed, _f) = on_title("Quarterly plan");
    paint(&mut ed);
    assert_eq!(ed.part(), EditorPart::Title);
    let t = ed.caret_rect().expect("the title has a caret");
    caret_at(&mut ed, 1, 0);
    assert_eq!(ed.part(), EditorPart::Body, "set_selection puts the caret in the body");
    let b = ed.caret_rect().expect("the body has a caret");
    assert!(t.y1 <= b.y0 + 0.5, "title caret {t:?} is above the body's first line {b:?}");
}

// T1: "… in view mode and in edit mode". The editor v2 window has no separate
// view mode: the field is editable as soon as the window opens, with no Edit
// step, and editing it leaves the body as it was.
#[test]
fn t1_the_title_is_editable_at_once() {
    let (mut ed, f) = open_t("Old name", &doc1(BODY), OpenMode::Edit);
    ed.focus_title();
    type_text(&mut ed, "New name");
    assert_eq!(ed.title(), "New name");
    assert_eq!(body(&ed), BODY, "the body is untouched");
    assert!(ed.has_unsaved_changes(), "the change is the editor's to save");
    assert!(f.saves.borrow().iter().all(|r| r.title.as_deref() != Some("Old name")));
}

// T1 (with T6, "reachable"): a click on the title puts the caret in it; a
// click in the body puts it back in the body.
#[test]
fn t1_a_click_on_the_title_puts_the_caret_there() {
    let (mut ed, _f) = on_title("Click me");
    paint(&mut ed);
    let t = ed.caret_rect().expect("title caret");
    caret_at(&mut ed, 1, 3);
    let b = ed.caret_rect().expect("body caret");
    let click = |ed: &mut Editor, x: f64, y: f64| {
        ed.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: 1,
            modifiers: Modifiers::NONE,
        }));
        ed.handle_event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
    };
    click(&mut ed, t.x0, (t.y0 + t.y1) / 2.0);
    assert_eq!(ed.part(), EditorPart::Title, "a click on the title");
    click(&mut ed, b.x0, (b.y0 + b.y1) / 2.0);
    assert_eq!(ed.part(), EditorPart::Body, "a click in the body");
    assert_eq!(caret(&ed).block, bid(1));
}

// T1: "Empty, it shows *Untitled* as a placeholder (not as text: the stored
// title stays empty until typed)."
#[test]
fn t1_an_empty_title_shows_untitled_as_a_placeholder_not_as_text() {
    let (mut ed, f) = open_t("", &doc1(BODY), OpenMode::Edit);
    assert_eq!(ed.title(), "", "the placeholder is not text");
    let tree = a11y_tree(&mut ed);
    let id = title_field(&tree);
    let state = tree.state();
    let n = state.node_by_id(id).unwrap();
    assert_eq!(n.value().unwrap_or_default(), "", "the field's value is empty");
    assert_eq!(n.placeholder(), Some("Untitled"));
    // Saving without typing a title never stores the placeholder.
    caret_at(&mut ed, 1, BODY.len());
    type_text(&mut ed, "!");
    ctrl(&mut ed, "s");
    let t = last_save(&f).title;
    assert!(t.is_none() || t.as_deref() == Some(""), "the placeholder is never saved ({t:?})");
    // Typed, the title is exactly what was typed.
    ed.focus_title();
    type_text(&mut ed, "A");
    assert_eq!(ed.title(), "A");
}

// ======================================================== T1 (revised)
// "Revised after Céline's manual run" (2026-10-17, main 6cd4787): "The title
// field scrolls with the body: it is the top of the page, not a fixed band.
// Its text is 1.1 × the body size, semi-bold (Tauri's `.title-input`: 1.1rem,
// weight 600), with the thin rule under it." (The rule is drawing: checked by
// hand, EDITOR_V2_CHECKLIST.md §14.)

/// A long document (far taller than the 1000 px viewport), its title focused.
fn long_on_title(title: &str) -> (Editor, Fakes) {
    let blocks: Vec<String> = (1..=120).map(|n| text_block(n, &format!("line {n} of a long body"))).collect();
    let (mut ed, f) = open_t(title, &v2(&blocks), OpenMode::Edit);
    ed.focus_title();
    paint(&mut ed);
    (ed, f)
}

// T1 revised: scrolling the body by N moves the title by N, and far enough
// it leaves the view.
#[test]
fn t1r_the_title_scrolls_with_the_body() {
    let (mut ed, _f) = long_on_title("Scrolled title");
    assert_eq!(ed.part(), EditorPart::Title);
    let t0 = ed.caret_rect().expect("title caret");
    assert!(ed.scroll_offset().abs() < 0.01, "opens at the top");
    ed.scroll_by(30.0);
    let n = ed.scroll_offset();
    assert!(n > 0.0, "the long body scrolls");
    paint(&mut ed);
    let t1 = ed.caret_rect().expect("title caret");
    assert!(((t0.y0 - t1.y0) - n).abs() < 0.5, "scrolled by {n}: the title moved by {} ({t0:?} -> {t1:?})", t0.y0 - t1.y0);
    assert!(((t0.y1 - t1.y1) - n).abs() < 0.5);
    assert!((t0.x0 - t1.x0).abs() < 0.5, "and not sideways");

    ed.scroll_by(5000.0);
    paint(&mut ed);
    let t2 = ed.caret_rect().expect("title caret");
    assert!(t2.y1 <= 0.0, "scrolled far, the title is above the view ({t2:?})");
}

// T1 revised: clicks follow the scrolled title — a click on where the title
// now is reaches it; a click where it was before scrolling (body there now)
// reaches the body.
#[test]
fn t1r_clicks_follow_the_scrolled_title() {
    let click = |ed: &mut Editor, x: f64, y: f64| {
        ed.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: 1,
            modifiers: Modifiers::NONE,
        }));
        ed.handle_event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
    };
    // Where the title is, unscrolled (same document, same layout).
    let (mut unscrolled, _f0) = long_on_title("Click target");
    let t0 = unscrolled.caret_rect().expect("title caret");
    let h = t0.y1 - t0.y0;
    let mid0 = (t0.y0 + t0.y1) / 2.0;

    // A small scroll with the caret in the body (at its start, so nothing
    // scrolls the view back): the title is still in view, higher up.
    let blocks: Vec<String> = (1..=120).map(|n| text_block(n, &format!("line {n} of a long body"))).collect();
    let (mut ed, _f) = open_t("Click target", &v2(&blocks), OpenMode::Edit);
    paint(&mut ed);
    assert_eq!(ed.part(), EditorPart::Body);
    ed.scroll_by(h / 3.0);
    let n = ed.scroll_offset();
    assert!(n > 0.0);
    paint(&mut ed);
    click(&mut ed, t0.x0, mid0 - n);
    assert_eq!(ed.part(), EditorPart::Title, "a click on the scrolled title (scrolled {n})");

    // Scrolled past the title: where it was is body now.
    let (mut ed, _f) = long_on_title("Click target");
    ed.scroll_by(h * 4.0 + 100.0);
    paint(&mut ed);
    assert!(ed.scroll_offset() > h * 2.0);
    click(&mut ed, t0.x0, mid0);
    assert_eq!(ed.part(), EditorPart::Body, "where the title was, before scrolling, is body now");
}

// T1 revised: an input method composing in a scrolled title reports its
// candidate area at the scrolled title (the caret it is drawn at).
#[test]
fn t1r_the_ime_area_follows_the_scrolled_title() {
    let (mut ed, f) = long_on_title("");
    let t0 = ed.caret_rect().expect("title caret");
    ed.scroll_by((t0.y1 - t0.y0) / 2.0);
    let n = ed.scroll_offset();
    assert!(n > 0.0);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "か");
    paint(&mut ed);
    let area = *f.ime_areas.borrow().last().expect("a caret area while composing");
    assert!((area.y0 - (t0.y0 - n)).abs() < 1.0, "candidate area {area:?} at the scrolled title (top {})", t0.y0 - n);
    let c = ed.caret_rect().expect("title caret");
    assert!((area.y0 - c.y0).abs() < 1.0, "and at the caret the editor reports ({c:?})");
}

// T1 revised: "1.1 × the body size, semi-bold (… weight 600)".
#[test]
fn t1r_the_title_is_1_1_times_the_body_size_semi_bold() {
    let (ed, _f) = on_title("Hag");
    let font = ed.title_font();
    assert_eq!(font.weight, 600, "semi-bold");
    let ratio = font.size / font.body_size;
    assert!((ratio - 1.1).abs() < 0.01, "title {} px for a {} px body: {ratio:.3} × (want 1.1 ×)", font.size, font.body_size);
}

// T1 revised, measured on the layout rather than declared: the title's line
// is about 1.1 × the height of a body line of the same plain text (the old
// band's heading size, 1.4 ×, fails).
#[test]
fn t1r_the_title_line_is_about_1_1_times_a_body_line() {
    let (mut ed, _f) = open_t("Hag", &doc1("Hag"), OpenMode::Edit);
    paint(&mut ed);
    caret_at(&mut ed, 1, 0);
    let b = ed.caret_rect().expect("body caret");
    ed.focus_title();
    let t = ed.caret_rect().expect("title caret");
    let ratio = (t.y1 - t.y0) / (b.y1 - b.y0);
    assert!((1.04..=1.17).contains(&ratio), "title line / body line = {ratio:.3} (want about 1.1)");
}

// =================================================================== T9
// "Second manual run" (2026-10-17, main 595ce3d): "The title field never
// wraps; when its text is wider than the field, it scrolls horizontally so the
// caret is always visible (typing, arrows, Home/End, selection, a click near
// the edge), as an HTML single-line input does."

/// The editor's width in these tests: narrow, so the title overflows.
const NARROW: f64 = 300.0;
const LONG_TITLE: &str = "A title much longer than the narrow field can show at once, word after word after word";

/// A narrow editor on a long title, the caret in the title.
fn narrow_on_title(title: &str) -> (Editor, Fakes) {
    let (mut ed, f) = open_t(title, &doc1(BODY), OpenMode::Edit);
    ed.set_viewport(NARROW, 600.0, 1.0);
    ed.focus_title();
    paint(&mut ed);
    (ed, f)
}

/// The title's caret is inside the editor's width, on the title's one line.
fn assert_caret_visible(ed: &mut Editor, line_y: f64, what: &str) {
    paint(ed);
    let c = ed.caret_rect().expect("title caret");
    assert!(c.x0 >= -0.5 && c.x1 <= NARROW + 0.5, "{what}: caret {c:?} outside the {NARROW} px field");
    assert!((c.y0 - line_y).abs() < 0.5, "{what}: the caret left the title's line ({c:?}, line at {line_y}): it wrapped");
}

// T9: Home, End and the arrows keep the caret visible and on one line.
#[test]
fn t9_home_end_and_arrows_keep_the_caret_visible() {
    let (mut ed, _f) = narrow_on_title(LONG_TITLE);
    press(&mut ed, NamedKey::Home);
    paint(&mut ed);
    let line_y = ed.caret_rect().expect("caret").y0;
    assert_caret_visible(&mut ed, line_y, "Home");
    press(&mut ed, NamedKey::End);
    assert_eq!(ed.title_selection(), (LONG_TITLE.len(), LONG_TITLE.len()), "End is the end of the title");
    assert_caret_visible(&mut ed, line_y, "End");
    for i in 0..LONG_TITLE.len() {
        press(&mut ed, NamedKey::ArrowLeft);
        assert_caret_visible(&mut ed, line_y, &format!("Left x{}", i + 1));
    }
    assert_eq!(ed.title_selection().1, 0);
    for i in 0..LONG_TITLE.len() {
        press(&mut ed, NamedKey::ArrowRight);
        assert_caret_visible(&mut ed, line_y, &format!("Right x{}", i + 1));
    }
    press(&mut ed, NamedKey::Home);
    assert_caret_visible(&mut ed, line_y, "Home again");
}

// T9: typing past the field's edge keeps the caret visible; the title never
// wraps.
#[test]
fn t9_typing_past_the_edge_keeps_the_caret_visible() {
    let (mut ed, _f) = narrow_on_title("");
    let line_y = ed.caret_rect().expect("caret").y0;
    for (i, g) in LONG_TITLE.graphemes(true).enumerate() {
        ed.handle_event(InputEvent::Text(g.to_string()));
        assert_caret_visible(&mut ed, line_y, &format!("after {} typed", i + 1));
    }
    assert_eq!(ed.title(), LONG_TITLE);
}

// T9: the moving end of a selection (its focus) stays visible, both ways.
#[test]
fn t9_a_selection_keeps_its_moving_end_visible() {
    let (mut ed, _f) = narrow_on_title(LONG_TITLE);
    press(&mut ed, NamedKey::Home);
    paint(&mut ed);
    let line_y = ed.caret_rect().expect("caret").y0;
    key(&mut ed, NamedKey::End, Modifiers::SHIFT);
    assert_eq!(ed.title_selection(), (0, LONG_TITLE.len()));
    assert_caret_visible(&mut ed, line_y, "Shift+End");
    key(&mut ed, NamedKey::Home, Modifiers::SHIFT);
    assert_eq!(ed.title_selection(), (0, 0));
    assert_caret_visible(&mut ed, line_y, "Shift+Home");
    for i in 0..20 {
        key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
        assert_caret_visible(&mut ed, line_y, &format!("Shift+Right x{}", i + 1));
    }
}

// T9: a click near the edge lands on the text shown there (the scrolled
// text), not on what would be there unscrolled.
#[test]
fn t9_a_click_near_the_edge_lands_on_the_text_shown_there() {
    let click = |ed: &mut Editor, x: f64, y: f64| {
        ed.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: 1,
            modifiers: Modifiers::NONE,
        }));
        ed.handle_event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
    };
    let len = LONG_TITLE.len();
    let (mut ed, _f) = narrow_on_title(LONG_TITLE);
    press(&mut ed, NamedKey::End);
    paint(&mut ed);
    let c = ed.caret_rect().expect("caret");
    let mid = (c.y0 + c.y1) / 2.0;
    // Scrolled to the end: the right edge shows the title's last characters.
    click(&mut ed, NARROW - 3.0, mid);
    let at = ed.title_selection().1;
    assert!(at + 3 >= len, "a click at the right edge, scrolled to the end, lands near the end (offset {at} of {len})");
    assert_caret_visible(&mut ed, c.y0, "after the click at the right edge");
    // Still scrolled toward the end: the left edge shows text well past the start.
    press(&mut ed, NamedKey::End);
    paint(&mut ed);
    click(&mut ed, 3.0, mid);
    let at = ed.title_selection().1;
    assert!(at > len / 3, "a click at the left edge of the scrolled title lands on the text shown there (offset {at} of {len})");
    assert_caret_visible(&mut ed, c.y0, "after the click at the left edge");
}

// T9: an input method composing at the end of a long title places its
// candidate window inside the field (at the visible caret).
#[test]
fn t9_the_ime_area_is_at_the_visible_caret() {
    let (mut ed, f) = narrow_on_title(LONG_TITLE);
    press(&mut ed, NamedKey::End);
    paint(&mut ed);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "にほん");
    paint(&mut ed);
    let area = *f.ime_areas.borrow().last().expect("a caret area while composing");
    assert!(area.x0 >= -0.5 && area.x0 <= NARROW + 0.5, "candidate area {area:?} outside the {NARROW} px field");
}

// =================================================================== T2

// T2: "saved … by the same delayed save as the body": a title-only change is
// saved after the same pause as a body-only change, and not before.
#[test]
fn t2_a_title_change_is_saved_by_the_same_delayed_save_as_the_body() {
    let (mut ed, f) = open_t("Old", &doc1(BODY), OpenMode::Edit);
    caret_at(&mut ed, 1, BODY.len());
    type_text(&mut ed, "x");
    let body_delay = time_to_autosave(&mut ed, &f).expect("the body autosaves");

    let (mut ed, f) = on_title("Old");
    let model_before = model_json(&ed);
    type_text(&mut ed, "New");
    let title_delay = time_to_autosave(&mut ed, &f).expect("a title-only change autosaves");
    assert_eq!(title_delay, body_delay, "same delay as the body");
    assert_eq!(save_count(&f), 1);
    let req = last_save(&f);
    assert_eq!(req.title.as_deref(), Some("New"), "the save carries the title");
    assert_eq!(req.content, model_before, "a title-only change leaves the body as it was");
}

// T2: "a title-only change and a body change made together land in one save".
#[test]
fn t2_title_and_body_changes_made_together_land_in_one_save() {
    let (mut ed, f) = on_title("Old");
    type_text(&mut ed, "New");
    caret_at(&mut ed, 1, BODY.len());
    type_text(&mut ed, " more");
    let_time_pass(&mut ed, &f);
    assert_eq!(save_count(&f), 1, "one save for both changes");
    let req = last_save(&f);
    assert_eq!(req.title.as_deref(), Some("New"));
    assert_eq!(texts_in(&req.content), vec![format!("{BODY} more")]);
    // Once the store has it, nothing is unsaved and nothing more is saved.
    finish(&mut ed, req.id, true);
    assert!(!ed.has_unsaved_changes());
    assert_eq!(ed.save_status(), SaveStatus::Saved);
    let_time_pass(&mut ed, &f);
    assert_eq!(save_count(&f), 1);
}

// T2: Ctrl+S (and the Save command the unsaved prompt uses) saves the title
// at once.
#[test]
fn t2_ctrl_s_saves_the_title_at_once() {
    let (mut ed, f) = on_title("Old");
    type_text(&mut ed, "Typed");
    ctrl(&mut ed, "s");
    assert_eq!(save_count(&f), 1);
    assert_eq!(last_save(&f).title.as_deref(), Some("Typed"));

    let (mut ed, f) = on_title("Old");
    type_text(&mut ed, "By command");
    cmd(&mut ed, sovereign_editor::input::Command::Save);
    assert_eq!(save_count(&f), 1);
    assert_eq!(last_save(&f).title.as_deref(), Some("By command"));
}

// T2: a save never rewrites a title nobody changed into something else.
#[test]
fn t2_an_unchanged_title_is_not_rewritten() {
    let (mut ed, f) = open_t("Keep me", &doc1(BODY), OpenMode::Edit);
    caret_at(&mut ed, 1, BODY.len());
    type_text(&mut ed, " edited");
    ctrl(&mut ed, "s");
    let t = last_save(&f).title;
    assert!(t.is_none() || t.as_deref() == Some("Keep me"), "{t:?}");
}

// T2: "a conflicting save behaves as the body's does" — the editor's side: a
// failed save of a title change keeps the title, says so, and the next save
// carries it again (the service's conflict is in the shell's tests).
#[test]
fn t2_a_failed_title_save_keeps_the_title_and_says_so() {
    let (mut ed, f) = on_title("Old");
    type_text(&mut ed, "New");
    ctrl(&mut ed, "s");
    let id = last_save(&f).id;
    finish(&mut ed, id, false);
    assert_eq!(ed.title(), "New", "the title is kept");
    assert!(ed.has_unsaved_changes());
    assert!(matches!(ed.save_status(), SaveStatus::Failed { .. }));
    assert!(ed.take_notices().iter().any(|n| n.kind == NoticeKind::SaveFailed), "and it says so");
    ctrl(&mut ed, "s");
    assert_eq!(last_save(&f).title.as_deref(), Some("New"), "the retry carries it");
}

// =================================================================== T3

// T3: "Creating a document opens its window with the title field focused and
// its text selected" — the editor's part: focus_title selects all of it, and
// typing replaces it.
#[test]
fn t3_focus_title_selects_the_whole_title() {
    for title in ["Draft", ""] {
        let (mut ed, _f) = on_title(title);
        assert_eq!(ed.part(), EditorPart::Title);
        assert_eq!(sel_range(&ed), (0, title.len()), "all of {title:?} is selected");
        type_text(&mut ed, "Plan");
        assert_eq!(ed.title(), "Plan", "typing replaces it");
        assert_eq!(body(&ed), BODY);
    }
}

// T3: "**Enter** or **Tab** moves the caret to the start of the body".
#[test]
fn t3_enter_or_tab_moves_the_caret_to_the_start_of_the_body() {
    for k in [NamedKey::Enter, NamedKey::Tab] {
        let (mut ed, _f) = on_title("Name");
        ed.set_title_selection(2, 2);
        let model = model_json(&ed);
        let r = key(&mut ed, k, Modifiers::NONE);
        assert_eq!(r, EventResult::Handled, "{k:?} is the editor's in the title (the host must not act on it)");
        assert_eq!(ed.part(), EditorPart::Body, "{k:?}");
        assert_eq!(ed.selection(), sovereign_editor::Selection::caret(pos(1, 0)), "{k:?}: the start of the body");
        assert_eq!(ed.title(), "Name", "{k:?}: the title is unchanged");
        assert_eq!(model_json(&ed), model, "{k:?}: the body is unchanged");
        type_text(&mut ed, "> ");
        assert_eq!(body(&ed), format!("> {BODY}"), "{k:?}: typing goes to the body's start");
    }
}

// T3: "**Shift+Tab** from anywhere in the body comes back to the title"
// (contract revised 2026-10-17, 99c662b: Tauri's field order) — from the
// start, the middle, the end, a selection, and a later block.
#[test]
fn t3_shift_tab_from_anywhere_in_the_body_comes_back_to_the_title() {
    let content = v2(&[text_block(1, BODY), text_block(2, "second block")]);
    let places: [(&str, fn(&mut Editor)); 5] = [
        ("start", |e| caret_at(e, 1, 0)),
        ("middle", |e| caret_at(e, 1, 4)),
        ("end", |e| caret_at(e, 1, BODY.len())),
        ("selection", |e| e.set_selection(sovereign_editor::Selection { anchor: pos(1, 2), focus: pos(1, 7) })),
        ("later block", |e| caret_at(e, 2, 3)),
    ];
    for (where_, place) in places {
        let (mut ed, _f) = open_t("Name", &content, OpenMode::Edit);
        place(&mut ed);
        assert_eq!(ed.part(), EditorPart::Body);
        let model = model_json(&ed);
        let r = key(&mut ed, NamedKey::Tab, Modifiers::SHIFT);
        assert_eq!(r, EventResult::Handled, "{where_}");
        assert_eq!(ed.part(), EditorPart::Title, "{where_}");
        assert_eq!(model_json(&ed), model, "{where_}: nothing typed into the body");
        type_text(&mut ed, "!");
        assert!(ed.title().contains('!'), "{where_}: typing goes to the title");
        assert_eq!(model_json(&ed), model, "{where_}: the body is unchanged");
    }
}

// T3 / T6 ("reachable by Tab" = the field order title → body, 99c662b), round
// trip: Tab from the title, Shift+Tab back, Tab again — the caret
// alternates and nothing is typed anywhere.
#[test]
fn t3_tab_and_shift_tab_alternate_between_title_and_body() {
    let (mut ed, _f) = on_title("Name");
    let model = model_json(&ed);
    for _ in 0..3 {
        key(&mut ed, NamedKey::Tab, Modifiers::NONE);
        assert_eq!(ed.part(), EditorPart::Body);
        key(&mut ed, NamedKey::Tab, Modifiers::SHIFT);
        assert_eq!(ed.part(), EditorPart::Title);
    }
    assert_eq!(ed.title(), "Name");
    assert_eq!(model_json(&ed), model);
}

// =================================================================== T4

// T4: "A newline cannot enter the title (typed Enter moves to the body …)".
// Enter with any modifier (Shift+Enter makes a soft break in the body) adds no
// line break to the title.
#[test]
fn t4_enter_never_puts_a_line_break_in_the_title() {
    for m in [Modifiers::NONE, Modifiers::SHIFT] {
        let (mut ed, _f) = on_title("ab");
        ed.set_title_selection(1, 1);
        key(&mut ed, NamedKey::Enter, m);
        assert!(!ed.title().contains(['\n', '\r']), "{m:?}: {:?}", ed.title());
        assert_eq!(ed.title(), "ab", "{m:?}");
    }
}

// T4: "… a pasted newline becomes a space".
#[test]
fn t4_a_pasted_newline_becomes_a_space() {
    let (mut ed, f) = on_title("");
    set_clip_text(&f, "first\nsecond");
    ctrl(&mut ed, "v");
    assert_eq!(ed.title(), "first second");
    assert_eq!(body(&ed), BODY, "the paste stayed in the title");

    // CR LF and a lone CR are line breaks too: no CR or LF gets in, the text
    // around is kept, and only spaces stand between.
    for clip in ["a\r\nb", "a\rb"] {
        let (mut ed, f) = on_title("");
        set_clip_text(&f, clip);
        ctrl(&mut ed, "v");
        let t = ed.title();
        assert!(!t.contains(['\n', '\r']), "{clip:?} -> {t:?}");
        assert!(t.starts_with('a') && t.ends_with('b') && t.len() >= 3, "{clip:?} -> {t:?}");
        assert!(t[1..t.len() - 1].chars().all(|c| c == ' '), "{clip:?} -> {t:?}");
    }
}

// T4: text that arrives with a line break by other paths (an input method's
// commit, a text event) does not put one in the title either.
#[test]
fn t4_no_line_break_enters_the_title_by_other_paths() {
    let (mut ed, _f) = on_title("");
    ed.handle_event(InputEvent::Text("one\ntwo".into()));
    ime(&mut ed, ImeEvent::Enabled);
    ime(&mut ed, ImeEvent::Commit("三\r\n四".into()));
    ime(&mut ed, ImeEvent::Disabled);
    let t = ed.title();
    assert!(!t.contains(['\n', '\r']), "{t:?}");
    for piece in ["one", "two", "三", "四"] {
        assert!(t.contains(piece), "{piece} kept in {t:?}");
    }
}

// T4: "Leading and trailing spaces are trimmed at save".
#[test]
fn t4_the_saved_title_is_trimmed() {
    let (mut ed, f) = on_title("");
    type_text(&mut ed, "  Annual report  ");
    ctrl(&mut ed, "s");
    let req = last_save(&f);
    assert_eq!(req.title.as_deref(), Some("Annual report"));
    // What the store got is what is in memory, trimmed: nothing stays
    // "unsaved" because of the spaces, and nothing is saved again.
    finish(&mut ed, req.id, true);
    assert!(!ed.has_unsaved_changes(), "the trimmed save saved the title");
    let_time_pass(&mut ed, &f);
    assert_eq!(save_count(&f), 1);
}

// T4: "a title of spaces only is saved empty (shown *Untitled*)".
#[test]
fn t4_a_title_of_spaces_only_is_saved_empty() {
    let (mut ed, f) = on_title("Something");
    type_text(&mut ed, "   ");
    ctrl(&mut ed, "s");
    assert_eq!(last_save(&f).title.as_deref(), Some(""));
    // Opened again from what was saved: the placeholder shows.
    let (mut again, _f) = open_t("", &doc1(BODY), OpenMode::Edit);
    let tree = a11y_tree(&mut again);
    let id = title_field(&tree);
    assert_eq!(tree.state().node_by_id(id).unwrap().placeholder(), Some("Untitled"));
}

// =================================================================== T5

// T5: "A title change not yet saved counts as an unsaved edit".
#[test]
fn t5_an_unsaved_title_is_an_unsaved_edit() {
    let (mut ed, _f) = on_title("Old");
    assert!(!ed.has_unsaved_changes());
    type_text(&mut ed, "New");
    assert!(ed.has_unsaved_changes());
    assert_eq!(ed.save_status(), SaveStatus::Unsaved);
}

// T5: "closing the window … behaves exactly as with an unsaved body" — the
// editor gives the same answer to a close request, before and after a failed
// save, whichever part holds the change.
#[test]
fn t5_closing_with_an_unsaved_title_answers_as_with_an_unsaved_body() {
    let (mut by_body, fb) = open_t("Old", &doc1(BODY), OpenMode::Edit);
    caret_at(&mut by_body, 1, BODY.len());
    type_text(&mut by_body, "x");
    let (mut by_title, ft) = on_title("Old");
    type_text(&mut by_title, "New");

    let a = by_body.request_close();
    let b = by_title.request_close();
    assert_eq!(b, a, "same answer as an unsaved body");
    assert_ne!(b, CloseRequest::Ready, "an unsaved title does not close silently");
    if b == CloseRequest::Saving {
        assert_eq!(last_save(&ft).title.as_deref(), Some("New"), "the save on close carries the title");
    }
    // After a failed save: the same answer again (the unsaved prompt).
    for (ed, f) in [(&mut by_body, &fb), (&mut by_title, &ft)] {
        if save_count(f) == 0 {
            ctrl(ed, "s");
        }
        let id = last_save(f).id;
        finish(ed, id, false);
    }
    assert_eq!(by_title.request_close(), by_body.request_close());
    assert_eq!(by_title.request_close(), CloseRequest::ConfirmUnsaved);
}

// =================================================================== T6

// T6: "The field is an AccessKit text input labelled "Title", reachable by
// Tab, announced with its value": focus, value, caret and selection follow
// the title; Tab and Shift+Tab move the accessible focus too.
#[test]
fn t6_the_title_field_is_announced_with_its_value_and_focus() {
    let (mut ed, _f) = on_title("Minutes");
    let tree = a11y_tree(&mut ed);
    let id = title_field(&tree);
    let state = tree.state();
    let focus = state.focus().expect("a focused node");
    assert_eq!(focus.id(), id, "the title field has the accessible focus");
    assert_eq!(focus.value().as_deref(), Some("Minutes"));
    let selected = focus.text_selection().map(|r| r.text());
    assert_eq!(selected.as_deref(), Some("Minutes"), "its selection is exposed");

    key(&mut ed, NamedKey::Tab, Modifiers::NONE);
    let tree = a11y_tree(&mut ed);
    let focus = tree.state().focus().expect("a focused node");
    assert_eq!(focus.role(), accesskit::Role::MultilineTextInput, "Tab: the body has the focus");

    key(&mut ed, NamedKey::Tab, Modifiers::SHIFT);
    let tree = a11y_tree(&mut ed);
    let id = title_field(&tree);
    assert_eq!(tree.state().focus().map(|n| n.id()), Some(id), "Shift+Tab: back on the title field");
}

// T6: "announced with its value" — live: the updates the editor sends while
// the title is typed give the same value as a fresh snapshot.
#[test]
fn t6_typing_in_the_title_updates_the_accessible_value() {
    let (mut ed, f) = on_title("ab");
    let mut tree = accesskit_consumer::Tree::new(ed.accessibility_tree(), true);
    f.a11y_updates.borrow_mut().clear();
    ed.set_title_selection(2, 2);
    type_text(&mut ed, "cd");
    press(&mut ed, NamedKey::ArrowLeft);
    paint(&mut ed);
    let updates: Vec<_> = f.a11y_updates.borrow_mut().drain(..).collect();
    assert!(!updates.is_empty(), "the editor pushed updates");
    for u in updates {
        tree.update_and_process_changes(u, &mut NoopHandler);
    }
    let id = title_field(&tree);
    let state = tree.state();
    let live = state.node_by_id(id).unwrap();
    assert_eq!(live.value().as_deref(), Some("abcd"));
    let caret = live.text_selection_focus().map(|p| p.to_global_usv_index());
    assert_eq!(caret, Some(3), "the caret is exposed");
}

// T6: "the same editing rules as the editor's text (caret …)" — the caret
// moves by grapheme cluster in the title, and is never left inside one.
#[test]
fn t6_caret_moves_by_grapheme_cluster_in_the_title() {
    let title = "<👩\u{200d}👩\u{200d}👧\u{200d}👦e\u{301}🇫🇷1\u{fe0f}\u{20e3}>";
    let expected = grapheme_boundaries(title);
    let (mut ed, _f) = on_title(title);
    ed.set_title_selection(0, 0);
    let mut seen = vec![ed.title_selection().1];
    for _ in 1..expected.len() {
        press(&mut ed, NamedKey::ArrowRight);
        seen.push(ed.title_selection().1);
    }
    assert_eq!(seen, expected, "Right visits every cluster boundary");
    for off in (0..=title.len()).filter(|o| title.is_char_boundary(*o)) {
        ed.set_title_selection(off, off);
        let c = ed.title_selection().1;
        assert!(expected.contains(&c), "caret {off} -> {c} is inside a cluster");
    }
    // Backspace removes at most one cluster and leaves the caret on a boundary.
    let end = title.len() - 1; // before '>'
    ed.set_title_selection(end, end);
    press(&mut ed, NamedKey::Backspace);
    let after = ed.title();
    assert!(after.len() < title.len() && after.len() >= title.len() - "1\u{fe0f}\u{20e3}".len(), "{after:?}");
    assert!(is_grapheme_boundary(&after, ed.title_selection().1));
    assert_eq!(body(&ed), BODY, "the body is untouched");
}

// T6: "… (selection …)" — Shift+arrows extend a selection in the title; Ctrl+A
// in the title selects the title only.
#[test]
fn t6_selection_and_select_all_stay_in_the_title() {
    let (mut ed, _f) = on_title("alpha beta");
    ed.set_title_selection(0, 0);
    for _ in 0..5 {
        key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    }
    assert_eq!(ed.title_selection(), (0, 5));
    ctrl(&mut ed, "a");
    assert_eq!(sel_range(&ed), (0, "alpha beta".len()));
    assert_eq!(ed.part(), EditorPart::Title);
    type_text(&mut ed, "z");
    assert_eq!(ed.title(), "z");
    assert_eq!(body(&ed), BODY, "Ctrl+A in the title did not reach the body");
}

// T6: "… (clipboard …)" — copy, cut and paste work in the title, with the same
// privacy flags as a copy from the body.
#[test]
fn t6_copy_cut_paste_in_the_title() {
    let (mut b, fb) = open_t("t", &doc1("alpha"), OpenMode::Edit);
    ctrl(&mut b, "a");
    ctrl(&mut b, "c");
    let body_privacy = last_write(&fb).privacy;

    let (mut ed, f) = on_title("alpha beta");
    ed.set_title_selection(0, 5);
    ctrl(&mut ed, "c");
    assert_eq!(clip_text(&f).as_deref(), Some("alpha"));
    assert_eq!(last_write(&f).privacy, body_privacy, "same privacy flags as the body's copies");
    ctrl(&mut ed, "x");
    assert_eq!(ed.title(), " beta");
    assert_eq!(clip_text(&f).as_deref(), Some("alpha"));
    let len = ed.title().len();
    ed.set_title_selection(len, len);
    ctrl(&mut ed, "v");
    assert_eq!(ed.title(), " betaalpha");
    assert_eq!(body(&ed), BODY);
}

// T6: "… (IME …)" — a composition in the title is shown, not committed, until
// the commit; it is committed once into the title; a cancel leaves the title
// as it was; the candidate window is placed at the title.
#[test]
fn t6_input_methods_compose_in_the_title() {
    let (mut ed, f) = on_title("");
    paint(&mut ed);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "にほん");
    assert_eq!(ed.title(), "", "a pre-edit is not committed text");
    paint(&mut ed);
    let area = *f.ime_areas.borrow().last().expect("a caret area is reported while composing in the title");
    ime(&mut ed, ImeEvent::Commit("日本".into()));
    ime(&mut ed, ImeEvent::Disabled);
    assert_eq!(ed.title(), "日本", "committed once");
    assert_eq!(body(&ed), BODY);

    caret_at(&mut ed, 1, 0);
    let b = ed.caret_rect().expect("body caret");
    assert!(area.y1 <= b.y0 + 0.5, "the candidate window sits at the title ({area:?}), above the body ({b:?})");

    let (mut ed, _f) = on_title("keep");
    ed.set_title_selection(4, 4);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "한");
    ime(&mut ed, ImeEvent::Preedit { text: String::new(), cursor: None });
    ime(&mut ed, ImeEvent::Disabled);
    assert_eq!(ed.title(), "keep", "a cancelled composition leaves the title as it was");
}

/// Presses `k` until the title's caret stops moving; the caret x after each
/// press (the start included).
fn walk_title(ed: &mut Editor, text: &str, k: NamedKey) -> Vec<f64> {
    let mut xs = vec![ed.caret_rect().expect("caret").x0];
    for _ in 0..(4 * text.len()) {
        let before = (ed.title_selection().1, xs[xs.len() - 1]);
        press(ed, k);
        assert!(is_grapheme_boundary(text, ed.title_selection().1), "caret inside a cluster");
        let x = ed.caret_rect().expect("caret").x0;
        if ed.title_selection().1 == before.0 && (x - before.1).abs() < 0.01 {
            break;
        }
        xs.push(x);
    }
    xs
}

fn distinct(xs: &[f64]) -> usize {
    let mut v: Vec<f64> = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    v.len()
}

// T6: "… (bidi)" — as in the body (C-TEXT 5, decided 2026-10-10: arrows move
// in visual order): walking a mixed-direction title with Right from its
// visual left end, the caret never goes back and visits every visual place;
// a selection is a logical range.
#[test]
fn t6_bidi_in_the_title() {
    let text = "abc אבגד efg";
    let clusters = text.graphemes(true).count();
    let (mut ed, f) = on_title(text);
    paint(&mut ed);
    let mut left_end = 0;
    let mut min_x = f64::INFINITY;
    for b in grapheme_boundaries(text) {
        ed.set_title_selection(b, b);
        let x = ed.caret_rect().expect("caret").x0;
        if x < min_x {
            min_x = x;
            left_end = b;
        }
    }
    ed.set_title_selection(left_end, left_end);
    let xs = walk_title(&mut ed, text, NamedKey::ArrowRight);
    assert!(xs.windows(2).all(|w| w[1] >= w[0] - 0.01), "Right went back ({xs:?})");
    assert_eq!(distinct(&xs), clusters + 1, "Right skipped or invented a visual place");

    let start = text.find('ב').unwrap();
    ed.set_title_selection(start, start);
    for _ in 0..4 {
        key(&mut ed, NamedKey::ArrowLeft, Modifiers::SHIFT);
    }
    let (a, b) = sel_range(&ed);
    assert_eq!(ed.title_selection().0, start, "the anchor stays");
    if a < b {
        ctrl(&mut ed, "c");
        assert_eq!(clip_text(&f).as_deref(), Some(&text[a..b]), "the copy is the logical range");
    }
}

// =================================================================== T7

fn read_only_editors(title: &str) -> Vec<(&'static str, Editor, Fakes)> {
    let mut out = vec![];
    let (ed, f) = open_t(title, &doc1(BODY), OpenMode::ReadOnly);
    out.push(("opened read-only", ed, f));
    let newer = format!(r#"{{"format":"sovereign.doc","v":99,"blocks":[{}]}}"#, text_block(1, BODY));
    let (ed, f) = open_t(title, &newer, OpenMode::Edit);
    assert!(ed.is_read_only(), "a newer format opens read-only");
    out.push(("newer format (cannot be read)", ed, f));
    let (mut ed, f) = open_t(title, &doc1(BODY), OpenMode::Edit);
    ed.editing_moved_to("the phone");
    out.push(("editing moved to another device", ed, f));
    out
}

// T7: "A document the window opens read-only … shows the title as text, not
// as a field": no text input (read-only or not) other than the body carries
// the title, and the title is still shown, by a node that is not the body.
// (The body may keep the title as its accessible name.)
#[test]
fn t7_a_read_only_document_shows_its_title_as_text() {
    for (why, mut ed, _f) in read_only_editors("Signed contract") {
        let tree = a11y_tree(&mut ed);
        let state = tree.state();
        let mut shown = false;
        for id in all_ids(&tree) {
            let n = state.node_by_id(id).unwrap();
            if n.role() == accesskit::Role::MultilineTextInput {
                continue; // the body
            }
            let names_it = n.label().as_deref() == Some("Title");
            let holds_it = n.value().as_deref().is_some_and(|v| v.contains("Signed contract"))
                || n.label().as_deref().is_some_and(|l| l.contains("Signed contract"));
            if names_it || holds_it {
                assert!(!n.is_text_input(), "{why}: the title is a field ({:?}), not text", n.role());
            }
            shown |= holds_it;
        }
        assert!(shown, "{why}: the title is shown as text");
    }
}

// T7: "Read-only stays read-only": no input changes the title, nothing is
// saved, and input does not disappear without a notice (C-TEXT, decided
// 2026-10-10).
#[test]
fn t7_no_input_changes_a_read_only_title() {
    for (why, mut ed, f) in read_only_editors("Signed contract") {
        ed.focus_title();
        let _ = ed.take_notices();
        set_clip_text(&f, "pasted");
        type_text(&mut ed, "typed");
        ctrl(&mut ed, "v");
        press(&mut ed, NamedKey::Backspace);
        ime(&mut ed, ImeEvent::Enabled);
        ime(&mut ed, ImeEvent::Commit("日".into()));
        ime(&mut ed, ImeEvent::Disabled);
        assert_eq!(ed.title(), "Signed contract", "{why}");
        assert!(
            f.saves.borrow().iter().all(|r| r.title.is_none() || r.title.as_deref() == Some("Signed contract")),
            "{why}: no title saved"
        );
        assert!(!ed.take_notices().is_empty(), "{why}: input does not disappear without a notice");
    }
}
