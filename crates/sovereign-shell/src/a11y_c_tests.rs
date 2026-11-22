//! v0.0.13, lot C of the shell's accessibility (spec ch. 12, SQ10 « C », with SQ5 and SQ8; the Fable
//! maille, by delegation of Céline (2026-11-14), on 2026-11-19, for the picker's and the menu's
//! particular cases): the bubble-style picker and the right-click menu are in the accessibility tree,
//! and a key does what a click does. The two email forms left this lot the same day (email is switched
//! off in this release; they are built reachable with it).
//!
//! Written before the code, by another author than whoever implements it (WSL, lot C's test author):
//! whoever implements makes these pass **without editing this file**; a test that looks wrong is
//! reported, never worked around.
//!
//! A child module of `app` (declared in `app.rs`), driving a real, headless `App`:
//! - the tree is read through `App::build_a11y_tree` after frames, by `first_run_tests::read_tree`;
//!   a name is what Narrator receives (`crate::a11y_spoken::spoken_name`);
//! - a screen reader's press is `App::a11y_action` with `Action::Click` (or `Action::Focus`);
//! - a key goes through `App::shell_key`, the one key seam of lots A, B and C (its signature is lot
//!   A's test author's), as `window_event` hands it a key;
//! - a click is `App::handle_click` at the centre of the drawn rectangle, from the layout the drawing
//!   uses (`panels::bubble_picker_layout`, `App::ctx_menu_geom`);
//! - opening the picker sets `bubble_picker`, and opening the menu with the mouse is
//!   `App::open_context_menu` at `cursor`, as the right button does.
//!
//! **Readings chosen** (reported with the tests):
//! - « A key does what a click does » is checked on twins: two shells in the same state, one clicked,
//!   one answered by the key or the screen reader, then compared on what the click changes.
//! - The picker is a dialog named by its title as drawn (« Choose your bubble »); its styles are radio
//!   buttons named by the style's word as drawn, the current one checked (`Toggled::True`) and no
//!   other; « Done » is a button.
//! - The menu is a node of role `Menu`; its rows are menu items named as drawn.
//! - « Escape gives the focus back »: the tree's focus is again the node that held it before the menu
//!   opened (a lane's label).
//! - « The focus stays inside »: after any number of Tab or Shift+Tab, the focused node is one of the
//!   menu's rows.
//! - The picker's styles are a group of choices (v2 of these tests, after the Fable maille amended
//!   the picker's first answer on 2026-11-19, by delegation of Céline (2026-11-14): one way for the
//!   setup's choices and the picker): one stop for Tab. Tab enters the group on its **checked**
//!   style, wherever the arrows had left the focus, and the next Tab goes to « Done »; Shift+Tab the
//!   same way back; the arrows move between the styles inside.
//! - « The dialog says its keys in one line »: one node of the tree, while the picker is shown, whose
//!   spoken name says « Tab », the arrows (« arrow », case aside), « Enter », « Space » and « Escape ».

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use accesskit::{Action, ActionRequest, NodeId, Role as AkRole, Toggled, TreeId};
use sovereign_ai::docs_host::testing;
use sovereign_core::profile::BubbleStyle;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::GraphDB;
use sovereign_editor::input::Modifiers;
use vello::kurbo;
use winit::keyboard::{Key, NamedKey};

use super::first_run_tests::{read_tree, Read, H, W};
use super::{App, CtxTarget};

// ======================================================================= shared

fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir().join(format!("sov-a11y-c-tests-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.build_scene(W, H);
    }
}

fn tree(app: &mut App) -> Read {
    frames(app, 2);
    read_tree(app)
}

/// A key as the window hands it, through the one seam. Returns whether the element on top took it.
fn key(app: &mut App, k: Key, text: Option<&str>, shift: bool) -> bool {
    let mods = Modifiers { shift, ..Modifiers::NONE };
    let took = app.shell_key(&k, text, mods);
    frames(app, 2);
    took
}

fn named(app: &mut App, k: NamedKey) -> bool {
    let text = match k {
        NamedKey::Space => Some(" "),
        _ => None,
    };
    key(app, Key::Named(k), text, false)
}

fn shift_tab(app: &mut App) -> bool {
    key(app, Key::Named(NamedKey::Tab), None, true)
}

