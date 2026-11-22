//! v0.0.12 lanes lot (item 16, C-LANE) — the guarantees whose seams the lot
//! adds. Contract: `doc/plans/V0012-lot-lanes.md`, N3 (the drop highlight),
//! N4 (scrolling), N5 (the status bar), N7 (colours), N8 (focusing an
//! off-screen label). Fixtures: `lanes_tests.rs`.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. **It does not compile until the API below exists** — that is
//! its red, not a fault of the file. The names are the test author's; the
//! bodies are the implementer's:
//!
//! - `App::canvas_wheel(&mut self, lines_x: f64, lines_y: f64, mods:
//!   winit::keyboard::ModifiersState)` — what the `MouseWheel` arm does when
//!   the wheel is over the canvas (no modal, not the taskbar, no window), in
//!   lines (a pixel delta divided as today), at `self.cursor`. `lines_y > 0`
//!   is the wheel turned up (winit's sign).
//! - `App::canvas_key(&mut self, key: &winit::keyboard::Key)` — the canvas's
//!   keyboard navigation when no window is open (arrows, `=`/`+`, `-`/`_`,
//!   `h`, `f`), as the `KeyboardInput` arm runs it today inline.
//! - `App::canvas_drag(&mut self, dx: f64, dy: f64)` — what the
//!   `CursorMoved` arm does while the canvas is being dragged (logical px,
//!   the pointer's movement; today `offset += (dx, dy)` inline).
//! - `App::view_resized(&mut self)` — what the `Resized` arm does once the
//!   surface is resized, `win_size()` giving the new size.
//! - `App::status_text(&self) -> String` — the status bar's text (after the
//!   notice), which `draw_status_bar` draws.
//! - `App::drop_target_thread(&self, screen_y: f64) -> Option<String>` — the
//!   thread a card dropped at `screen_y` moves to (`None`: the drop does
//!   nothing, and no drop highlight shows).
//! - `canvas::lane_hues(thread_ids: &[String]) -> Vec<usize>` — the hue (an
//!   index into the 12 hues) of each lane, for the live threads in lane order.
//! - `App::a11y_action(&mut self, request: accesskit::ActionRequest)` — an
//!   accessibility action from the adapter (the shell's actions are not wired
//!   today, `A11yAction::do_action` is empty).
//!
//! Each test calls `build_scene` after the input, as the event loop redraws
//! after every event: the stop (N4) may be applied by the input or by the
//! frame, the test does not mind which.
//!
//! What a winit event alone reaches (the routing of the wheel, a key, a
//! resize) is checked by reading `app.rs`, as the shell's other suites do.

use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::lanes_tests::{assert_stop, canvas_band, labelled, lane_y, lanes, show_lane, tname, L, H, W};
use super::App;
use crate::canvas::{lane_hues, FALLBACK_LANE_NAME, LANE_H};

fn frame(app: &mut App) {
    let (w, h) = app.win_size().expect("the test view");
    app.build_scene(w, h);
}

/// The world point under the screen point `(x, y)`.
fn world_at(app: &App, x: f64, y: f64) -> (f64, f64) {
    ((x - app.cam.offset_x) / app.cam.zoom, (y - app.cam.offset_y) / app.cam.zoom)
}

/// Scrolls (zoom 1) so the last of `n` lanes ends at the canvas bottom.
fn at_bottom(app: &mut App, n: usize) {
    let (_, bottom) = canvas_band(H);
    app.cam.zoom = 1.0;
    app.cam.offset_y = bottom - n as f64 * LANE_H;
}

/// Scrolls (zoom 1) so the first lane starts at the canvas top.
fn at_top(app: &mut App) {
    let (top, _) = canvas_band(H);
    app.cam.zoom = 1.0;
    app.cam.offset_y = top;
}

fn ch(s: &str) -> Key {
    Key::Character(s.into())
}

// ============================================================================ N4

