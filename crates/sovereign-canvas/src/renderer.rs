use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::GLArea;

use skia_safe::gpu::SurfaceOrigin;
use skia_safe::{gpu, Canvas, Color4f, ColorType, Font, Paint, PaintStyle, Path, Point, RRect, Rect};

use sovereign_core::interfaces::{SkillEvent, Viewport};

use crate::colors::*;
use crate::gl_loader;
use crate::layout::{CardLayout, LANE_HEADER_WIDTH};
use crate::lod::{zoom_level, ZoomLevel};
use crate::state::{hit_test, CanvasState};

/// Create the GLArea widget with Skia GPU rendering and event controllers.
pub fn create_gl_area(
    state: Rc<RefCell<CanvasState>>,
    viewport: Arc<Mutex<Viewport>>,
    skill_tx: Option<mpsc::Sender<SkillEvent>>,
) -> GLArea {
    let gl_area = GLArea::new();
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);
    gl_area.set_has_stencil_buffer(true);

    // ── Realize: init GL + Skia GPU context ─────────────────────────────
    {
        let state = state.clone();
        gl_area.connect_realize(move |area| {
            area.make_current();
            if let Some(err) = area.error() {
                tracing::error!("GLArea error on realize: {}", err);
                return;
            }

            let gl_proc_fn = unsafe { gl_loader::load_gl_proc_address() };

            let interface = if let Some(proc_fn) = gl_proc_fn {
                gl::load_with(|s| {
                    let c = CString::new(s).unwrap();
                    unsafe { proc_fn(c.as_ptr()) as *const _ }
                });

                gpu::gl::Interface::new_load_with(|name| {
                    let c = CString::new(name).unwrap();
                    unsafe { proc_fn(c.as_ptr()) as *const _ }
                })
            } else {
                tracing::warn!("Trying Interface::new_native() as fallback");
                gpu::gl::Interface::new_native()
            };

            let interface = match interface {
                Some(i) => {
                    tracing::debug!("Skia GL interface created");
                    i
                }
                None => {
                    tracing::error!("Failed to create Skia GL interface");
                    return;
                }
            };

            match gpu::direct_contexts::make_gl(interface, None) {
                Some(ctx) => {
                    tracing::debug!("Skia GPU DirectContext created");
                    state.borrow_mut().gr_context = Some(ctx);
                }
                None => {
                    tracing::error!("Failed to create Skia DirectContext");
                }
            }
        });
    }

    // ── Render: Skia GPU → GLArea FBO ───────────────────────────────────
    {
        let state = state.clone();
        let vp = viewport.clone();
        gl_area.connect_render(move |area, _gl_ctx| {
            let t0 = Instant::now();
            let w = area.width();
            let h = area.height();
            if w <= 0 || h <= 0 {
                return glib::Propagation::Stop;
            }

            let mut st = state.borrow_mut();
            st.viewport_width = w as f64;
            st.viewport_height = h as f64;

            // Update shared viewport for CanvasController
            if let Ok(mut vp) = vp.lock() {
                vp.x = st.camera.pan_x;
                vp.y = st.camera.pan_y;
                vp.zoom = st.camera.zoom;
                vp.width = w as f64;
                vp.height = h as f64;
            }

            // Take context out to avoid RefCell borrow conflict in render()
            let mut ctx = match st.gr_context.take() {
                Some(c) => c,
                None => return glib::Propagation::Stop,
            };

            let mut fbo: i32 = 0;
            unsafe {
                gl::GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut fbo);
            }

            let mut fb_info = gpu::gl::FramebufferInfo::from_fboid(fbo as u32);
            fb_info.format = gl::RGBA8;

            let target = gpu::backend_render_targets::make_gl((w, h), None, 8, fb_info);

            let surface = gpu::surfaces::wrap_backend_render_target(
                &mut ctx,
                &target,
                SurfaceOrigin::BottomLeft,
                ColorType::RGBA8888,
                None,
                None,
            );

            if let Some(mut surface) = surface {
                render(surface.canvas(), &*st, w as f32, h as f32);
                ctx.flush_and_submit();
            } else {
                tracing::error!("Failed to wrap FBO as Skia surface");
            }

            // Put context back
            st.gr_context = Some(ctx);

            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            st.frame_times.push(ms);
            if st.frame_times.len() > 300 {
                st.frame_times.drain(0..150);
            }

            glib::Propagation::Stop
        });
    }

    // ── Event controllers (M4) ──────────────────────────────────────────
    attach_scroll_zoom(&gl_area, state.clone());
    attach_drag_pan(&gl_area, state.clone());
    attach_motion_hover(&gl_area, state.clone());
    attach_click_select(&gl_area, state.clone(), skill_tx);

    // ── Continuous render via tick callback ──────────────────────────────
    {
        let a = gl_area.clone();
        gl_area.add_tick_callback(move |_, _| {
            a.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    gl_area
}

// ── Rendering functions ─────────────────────────────────────────────────────

fn render(canvas: &Canvas, state: &CanvasState, w: f32, h: f32) {
    canvas.clear(BG);

    canvas.save();
    canvas.concat(&state.camera.to_matrix());

    let lod = zoom_level(state.camera.zoom);

    draw_lane_backgrounds(canvas, state, w);
    draw_branch_edges(canvas, state);
    draw_timeline_markers(canvas, state, lod);
    draw_now_marker(canvas, state);
    if lod != ZoomLevel::Dots {
        draw_lane_headers(canvas, state);
    }
    for (i, card) in state.layout.cards.iter().enumerate() {
        if !state.filter.matches(card) {
            continue;
        }
        let hovered = state.hovered == Some(i);
        let selected = state.selected == Some(i);
        let highlighted = state.highlighted.contains(&card.doc_id);
        let anim_t = state
            .adoption_animations
            .get(&card.doc_id)
            .map(|a| a.progress());
        match lod {
            ZoomLevel::Dots => draw_dot(canvas, card),
            ZoomLevel::Simplified => draw_card_simplified(canvas, card, selected, highlighted),
            ZoomLevel::Full => {
                if let Some(t) = anim_t {
                    draw_card_adopting(canvas, card, t);
                } else {
                    draw_card(canvas, card, hovered, selected, highlighted);
                }
            }
        }
    }

    canvas.restore();

    // HUD (screen-space)
    draw_hud(canvas, state, w, h);

    // Minimap (screen-space, after HUD)
    if state.minimap_visible {
        crate::minimap::draw_minimap(canvas, state, w, h);
    }
}

fn draw_lane_backgrounds(canvas: &Canvas, state: &CanvasState, screen_w: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    for (i, lane) in state.layout.lanes.iter().enumerate() {
        paint.set_color4f(LANE_COLORS[i % LANE_COLORS.len()], None);
        let world_left = state.camera.pan_x as f32 - 200.0;
        let world_right = world_left + screen_w / state.camera.zoom as f32 + 400.0;
        canvas.draw_rect(
            Rect::from_xywh(world_left, lane.y, world_right - world_left, lane.height),
            &paint,
        );
    }
}

fn draw_lane_headers(canvas: &Canvas, state: &CanvasState) {
    let font = Font::default()
        .with_size(14.0)
        .unwrap_or_else(|| Font::default());
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(TEXT_DIM, None);

    for (i, lane) in state.layout.lanes.iter().enumerate() {
        // Header background — match the lane's color but more opaque
        let mut bg = Paint::default();
        let lane_color = LANE_COLORS[i % LANE_COLORS.len()];
        bg.set_color4f(
            Color4f::new(lane_color.r, lane_color.g, lane_color.b, 0.75),
            None,
        );
        bg.set_style(PaintStyle::Fill);
        canvas.draw_rect(
            Rect::from_xywh(
                state.camera.pan_x as f32 - 10.0,
                lane.y,
                LANE_HEADER_WIDTH - 10.0,
                lane.height,
            ),
            &bg,
        );

        // Lane name
        canvas.draw_str(
            &lane.thread_name,
            (state.camera.pan_x as f32, lane.y + 20.0),
            &font,
            &paint,
        );
    }
}

fn draw_branch_edges(canvas: &Canvas, state: &CanvasState) {
    if state.layout.branch_edges.is_empty() {
        return;
    }

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(TEXT_DIM, None);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    paint.set_path_effect(skia_safe::PathEffect::dash(&[6.0, 4.0], 0.0));

    for edge in &state.layout.branch_edges {
        let x0 = edge.from_x;
        let y0 = edge.from_y;
        let x1 = edge.from_x + 40.0;
        let y1 = edge.to_y;
        let mid_x = (x0 + x1) / 2.0;

        let mut builder = skia_safe::PathBuilder::new();
        builder.move_to(Point::new(x0, y0));
        builder.cubic_to(
            Point::new(mid_x, y0),
            Point::new(mid_x, y1),
            Point::new(x1, y1),
        );
        let path = builder.snapshot();

        canvas.draw_path(&path, &paint);
    }
}

fn draw_timeline_markers(canvas: &Canvas, state: &CanvasState, lod: ZoomLevel) {
    let total_height = state
        .layout
        .lanes
        .last()
        .map(|l| l.y + l.height)
        .unwrap_or(400.0);

    let label_y = state
        .layout
        .lanes
        .first()
        .map(|l| l.y + 14.0)
        .unwrap_or(12.0);

    let mut line_paint = Paint::default();
    line_paint.set_anti_alias(true);
    line_paint.set_color4f(TIMELINE_LINE, None);
    line_paint.set_style(PaintStyle::Stroke);
    line_paint.set_stroke_width(1.0);

    let mut text_paint = Paint::default();
    text_paint.set_anti_alias(true);
    text_paint.set_color4f(TEXT_DIM, None);

    let font_size = match lod {
        ZoomLevel::Dots => 16.0,
        ZoomLevel::Simplified => 12.0,
        ZoomLevel::Full => 11.0,
    };
    let font = Font::default()
        .with_size(font_size)
        .unwrap_or_else(|| Font::default());

    for marker in &state.layout.timeline_markers {
        // Milestones always render with accent color
        if marker.is_milestone {
            line_paint.set_color4f(ACCENT, None);
            text_paint.set_color4f(ACCENT, None);
        } else {
            line_paint.set_color4f(TIMELINE_LINE, None);
            text_paint.set_color4f(TEXT_DIM, None);
        }

        canvas.draw_line((marker.x, 0.0), (marker.x, total_height), &line_paint);

        // Label detail varies by zoom
        let label = match lod {
            ZoomLevel::Dots => {
                // Year only: extract from "M/YYYY"
                marker.label.rsplit('/').next().unwrap_or(&marker.label).to_string()
            }
            _ => marker.label.clone(),
        };
        canvas.draw_str(&label, (marker.x + 4.0, label_y), &font, &text_paint);
    }
}

fn draw_now_marker(canvas: &Canvas, state: &CanvasState) {
    if state.layout.cards.is_empty() {
        return;
    }
    let max_x = state
        .layout
        .cards
        .iter()
        .map(|c| c.x + c.w)
        .fold(f32::NEG_INFINITY, f32::max);
    let marker_x = max_x + 30.0;

    // Vertical dashed line
    let total_height = state
        .layout
        .lanes
        .last()
        .map(|l| l.y + l.height)
        .unwrap_or(400.0);

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(ACCENT, None);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.5);
    paint.set_path_effect(skia_safe::PathEffect::dash(&[8.0, 6.0], 0.0));

    canvas.draw_line((marker_x, 0.0), (marker_x, total_height), &paint);

    // "Now" label
    let font = Font::default()
        .with_size(13.0)
        .unwrap_or_else(|| Font::default());
    let mut tp = Paint::default();
    tp.set_anti_alias(true);
    tp.set_color4f(ACCENT, None);
    canvas.draw_str("Now", (marker_x - 12.0, -8.0), &font, &tp);
}

fn draw_dot(canvas: &Canvas, card: &CardLayout) {
    let color = if card.is_owned {
        OWNED_BORDER
    } else {
        EXT_BORDER
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(color, None);
    paint.set_style(PaintStyle::Fill);
    let (cx, cy) = card.center();
    canvas.draw_circle((cx, cy), 6.0, &paint);
}

fn draw_card_simplified(
    canvas: &Canvas,
    card: &CardLayout,
    selected: bool,
    highlighted: bool,
) {
    let (fill, border) = if card.is_owned {
        (OWNED_FILL, OWNED_BORDER)
    } else {
        (EXT_FILL, EXT_BORDER)
    };

    let mut fp = Paint::default();
    fp.set_anti_alias(true);
    fp.set_color4f(fill, None);
    fp.set_style(PaintStyle::Fill);

    let mut bp = Paint::default();
    bp.set_anti_alias(true);
    bp.set_style(PaintStyle::Stroke);
    if selected || highlighted {
        bp.set_color4f(ACCENT, None);
        bp.set_stroke_width(2.5);
    } else {
        bp.set_color4f(border, None);
        bp.set_stroke_width(1.5);
    }

    if card.is_owned {
        let rect = Rect::from_xywh(card.x, card.y, card.w, card.h);
        let rr = RRect::new_rect_xy(rect, 8.0, 8.0);
        canvas.draw_rrect(rr, &fp);
        canvas.draw_rrect(rr, &bp);
    } else {
        let skew = 14.0_f32;
        let path = Path::polygon(
            &[
                Point::new(card.x + skew, card.y),
                Point::new(card.x + card.w + skew, card.y),
                Point::new(card.x + card.w - skew, card.y + card.h),
                Point::new(card.x - skew, card.y + card.h),
            ],
            true,
            None,
            None,
        );
        canvas.draw_path(&path, &fp);
        canvas.draw_path(&path, &bp);
    }

    // Title (smaller font for simplified zoom)
    let title_font = Font::default()
        .with_size(11.0)
        .unwrap_or_else(|| Font::default());
    let mut tp = Paint::default();
    tp.set_anti_alias(true);
    tp.set_color4f(TEXT_PRIMARY, None);
    let max = (card.w / 9.0) as usize;
    let label = if card.title.len() > max {
        format!("{}...", &card.title[..max.saturating_sub(3)])
    } else {
        card.title.clone()
    };
    canvas.draw_str(&label, (card.x + 10.0, card.y + 20.0), &title_font, &tp);
}

/// Linearly interpolate between two Color4f values.
pub fn lerp_color(a: Color4f, b: Color4f, t: f32) -> Color4f {
    Color4f::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Draw a card undergoing adoption animation (external→owned).
/// t: 0.0 = fully external, 1.0 = fully owned.
fn draw_card_adopting(canvas: &Canvas, card: &CardLayout, t: f32) {
    let fill = lerp_color(EXT_FILL, OWNED_FILL, t);
    let border = lerp_color(EXT_BORDER, OWNED_BORDER, t);
    let skew = 14.0 * (1.0 - t); // 14→0 as t→1
    let radius = 8.0 * t; // 0→8 as t→1

    let mut fp = Paint::default();
    fp.set_anti_alias(true);
    fp.set_color4f(fill, None);
    fp.set_style(PaintStyle::Fill);

    let mut bp = Paint::default();
    bp.set_anti_alias(true);
    bp.set_color4f(border, None);
    bp.set_style(PaintStyle::Stroke);
    bp.set_stroke_width(2.0);

    if skew < 1.0 {
        // Close enough to owned — draw rounded rect
        let rect = Rect::from_xywh(card.x, card.y, card.w, card.h);
        let rr = RRect::new_rect_xy(rect, radius, radius);
        canvas.draw_rrect(rr, &fp);
        canvas.draw_rrect(rr, &bp);
    } else {
        // Transitioning parallelogram
        let path = Path::polygon(
            &[
                Point::new(card.x + skew, card.y),
                Point::new(card.x + card.w + skew, card.y),
                Point::new(card.x + card.w - skew, card.y + card.h),
                Point::new(card.x - skew, card.y + card.h),
            ],
            true,
            None,
            None,
        );
        canvas.draw_path(&path, &fp);
        canvas.draw_path(&path, &bp);
    }

    // Title
    let title_font = Font::default()
        .with_size(13.0)
        .unwrap_or_else(|| Font::default());
    let mut tp = Paint::default();
    tp.set_anti_alias(true);
    tp.set_color4f(TEXT_PRIMARY, None);
    let max = (card.w / 8.0) as usize;
    let label = if card.title.len() > max {
        format!("{}...", &card.title[..max.saturating_sub(3)])
    } else {
        card.title.clone()
    };
    canvas.draw_str(&label, (card.x + 14.0, card.y + 26.0), &title_font, &tp);
}

fn draw_card(
    canvas: &Canvas,
    card: &CardLayout,
    hovered: bool,
    selected: bool,
    highlighted: bool,
) {
    let (fill, border) = if card.is_owned {
        (OWNED_FILL, OWNED_BORDER)
    } else {
        (EXT_FILL, EXT_BORDER)
    };

    let mut fp = Paint::default();
    fp.set_anti_alias(true);
    fp.set_color4f(fill, None);
    fp.set_style(PaintStyle::Fill);

    let mut bp = Paint::default();
    bp.set_anti_alias(true);
    bp.set_style(PaintStyle::Stroke);

    if selected {
        bp.set_color4f(ACCENT, None);
        bp.set_stroke_width(3.0);
    } else if highlighted {
        bp.set_color4f(ACCENT, None);
        bp.set_stroke_width(2.5);
    } else if hovered {
        bp.set_color4f(border, None);
        bp.set_stroke_width(3.0);
    } else {
        bp.set_color4f(border, None);
        bp.set_stroke_width(1.5);
    }

    if card.is_owned {
        // Rounded rectangle for owned content
        let rect = Rect::from_xywh(card.x, card.y, card.w, card.h);
        let rr = RRect::new_rect_xy(rect, 8.0, 8.0);
        canvas.draw_rrect(rr, &fp);
        canvas.draw_rrect(rr, &bp);
    } else {
        // Parallelogram for external content
        let skew = 14.0_f32;
        let path = Path::polygon(
            &[
                Point::new(card.x + skew, card.y),
                Point::new(card.x + card.w + skew, card.y),
                Point::new(card.x + card.w - skew, card.y + card.h),
                Point::new(card.x - skew, card.y + card.h),
            ],
            true,
            None,
            None,
        );
        canvas.draw_path(&path, &fp);
        canvas.draw_path(&path, &bp);
    }

    // Title
    let title_font = Font::default()
        .with_size(13.0)
        .unwrap_or_else(|| Font::default());
    let mut tp = Paint::default();
    tp.set_anti_alias(true);
    tp.set_color4f(TEXT_PRIMARY, None);
    let max = (card.w / 8.0) as usize;
    let label = if card.title.len() > max {
        format!("{}...", &card.title[..max.saturating_sub(3)])
    } else {
        card.title.clone()
    };
    canvas.draw_str(&label, (card.x + 14.0, card.y + 26.0), &title_font, &tp);

    // Sovereignty indicator
    let badge_font = Font::default()
        .with_size(10.0)
        .unwrap_or_else(|| Font::default());
    let ind = if card.is_owned { "owned" } else { "external" };
    tp.set_color4f(border, None);
    canvas.draw_str(
        ind,
        (card.x + 14.0, card.y + card.h - 10.0),
        &badge_font,
        &tp,
    );
}

fn draw_hud(canvas: &Canvas, state: &CanvasState, w: f32, _h: f32) {
    let font = Font::default()
        .with_size(12.0)
        .unwrap_or_else(|| Font::default());
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color4f(TEXT_DIM, None);

    let fps = state.avg_fps();
    let zoom_pct = (state.camera.zoom * 100.0) as i32;
    let info = format!(
        "FPS: {:.0}  |  Zoom: {}%  |  Docs: {}  |  Threads: {}  |  GPU",
        fps,
        zoom_pct,
        state.layout.cards.len(),
        state.layout.lanes.len(),
    );

    // HUD background
    let mut bg = Paint::default();
    bg.set_color4f(LANE_HEADER_BG, None);
    bg.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, w, 28.0), &bg);

    canvas.draw_str(&info, (10.0, 18.0), &font, &p);
}

// ── Event controllers (Milestone 4) ─────────────────────────────────────────

fn attach_scroll_zoom(gl_area: &GLArea, state: Rc<RefCell<CanvasState>>) {
    let area = gl_area.clone();
    let ctrl =
        gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    ctrl.connect_scroll(move |_, _dx, dy| {
        let factor = if dy < 0.0 { 1.15 } else { 1.0 / 1.15 };
        let mut st = state.borrow_mut();
        let (mx, my) = (st.mouse_x, st.mouse_y);
        st.camera.zoom_at(mx, my, factor);
        drop(st);
        area.queue_draw();
        glib::Propagation::Stop
    });
    gl_area.add_controller(ctrl);
}

fn attach_drag_pan(gl_area: &GLArea, state: Rc<RefCell<CanvasState>>) {
    let a = gl_area.clone();
    let drag = gtk4::GestureDrag::new();
    drag.set_button(1);

    let start_pan = Rc::new(RefCell::new((0.0_f64, 0.0_f64)));

    {
        let state = state.clone();
        let sp = start_pan.clone();
        drag.connect_drag_begin(move |_, _, _| {
            let st = state.borrow();
            *sp.borrow_mut() = (st.camera.pan_x, st.camera.pan_y);
        });
    }
    {
        let state = state.clone();
        let a = a.clone();
        let sp = start_pan.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            let (sx, sy) = *sp.borrow();
            let mut st = state.borrow_mut();
            st.camera.pan_x = sx - dx / st.camera.zoom;
            st.camera.pan_y = sy - dy / st.camera.zoom;
            drop(st);
            a.queue_draw();
        });
    }

    gl_area.add_controller(drag);
}