fn shift_f10(app: &mut App) -> bool {
    key(app, Key::Named(NamedKey::F10), None, true)
}

fn sr(app: &mut App, action: Action, id: NodeId) {
    app.a11y_action(ActionRequest { action, target_tree: TreeId::ROOT, target_node: id, data: None });
    frames(app, 2);
}

fn click_at(app: &mut App, r: kurbo::Rect) {
    let c = r.center();
    app.handle_click(c.x, c.y);
    frames(app, 2);
}

/// The node whose spoken name is `name` and whose role is one of `roles`.
fn node(app: &mut App, roles: &[AkRole], name: &str) -> Option<NodeId> {
    tree(app).all.into_iter().find(|s| roles.contains(&s.role) && s.spoken == name).map(|s| s.id)
}

fn focused_name(app: &mut App) -> String {
    let t = tree(app);
    t.all.iter().find(|s| s.id == t.focus).map(|s| s.spoken.clone()).unwrap_or_default()
}

fn focused_role(app: &mut App) -> Option<AkRole> {
    let t = tree(app);
    t.all.iter().find(|s| s.id == t.focus).map(|s| s.role)
}

// ======================================================================= the picker

const PICKER_TITLE: &str = "Choose your bubble";
const DONE: &str = "Done";
const DIALOG: &[AkRole] = &[AkRole::Dialog, AkRole::AlertDialog];
const RADIO: &[AkRole] = &[AkRole::RadioButton];
const BUTTON: &[AkRole] = &[AkRole::Button, AkRole::DefaultButton];

fn styles() -> Vec<BubbleStyle> {
    BubbleStyle::all().to_vec()
}

/// An unlocked shell with no store and no window, its bubble style `current`.
fn unlocked(current: BubbleStyle) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.test_view = Some((W, H));
    app.wizard = None;
    app.locked = false;
    app.windows.clear();
    app.bubble_style = current;
    frames(&mut app, 2);
    app
}

fn picker_open(current: BubbleStyle) -> App {
    let mut app = unlocked(current);
    app.bubble_picker = true;
    frames(&mut app, 2);
    app
}

/// Whether the style's radio button is checked, read from the raw tree.
fn checked(app: &mut App, style: BubbleStyle) -> Option<Toggled> {
    frames(app, 2);
    let id = node(app, RADIO, style.label())?;
    let up = app.build_a11y_tree();
    up.nodes.iter().find(|(i, _)| *i == id).and_then(|(_, n)| n.toggled())
}

fn picker_controls(app: &mut App) -> Vec<String> {
    let t = tree(app);
    t.all
        .iter()
        .filter(|s| (RADIO.contains(&s.role) && styles().iter().any(|st| st.label() == s.spoken)) || (BUTTON.contains(&s.role) && s.spoken == DONE))
        .map(|s| s.spoken.clone())
        .collect()
}

/// SQ10 C: while the picker is shown it is a dialog named by its title, each style a radio button
/// named as drawn, « Done » a button; closed, none of it is in the tree.
#[test]
fn picker_is_in_the_tree_while_shown_and_only_then() {
    let mut app = unlocked(BubbleStyle::Icon);
    assert!(node(&mut app, DIALOG, PICKER_TITLE).is_none(), "closed, the picker is not in the tree:\n{}", tree(&mut app).dump());

    let mut app = picker_open(BubbleStyle::Icon);
    let dump = tree(&mut app).dump();
    assert!(node(&mut app, DIALOG, PICKER_TITLE).is_some(), "a dialog named « {PICKER_TITLE} »:\n{dump}");
    for st in styles() {
        assert!(node(&mut app, RADIO, st.label()).is_some(), "the style « {} » is a radio button:\n{dump}", st.label());
    }
    assert!(node(&mut app, BUTTON, DONE).is_some(), "« {DONE} » is a button:\n{dump}");

    app.bubble_picker = false;
    frames(&mut app, 2);
    let dump = tree(&mut app).dump();
    assert!(node(&mut app, DIALOG, PICKER_TITLE).is_none(), "closed again, the picker leaves the tree:\n{dump}");
    for st in styles() {
        assert!(node(&mut app, RADIO, st.label()).is_none(), "« {} » left with it:\n{dump}", st.label());
    }
}