/// N4: "The wheel (and a touchpad's two-finger scroll) scrolls the lanes
/// vertically" — settled direction (2026-10-24): turned down, the view moves
/// down the lanes (later lanes come up); turned up, earlier lanes come down.
/// Time and zoom do not move.
#[test]
fn n4_the_wheel_scrolls_the_lanes_vertically() {
    let mut l = lanes("n4-wheel", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    l.app.cursor = (W * 0.5, H * 0.5);
    let (x0, y0, z0) = (l.app.cam.offset_x, l.app.cam.offset_y, l.app.cam.zoom);
    l.app.canvas_wheel(0.0, -1.0, ModifiersState::empty());
    frame(&mut l.app);
    assert!(l.app.cam.offset_y < y0, "wheel down: the lanes move up ({y0} -> {})", l.app.cam.offset_y);
    assert_eq!((l.app.cam.offset_x, l.app.cam.zoom), (x0, z0), "time and zoom unchanged");
    let y1 = l.app.cam.offset_y;
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::empty());
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::empty());
    frame(&mut l.app);
    assert!(l.app.cam.offset_y > y1, "wheel up: the lanes move down");
    assert_eq!((l.app.cam.offset_x, l.app.cam.zoom), (x0, z0), "time and zoom unchanged");
}

/// N4: a touchpad's two-finger scroll (a pixel delta, divided into lines as
/// today) scrolls the lanes too: the same routine, fractional lines.
#[test]
fn n4_a_fractional_scroll_scrolls_too() {
    let mut l = lanes("n4-touchpad", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let (y0, z0) = (l.app.cam.offset_y, l.app.cam.zoom);
    l.app.canvas_wheel(0.0, -0.4, ModifiersState::empty());
    frame(&mut l.app);
    assert!(l.app.cam.offset_y < y0, "a small downward scroll moves the lanes up");
    assert_eq!(l.app.cam.zoom, z0, "and does not zoom");
}

/// N4: "Shift+wheel scrolls in time (horizontally)" — settled reading
/// (2026-10-24): Shift+wheel turned down moves later in time (to the right),
/// turned up earlier.
#[test]
fn n4_shift_wheel_scrolls_in_time() {
    let mut l = lanes("n4-shift", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let (x0, y0, z0) = (l.app.cam.offset_x, l.app.cam.offset_y, l.app.cam.zoom);
    l.app.canvas_wheel(0.0, -1.0, ModifiersState::SHIFT);
    frame(&mut l.app);
    assert!(l.app.cam.offset_x < x0, "Shift+wheel down shows later time ({x0} -> {})", l.app.cam.offset_x);
    assert_eq!((l.app.cam.offset_y, l.app.cam.zoom), (y0, z0), "lanes and zoom did not move");
    let x1 = l.app.cam.offset_x;
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::SHIFT);
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::SHIFT);
    frame(&mut l.app);
    assert!(l.app.cam.offset_x > x1, "Shift+wheel up shows earlier time");
    assert_eq!((l.app.cam.offset_y, l.app.cam.zoom), (y0, z0), "lanes and zoom did not move");
}

/// N4, settled reading: "A horizontal wheel or touchpad delta scrolls in time
/// directly", without a modifier. winit's sign: a positive horizontal delta
/// moves the content right (earlier time comes into view), as a positive
/// vertical delta moves it down.
#[test]
fn n4_a_horizontal_delta_scrolls_in_time() {
    let mut l = lanes("n4-horizontal", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let (x0, y0, z0) = (l.app.cam.offset_x, l.app.cam.offset_y, l.app.cam.zoom);
    l.app.canvas_wheel(-1.0, 0.0, ModifiersState::empty());
    frame(&mut l.app);
    assert!(l.app.cam.offset_x < x0, "a leftward swipe (negative x) shows later time ({x0} -> {})", l.app.cam.offset_x);
    assert_eq!((l.app.cam.offset_y, l.app.cam.zoom), (y0, z0), "lanes and zoom did not move");
    let x1 = l.app.cam.offset_x;
    l.app.canvas_wheel(0.6, 0.0, ModifiersState::empty());
    l.app.canvas_wheel(0.6, 0.0, ModifiersState::empty());
    frame(&mut l.app);
    assert!(l.app.cam.offset_x > x1, "a rightward swipe (positive x) shows earlier time");
    assert_eq!((l.app.cam.offset_y, l.app.cam.zoom), (y0, z0), "lanes and zoom did not move");
}

/// N4, settled reading: "The stop applies to every vertical movement" —
/// dragging the canvas cannot push every lane out of the canvas area, either
/// way; a horizontal drag still moves time.
#[test]
fn n4_a_drag_of_the_canvas_respects_the_stop() {
    let n = 40;
    let mut l = lanes("n4-drag", n);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    for _ in 0..50 {
        l.app.canvas_drag(0.0, 200.0); // pulled down: towards the first lane
    }
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "after dragging down past the first lane");
    for _ in 0..100 {
        l.app.canvas_drag(0.0, -200.0); // pushed up: towards the last lane
    }
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "after dragging up past the last lane");
    let x0 = l.app.cam.offset_x;
    l.app.canvas_drag(120.0, 0.0);
    frame(&mut l.app);
    assert_eq!(l.app.cam.offset_x, x0 + 120.0, "a horizontal drag moves time as today");
}

