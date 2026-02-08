//! Spike 1: Skia GPU <-> GTK4 Canvas (Wayland/EGL)
//!
//! Validates GPU-accelerated Skia rendering inside a GTK4 GLArea widget.
//! Run under Weston compositor in WSL2 for proper EGL support.
//!
//! Usage:
//!   # Start Weston first, then in a Weston terminal:
//!   GDK_BACKEND=wayland cargo run --release
//!
//! Controls:
//!   Left-click drag  — pan the canvas
//!   Scroll wheel     — zoom (centered on cursor)
//!   Left-click       — select document (prints to stdout)

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, GLArea};

use skia_safe::{
    gpu, Canvas, Color4f, ColorType, Font, Paint, PaintStyle, Path, Point, RRect, Rect,
};
use skia_safe::gpu::SurfaceOrigin;

use std::cell::RefCell;
use std::ffi::{c_void, CString};
use std::rc::Rc;
use std::time::Instant;

// ── FFI for runtime loading of GL proc address ──────────────────────────────

extern "C" {
    fn dlopen(filename: *const i8, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
    fn dlerror() -> *const i8;
}
const RTLD_LAZY: i32 = 1;

type GlGetProcAddr = unsafe extern "C" fn(*const i8) -> *const c_void;

unsafe fn load_gl_proc_address() -> Option<GlGetProcAddr> {
    // Clear previous error
    dlerror();

    // Strategy 1: find epoxy in the current process (GTK4 links against it)
    let handle = dlopen(std::ptr::null(), RTLD_LAZY);
    if !handle.is_null() {
        let sym_name = CString::new("epoxy_get_proc_address").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            eprintln!("[spike-1] Found epoxy_get_proc_address in process");
            return Some(std::mem::transmute(sym));
        }
        eprintln!("[spike-1] epoxy not in process space, trying dlopen...");
    }

    // Strategy 2: dlopen libepoxy explicitly
    for lib in &["libepoxy.so.0", "libepoxy.so"] {
        let lib_name = CString::new(*lib).unwrap();
        dlerror();
        let handle = dlopen(lib_name.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            let err = dlerror();
            let msg = if !err.is_null() {
                std::ffi::CStr::from_ptr(err).to_string_lossy().to_string()
            } else {
                "unknown".to_string()
            };
            eprintln!("[spike-1] dlopen({}) failed: {}", lib, msg);
            continue;
        }
        let sym_name = CString::new("epoxy_get_proc_address").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            eprintln!("[spike-1] Found epoxy_get_proc_address via {}", lib);
            return Some(std::mem::transmute(sym));
        }
        eprintln!("[spike-1] {} loaded but epoxy_get_proc_address not found", lib);
    }

    // Strategy 3: use eglGetProcAddress directly
    let lib_name = CString::new("libEGL.so.1").unwrap();
    dlerror();
    let handle = dlopen(lib_name.as_ptr(), RTLD_LAZY);
    if !handle.is_null() {
        let sym_name = CString::new("eglGetProcAddress").unwrap();
        let sym = dlsym(handle, sym_name.as_ptr());
        if !sym.is_null() {
            eprintln!("[spike-1] Using eglGetProcAddress as fallback");
            return Some(std::mem::transmute(sym));
        }
    }

    eprintln!("[spike-1] Could not find any GL proc address function");
    None
}

// ── Colors (from sovereign_os_wireframes_1.html) ────────────────────────────

const BG: Color4f = Color4f::new(0.055, 0.055, 0.063, 1.0); // #0e0e10
const OWNED_FILL: Color4f = Color4f::new(0.106, 0.165, 0.227, 1.0); // #1b2a3a
const OWNED_BORDER: Color4f = Color4f::new(0.353, 0.624, 0.831, 1.0); // #5a9fd4
const EXT_FILL: Color4f = Color4f::new(0.227, 0.125, 0.125, 1.0); // #3a2020
const EXT_BORDER: Color4f = Color4f::new(0.878, 0.486, 0.416, 1.0); // #e07c6a
const ACCENT: Color4f = Color4f::new(0.420, 0.639, 0.839, 1.0); // #6ba3d6
const MILESTONE: Color4f = Color4f::new(0.831, 0.659, 0.325, 1.0); // #d4a853
const TEXT_PRIMARY: Color4f = Color4f::new(0.9, 0.9, 0.9, 1.0);
const TEXT_DIM: Color4f = Color4f::new(0.6, 0.6, 0.6, 1.0);
const GRID_LINE: Color4f = Color4f::new(0.15, 0.15, 0.17, 1.0);