/// The picker, as decided (see the header):the current style is checked, and no other.
#[test]
fn picker_the_current_style_is_checked_and_no_other() {
    let mut app = picker_open(BubbleStyle::Rings);
    for st in styles() {
        let want = if st == BubbleStyle::Rings { Some(Toggled::True) } else { Some(Toggled::False) };
        let got = checked(&mut app, st);
        assert!(
            got == want || (st != BubbleStyle::Rings && got.is_none()),
            "« {} »: checked {:?}, expected {:?}",
            st.label(),
            got,
            want
        );
    }
}

/// The picker, as decided (see the header):the focus starts on the current style.
#[test]
fn picker_the_focus_starts_on_the_current_style() {
    let mut app = picker_open(BubbleStyle::Matrix);
    assert_eq!(focused_name(&mut app), "Matrix", "the focus starts on the current style:\n{}", tree(&mut app).dump());
    assert_eq!(focused_role(&mut app), Some(AkRole::RadioButton));
}

/// SQ10 with SQ5 and SQ8: a screen reader's Click on a style does what a click on its cell does: it
/// picks it, and the picker stays open; then it is the one checked.
#[test]
fn picker_a_screen_reader_click_on_a_style_does_what_a_click_does() {
    let (_, cells, _) = crate::panels::bubble_picker_layout(W, H);
    let (cell, _) = *cells.iter().find(|(_, st)| *st == BubbleStyle::Orbit).expect("Orbit has a cell");

    let mut clicked = picker_open(BubbleStyle::Icon);
    click_at(&mut clicked, cell);
    assert_eq!(clicked.bubble_style, BubbleStyle::Orbit, "precondition: a click on its cell picks Orbit");

    let mut heard = picker_open(BubbleStyle::Icon);
    let id = node(&mut heard, RADIO, "Orbit").expect("Orbit is in the tree");
    sr(&mut heard, Action::Click, id);
    assert_eq!(heard.bubble_style, clicked.bubble_style, "the screen reader's Click picks what the click picks");
    assert_eq!(heard.bubble_picker, clicked.bubble_picker, "and leaves the picker as the click does (open)");
    assert!(heard.bubble_picker, "the picker stays open");
    assert_eq!(checked(&mut heard, BubbleStyle::Orbit), Some(Toggled::True), "Orbit is now the one checked");
}

/// SQ10: a screen reader's Click on « Done » closes the picker, as a click does; the style is kept.
#[test]
fn picker_a_screen_reader_click_on_done_closes_it() {
    let (_, _, done) = crate::panels::bubble_picker_layout(W, H);
    let mut clicked = picker_open(BubbleStyle::Wave);
    click_at(&mut clicked, done);
    assert!(!clicked.bubble_picker, "precondition: a click on « Done » closes the picker");

    let mut heard = picker_open(BubbleStyle::Wave);
    let id = node(&mut heard, BUTTON, DONE).expect("« Done » is in the tree");
    sr(&mut heard, Action::Click, id);
    assert!(!heard.bubble_picker, "the screen reader's Click on « Done » closes it");
    assert_eq!(heard.bubble_style, BubbleStyle::Wave, "the style is kept");
}

/// SQ10's group of choices (amending the picker's decision in the header):the styles are one stop for Tab. From the
/// checked style, Tab goes to « Done », and the next Tab comes back into the group on the checked
/// style; Shift+Tab the same way back. The focus never leaves the picker.
#[test]
fn picker_tab_and_shift_tab_go_between_the_group_and_done() {
    let mut app = picker_open(BubbleStyle::Icon);
    let controls = picker_controls(&mut app);
    assert_eq!(controls.len(), styles().len() + 1, "nine styles and « Done » are in the tree (SQ10 C)");
    assert_eq!(focused_name(&mut app), "Icon", "the focus starts on the checked style");

    assert!(named(&mut app, NamedKey::Tab), "the picker takes Tab");
    assert_eq!(focused_name(&mut app), DONE, "Tab leaves the group, to « Done »");
    assert!(named(&mut app, NamedKey::Tab));
    assert_eq!(focused_name(&mut app), "Icon", "the next Tab comes back into the group, on the checked style");

    assert!(shift_tab(&mut app), "the picker takes Shift+Tab");
    assert_eq!(focused_name(&mut app), DONE, "Shift+Tab from the group goes to « Done »");
    assert!(shift_tab(&mut app));
    assert_eq!(focused_name(&mut app), "Icon", "and back into the group, on the checked style");
}