/// N4, settled reading: the arrow keys cannot push every lane out of the
/// canvas area either.
#[test]
fn n4_the_arrow_keys_respect_the_stop() {
    let n = 40;
    let mut l = lanes("n4-arrows-stop", n);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    for _ in 0..200 {
        l.app.canvas_key(&Key::Named(NamedKey::ArrowUp));
    }
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "after ↑ past the first lane");
    for _ in 0..400 {
        l.app.canvas_key(&Key::Named(NamedKey::ArrowDown));
    }
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "after ↓ past the last lane");
}

/// N4, settled reading: when every lane fits, a vertical drag or a vertical
/// arrow does not move them vertically (as the wheel does not).
#[test]
fn n4_with_every_lane_fitting_a_drag_or_an_arrow_does_not_move_them_vertically() {
    let mut l = lanes("n4-fit-drag", 3);
    l.app.cam.zoom = 1.0;
    l.app.cam.offset_y = 120.0;
    frame(&mut l.app);
    assert_stop(&l.app, 3, H, "setup");
    let y0 = l.app.cam.offset_y;
    for (dx, dy) in [(0.0, 60.0), (0.0, -60.0), (40.0, 300.0)] {
        l.app.canvas_drag(dx, dy);
        frame(&mut l.app);
        assert_eq!(l.app.cam.offset_y, y0, "a drag by ({dx}, {dy}) does not move lanes that all fit");
    }
    for k in [NamedKey::ArrowUp, NamedKey::ArrowDown] {
        l.app.canvas_key(&Key::Named(k));
        frame(&mut l.app);
        assert_eq!(l.app.cam.offset_y, y0, "{k:?} does not move lanes that all fit");
    }
}

/// N4: "Ctrl+wheel zooms about the cursor, as the wheel does today" — the
/// point under the cursor stays under it (away from the stops).
#[test]
fn n4_ctrl_wheel_zooms_about_the_cursor() {
    let mut l = lanes("n4-ctrl", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let cursor = (W * 0.4, H * 0.45);
    l.app.cursor = cursor;
    let z0 = l.app.cam.zoom;
    let under = world_at(&l.app, cursor.0, cursor.1);
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::CONTROL);
    frame(&mut l.app);
    assert!(l.app.cam.zoom > z0, "Ctrl+wheel up zooms in ({z0} -> {})", l.app.cam.zoom);
    let after = world_at(&l.app, cursor.0, cursor.1);
    assert!((after.0 - under.0).abs() < 1e-6 && (after.1 - under.1).abs() < 1e-6, "the point under the cursor stays: {under:?} -> {after:?}");
    let z1 = l.app.cam.zoom;
    l.app.canvas_wheel(0.0, -1.0, ModifiersState::CONTROL);
    frame(&mut l.app);
    assert!(l.app.cam.zoom < z1, "Ctrl+wheel down zooms out");
}

/// N4 [choice]: "Vertical scrolling stops when the first lane reaches the top
/// or the last lane the bottom, so the lanes never all leave the screen."
#[test]
fn n4_scrolling_stops_at_the_first_and_the_last_lane() {
    let n = 40;
    let mut l = lanes("n4-stops", n);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let (top, bottom) = canvas_band(H);
    for _ in 0..200 {
        l.app.canvas_wheel(0.0, 5.0, ModifiersState::empty());
    }
    frame(&mut l.app);
    assert!((l.app.cam.offset_y - top).abs() < 0.5, "wheel up stops with the first lane at the top: {}", l.app.cam.offset_y);
    for _ in 0..200 {
        l.app.canvas_wheel(0.0, -5.0, ModifiersState::empty());
    }
    frame(&mut l.app);
    let last = l.app.cam.offset_y + n as f64 * LANE_H * l.app.cam.zoom;
    assert!((last - bottom).abs() < 0.5, "wheel down stops with the last lane at the bottom: {last} vs {bottom}");
}