// ── Data types ──────────────────────────────────────────────────────────────

struct DocumentCard {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    title: String,
    doc_type: String,
    is_owned: bool,
}

/// 2-D camera: world_pos = screen_pos / zoom + pan
struct Camera {
    pan_x: f64,
    pan_y: f64,
    zoom: f64,
}

impl Camera {
    fn new() -> Self {
        Self {
            pan_x: -200.0,
            pan_y: -100.0,
            zoom: 1.0,
        }
    }

    fn to_matrix(&self) -> skia_safe::Matrix {
        let mut m = skia_safe::Matrix::new_identity();
        m.pre_scale((self.zoom as f32, self.zoom as f32), None);
        m.pre_translate((-self.pan_x as f32, -self.pan_y as f32));
        m
    }

    fn screen_to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        (sx / self.zoom + self.pan_x, sy / self.zoom + self.pan_y)
    }

    fn zoom_at(&mut self, screen_x: f64, screen_y: f64, factor: f64) {
        let old = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.02, 20.0);
        self.pan_x += screen_x * (1.0 / old - 1.0 / self.zoom);
        self.pan_y += screen_y * (1.0 / old - 1.0 / self.zoom);
    }
}

struct AppState {
    camera: Camera,
    docs: Vec<DocumentCard>,
    mouse_x: f64,
    mouse_y: f64,
    hovered: Option<usize>,
    frame_times: Vec<f64>,
    gr_context: Option<gpu::DirectContext>,
}

impl AppState {
    fn new() -> Self {
        Self {
            camera: Camera::new(),
            docs: make_sample_docs(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            hovered: None,
            frame_times: Vec::with_capacity(300),
            gr_context: None,
        }
    }

    fn avg_fps(&self) -> f64 {
        if self.frame_times.len() < 2 {
            return 0.0;
        }
        let n = self.frame_times.len().min(60);
        let slice = &self.frame_times[self.frame_times.len() - n..];
        let avg_ms: f64 = slice.iter().sum::<f64>() / n as f64;
        if avg_ms > 0.0 {
            1000.0 / avg_ms
        } else {
            0.0
        }
    }
}

// ── Sample data ─────────────────────────────────────────────────────────────

fn make_sample_docs() -> Vec<DocumentCard> {
    let owned = [
        ("Research Notes", "markdown"),
        ("Project Plan", "markdown"),
        ("Architecture Diagram", "image"),
        ("API Specification", "markdown"),
        ("Budget Overview", "spreadsheet"),
        ("Meeting Notes Q1", "markdown"),
        ("Design Document", "markdown"),
        ("Test Results", "data"),
    ];
    let external = [
        ("Wikipedia: Rust", "web"),
        ("SO: GTK4 bindings", "web"),
        ("GitHub Issue #42", "web"),
        ("Research Paper (PDF)", "pdf"),
        ("Shared Spec", "markdown"),
        ("API Response Log", "data"),
    ];
    let threads = ["Research", "Development", "Design", "Admin"];

    let mut docs = Vec::new();
    for (ti, _) in threads.iter().enumerate() {
        let row_y = ti as f32 * 200.0;
        for (ci, (title, dtype)) in owned.iter().enumerate() {
            docs.push(DocumentCard {
                x: ci as f32 * 220.0,
                y: row_y + (ci % 2) as f32 * 20.0,
                w: 180.0,
                h: 100.0,
                title: title.to_string(),
                doc_type: dtype.to_string(),
                is_owned: true,
            });
        }
        for (ci, (title, dtype)) in external.iter().enumerate() {
            docs.push(DocumentCard {
                x: ci as f32 * 220.0 + 110.0,
                y: row_y + 120.0,
                w: 180.0,
                h: 100.0,
                title: title.to_string(),
                doc_type: dtype.to_string(),
                is_owned: false,
            });
        }
    }
    docs
}

// ── Rendering ───────────────────────────────────────────────────────────────

fn render(canvas: &Canvas, state: &AppState, w: f32, h: f32) {
    canvas.clear(BG);

    canvas.save();
    canvas.concat(&state.camera.to_matrix());

    draw_grid(canvas, state, w, h);
    draw_thread_labels(canvas);
    for (i, doc) in state.docs.iter().enumerate() {
        draw_card(canvas, doc, state.hovered == Some(i));
    }

    canvas.restore();

    // HUD (screen-space)
    draw_hud(canvas, state, w, h);
}

fn draw_grid(canvas: &Canvas, state: &AppState, screen_w: f32, _screen_h: f32) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    let (wx0, wy0) = state.camera.screen_to_world(0.0, 0.0);
    let (wx1, wy1) = state.camera.screen_to_world(screen_w as f64, 800.0);

