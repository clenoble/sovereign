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
use skia_safe::{gpu, Canvas, ColorType, Font, Paint, PaintStyle, Path, Point, RRect, Rect};

use sovereign_core::interfaces::{SkillEvent, Viewport};

use crate::colors::*;
use crate::gl_loader;
use crate::layout::{CardLayout, LANE_HEADER_WIDTH};
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

    draw_lane_backgrounds(canvas, state, w);
    draw_lane_headers(canvas, state);
    for (i, card) in state.layout.cards.iter().enumerate() {
        let hovered = state.hovered == Some(i);
        let selected = state.selected == Some(i);
        let highlighted = state.highlighted.contains(&card.doc_id);
        draw_card(canvas, card, hovered, selected, highlighted);
    }

    canvas.restore();

    // HUD (screen-space)
    draw_hud(canvas, state, w, h);
}

fn draw_lane_backgrounds(canvas: &Canvas, state: &CanvasState, screen_w: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(PaintStyle::Fill);

    // Alternating subtle tint for lanes
    let (_, _) = state.camera.screen_to_world(screen_w as f64, 0.0);

    for (i, lane) in state.layout.lanes.iter().enumerate() {
        if i % 2 == 1 {
            paint.set_color4f(LANE_HEADER_BG, None);
            let world_left = state.camera.pan_x as f32 - 200.0;
            let world_right = world_left + screen_w / state.camera.zoom as f32 + 400.0;
            canvas.draw_rect(
                Rect::from_xywh(world_left, lane.y, world_right - world_left, lane.height),
                &paint,
            );
        }
    }
}

fn draw_lane_headers(canvas: &Canvas, state: &CanvasState) {
    let font = Font::default()
        .with_size(14.0)
        .unwrap_or_else(|| Font::default());
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(TEXT_DIM, None);

    for lane in &state.layout.lanes {
        // Header background
        let mut bg = Paint::default();
        bg.set_color4f(LANE_HEADER_BG, None);
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