/// SQ10's group of choices: Tab enters the group on its checked style, not where the arrows had left
/// the focus (the arrows move the focus and pick nothing).
#[test]
fn picker_tab_enters_the_group_on_the_checked_style() {
    let mut app = picker_open(BubbleStyle::Blink);
    assert!(named(&mut app, NamedKey::ArrowRight));
    let moved = focused_name(&mut app);
    assert_ne!(moved, "Blink", "precondition: the arrow moved the focus off the checked style");
    assert_eq!(app.bubble_style, BubbleStyle::Blink, "precondition: the arrow picked nothing");

    assert!(named(&mut app, NamedKey::Tab));
    assert_eq!(focused_name(&mut app), DONE, "Tab leaves the group from any style, to « Done »");
    assert!(named(&mut app, NamedKey::Tab));
    assert_eq!(focused_name(&mut app), "Blink", "Tab comes back on the checked style, not on « {moved} »");
}

/// SQ10: the picker says its keys in one line (as SQ4 asks of the prompts).
#[test]
fn picker_a_line_says_its_keys() {
    let mut app = picker_open(BubbleStyle::Icon);
    let t = tree(&mut app);
    let line = t.all.iter().find(|s| {
        let said = s.spoken.to_lowercase();
        ["tab", "arrow", "enter", "space", "escape"].iter().all(|k| said.contains(k))
    });
    assert!(line.is_some(), "one line of the picker says Tab, the arrows, Enter, Space and Escape:\n{}", t.dump());
    app.bubble_picker = false;
    frames(&mut app, 2);
    let t = tree(&mut app);
    assert!(
        !t.all.iter().any(|s| s.spoken.to_lowercase().contains("arrow") && s.spoken.to_lowercase().contains("escape")),
        "closed, the picker's keys line leaves the tree"
    );
}

/// The picker, as decided (see the header):the arrows move the focus from one style to another. From the grid's centre
/// (Blink, in the 3×3 grid), so each arrow has a style to go to without wrapping.
#[test]
fn picker_the_arrows_move_between_styles() {
    for k in [NamedKey::ArrowRight, NamedKey::ArrowDown, NamedKey::ArrowLeft, NamedKey::ArrowUp] {
        let mut app = picker_open(BubbleStyle::Blink);
        assert!(named(&mut app, k.clone()), "the picker takes {k:?}");
        let now = focused_name(&mut app);
        assert_ne!(now, "Blink", "{k:?} moves the focus off the current style");
        assert!(styles().iter().any(|s| s.label() == now), "{k:?} lands on a style, not « {now} »");
        assert_eq!(focused_role(&mut app), Some(AkRole::RadioButton));
        assert_eq!(app.bubble_style, BubbleStyle::Blink, "{k:?} moves the focus only; it picks nothing");
    }
}

/// The picker, as decided (see the header):Enter or Space picks the focused style and leaves the picker open, as a click on
/// it does; Enter no longer closes the picker (SQ8's line for the picker).
#[test]
fn picker_enter_or_space_picks_the_focused_style_and_stays_open() {
    for k in [NamedKey::Enter, NamedKey::Space] {
        let mut app = picker_open(BubbleStyle::Icon);
        assert!(named(&mut app, NamedKey::ArrowRight));
        let target = focused_name(&mut app);
        let style = styles().into_iter().find(|s| s.label() == target).expect("the focus is on a style");
        assert_ne!(style, BubbleStyle::Icon, "precondition: the focus moved off the current style");

        let (_, cells, _) = crate::panels::bubble_picker_layout(W, H);
        let (cell, _) = *cells.iter().find(|(_, st)| *st == style).unwrap();
        let mut clicked = picker_open(BubbleStyle::Icon);
        click_at(&mut clicked, cell);

        assert!(named(&mut app, k.clone()), "the picker takes {k:?}");
        assert_eq!(app.bubble_style, style, "{k:?} picks the focused style « {target} »");
        assert_eq!(app.bubble_style, clicked.bubble_style, "{k:?} does what a click on it does");
        assert!(app.bubble_picker, "{k:?} leaves the picker open, as the click does");
        assert_eq!(checked(&mut app, style), Some(Toggled::True));
    }
    let mut app = picker_open(BubbleStyle::Pulse);
    assert!(named(&mut app, NamedKey::Enter));
    assert!(app.bubble_picker, "Enter on the current style leaves the picker open");
    assert_eq!(app.bubble_style, BubbleStyle::Pulse);
}

