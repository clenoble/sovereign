use skia_safe::{Canvas, Contains, Paint, PaintStyle, Rect};

use crate::colors::*;
use crate::state::CanvasState;

const MINIMAP_W: f32 = 200.0;
const MINIMAP_H: f32 = 120.0;
const MINIMAP_MARGIN: f32 = 12.0;

/// Compute the screen-space bounding box of the minimap overlay (top-right).
pub fn minimap_rect(screen_w: f32, screen_h: f32) -> Rect {
    let _ = screen_h; // kept in signature for API compat
    Rect::from_xywh(
        screen_w - MINIMAP_W - MINIMAP_MARGIN,
        MINIMAP_MARGIN,
        MINIMAP_W,
        MINIMAP_H,
    )
}

/// Draw the minimap overlay in screen-space. Call after HUD (no camera transform).
pub fn draw_minimap(canvas: &Canvas, state: &CanvasState, screen_w: f32, screen_h: f32) {
    if state.layout.cards.is_empty() {
        return;
    }

    let mr = minimap_rect(screen_w, screen_h);

    // Background
    let mut bg = Paint::default();
    bg.set_anti_alias(true);
    bg.set_color4f(LANE_HEADER_BG, None);
    bg.set_style(PaintStyle::Fill);
    canvas.draw_rect(mr, &bg);

    // Border
    let mut border = Paint::default();
    border.set_anti_alias(true);
    border.set_color4f(TEXT_DIM, None);
    border.set_style(PaintStyle::Stroke);
    border.set_stroke_width(1.5);
    canvas.draw_rect(mr, &border);

    // Compute world bounding box of all cards
    let (world_bbox, _) = world_bounding_box(state);

    if world_bbox.width() <= 0.0 || world_bbox.height() <= 0.0 {
        return;
    }

    // Scale factor: world → minimap
    let padding = 8.0;
    let inner_w = MINIMAP_W - padding * 2.0;
    let inner_h = MINIMAP_H - padding * 2.0;
    let sx = inner_w / world_bbox.width();
    let sy = inner_h / world_bbox.height();
    let scale = sx.min(sy);

    let offset_x = mr.left + padding;
    let offset_y = mr.top + padding;

    // Draw each card as a tiny rect
    let mut owned_paint = Paint::default();
    owned_paint.set_anti_alias(true);
    owned_paint.set_color4f(OWNED_BORDER, None);
    owned_paint.set_style(PaintStyle::Fill);

    let mut ext_paint = Paint::default();
    ext_paint.set_anti_alias(true);
    ext_paint.set_color4f(EXT_BORDER, None);
    ext_paint.set_style(PaintStyle::Fill);

    for card in &state.layout.cards {
        if !state.filter.matches(card) {
            continue;
        }
        let rx = offset_x + (card.x - world_bbox.left) * scale;
        let ry = offset_y + (card.y - world_bbox.top) * scale;
        let rw = (card.w * scale).max(2.0);
        let rh = (card.h * scale).max(1.5);
        let paint = if card.is_owned {
            &owned_paint
        } else {
            &ext_paint
        };
        canvas.draw_rect(Rect::from_xywh(rx, ry, rw, rh), paint);
    }

    // Viewport rectangle
    let vp_left = state.camera.pan_x as f32;
    let vp_top = state.camera.pan_y as f32;
    let vp_w = state.viewport_width as f32 / state.camera.zoom as f32;
    let vp_h = state.viewport_height as f32 / state.camera.zoom as f32;

    let vx = offset_x + (vp_left - world_bbox.left) * scale;
    let vy = offset_y + (vp_top - world_bbox.top) * scale;
    let vw = vp_w * scale;
    let vh = vp_h * scale;

    // Clamp viewport rect to minimap bounds
    let clamp_x = vx.max(mr.left).min(mr.right - 4.0);
    let clamp_y = vy.max(mr.top).min(mr.bottom - 4.0);
    let clamp_w = vw.min(mr.right - clamp_x).max(4.0);
    let clamp_h = vh.min(mr.bottom - clamp_y).max(4.0);

    let mut vp_paint = Paint::default();
    vp_paint.set_anti_alias(true);
    vp_paint.set_color4f(TEXT_PRIMARY, None);
    vp_paint.set_style(PaintStyle::Stroke);
    vp_paint.set_stroke_width(1.0);
    canvas.draw_rect(Rect::from_xywh(clamp_x, clamp_y, clamp_w, clamp_h), &vp_paint);
}