    // Vertical grid every 220 px (card column spacing)
    paint.set_color4f(GRID_LINE, None);
    paint.set_stroke_width(1.0);
    paint.set_style(PaintStyle::Stroke);
    let step = 220.0_f32;
    let col0 = (wx0 as f32 / step).floor() as i32;
    let col1 = (wx1 as f32 / step).ceil() as i32;
    for c in col0..=col1 {
        let x = c as f32 * step;
        canvas.draw_line((x, wy0 as f32 - 200.0), (x, wy1 as f32 + 200.0), &paint);
    }

    // "NOW" marker
    let now_x = 4.0 * step;
    paint.set_color4f(ACCENT, None);
    paint.set_stroke_width(2.0);
    canvas.draw_line((now_x, wy0 as f32 - 200.0), (now_x, wy1 as f32 + 200.0), &paint);
    let font = Font::default().with_size(13.0).unwrap();
    paint.set_style(PaintStyle::Fill);
    canvas.draw_str("NOW", (now_x + 5.0, -80.0), &font, &paint);

    // Milestone
    let ms_x = 2.0 * step;
    paint.set_color4f(MILESTONE, None);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(2.0);
    canvas.draw_line((ms_x, wy0 as f32 - 200.0), (ms_x, wy1 as f32 + 200.0), &paint);
    paint.set_style(PaintStyle::Fill);
    canvas.draw_str("v0.1 Release", (ms_x + 5.0, -80.0), &font, &paint);
}

fn draw_thread_labels(canvas: &Canvas) {
    let threads = ["Research", "Development", "Design", "Admin"];
    let font = Font::default().with_size(16.0).unwrap();
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(TEXT_DIM, None);
    for (i, label) in threads.iter().enumerate() {
        canvas.draw_str(label, (-140.0, i as f32 * 200.0 + 55.0), &font, &paint);
    }
}