/// The picker, as decided (see the header):Escape closes the picker and picks nothing.
#[test]
fn picker_escape_closes_it_and_picks_nothing() {
    let mut app = picker_open(BubbleStyle::Blink);
    assert!(named(&mut app, NamedKey::ArrowRight));
    assert!(named(&mut app, NamedKey::Escape), "the picker takes Escape");
    assert!(!app.bubble_picker, "Escape closes the picker");
    assert_eq!(app.bubble_style, BubbleStyle::Blink, "Escape picks nothing");
    assert!(node(&mut app, DIALOG, PICKER_TITLE).is_none(), "and it leaves the tree");
}

// ======================================================================= the right-click menu

const MENU: &[AkRole] = &[AkRole::Menu];
const ITEM: &[AkRole] = &[AkRole::MenuItem, AkRole::MenuItemCheckBox, AkRole::MenuItemRadio];

/// A shell unlocked on an encrypted in-memory store holding threads « Alpha » and « Beta », a
/// document « Notes » in « Alpha », the workspace loaded.
fn with_lanes(tag: &str) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.test_view = Some((W, H));
    let store: Arc<EncryptedGraphDB> = app.rt.block_on(testing::encrypted_memory_store(tag));
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let mut threads = vec![];
    for (i, name) in ["Alpha", "Beta"].iter().enumerate() {
        let mut t = Thread::new((*name).to_string(), String::new());
        t.created_at = base + chrono::Duration::seconds(i as i64);
        threads.push(app.rt.block_on(store.create_thread(t)).unwrap().id_string().unwrap());
    }
    let d = Document::new("Notes".to_string(), threads[0].clone(), true);
    app.rt.block_on(store.create_document(d)).unwrap();
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.wizard = None;
    app.locked = false;
    app.windows.clear();
    app.load_workspace_now();
    frames(&mut app, 3);
    assert!(app.lane_labels.len() >= 2, "precondition: the two lanes have labels");
    assert!(!app.cards.is_empty(), "precondition: « Notes » is a card");
    // The load converts « Notes » to the new document format and says so in a notice; closed here,
    // as she closes it after reading it, so it does not take the clicks below.
    if app.notice.as_ref().is_some_and(|n| n.said.0 == "Document format") {
        app.notice = None;
        frames(&mut app, 2);
    }
    let notice = app.notice.as_ref().map(|n| n.said.clone());
    assert!(notice.is_none(), "precondition: no notice over the canvas, which would take the clicks: {notice:?}");
    app
}

/// A point on the canvas over lane `lane`, clear of cards, windows and the taskbar.
fn canvas_point(app: &App, lane: usize) -> (f64, f64) {
    let y = app.cam.offset_y + (lane as f64 + 0.5) * crate::canvas::LANE_H * app.cam.zoom;
    let mut x = W * 0.5;
    while x < W - 10.0 && app.card_at(x, y, W, H).is_some() {
        x += 10.0;
    }
    (x, y)
}

/// A point over the first card.
fn card_point(app: &App) -> (f64, f64) {
    let mut y = 0.0;
    while y < H {
        let mut x = 0.0;
        while x < W {
            if app.card_at(x, y, W, H).is_some() && !app.in_taskbar(kurbo::Point::new(x, y), W, H) {
                return (x, y);
            }
            x += 8.0;
        }
        y += 8.0;
    }
    panic!("precondition: a card is on screen");
}

/// The right button at (x, y): `open_context_menu` at the cursor, as the window does.
fn right_click(app: &mut App, at: (f64, f64)) {
    app.cursor = at;
    app.open_context_menu();
    frames(app, 2);
}

fn lane_label_node(app: &mut App, name: &str) -> NodeId {
    let t = tree(app);
    t.all
        .iter()
        .find(|s| s.spoken.contains(name) && s.spoken.to_lowercase().contains("rename"))
        .map(|s| s.id)
        .unwrap_or_else(|| panic!("precondition: lane « {name} » has its label in the tree:\n{}", t.dump()))
}