fn attach_motion_hover(gl_area: &GLArea, state: Rc<RefCell<CanvasState>>) {
    let a = gl_area.clone();
    let motion = gtk4::EventControllerMotion::new();
    motion.connect_motion(move |_, x, y| {
        let mut st = state.borrow_mut();
        st.mouse_x = x;
        st.mouse_y = y;
        let prev = st.hovered;
        st.hovered = hit_test(&st, x, y);
        if st.hovered != prev {
            drop(st);
            a.queue_draw();
        }
    });
    gl_area.add_controller(motion);
}

fn attach_click_select(
    gl_area: &GLArea,
    state: Rc<RefCell<CanvasState>>,
    skill_tx: Option<mpsc::Sender<SkillEvent>>,
) {
    let a = gl_area.clone();
    let click = gtk4::GestureClick::new();
    click.set_button(1);
    click.connect_released(move |_, n, x, y| {
        let mut st = state.borrow_mut();
        let hit = hit_test(&st, x, y);

        if n == 1 {
            // Single click: select
            if st.selected != hit {
                st.selected = hit;
                if let Some(i) = hit {
                    let card = &st.layout.cards[i];
                    tracing::info!(
                        "Selected: \"{}\" [{}]",
                        card.title,
                        if card.is_owned { "owned" } else { "external" }
                    );
                }
                drop(st);
                a.queue_draw();
            }
        } else if n == 2 {
            // Double click: open document
            if let Some(i) = hit {
                let card = &st.layout.cards[i];
                tracing::info!("Opening document: \"{}\"", card.title);
                if let Some(ref tx) = skill_tx {
                    let _ = tx.send(SkillEvent::OpenDocument {
                        doc_id: card.doc_id.clone(),
                    });
                }
            }
        }
    });
    gl_area.add_controller(click);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_color_at_zero() {
        let c = lerp_color(EXT_FILL, OWNED_FILL, 0.0);
        assert!((c.r - EXT_FILL.r).abs() < 0.001);
        assert!((c.g - EXT_FILL.g).abs() < 0.001);
        assert!((c.b - EXT_FILL.b).abs() < 0.001);
    }

    #[test]
    fn lerp_color_at_one() {
        let c = lerp_color(EXT_FILL, OWNED_FILL, 1.0);
        assert!((c.r - OWNED_FILL.r).abs() < 0.001);
        assert!((c.g - OWNED_FILL.g).abs() < 0.001);
        assert!((c.b - OWNED_FILL.b).abs() < 0.001);
    }

    #[test]
    fn lerp_color_at_half() {
        let c = lerp_color(EXT_FILL, OWNED_FILL, 0.5);
        let expected_r = (EXT_FILL.r + OWNED_FILL.r) / 2.0;
        assert!((c.r - expected_r).abs() < 0.001);
    }
}