/// Test whether a screen-space click falls inside the minimap.
/// If so, returns the world coordinates the click maps to.
pub fn minimap_hit_test(
    screen_x: f32,
    screen_y: f32,
    screen_w: f32,
    screen_h: f32,
    state: &CanvasState,
) -> Option<(f64, f64)> {
    let mr = minimap_rect(screen_w, screen_h);
    if !mr.contains(skia_safe::Point::new(screen_x, screen_y)) {
        return None;
    }

    if state.layout.cards.is_empty() {
        return None;
    }

    let (world_bbox, _) = world_bounding_box(state);
    if world_bbox.width() <= 0.0 || world_bbox.height() <= 0.0 {
        return None;
    }

    let padding = 8.0;
    let inner_w = MINIMAP_W - padding * 2.0;
    let inner_h = MINIMAP_H - padding * 2.0;
    let sx = inner_w / world_bbox.width();
    let sy = inner_h / world_bbox.height();
    let scale = sx.min(sy);

    let local_x = screen_x - mr.left - padding;
    let local_y = screen_y - mr.top - padding;

    let world_x = (local_x / scale + world_bbox.left) as f64;
    let world_y = (local_y / scale + world_bbox.top) as f64;

    Some((world_x, world_y))
}

/// Compute the world-space bounding box of all cards.
fn world_bounding_box(state: &CanvasState) -> (Rect, bool) {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for card in &state.layout.cards {
        min_x = min_x.min(card.x);
        min_y = min_y.min(card.y);
        max_x = max_x.max(card.x + card.w);
        max_y = max_y.max(card.y + card.h);
    }

    let valid = min_x.is_finite() && min_y.is_finite();
    let bbox = Rect::from_ltrb(min_x - 20.0, min_y - 20.0, max_x + 20.0, max_y + 20.0);
    (bbox, valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{CanvasLayout, CardLayout, LaneLayout};

    fn test_state_with_cards() -> CanvasState {
        let layout = CanvasLayout {
            cards: vec![
                CardLayout {
                    doc_id: "d1".into(),
                    title: "A".into(),
                    is_owned: true,
                    thread_id: "t1".into(),
                    created_at_ts: 0,
                    x: 100.0,
                    y: 30.0,
                    w: 200.0,
                    h: 80.0,
                },
                CardLayout {
                    doc_id: "d2".into(),
                    title: "B".into(),
                    is_owned: false,
                    thread_id: "t1".into(),
                    created_at_ts: 0,
                    x: 400.0,
                    y: 30.0,
                    w: 200.0,
                    h: 80.0,
                },
            ],
            lanes: vec![LaneLayout {
                thread_id: "t1".into(),
                thread_name: "Test".into(),
                y: 0.0,
                height: 110.0,
            }],
            timeline_markers: vec![],
            branch_edges: vec![],
        };
        CanvasState::new(layout)
    }

    #[test]
    fn minimap_rect_position() {
        let r = minimap_rect(1280.0, 720.0);
        assert!(r.right <= 1280.0);
        // Top-right: top edge is MINIMAP_MARGIN
        assert_eq!(r.top, MINIMAP_MARGIN);
        assert_eq!(r.width(), MINIMAP_W);
        assert_eq!(r.height(), MINIMAP_H);
    }

    #[test]
    fn hit_test_outside_minimap_returns_none() {
        let state = test_state_with_cards();
        let result = minimap_hit_test(10.0, 10.0, 1280.0, 720.0, &state);
        assert!(result.is_none());
    }

    #[test]
    fn hit_test_inside_minimap_returns_world_coords() {
        let state = test_state_with_cards();
        let r = minimap_rect(1280.0, 720.0);
        let cx = r.left + r.width() / 2.0;
        let cy = r.top + r.height() / 2.0;
        let result = minimap_hit_test(cx, cy, 1280.0, 720.0, &state);
        assert!(result.is_some());
    }

    #[test]
    fn world_bounding_box_is_valid() {
        let state = test_state_with_cards();
        let (bbox, _valid) = world_bounding_box(&state);
        assert!(bbox.width() > 0.0);
        assert!(bbox.height() > 0.0);
    }
}