fn menu_items(app: &mut App) -> Vec<String> {
    tree(app).all.into_iter().filter(|s| ITEM.contains(&s.role)).map(|s| s.spoken).collect()
}

fn menu_lane(app: &App) -> Option<usize> {
    match app.ctx_menu.as_ref().map(|m| &m.target) {
        Some(CtxTarget::Canvas(l)) => Some(*l),
        _ => None,
    }
}

/// The row rectangle drawn for `label`.
fn row_rect(app: &App, label: &str) -> kurbo::Rect {
    let rows = app.ctx_menu_rows();
    let (_, geom) = app.ctx_menu_geom(W, H).expect("the menu is open");
    let i = rows.iter().position(|(l, _, _)| l == label).unwrap_or_else(|| panic!("a row « {label} »"));
    geom[i].0
}

/// SQ10 C: a menu opened by the right button is in the tree while shown, its rows as menu items named
/// as drawn, the focus on its first row; closed, it leaves the tree.
#[test]
fn menu_opened_by_the_mouse_is_in_the_tree_while_shown() {
    let mut app = with_lanes("a11y-c-menu-tree");
    assert!(node(&mut app, MENU, "").is_none() && menu_items(&mut app).is_empty(), "closed, no menu in the tree");

    let at = canvas_point(&app, 0);
    right_click(&mut app, at);
    assert_eq!(menu_lane(&app), Some(0), "precondition: the lane's menu opened on « Alpha »");
    let drawn: Vec<String> = app.ctx_menu_rows().into_iter().map(|(l, _, _)| l).collect();
    let t = tree(&mut app);
    assert!(t.all.iter().any(|s| MENU.contains(&s.role)), "a node of role Menu:\n{}", t.dump());
    assert_eq!(menu_items(&mut app), drawn, "its rows, as menu items named as drawn");
    assert_eq!(focused_name(&mut app), drawn[0], "the focus is on its first row");

    assert!(named(&mut app, NamedKey::Escape), "the menu takes Escape");
    assert!(app.ctx_menu.is_none(), "Escape closes it");
    assert!(menu_items(&mut app).is_empty(), "closed, its rows leave the tree");
}

/// SQ10 C: a card's menu, opened by the right button, is in the tree too, its rows named as drawn;
/// « Skills › » turns it into its submenu, the focus on the submenu's first row.
#[test]
fn menu_a_card_menu_and_its_skills_submenu() {
    let mut app = with_lanes("a11y-c-card-menu");
    let at = card_point(&app);
    right_click(&mut app, at);
    assert!(matches!(app.ctx_menu.as_ref().map(|m| &m.target), Some(CtxTarget::Card(_))), "precondition: a card's menu");
    let drawn: Vec<String> = app.ctx_menu_rows().into_iter().map(|(l, _, _)| l).collect();
    assert_eq!(menu_items(&mut app), drawn, "the card's rows, as menu items named as drawn");
    assert_eq!(focused_name(&mut app), drawn[0]);

    let skills = drawn.iter().position(|l| l.starts_with("Skills")).expect("a « Skills › » row");
    for _ in 0..skills {
        assert!(named(&mut app, NamedKey::ArrowDown));
    }
    assert_eq!(focused_name(&mut app), drawn[skills], "precondition: the focus on « Skills › »");
    assert!(named(&mut app, NamedKey::Enter), "the menu takes Enter");
    let sub: Vec<String> = app.ctx_menu_rows().into_iter().map(|(l, _, _)| l).collect();
    assert_ne!(sub, drawn, "precondition: Enter on « Skills › » turned the menu into its submenu");
    assert!(!sub.is_empty(), "precondition: the submenu has rows (the built-in skills)");
    assert_eq!(menu_items(&mut app), sub, "the submenu's rows, as menu items named as drawn");
    assert_eq!(focused_name(&mut app), sub[0], "the focus on the submenu's first row");
}