/// N4: "When all lanes fit on screen, there is no vertical scroll at all: the
/// wheel does not drift three lanes" (PANDA2 0141).
#[test]
fn n4_no_vertical_scroll_when_all_lanes_fit() {
    let mut l = lanes("n4-fit", 3);
    l.app.cam.zoom = 1.0;
    l.app.cam.offset_y = 120.0; // three lanes, 288 px, in a 694 px canvas
    frame(&mut l.app);
    assert_stop(&l.app, 3, H, "setup");
    let y0 = l.app.cam.offset_y;
    for lines in [-1.0, -3.0, 1.0, 3.0] {
        l.app.canvas_wheel(0.0, lines, ModifiersState::empty());
        frame(&mut l.app);
        assert_eq!(l.app.cam.offset_y, y0, "the wheel ({lines} lines) does not move lanes that all fit");
    }
}

/// N4: "a zoom out at the bottom leaves no empty space below the last lane"
/// (Ctrl+wheel, about a cursor near the top).
#[test]
fn n4_the_stop_holds_after_a_ctrl_wheel_zoom_out_at_the_bottom() {
    let n = 40;
    let mut l = lanes("n4-zoom-out-bottom", n);
    at_bottom(&mut l.app, n);
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "setup");
    l.app.cursor = (W * 0.5, 100.0);
    l.app.canvas_wheel(0.0, -2.0, ModifiersState::CONTROL);
    frame(&mut l.app);
    assert!(l.app.cam.zoom < 1.0, "zoomed out");
    assert_stop(&l.app, n, H, "after Ctrl+wheel out at the bottom");
}

/// N4: "a zoom in does not leave the first lane above the top with no wheel
/// event to correct it": lanes that still fit after the zoom stay whole on
/// screen (the wheel could not bring them back: they all fit).
#[test]
fn n4_the_stop_holds_after_a_ctrl_wheel_zoom_in_while_the_lanes_fit() {
    let mut l = lanes("n4-zoom-in-fit", 3);
    l.app.cam.zoom = 0.5;
    l.app.cam.offset_y = canvas_band(H).0; // first lane at the top
    frame(&mut l.app);
    l.app.cursor = (W * 0.5, H - 120.0); // below the lanes: zooming in pushes them up
    l.app.canvas_wheel(0.0, 1.0, ModifiersState::CONTROL);
    frame(&mut l.app);
    assert!(l.app.cam.zoom > 0.5, "zoomed in");
    assert!(3.0 * LANE_H * l.app.cam.zoom < canvas_band(H).1 - canvas_band(H).0, "setup: the three lanes still fit");
    assert_stop(&l.app, 3, H, "after Ctrl+wheel in while the lanes fit");
}

/// N4: "The stop is applied again after a zoom (… `=`/`-` …)": `-` at the
/// bottom and at the top leaves no empty space.
#[test]
fn n4_the_stop_holds_after_the_zoom_keys() {
    let n = 40;
    let mut l = lanes("n4-keys", n);
    at_bottom(&mut l.app, n);
    frame(&mut l.app);
    l.app.canvas_key(&ch("-"));
    frame(&mut l.app);
    assert!(l.app.cam.zoom < 1.0, "- zooms out");
    assert_stop(&l.app, n, H, "after - at the bottom");

    at_top(&mut l.app);
    frame(&mut l.app);
    l.app.canvas_key(&ch("-"));
    frame(&mut l.app);
    assert_stop(&l.app, n, H, "after - at the top");

    at_top(&mut l.app);
    frame(&mut l.app);
    l.app.canvas_key(&ch("="));
    frame(&mut l.app);
    assert!(l.app.cam.zoom > 1.0, "= zooms in");
    assert_stop(&l.app, n, H, "after = at the top");
}