fn draw_card(canvas: &Canvas, doc: &DocumentCard, hovered: bool) {
    let (fill, border) = if doc.is_owned {
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
    bp.set_color4f(border, None);
    bp.set_style(PaintStyle::Stroke);
    bp.set_stroke_width(if hovered { 3.0 } else { 1.5 });

    if doc.is_owned {
        // Rounded rectangle — sovereignty shape for owned content
        let rect = Rect::from_xywh(doc.x, doc.y, doc.w, doc.h);
        let rr = RRect::new_rect_xy(rect, 8.0, 8.0);
        canvas.draw_rrect(rr, &fp);
        canvas.draw_rrect(rr, &bp);
    } else {
        // Parallelogram — sovereignty shape for external content
        let skew = 14.0_f32;
        let path = Path::polygon(
            &[
                Point::new(doc.x + skew, doc.y),
                Point::new(doc.x + doc.w + skew, doc.y),
                Point::new(doc.x + doc.w - skew, doc.y + doc.h),
                Point::new(doc.x - skew, doc.y + doc.h),
            ],
            true,
            None,
            None,
        );
        canvas.draw_path(&path, &fp);
        canvas.draw_path(&path, &bp);
    }

    // Title
    let title_font = Font::default().with_size(13.0).unwrap();
    let mut tp = Paint::default();
    tp.set_anti_alias(true);
    tp.set_color4f(TEXT_PRIMARY, None);
    let max = (doc.w / 8.0) as usize;
    let label = if doc.title.len() > max {
        format!("{}...", &doc.title[..max.saturating_sub(3)])
    } else {
        doc.title.clone()
    };
    canvas.draw_str(&label, (doc.x + 14.0, doc.y + 26.0), &title_font, &tp);

    // Type badge
    let badge_font = Font::default().with_size(10.0).unwrap();
    tp.set_color4f(TEXT_DIM, None);
    canvas.draw_str(&doc.doc_type, (doc.x + 14.0, doc.y + 44.0), &badge_font, &tp);

    // Sovereignty indicator
    let ind = if doc.is_owned {
        "owned"
    } else {
        "external"
    };
    tp.set_color4f(border, None);
    canvas.draw_str(ind, (doc.x + 14.0, doc.y + doc.h - 10.0), &badge_font, &tp);
}

fn draw_hud(canvas: &Canvas, state: &AppState, w: f32, _h: f32) {
    let font = Font::default().with_size(12.0).unwrap();
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color4f(TEXT_DIM, None);

    let fps = state.avg_fps();
    let zoom_pct = (state.camera.zoom * 100.0) as i32;
    let info = format!(
        "FPS: {:.0}  |  Zoom: {}%  |  Pan: ({:.0}, {:.0})  |  Docs: {}  |  GPU",
        fps,
        zoom_pct,
        state.camera.pan_x,
        state.camera.pan_y,
        state.docs.len()
    );
    canvas.draw_str(&info, (10.0, 20.0), &font, &p);

    // Minimap
    draw_minimap(canvas, state, w);
}

fn draw_minimap(canvas: &Canvas, state: &AppState, screen_w: f32) {
    let mw = 160.0_f32;
    let mh = 100.0_f32;
    let mx = screen_w - mw - 12.0;
    let my = 30.0;

    // Background
    let mut bg = Paint::default();
    bg.set_color4f(Color4f::new(0.08, 0.08, 0.10, 0.85), None);
    bg.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(mx, my, mw, mh), &bg);

    let mut border = Paint::default();
    border.set_color4f(GRID_LINE, None);
    border.set_style(PaintStyle::Stroke);
    border.set_stroke_width(1.0);
    canvas.draw_rect(Rect::from_xywh(mx, my, mw, mh), &border);

    // World bounding box (approximate)
    let (wmin_x, wmax_x) = (-200.0_f32, 2000.0);
    let (wmin_y, wmax_y) = (-200.0_f32, 1000.0);
    let ww = wmax_x - wmin_x;
    let wh = wmax_y - wmin_y;

    // Document dots
    let mut dot = Paint::default();
    dot.set_style(PaintStyle::Fill);
    for d in &state.docs {
        let dx = mx + ((d.x - wmin_x) / ww) * mw;
        let dy = my + ((d.y - wmin_y) / wh) * mh;
        dot.set_color4f(if d.is_owned { OWNED_BORDER } else { EXT_BORDER }, None);
        canvas.draw_circle((dx, dy), 1.5, &dot);
    }

    // Viewport indicator
    let vx = mx + ((state.camera.pan_x as f32 - wmin_x) / ww) * mw;
    let vy = my + ((state.camera.pan_y as f32 - wmin_y) / wh) * mh;
    let vw = (screen_w / state.camera.zoom as f32 / ww) * mw;
    let vh = (700.0 / state.camera.zoom as f32 / wh) * mh;

    let mut vp = Paint::default();
    vp.set_color4f(ACCENT, None);
    vp.set_style(PaintStyle::Stroke);
    vp.set_stroke_width(1.5);
    canvas.draw_rect(Rect::from_xywh(vx, vy, vw, vh), &vp);
}

// ── Hit testing ─────────────────────────────────────────────────────────────

fn hit_test(state: &AppState, sx: f64, sy: f64) -> Option<usize> {
    let (wx, wy) = state.camera.screen_to_world(sx, sy);
    for (i, d) in state.docs.iter().enumerate().rev() {
        if wx >= d.x as f64
            && wx <= (d.x + d.w) as f64
            && wy >= d.y as f64
            && wy <= (d.y + d.h) as f64
        {
            return Some(i);
        }
    }
    None
}

// ── Application ─────────────────────────────────────────────────────────────