/// The right-click menu, as decided (see the header):Shift+F10 and the Menu key open the menu on what holds the focus: a lane's
/// label opens that lane's menu, the focus on its first row.
#[test]
fn menu_shift_f10_and_the_menu_key_open_it_on_the_focused_lane() {
    for how in ["Shift+F10", "the Menu key"] {
        let mut app = with_lanes("a11y-c-open-by-key");
        let beta = lane_label_node(&mut app, "Beta");
        sr(&mut app, Action::Focus, beta);
        assert_eq!(tree(&mut app).focus, beta, "precondition: Beta's label holds the focus");

        let took = if how == "Shift+F10" { shift_f10(&mut app) } else { named(&mut app, NamedKey::ContextMenu) };
        assert!(took, "{how} is taken");
        assert_eq!(menu_lane(&app), Some(1), "{how} opens Beta's lane menu");
        let drawn: Vec<String> = app.ctx_menu_rows().into_iter().map(|(l, _, _)| l).collect();
        assert_eq!(menu_items(&mut app), drawn, "{how}: the menu is in the tree");
        assert_eq!(focused_name(&mut app), drawn[0], "{how}: the focus on its first row");
    }
}

/// The right-click menu, as decided (see the header):Up, Down and Tab move between the rows; Escape closes the menu and gives the focus
/// back to where it was.
#[test]
fn menu_up_down_tab_move_and_escape_gives_the_focus_back() {
    let mut app = with_lanes("a11y-c-walk");
    let alpha = lane_label_node(&mut app, "Alpha");
    sr(&mut app, Action::Focus, alpha);
    assert!(shift_f10(&mut app));
    let drawn: Vec<String> = app.ctx_menu_rows().into_iter().map(|(l, _, _)| l).collect();
    assert!(drawn.len() >= 2, "precondition: Alpha's lane menu has two rows: {drawn:?}");

    assert!(named(&mut app, NamedKey::ArrowDown));
    assert_eq!(focused_name(&mut app), drawn[1], "Down moves to the next row");
    assert!(named(&mut app, NamedKey::ArrowUp));
    assert_eq!(focused_name(&mut app), drawn[0], "Up moves back");
    assert!(named(&mut app, NamedKey::Tab));
    assert_eq!(focused_name(&mut app), drawn[1], "Tab moves to the next row");
    for _ in 0..drawn.len() * 2 {
        assert!(named(&mut app, NamedKey::Tab));
        assert!(drawn.contains(&focused_name(&mut app)), "the focus stays in the menu");
    }

    assert!(named(&mut app, NamedKey::Escape));
    assert!(app.ctx_menu.is_none(), "Escape closes the menu");
    assert_eq!(tree(&mut app).focus, alpha, "and gives the focus back to Alpha's label");
}

/// The right-click menu, as decided (see the header), with SQ5 and SQ8:Enter, Space and a screen reader's Click on a row each do what
/// a click on that row does.
#[test]
fn menu_enter_space_and_a_screen_reader_click_do_what_a_click_does() {
    let mut clicked = with_lanes("a11y-c-row-click");
    let at = canvas_point(&clicked, 0);
    right_click(&mut clicked, at);
    let first = clicked.ctx_menu_rows()[0].0.clone();
    assert_eq!(first, "New lane", "precondition: the lane's menu starts with « New lane »");
    let r = row_rect(&clicked, &first);
    click_at(&mut clicked, r);
    assert!(clicked.ctx_menu.is_none(), "precondition: a click on the row closes the menu");
    assert!(clicked.new_thread_prompt.is_some(), "precondition: a click on « New lane » opens the new-thread prompt");

    for how in ["Enter", "Space", "a screen reader's Click"] {
        let mut app = with_lanes(&format!("a11y-c-row-{}", how.len()));
        let at = canvas_point(&app, 0);
        right_click(&mut app, at);
        assert_eq!(focused_name(&mut app), first, "the focus on the first row, « {first} »");
        match how {
            "Enter" => assert!(named(&mut app, NamedKey::Enter)),
            "Space" => assert!(named(&mut app, NamedKey::Space)),
            _ => {
                let id = node(&mut app, ITEM, &first).expect("the row is in the tree");
                sr(&mut app, Action::Click, id);
            }
        }
        assert_eq!(app.ctx_menu.is_none(), clicked.ctx_menu.is_none(), "{how}: the menu closes as after the click");
        assert_eq!(
            app.new_thread_prompt.is_some(),
            clicked.new_thread_prompt.is_some(),
            "{how}: « New lane » opens the new-thread prompt, as the click does"
        );
    }
}