/// N4: "The stop is applied again after … a window resize": a taller window
/// at the bottom stop leaves no empty space below the last lane; a window
/// where every lane fits shows them all.
#[test]
fn n4_the_stop_holds_after_a_resize() {
    let n = 40;
    let mut l = lanes("n4-resize", n);
    at_bottom(&mut l.app, n);
    frame(&mut l.app);
    for (w, h) in [(W, 1000.0), (W, 600.0)] {
        l.app.test_view = Some((w, h));
        l.app.view_resized();
        l.app.build_scene(w, h);
        assert_stop(&l.app, n, h, &format!("after a resize to {w}x{h}"));
    }
    // Zoomed out so that the 40 lanes fit in a tall window.
    l.app.cam.zoom = 0.2;
    l.app.cam.offset_y = -500.0;
    l.app.test_view = Some((W, 1000.0));
    l.app.view_resized();
    l.app.build_scene(W, 1000.0);
    assert_stop(&l.app, n, 1000.0, "after a resize, every lane fitting");
}

/// N4: "Dragging the canvas and the arrow keys move as today"; "the `=`/`-`
/// zoom keys … are unchanged".
#[test]
fn n4_the_arrow_keys_and_zoom_keys_are_unchanged() {
    let mut l = lanes("n4-arrows", 40);
    show_lane(&mut l.app, 20);
    frame(&mut l.app);
    let (x0, y0) = (l.app.cam.offset_x, l.app.cam.offset_y);
    l.app.canvas_key(&Key::Named(NamedKey::ArrowLeft));
    assert!(l.app.cam.offset_x > x0, "← shows earlier time");
    l.app.canvas_key(&Key::Named(NamedKey::ArrowRight));
    l.app.canvas_key(&Key::Named(NamedKey::ArrowRight));
    assert!(l.app.cam.offset_x < x0, "→ shows later time");
    l.app.canvas_key(&Key::Named(NamedKey::ArrowDown));
    assert!(l.app.cam.offset_y < y0, "↓ shows later lanes");
    let z0 = l.app.cam.zoom;
    l.app.canvas_key(&ch("="));
    assert!(l.app.cam.zoom > z0, "= zooms in");
    l.app.canvas_key(&ch("-"));
    l.app.canvas_key(&ch("-"));
    assert!(l.app.cam.zoom < z0, "- zooms out");
}

/// N4, the routing (a winit event): the wheel over the canvas goes to
/// `canvas_wheel` with the modifiers held, and the canvas branch no longer
/// zooms by itself; the canvas keys go to `canvas_key`; a resize to
/// `view_resized`; a drag of the canvas to `canvas_drag`.
#[test]
fn n4_the_event_loop_routes_the_wheel_keys_and_resize_through_the_seams() {
    let prod = app_production();
    let wheel = arm(&prod, "WindowEvent::MouseWheel");
    assert!(wheel.contains("canvas_wheel("), "the wheel over the canvas goes to canvas_wheel:\n{wheel}");
    assert!(wheel.contains("self.mods"), "with the modifiers held (Shift, Ctrl)");
    assert!(!wheel.contains("zoom_toward("), "the wheel arm no longer zooms on its own (Ctrl+wheel is canvas_wheel's)");
    // The key arm hands the key to `key_pressed` since SQ11d (spec ch. 12, 2026-11-19).
    let keys = arm(&prod, "WindowEvent::KeyboardInput");
    assert!(keys.contains("key_pressed("), "the key arm hands the key to key_pressed");
    let pressed = prod.find("fn key_pressed(").map(|a| &prod[a..]).unwrap_or("");
    let pressed = &pressed[..pressed[1..].find("\n    fn ").map(|e| e + 1).unwrap_or(pressed.len())];
    assert!(pressed.contains("canvas_key("), "the canvas keys go to canvas_key");
    let resized = arm(&prod, "WindowEvent::Resized");
    assert!(resized.contains("view_resized("), "a resize calls view_resized");
    let moved = arm(&prod, "WindowEvent::CursorMoved");
    assert!(moved.contains("canvas_drag("), "a drag of the canvas goes to canvas_drag (the stop applies to it)");
    assert!(
        !moved.contains("self.cam.offset_y +="),
        "the drag no longer moves the lanes around the stop:\n{moved}"
    );
}

// ============================================================================ N3

