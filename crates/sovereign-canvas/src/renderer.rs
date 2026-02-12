use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use iced::mouse::{self, Cursor, ScrollDelta};
use iced::widget::shader::{self, Action};
use iced::wgpu;
use iced::{Event, Rectangle};

use skia_safe::{Canvas, Color4f, ColorType, Font, FontMgr, FontStyle, Paint, PaintStyle, Path, Point, RRect, Rect};
use skia_safe::AlphaType;

use crate::colors::*;
use crate::layout::{CardLayout, LANE_HEADER_WIDTH};
use crate::lod::{zoom_level, ZoomLevel};
use crate::state::CanvasState;

/// WGSL shader for blitting the Skia texture to screen.
const BLIT_SHADER: &str = r#"
struct Uniforms {
    bounds_pos: vec2<f32>,
    bounds_size: vec2<f32>,
    viewport_size: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var canvas_texture: texture_2d<f32>;
@group(0) @binding(2) var canvas_sampler: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VsOut {
    var quad_uv = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(0.0, 1.0),
        vec2(1.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0),
    );
    let uv = quad_uv[vi];
    let pixel = u.bounds_pos + uv * u.bounds_size;
    var clip: vec2<f32>;
    clip.x = pixel.x / u.viewport_size.x * 2.0 - 1.0;
    clip.y = 1.0 - pixel.y / u.viewport_size.y * 2.0;

    var out: VsOut;
    out.pos = vec4(clip, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(canvas_texture, canvas_sampler, in.uv);
}
"#;

/// Uniforms for the blit shader.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    bounds_pos: [f32; 2],
    bounds_size: [f32; 2],
    viewport_size: [f32; 2],
    _pad: [f32; 2],
}

/// The data produced each frame by Skia CPU rendering.
#[derive(Debug)]
pub struct CanvasPrimitive {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// GPU resources for blitting the Skia texture.
pub struct CanvasPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    texture_view: Option<wgpu::TextureView>,
    bind_group: Option<wgpu::BindGroup>,
    texture_size: (u32, u32),
}

impl shader::Primitive for CanvasPrimitive {
    type Pipeline = CanvasPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let vp = viewport.physical_size();
        let w = self.width;
        let h = self.height;

        if w == 0 || h == 0 {
            return;
        }

        // Recreate texture if size changed
        if pipeline.texture_size != (w, h) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("canvas_skia"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("canvas_bind_group"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: pipeline.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });

            pipeline.texture = Some(texture);
            pipeline.texture_view = Some(view);
            pipeline.bind_group = Some(bind_group);
            pipeline.texture_size = (w, h);
        }

        // Upload pixel data
        if let Some(ref texture) = pipeline.texture {
            if self.pixels.len() == (w * h * 4) as usize {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &self.pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * w),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        // Update uniforms
        let uniforms = Uniforms {
            bounds_pos: [bounds.x, bounds.y],
            bounds_size: [bounds.width, bounds.height],
            viewport_size: [vp.width as f32, vp.height as f32],
            _pad: [0.0; 2],
        };
        queue.write_buffer(&pipeline.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let Some(ref bind_group) = pipeline.bind_group else {
            return;
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas_blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

impl shader::Pipeline for CanvasPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas_blit_shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas_blit_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("canvas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas_uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            render_pipeline,
            bind_group_layout,
            sampler,
            uniform_buffer,
            texture: None,
            texture_view: None,
            bind_group: None,
            texture_size: (0, 0),
        }
    }
}

/// Iced Shader Program that renders the canvas via Skia CPU.
#[derive(Clone)]
pub struct CanvasProgram {
    pub state: Arc<Mutex<CanvasState>>,
}

/// Message type for canvas interactions.
#[derive(Debug, Clone)]
pub enum CanvasMessage {
    Scrolled { dy: f32 },
    DragStarted,
    DragUpdate { dx: f32, dy: f32 },
    Clicked { x: f32, y: f32 },
    DoubleClicked { x: f32, y: f32 },
    Hovered { x: f32, y: f32 },
}

impl<Message> shader::Program<Message> for CanvasProgram
where
    Message: 'static,
{
    type State = CanvasWidgetState;
    type Primitive = CanvasPrimitive;

    fn update(
        &self,
        wstate: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        // Only process mouse events when cursor is within the shader bounds.
        let pos = cursor.position_in(bounds)?;
        let (x, y) = (pos.x as f64, pos.y as f64);

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let mut st = self.state.lock().unwrap();
                let old_hovered = st.hovered;
                st.mouse_x = x;
                st.mouse_y = y;
                st.hovered = crate::state::hit_test(&st, x, y);

                // Drag panning
                if let Some((start_x, start_y)) = wstate.drag_start_mouse {
                    let dx = x - start_x;
                    let dy = y - start_y;
                    if !wstate.is_dragging && (dx.abs() > 3.0 || dy.abs() > 3.0) {
                        wstate.is_dragging = true;
                    }
                    if wstate.is_dragging {
                        if let Some((sx, sy)) = wstate.drag_start_pan {
                            st.camera.pan_x = sx - dx / st.camera.zoom;
                            st.camera.pan_y = sy - dy / st.camera.zoom;
                        }
                        st.mark_dirty();
                        return Some(Action::request_redraw().and_capture());
                    }
                }

                if old_hovered != st.hovered {
                    // Hover changed — redraw so mouse_interaction() updates the cursor.
                    // No mark_dirty(): draw() returns cached pixels, no Skia re-render.
                    Some(Action::request_redraw().and_capture())
                } else {
                    Some(Action::capture())
                }
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let st = self.state.lock().unwrap();
                wstate.drag_start_mouse = Some((x, y));
                wstate.drag_start_pan = Some((st.camera.pan_x, st.camera.pan_y));
                wstate.is_dragging = false;
                Some(Action::capture())
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if !wstate.is_dragging {
                    if let Some((cx, cy)) = wstate.drag_start_mouse {
                        let now = Instant::now();
                        let is_double = wstate
                            .last_click_time
                            .map(|t| now.duration_since(t) < Duration::from_millis(400))
                            .unwrap_or(false)
                            && wstate
                                .last_click_pos
                                .map(|(lx, ly)| (cx - lx).abs() < 5.0 && (cy - ly).abs() < 5.0)
                                .unwrap_or(false);

                        let mut st = self.state.lock().unwrap();
                        if is_double {
                            if let Some(i) = crate::state::hit_test(&st, cx, cy) {
                                st.pending_open =
                                    Some(st.layout.cards[i].doc_id.clone());
                            }
                            wstate.last_click_time = None;
                        } else {
                            // Single click — select
                            let hit = crate::state::hit_test(&st, cx, cy);
                            if st.selected != hit {
                                st.selected = hit;
                            }
                            wstate.last_click_time = Some(now);
                            wstate.last_click_pos = Some((cx, cy));
                        }
                        st.mark_dirty();
                    }
                }
                wstate.drag_start_mouse = None;
                wstate.drag_start_pan = None;
                wstate.is_dragging = false;
                Some(Action::request_redraw().and_capture())
            }

            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let dy = match delta {
                    ScrollDelta::Lines { y, .. } => *y,
                    ScrollDelta::Pixels { y, .. } => *y,
                };
                let mut st = self.state.lock().unwrap();
                let factor = if dy < 0.0 { 1.15 } else { 1.0 / 1.15 };
                st.camera.zoom_at(x, y, factor);
                st.mark_dirty();
                Some(Action::request_redraw().and_capture())
            }

            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if cursor.position_in(bounds).is_some() {
            let st = self.state.lock().unwrap();
            if st.hovered.is_some() {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::Grab
            }
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let w = bounds.width.max(1.0) as u32;
        let h = bounds.height.max(1.0) as u32;

        let mut state = self.state.lock().unwrap();

        // Reuse cached pixels if nothing visual changed and size matches.
        if state.cached_gen == state.render_gen
            && state.cached_size == (w, h)
        {
            if let Some(ref pixels) = state.cached_pixels {
                return CanvasPrimitive {
                    pixels: pixels.clone(),
                    width: w,
                    height: h,
                };
            }
        }

        // Full Skia render.
        let info = skia_safe::ImageInfo::new(
            (w as i32, h as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let row_bytes = (w * 4) as usize;
        let mut pixels = vec![0u8; row_bytes * h as usize];

        if let Some(mut surface) =
            skia_safe::surfaces::wrap_pixels(&info, &mut pixels, Some(row_bytes), None)
        {
            render(surface.canvas(), &state, w as f32, h as f32);
        }

        // Store in cache.
        state.cached_pixels = Some(pixels.clone());
        state.cached_size = (w, h);
        state.cached_gen = state.render_gen;

        CanvasPrimitive {
            pixels,
            width: w,
            height: h,
        }
    }
}

/// Widget-local state for the canvas shader (drag, click tracking).
#[derive(Default)]
pub struct CanvasWidgetState {
    drag_start_mouse: Option<(f64, f64)>,
    drag_start_pan: Option<(f64, f64)>,
    is_dragging: bool,
    last_click_time: Option<Instant>,
    last_click_pos: Option<(f64, f64)>,
}

// ── Rendering functions ─────────────────────────────────────────────────────

/// Cached typeface resolved once via FontMgr, reused for every subsequent call.
static TYPEFACE: OnceLock<Option<skia_safe::Typeface>> = OnceLock::new();

/// Create a font at the given size using the cached system typeface.
/// The typeface is resolved once on first call; subsequent calls just wrap it at the requested size.
fn canvas_font(size: f32) -> Font {
    let typeface = TYPEFACE.get_or_init(|| {
        let mgr = FontMgr::default();
        for family in &[
            "DejaVu Sans",
            "Liberation Sans",
            "Noto Sans",
            "Ubuntu",
            "Arial",
            "sans-serif",
        ] {
            if let Some(tf) = mgr.match_family_style(family, FontStyle::normal()) {
                if tf.count_glyphs() > 0 {
                    return Some(tf);
                }
            }
        }
        None
    });
    match typeface {
        Some(tf) => Font::from_typeface(tf, size),
        None => Font::default()
            .with_size(size)
            .unwrap_or_else(|| Font::default()),
    }
}

pub fn render(canvas: &Canvas, state: &CanvasState, w: f32, h: f32) {
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
    let font = canvas_font(14.0);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color4f(TEXT_DIM, None);

    for (i, lane) in state.layout.lanes.iter().enumerate() {
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
    let font = canvas_font(font_size);

    for marker in &state.layout.timeline_markers {
        if marker.is_milestone {
            line_paint.set_color4f(ACCENT, None);
            text_paint.set_color4f(ACCENT, None);
        } else {
            line_paint.set_color4f(TIMELINE_LINE, None);
            text_paint.set_color4f(TEXT_DIM, None);
        }

        canvas.draw_line((marker.x, 0.0), (marker.x, total_height), &line_paint);

        let label = match lod {
            ZoomLevel::Dots => {
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

    let font = canvas_font(13.0);
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

    let title_font = canvas_font(11.0);
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

fn draw_card_adopting(canvas: &Canvas, card: &CardLayout, t: f32) {
    let fill = lerp_color(EXT_FILL, OWNED_FILL, t);
    let border = lerp_color(EXT_BORDER, OWNED_BORDER, t);
    let skew = 14.0 * (1.0 - t);
    let radius = 8.0 * t;

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
        let rect = Rect::from_xywh(card.x, card.y, card.w, card.h);
        let rr = RRect::new_rect_xy(rect, radius, radius);
        canvas.draw_rrect(rr, &fp);
        canvas.draw_rrect(rr, &bp);
    } else {
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

    let title_font = canvas_font(13.0);
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

    let title_font = canvas_font(13.0);
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

    let badge_font = canvas_font(10.0);
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
    let font = canvas_font(12.0);
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color4f(TEXT_DIM, None);

    let fps = state.avg_fps();
    let zoom_pct = (state.camera.zoom * 100.0) as i32;
    let info = format!(
        "FPS: {:.0}  |  Zoom: {}%  |  Docs: {}  |  Threads: {}  |  Skia CPU",
        fps,
        zoom_pct,
        state.layout.cards.len(),
        state.layout.lanes.len(),
    );

    let mut bg = Paint::default();
    bg.set_color4f(LANE_HEADER_BG, None);
    bg.set_style(PaintStyle::Fill);
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, w, 28.0), &bg);

    canvas.draw_str(&info, (10.0, 18.0), &font, &p);
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

    #[test]
    fn canvas_font_renders_text() {
        let w = 200i32;
        let h = 50i32;
        let info = skia_safe::ImageInfo::new(
            (w, h),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = (w as usize) * 4;
        let mut pixels = vec![0u8; row_bytes * h as usize];

        let mut surface =
            skia_safe::surfaces::wrap_pixels(&info, &mut pixels, Some(row_bytes), None)
                .expect("surface creation");

        let canvas = surface.canvas();
        canvas.clear(Color4f::new(0.0, 0.0, 0.0, 1.0));

        let font = canvas_font(16.0);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color4f(Color4f::new(1.0, 1.0, 1.0, 1.0), None);
        paint.set_style(PaintStyle::Fill);

        canvas.draw_str("Hello World", (10.0, 30.0), &font, &paint);
        drop(surface);

        let has_text = pixels.chunks(4).any(|px| px[0] > 0 || px[1] > 0 || px[2] > 0);

        let typeface = font.typeface();
        eprintln!("Font family: {:?}", typeface.family_name());
        eprintln!("Glyph count: {}", typeface.count_glyphs());
        eprintln!("Font size: {}", font.size());
        eprintln!("Has visible text pixels: {}", has_text);

        assert!(has_text, "canvas_font() did not render any visible text!");
    }
}
