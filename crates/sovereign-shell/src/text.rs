//! Text shaping (parley) + glyph drawing into a vello Scene.

use parley::{FontContext, Layout, LayoutContext, PositionedLayoutItem, StyleProperty};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::{Glyph, Scene};

/// parley's brush must be `Default`; we set the real color at draw time, so a
/// plain RGBA array suffices (peniko::Color lacks a Default impl).
pub(crate) type Brush = [u8; 4];

// ---- Text shaping -------------------------------------------------------

pub(crate) struct TextShaper {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
}
impl TextShaper {
    pub(crate) fn new() -> Self {
        Self { font_cx: FontContext::new(), layout_cx: LayoutContext::new() }
    }
    pub(crate) fn shape(&mut self, text: &str, max_width: f32, font_px: f32) -> Layout<Brush> {
        // DOS-001: cap shaped input. parley's break_all_lines over a multi-MB
        // body (attacker-influenced via P2P sync / import / saved web page) would
        // pin the single UI thread (OOM/hang). Truncate at a char boundary; this
        // is the one chokepoint all text passes through, so it also bounds the
        // canvas, inbox, and chat. Normal titles/messages are far under the cap.
        const MAX_SHAPE_BYTES: usize = 256 * 1024;
        let text: &str = if text.len() > MAX_SHAPE_BYTES {
            let mut end = MAX_SHAPE_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            &text[..end]
        } else {
            text
        };
        let mut b = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0, true);
        b.push_default(StyleProperty::FontSize(font_px));
        let mut layout = b.build(text);
        layout.break_all_lines(Some(max_width));
        layout
    }
}

/// Pixel position of a caret at the very end of `layout`, in the layout's own
/// coordinate space: `(x, baseline)`. Matches the rest of the UI, where the
/// caret is pinned to the end of the text (no mid-text cursor yet). Returns
/// `(0.0, font_px)` for empty/blank layouts so the caret still renders.
pub(crate) fn end_caret(layout: &Layout<Brush>, font_px: f32) -> (f64, f64) {
    let mut caret: Option<(f64, f64)> = None;
    for line in layout.lines() {
        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(gr) = item {
                let mut x = gr.offset() as f64;
                for g in gr.glyphs() {
                    x += g.advance as f64;
                }
                caret = Some((x, gr.baseline() as f64));
            }
        }
    }
    caret.unwrap_or((0.0, font_px as f64))
}

pub(crate) fn draw_text(scene: &mut Scene, layout: &Layout<Brush>, transform: Affine, color: Color) {
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else {
                continue;
            };
            let run = gr.run();
            let font = run.font();
            let font_size = run.font_size();
            let coords = run.normalized_coords();
            let mut gx = gr.offset();
            let gy = gr.baseline();
            scene
                .draw_glyphs(font)
                .font_size(font_size)
                .brush(color)
                .transform(transform)
                .normalized_coords(coords)
                .draw(
                    Fill::NonZero,
                    gr.glyphs().map(|g| {
                        let x = gx + g.x;
                        let y = gy - g.y;
                        gx += g.advance;
                        Glyph { id: g.id as u32, x, y }
                    }),
                );
        }
    }
}