/// N3: "a dropped card does nothing, and no drop highlight shows" on the
/// Unfiled lane; over a thread's lane the drop target is that thread.
#[test]
fn n3_no_drop_target_and_no_highlight_over_the_unfiled_lane() {
    let mut l = lanes("n3-target", 2);
    let gone = l.app.rt.block_on(super::lanes_tests::make_threads(&l.store, &["Gone".to_string()])).remove(0);
    let docs = l.docs();
    l.app.rt.block_on(docs.create(sovereign_ai::docs_host::new_user_document("Orphan", &gone, "x"))).unwrap();
    l.app.rt.block_on(sovereign_db::traits::GraphDB::soft_delete_thread(l.store.as_ref(), &gone)).unwrap();
    l.app.load_workspace_now();
    let unfiled = l.lane_named(FALLBACK_LANE_NAME);
    show_lane(&mut l.app, unfiled);
    assert_eq!(l.app.drop_target_thread(lane_y(&l.app, unfiled)), None, "no drop target over Unfiled");
    show_lane(&mut l.app, 1);
    assert_eq!(l.app.drop_target_thread(lane_y(&l.app, 1)), Some(l.threads[1].clone()), "over a thread's lane, that thread");

    let prod = app_production();
    let scene = fn_body(&prod, "build_scene");
    assert!(scene.contains("drop_target_thread("), "the drop highlight shows only where a drop does something");
    let drop = fn_body(&prod, "drop_card_on_lane");
    assert!(drop.contains("drop_target_thread("), "and a drop goes where the highlight says");
}

// ============================================================================ N5

/// N5: "The status bar shows the number of live threads" — 103, with an
/// Unfiled lane drawn as well (not a thread).
#[test]
fn n5_the_status_bar_counts_the_live_threads() {
    let mut l = lanes("n5-status", 103);
    let gone = l.app.rt.block_on(super::lanes_tests::make_threads(&l.store, &["Gone".to_string()])).remove(0);
    let docs = l.docs();
    l.app.rt.block_on(docs.create(sovereign_ai::docs_host::new_user_document("Orphan", &gone, "x"))).unwrap();
    l.app.rt.block_on(sovereign_db::traits::GraphDB::soft_delete_thread(l.store.as_ref(), &gone)).unwrap();
    l.app.load_workspace_now();
    let text = l.app.status_text();
    assert!(text.contains("103 threads"), "the status bar counts 103 live threads: {text}");
    let prod = app_production();
    assert!(fn_body(&prod, "draw_status_bar").contains("status_text("), "draw_status_bar draws status_text");
}

// ============================================================================ N7

fn ids(n: usize, tag: &str) -> Vec<String> {
    (0..n).map(|i| format!("thread:{tag}{i:04}x")).collect()
}

/// N7: "A thread's hue is derived from its id (one of the 12 hues)": the
/// same for the same id, whichever list it heads, and one of 12.
#[test]
fn n7_a_threads_hue_is_derived_from_its_id() {
    let all = ids(40, "h");
    let hues = lane_hues(&all);
    assert_eq!(hues.len(), all.len(), "one hue per lane");
    assert!(hues.iter().all(|&h| h < 12), "one of the 12 hues: {hues:?}");
    assert_eq!(lane_hues(&all), hues, "deterministic");
    for id in &all {
        let alone = lane_hues(std::slice::from_ref(id))[0];
        let heading = lane_hues(&[id.clone(), "thread:other".to_string()])[0];
        assert_eq!(alone, heading, "{id}: the first lane's hue is its own, whatever follows");
    }
    let distinct: std::collections::HashSet<usize> = all.iter().map(|id| lane_hues(std::slice::from_ref(id))[0]).collect();
    assert!(distinct.len() >= 6, "40 ids spread over the hues, not one or two: {distinct:?}");
}

/// N7: "so it does not change when another thread is deleted, here or on
/// another device" — deleting thread 0, or thread 17, leaves the hue of every
/// thread whose lane above shows the same thread as before.
///
/// CONTRACT-QUESTION (N7): "When it equals the hue of the lane just above,
/// it moves to the next hue" — read as comparing the thread's own hue with
/// the hue derived from the id of the thread just above (so a thread's hue
/// depends on its id and its upper neighbour's, and neighbours differ "in
/// nearly every case"). Comparing with the hue the lane above is *drawn* in
/// would cascade: deleting any earlier thread could change every hue below
/// it, against "does not change when another thread is deleted". Under this
/// reading the one thread whose hue may change is the one whose upper
/// neighbour changed.
#[test]
fn n7_a_threads_hue_is_stable_when_an_earlier_thread_is_deleted() {
    let all = ids(40, "s");
    let hues = lane_hues(&all);
    for gone in [0usize, 17] {
        let rest: Vec<String> = all.iter().enumerate().filter(|(i, _)| *i != gone).map(|(_, t)| t.clone()).collect();
        let after = lane_hues(&rest);
        for (j, id) in rest.iter().enumerate() {
            let i = all.iter().position(|t| t == id).unwrap();
            let neighbour_changed = i == gone + 1;
            if neighbour_changed {
                continue;
            }
            assert_eq!(after[j], hues[i], "{id}: its hue after deleting thread {gone}");
        }
    }
}