fn main() {
    let app = Application::builder()
        .application_id("org.sovereign.spike1")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let state = Rc::new(RefCell::new(AppState::new()));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Spike 1 — Skia + GTK4 Canvas (GPU)")
        .default_width(1280)
        .default_height(720)
        .build();

    let gl_area = GLArea::new();
    gl_area.set_hexpand(true);
    gl_area.set_vexpand(true);
    gl_area.set_has_stencil_buffer(true);

    // ── Realize: init GL + Skia GPU context ─────────────────────────────────
    {
        let state = state.clone();
        gl_area.connect_realize(move |area| {
            area.make_current();
            if let Some(err) = area.error() {
                eprintln!("[spike-1] GLArea error on realize: {}", err);
                return;
            }

            let gl_proc_fn = unsafe { load_gl_proc_address() };

            // Load gl crate + create Skia interface
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
                eprintln!("[spike-1] Trying Interface::new_native() as fallback...");
                gpu::gl::Interface::new_native()
            };

            // Print GL info for debugging
            unsafe {
                let r = gl::GetString(gl::RENDERER);
                let v = gl::GetString(gl::VERSION);
                if !r.is_null() {
                    eprintln!(
                        "[spike-1] GL Renderer: {}",
                        std::ffi::CStr::from_ptr(r as *const i8).to_string_lossy()
                    );
                } else {
                    eprintln!("[spike-1] GL Renderer: (null — no GL context?)");
                }
                if !v.is_null() {
                    eprintln!(
                        "[spike-1] GL Version: {}",
                        std::ffi::CStr::from_ptr(v as *const i8).to_string_lossy()
                    );
                }
            }

            let interface = match interface {
                Some(i) => {
                    eprintln!("[spike-1] Skia GL interface created");
                    i
                }
                None => {
                    eprintln!("[spike-1] Failed to create Skia GL interface");
                    return;
                }
            };

            // Create GPU DirectContext
            match gpu::direct_contexts::make_gl(interface, None) {
                Some(ctx) => {
                    eprintln!("[spike-1] Skia GPU DirectContext created successfully");
                    state.borrow_mut().gr_context = Some(ctx);
                }
                None => {
                    eprintln!("[spike-1] Failed to create Skia DirectContext");
                }
            }
        });
    }

    // ── Render: Skia GPU → GLArea FBO ───────────────────────────────────────
    {
        let state = state.clone();
        gl_area.connect_render(move |area, _gl_ctx| {
            let t0 = Instant::now();
            let w = area.width();
            let h = area.height();
            if w <= 0 || h <= 0 {
                return glib::Propagation::Stop;
            }

            let mut st = state.borrow_mut();

            // Take the context out to avoid borrow conflict with &*st in render()
            let mut ctx = match st.gr_context.take() {
                Some(c) => c,
                None => return glib::Propagation::Stop,
            };

            // Get the FBO that GTK4 GLArea provides
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
                eprintln!("[spike-1] Failed to wrap FBO as Skia surface");
            }

            // Put the context back
            st.gr_context = Some(ctx);

            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            st.frame_times.push(ms);
            if st.frame_times.len() > 300 {
                st.frame_times.drain(0..150);
            }

            glib::Propagation::Stop
        });
    }

    // ── Scroll → zoom ───────────────────────────────────────────────────────
    {
        let state = state.clone();
        let area = gl_area.clone();
        let ctrl = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::VERTICAL,
        );
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

    // ── Drag → pan ──────────────────────────────────────────────────────────
    {
        let state = state.clone();
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

    // ── Motion → hover + cursor tracking ────────────────────────────────────
    {
        let state = state.clone();
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

    // ── Click → select ──────────────────────────────────────────────────────
    {
        let state = state.clone();
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.connect_released(move |_, _n, x, y| {
            let st = state.borrow();
            if let Some(i) = hit_test(&st, x, y) {
                let d = &st.docs[i];
                println!(
                    "[spike-1] Selected: \"{}\" ({}) [{}]",
                    d.title,
                    d.doc_type,
                    if d.is_owned { "owned" } else { "external" }
                );
            }
        });
        gl_area.add_controller(click);
    }

    // ── Continuous render via tick callback ──────────────────────────────────
    {
        let a = gl_area.clone();
        gl_area.add_tick_callback(move |_, _| {
            a.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    window.set_child(Some(&gl_area));
    window.present();

    println!("[spike-1] Window opened (GPU mode) — drag to pan, scroll to zoom");
    println!("[spike-1] {} documents on canvas", state.borrow().docs.len());
}