/// N7: "When it equals the hue of the lane just above, it moves to the next
/// hue, so neighbours differ": every pair of threads, in either order, gets
/// two different hues (40 ids give pairs whose own hues are equal).
#[test]
fn n7_a_lane_differs_from_the_lane_above() {
    let all = ids(40, "n");
    for a in &all {
        for b in &all {
            if a == b {
                continue;
            }
            let h = lane_hues(&[a.clone(), b.clone()]);
            assert_ne!(h[0], h[1], "{a} above {b}: neighbours differ");
        }
    }
}

/// N7: "The same hue colours the lane's band, its label, its links and its
/// minimap row" — the shell takes its lanes' colours from `lane_hues`.
#[test]
fn n7_the_shell_colours_lanes_by_lane_hues() {
    let used = super::lanes_tests::shell_production().iter().any(|(_, p)| p.contains("lane_hues("));
    assert!(used, "the shell's drawing reads lane_hues");
}

// ============================================================================ N8

/// N8: "focusing an off-screen lane's label scrolls it into view": the
/// label's button takes the focus action, and focusing lane 100's label (the
/// camera on lane 2) brings lane 100 into the canvas.
#[test]
fn n8_focusing_an_off_screen_lanes_label_scrolls_it_into_view() {
    let mut l: L = lanes("n8-focus", 103);
    show_lane(&mut l.app, 2);
    frame(&mut l.app);
    let want = format!("{}{}", crate::lane_rename::LABEL_BUTTON_PREFIX, tname(100));
    let tree = l.app.build_a11y_tree();
    let (id, _) = labelled(&tree).into_iter().find(|(_, s)| *s == want).expect("lane 100's label is in the tree");
    let node = tree.nodes.iter().find(|(n, _)| *n == id).map(|(_, n)| n.clone()).unwrap();
    assert!(node.supports_action(accesskit::Action::Focus), "a label button can take the focus");
    l.app.a11y_action(accesskit::ActionRequest {
        action: accesskit::Action::Focus,
        target_tree: accesskit::TreeId::ROOT,
        target_node: id,
        data: None,
    });
    frame(&mut l.app);
    let (top, bottom) = canvas_band(H);
    let y0 = l.app.cam.w2s_y(100.0 * LANE_H);
    let y1 = l.app.cam.w2s_y(101.0 * LANE_H);
    assert!(y0 >= top - 0.5 && y1 <= bottom + 0.5, "lane 100 is in view: {y0:.1}..{y1:.1} in {top}..{bottom}");
    let tree = l.app.build_a11y_tree();
    assert_eq!(tree.focus, id, "and its label holds the focus");
}

// ------------------------------------------------------------------ source reading

/// `app.rs` before its trailing test module, `//` comments removed.
fn app_production() -> String {
    super::lanes_tests::shell_production()
        .into_iter()
        .find(|(f, _)| f == "app.rs")
        .expect("app.rs")
        .1
}

/// The body of `fn <name>(` up to the next method.
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let at = src.find(&sig).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &src[at + sig.len()..];
    let end = ["\n    fn ", "\n    pub(crate) fn ", "\n    pub fn "].iter().filter_map(|m| rest.find(m)).min().unwrap_or(rest.len());
    &rest[..end]
}

/// The text of the match arm starting at `pat` (to the next `WindowEvent::` arm).
fn arm<'a>(src: &'a str, pat: &str) -> &'a str {
    let at = src.find(pat).unwrap_or_else(|| panic!("{pat} not found"));
    let rest = &src[at + pat.len()..];
    let end = rest.find("\n            WindowEvent::").unwrap_or(rest.len());
    &rest[..end]
}
