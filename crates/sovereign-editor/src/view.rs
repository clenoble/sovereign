//! Layout of the document for display, caret geometry and painting.
//!
//! Each text block is cut into units: its paragraphs, and a paragraph longer
//! than `text::MAX_UNIT` in pieces. A piece always starts where a line of the
//! whole paragraph starts (found by laying out a window of the paragraph and
//! keeping only the lines the rest of the paragraph cannot change), and
//! carries invisible bidi marks so it resolves as inside the paragraph: the
//! pieces together show exactly the lines of the paragraph laid out whole.
//! A long paragraph is cut lazily, from its start, as far as something needs
//! it; its rest is one *pending* unit with an estimated height.
//!
//! Each unit is laid out by parley on its displayed text (tokens shown as
//! labels, pre-edit shown) only when needed — painted, or touched by the
//! caret. This keeps opening and typing in a large document proportional to
//! what is on screen (guarantee 9).

use std::collections::HashMap;

use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontContext, Layout, LayoutContext, OverflowWrap, PositionedLayoutItem,
    StyleProperty,
};
use vello::kurbo::{Affine, Rect as KRect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::{Glyph, Scene};

use crate::doc::{Block, BlockId, BlockKind, DocumentV2};
use crate::text::{self, DisplayInput, DisplayMap, Resolve, Seg, SegKind, Snap, Strong, UnitSpan};
use crate::md::{self, Ink};

/// The layouts' brush: how each glyph is drawn (`md::Ink`). It never changes
/// shaping, so it never changes line breaks or caret places.
pub(crate) type Brush = Ink;

pub(crate) const PAD: f32 = 12.0;
pub(crate) const FONT_SIZE: f32 = 16.0;
const BLOCK_GAP: f32 = 8.0;
pub(crate) const CARET_W: f32 = 1.5;
/// The "heading level" a document's title field is laid out at (title
/// contract T1, revised 2026-10-17): 1.1 × the body size, semi-bold, as
/// the Tauri desktop's title (1.1rem, weight 600).
pub(crate) const TITLE_LEVEL: u8 = u8::MAX;
pub(crate) const TITLE_SIZE: f32 = FONT_SIZE * 1.1;
pub(crate) const TITLE_WEIGHT: u16 = 600;
/// Rough advance of an average character, for height estimates only.
const EST_ADVANCE: f32 = 8.0;

// ------------------------------------------------------------ shaping

pub(crate) struct Shaper {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
}

impl Shaper {
    pub(crate) fn new(fonts: &crate::Fonts) -> Shaper {
        let font_cx = match fonts {
            crate::Fonts::System => {
                let mut cx = FontContext::new();
                widen_han_fallback(&mut cx.collection);
                cx
            }
            crate::Fonts::Only(files) => {
                use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};
                let mut collection = Collection::new(CollectionOptions { shared: false, system_fonts: false });
                let mut families = vec![];
                for f in files {
                    let blob = Blob::new(std::sync::Arc::new(f.clone()));
                    for (fam, _) in collection.register_fonts(blob, None) {
                        if !families.contains(&fam) {
                            families.push(fam);
                        }
                    }
                }
                for g in [
                    GenericFamily::SansSerif,
                    GenericFamily::Serif,
                    GenericFamily::Monospace,
                    GenericFamily::SystemUi,
                    GenericFamily::UiSansSerif,
                ] {
                    collection.set_generic_families(g, families.iter().copied());
                }
                FontContext { collection, source_cache: SourceCache::default() }
            }
        };
        Shaper { font_cx, layout_cx: LayoutContext::new() }
    }

    #[cfg(test)]
    fn layout(&mut self, text: &str, width: Option<f32>) -> Layout<Brush> {
        self.layout_styled(text, width, 0, &[])
    }

    /// Lays out `text`. `heading`: the paragraph's heading level (0: body
    /// text; [`TITLE_LEVEL`]: a document's title), the only style that
    /// changes metrics; `inks`: how ranges of the displayed text are drawn
    /// (brushes only — no effect on shaping).
    fn layout_styled(&mut self, text: &str, width: Option<f32>, heading: u8, inks: &[(usize, usize, Ink)]) -> Layout<Brush> {
        let mut b = self.layout_cx.ranged_builder(&mut self.font_cx, text, 1.0, true);
        if heading == TITLE_LEVEL {
            b.push_default(StyleProperty::FontSize(TITLE_SIZE));
            b.push_default(StyleProperty::FontWeight(parley::FontWeight::new(TITLE_WEIGHT as f32)));
        } else {
            b.push_default(StyleProperty::FontSize(FONT_SIZE * md::heading_scale(heading)));
            if heading > 0 {
                b.push_default(StyleProperty::FontWeight(parley::FontWeight::BOLD));
            }
        }
        // A word wider than the line wraps instead of running off the edge.
        b.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        for &(a, e, ink) in inks {
            b.push(StyleProperty::Brush(ink), a..e);
        }
        let mut layout = b.build(text);
        layout.break_all_lines(width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }
}

/// Han text falls back, character by character, through every CJK font the
/// system offers — not only the one it picks for Han with no language.
///
/// parley selects a font per character cluster: the style's families first,
/// then the fallback families for the run's script (`Query::set_fallbacks`,
/// to which fontique always adds the Han fallbacks). But the system's
/// fallback for a script is **one** family, found from a sample character
/// (`今` for Hani) and the locale (fontique `backend/dwrite.rs`,
/// `SystemFonts::fallback`); the editor sets no locale, and with none Windows
/// answers with a Japanese font (Yu Gothic UI on Windows 11), which lacks
/// characters used only in Chinese or Cantonese — 佢 (U+4F62) showed as a
/// box (of the fonts Windows offers for Han, Microsoft JhengHei UI, its
/// answer for zh-TW and zh-HK, has it). Here the Hani fallback (no
/// locale) becomes the system's answer for no locale, then its answers for
/// zh-CN, zh-TW, zh-HK, ja and ko, in that order, without duplicates: a
/// character the first font lacks is drawn with the next that has it. The
/// first font still wins where it has the glyph, so the forms of characters
/// common to all are unchanged.
pub(crate) fn widen_han_fallback(collection: &mut parley::fontique::Collection) {
    use parley::fontique::{FallbackKey, Language, Script};
    let hani = Script::from_bytes(*b"Hani");
    let mut ids: Vec<parley::fontique::FamilyId> = collection.fallback_families(FallbackKey::new(hani, None)).collect();
    for loc in ["zh-CN", "zh-TW", "zh-HK", "ja", "ko"] {
        let Ok(lang) = Language::parse(loc) else { continue };
        let more: Vec<_> = collection.fallback_families(FallbackKey::new(hani, Some(&lang))).collect();
        for id in more {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    if !ids.is_empty() {
        collection.set_fallbacks(FallbackKey::new(hani, None), ids.into_iter());
    }
}

// ------------------------------------------------------------ units

pub(crate) struct UnitLayout {
    pub map: DisplayMap,
    pub layout: Layout<Brush>,
    width: Option<f32>,
    preedit: Option<(usize, String)>,
    /// The paragraph's heading level it was laid out with.
    heading: u8,
}

pub(crate) struct UnitView {
    pub span: UnitSpan,
    /// The rest of a long paragraph, not cut into pieces yet.
    pub pending: bool,
    /// For a unit of a long paragraph: the paragraph's direction (right to
    /// left?) and the last strong direction before the unit.
    pub marks: Option<(bool, Option<Strong>)>,
    pub layout: Option<Box<UnitLayout>>,
    pub height: f32,
    /// The unit's accessible text run, once built.
    pub a11y: Option<Box<A11yRun>>,
}

/// A unit's text run in the accessibility tree. A unit whose text changes is
/// a new unit with a new run id, so an update sends only new runs.
pub(crate) struct A11yRun {
    pub id: u64,
    pub map: DisplayMap,
    pub run: crate::a11y::Run,
    /// Whether the run ends with a line break.
    pub nl: bool,
    /// Sent to the accessibility sink already.
    pub sent: bool,
}

pub(crate) struct BlockView {
    rev: u64,
    pub units: Vec<UnitView>,
}

/// What laying out needs besides the document.
pub(crate) struct Env<'a> {
    /// How text in token form is shown: `None` when the host says it is not
    /// a token in this document.
    pub resolve: Resolve<'a>,
    /// Pre-edit shown at (block, stored offset).
    pub preedit: Option<(BlockId, usize, &'a str)>,
    /// Shows bullets instead of text (password field).
    pub mask: bool,
    /// Draws markdown lightly (`md`): document text blocks, not fields.
    pub markdown: bool,
}

/// A caret place on a line: a stored offset, an affinity, and its x.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Place {
    pub offset: usize,
    pub aff: Affinity,
    pub x: f32,
}

/// A visual line: block index, unit index, line index in the unit's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineRef {
    pub block: usize,
    pub unit: usize,
    pub line: usize,
}

pub(crate) struct View {
    shaper: Shaper,
    blocks: HashMap<BlockId, BlockView>,
    /// Text width for line breaking (`None`: no wrapping).
    pub width: Option<f32>,
    pub line_h: f32,
    next_a11y_id: u64,
    /// The heading level text is laid out at where markdown is not drawn: 0
    /// (body text) except for a document's title field.
    pub plain_heading: u8,
}

pub(crate) fn placeholder_text(b: &Block) -> String {
    match b.kind() {
        BlockKind::Image => match b.image_alt() {
            Some(alt) if !alt.is_empty() => format!("Image: {alt}"),
            _ if b.needs_description() => "Image (description to be written)".to_string(),
            _ => "Image".to_string(),
        },
        BlockKind::Video => "Video".to_string(),
        BlockKind::Table => "Table".to_string(),
        _ => "This kind of block is not supported in this version".to_string(),
    }
}

impl View {
    pub(crate) fn new(fonts: &crate::Fonts) -> View {
        View {
            shaper: Shaper::new(fonts),
            blocks: HashMap::new(),
            width: Some(600.0),
            line_h: 20.0,
            next_a11y_id: crate::a11y::RUN_BASE,
            plain_heading: 0,
        }
    }

    pub(crate) fn set_width(&mut self, width: Option<f32>) {
        if self.width != width {
            self.width = width;
            // Pieces of long paragraphs depend on the width: cut again.
            self.blocks.clear();
        }
    }

    /// Drops every accessible run (token display changed).
    pub(crate) fn invalidate_a11y(&mut self) {
        for bv in self.blocks.values_mut() {
            for u in &mut bv.units {
                u.a11y = None;
            }
        }
    }

    pub(crate) fn alloc_a11y_id(&mut self) -> u64 {
        let id = self.next_a11y_id;
        self.next_a11y_id += 1;
        id
    }

    /// The view of a block if it is built and current.
    pub(crate) fn peek(&self, b: &Block) -> Option<&BlockView> {
        self.blocks.get(&b.id()).filter(|v| v.rev == b.rev())
    }

    /// Drops every layout (token display changed: widths change, so pieces of
    /// long paragraphs are cut again).
    pub(crate) fn invalidate_layouts(&mut self) {
        self.blocks.clear();
    }

    /// Forgets views of blocks no longer in the document.
    pub(crate) fn retain(&mut self, doc: &DocumentV2) {
        if self.blocks.len() > doc.blocks().len() {
            let ids: std::collections::HashSet<BlockId> = doc.blocks().iter().map(|b| b.id()).collect();
            self.blocks.retain(|k, _| ids.contains(k));
        }
    }

    fn estimate(&self, len: usize) -> f32 {
        let per_line = match self.width {
            Some(w) => (w / EST_ADVANCE).max(1.0),
            None => f32::MAX,
        };
        let lines = ((len as f32) / per_line).ceil().max(1.0);
        lines * self.line_h
    }

    fn build_block(&self, b: &Block) -> BlockView {
        let mut spans = vec![];
        if b.kind() == BlockKind::Text {
            let t = b.text_str();
            text::split_paragraphs(t, 0, t.len(), &mut spans);
        } else {
            spans.push(UnitSpan { start: 0, end: 0, term: 0 });
        }
        let units = spans.into_iter().map(|span| self_paragraph_unit(self.width, self.line_h, b.text_str(), span, None)).collect();
        BlockView { rev: b.rev(), units }
    }

    /// The block's view, built or rebuilt if the block changed.
    pub(crate) fn block_view(&mut self, doc: &DocumentV2, idx: usize) -> &mut BlockView {
        let b = &doc.blocks()[idx];
        let stale = self.blocks.get(&b.id()).is_none_or(|v| v.rev != b.rev());
        if stale {
            let v = self.build_block(b);
            self.blocks.insert(b.id(), v);
        }
        self.blocks.get_mut(&b.id()).expect("just inserted")
    }

    /// A text block's text changed: `old_len` bytes at `s` became `new_len`
    /// bytes. Updates the units touched, shifts the rest.
    pub(crate) fn edited(&mut self, b: &Block, old_rev: u64, s: usize, old_len: usize, new_len: usize) {
        let est_w = self.width;
        let line_h = self.line_h;
        let id = b.id();
        let Some(bv) = self.blocks.get_mut(&id) else { return };
        if bv.rev != old_rev || bv.units.is_empty() {
            self.blocks.remove(&id);
            return;
        }
        let text = b.text_str();
        let e_old = s + old_len;
        let n = bv.units.len();
        let mut first = bv.units.iter().position(|u| u.span.end + u.span.term >= s).unwrap_or(n - 1);
        // A change at the start of a piece can pull its first word up onto
        // the previous piece's last line: lay out from the previous piece.
        if first > 0 && bv.units[first - 1].span.term == 0 {
            first -= 1;
        }
        let mut last = first;
        while last + 1 < n && bv.units[last + 1].span.start <= e_old {
            last += 1;
        }
        let region_start = bv.units[first].span.start;
        // Marks carry over only into a piece in the middle of a paragraph; a
        // paragraph start finds its direction again.
        let first_marks = if first > 0 && bv.units[first - 1].span.term == 0 { bv.units[first].marks } else { None };
        let old_region_end = bv.units[last].span.end + bv.units[last].span.term;
        let delta = new_len as isize - old_len as isize;
        let region_to = if last + 1 == n { text.len() } else { (old_region_end as isize + delta) as usize };
        let mut spans = vec![];
        text::split_paragraphs(text, region_start, region_to, &mut spans);
        let count = spans.len();
        // When pieces of the same paragraph follow the region, its last part
        // is pending and *bounded*: cutting it checks that the lines after the
        // change start again where the following piece starts, and takes that
        // piece in if they do not (`cut`).
        let continues = last + 1 < n && bv.units[last].span.term == 0;
        let new_units: Vec<UnitView> = spans
            .into_iter()
            .enumerate()
            .map(|(i, span)| {
                let mut uv = self_paragraph_unit(est_w, line_h, text, span, if i == 0 { first_marks } else { None });
                if continues && i + 1 == count {
                    uv.pending = true;
                    if uv.marks.is_none() {
                        let para = text[span.start..].split('\n').next().unwrap_or("");
                        let rtl = matches!(text::first_strong(para), Some(Strong::R | Strong::Al));
                        uv.marks = Some((rtl, None));
                    }
                }
                uv
            })
            .collect();
        bv.units.splice(first..=last, new_units);
        for u in bv.units.iter_mut().skip(first + count) {
            u.span.start = (u.span.start as isize + delta) as usize;
            u.span.end = (u.span.end as isize + delta) as usize;
            // what is cached for the unit keeps its text, at new offsets
            if delta != 0 {
                if let Some(l) = &mut u.layout {
                    l.map.shift(delta);
                    if let Some((p, _)) = &mut l.preedit {
                        *p = (*p as isize + delta) as usize;
                    }
                }
                if let Some(a) = &mut u.a11y {
                    a.map.shift(delta);
                }
            }
        }
        bv.rev = b.rev();
    }

    /// The unit holding a stored offset, cutting a long paragraph as far as
    /// needed so that it is laid out-able.
    pub(crate) fn unit_for(&mut self, env: &mut Env<'_>, doc: &DocumentV2, idx: usize, offset: usize, aff: Affinity) -> usize {
        loop {
            let u = self.unit_index(doc, idx, offset, aff);
            if !self.block_view(doc, idx).units[u].pending {
                return u;
            }
            self.cut(env, doc, idx, u);
        }
    }

    /// Cuts the paragraph of pending unit `u` until it is all pieces; returns
    /// the index of its last piece.
    pub(crate) fn cut_all(&mut self, env: &mut Env<'_>, doc: &DocumentV2, idx: usize, mut u: usize) -> usize {
        while self.block_view(doc, idx).units[u].pending {
            if self.cut(env, doc, idx, u) {
                u += 1;
            }
        }
        u
    }

    /// Cuts the first piece off pending unit `u`: a window of the paragraph
    /// is laid out, and the piece ends at a line start the rest of the
    /// paragraph cannot move — one at most at the start of the last-but-one
    /// line, since the next word after each earlier break lies inside the
    /// window. Returns whether a new pending unit follows.
    ///
    /// A *bounded* pending unit (pieces of its paragraph follow, after an
    /// edit) ends where the next piece starts; when the lines laid out from
    /// its start begin again exactly there, the following pieces still hold
    /// and are kept; when not, the next piece is taken into it and the check
    /// goes on.
    fn cut(&mut self, env: &mut Env<'_>, doc: &DocumentV2, idx: usize, u: usize) -> bool {
        let heading = self.heading_of(env, doc, idx, u);
        let b = doc.blocks()[idx].share();
        let t = b.text_str();
        let is_tok = |id: &str| (env.resolve)(id).is_some();
        let max = self.width.unwrap_or(f32::MAX);
        loop {
            let (span, marks, bounded, para_end) = {
                let bv = self.block_view(doc, idx);
                let uv = &bv.units[u];
                let mut k = u;
                while k + 1 < bv.units.len() && bv.units[k].span.term == 0 {
                    k += 1;
                }
                (uv.span, uv.marks, k > u, bv.units[k].span.end)
            };
            let (rtl, before) = marks.unwrap_or((false, None));
            let (p, end) = (span.start, span.end);
            if !bounded && (self.width.is_none() || end - p <= text::MAX_UNIT) {
                self.block_view(doc, idx).units[u].pending = false;
                return false;
            }
            let mark_str = text::piece_marks(rtl, before);
            let mut w_end = text::snap_caret(t, (p + text::MAX_UNIT).min(para_end), Snap::Floor, &is_tok).max(p + 1).min(para_end);
            // What the window shows: a line start to cut at (`Cut`), the
            // bounded end confirmed (`Kept`), or not (`Merge`).
            enum Found {
                Cut(usize),
                Kept,
                Merge,
                Whole,
            }
            let found = loop {
                let map = DisplayMap::build(
                    DisplayInput { text: t, start: p, end: w_end, mask: env.mask, preedit: None, marks: &mark_str },
                    env.resolve,
                );
                let layout = self.shaper.layout_styled(&map.text, self.width, heading, &[]);
                let n = layout.len();
                let exact = w_end >= para_end;
                // Lines whose start is final: all of them when the window
                // reaches the paragraph's end, else up to the last-but-one.
                let top = if exact { n } else { n.saturating_sub(1) };
                // Line starts usable as piece ends: the previous line does not
                // end with whitespace hanging past the width (parley would end
                // such a piece with an extra empty line).
                let starts: Vec<usize> = (1..top)
                    .filter(|&k| layout.get(k - 1).expect("line").metrics().advance <= max + 0.001)
                    .map(|k| text::snap_caret(t, map.d2m(layout.get(k).expect("line").text_range().start), Snap::Floor, &is_tok))
                    .filter(|&q| q > p)
                    .collect();
                let confirmed_to = if exact {
                    para_end
                } else {
                    layout.get(top.max(1) - 1).map(|l| map.d2m(l.text_range().start)).unwrap_or(p)
                };
                if bounded {
                    let has_end = starts.contains(&end);
                    if has_end && end - p <= text::MAX_UNIT {
                        break Found::Kept;
                    }
                    if let Some(&q) = starts.iter().filter(|&&q| q < end).max() {
                        break Found::Cut(q);
                    }
                    if has_end {
                        break Found::Kept;
                    }
                    if confirmed_to > end || exact {
                        break Found::Merge;
                    }
                } else {
                    if let Some(&q) = starts.iter().max() {
                        break Found::Cut(q);
                    }
                    if exact {
                        break Found::Whole;
                    }
                }
                let grow = p + 2 * (w_end - p);
                w_end = if grow >= para_end { para_end } else { text::snap_caret(t, grow, Snap::Floor, &is_tok).max(w_end + 1) };
            };
            match found {
                Found::Whole => {
                    self.block_view(doc, idx).units[u].pending = false;
                    return false;
                }
                Found::Merge => {
                    // The lines do not start again where the next piece
                    // starts: take it in, and look again.
                    let bv = self.block_view(doc, idx);
                    let next = bv.units.remove(u + 1);
                    let uv = &mut bv.units[u];
                    uv.span.end = next.span.end;
                    uv.span.term = next.span.term;
                    uv.height += next.height;
                    uv.layout = None;
                    uv.a11y = None;
                }
                Found::Kept => {
                    // The following pieces hold. Their marks follow the new
                    // text before them.
                    let map = DisplayMap::build(
                        DisplayInput { text: t, start: p, end, mask: env.mask, preedit: None, marks: "" },
                        env.resolve,
                    );
                    let next_before = text::last_strong(&map.text).or(before);
                    let bv = self.block_view(doc, idx);
                    bv.units[u].pending = false;
                    let mut k = u + 1;
                    let mut nb = next_before;
                    while k < bv.units.len() {
                        let want = Some((rtl, nb));
                        let uv = &mut bv.units[k];
                        if uv.marks != want {
                            uv.marks = want;
                            uv.layout = None;
                        }
                        if uv.span.term > 0 || uv.pending {
                            break;
                        }
                        // the strong direction before the piece after it
                        nb = text::last_strong(&t[uv.span.start..uv.span.end]).or(nb);
                        k += 1;
                    }
                    return false;
                }
                Found::Cut(q) => {
                    let map = DisplayMap::build(
                        DisplayInput { text: t, start: p, end: q, mask: env.mask, preedit: None, marks: "" },
                        env.resolve,
                    );
                    let next_before = text::last_strong(&map.text).or(before);
                    let h1 = self.estimate(q - p);
                    let h2 = self.estimate(end - q);
                    let bv = self.block_view(doc, idx);
                    let uv = &mut bv.units[u];
                    uv.span = UnitSpan { start: p, end: q, term: 0 };
                    uv.pending = false;
                    uv.layout = None;
                    uv.a11y = None;
                    uv.height = h1;
                    bv.units.insert(
                        u + 1,
                        UnitView {
                            span: UnitSpan { start: q, end, term: span.term },
                            pending: true,
                            marks: Some((rtl, next_before)),
                            layout: None,
                            height: h2,
                            a11y: None,
                        },
                    );
                    return true;
                }
            }
        }
    }

    pub(crate) fn unit_index(&mut self, doc: &DocumentV2, idx: usize, offset: usize, aff: Affinity) -> usize {
        let bv = self.block_view(doc, idx);
        let mut u = match bv.units.binary_search_by(|x| x.span.start.cmp(&offset)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        if u > 0 && aff == Affinity::Upstream {
            let prev = &bv.units[u - 1].span;
            if prev.term == 0 && prev.end == offset && bv.units[u].span.start == offset {
                u -= 1;
            }
        }
        u
    }

    /// Lays out a unit if needed (a pending unit is first cut: `u` is then
    /// its first piece).
    pub(crate) fn ensure_layout(&mut self, env: &mut Env<'_>, doc: &DocumentV2, idx: usize, u: usize) -> &UnitLayout {
        if self.block_view(doc, idx).units[u].pending {
            self.cut(env, doc, idx, u);
        }
        let b = &doc.blocks()[idx];
        let id = b.id();
        let width = self.width;
        let (span, marks) = {
            let uv = &self.block_view(doc, idx).units[u];
            (uv.span, uv.marks)
        };
        let mark_str = marks.map(|(rtl, before)| text::piece_marks(rtl, before)).unwrap_or_default();
        let pre: Option<(usize, String)> = env
            .preedit
            .filter(|(pid, p, _)| *pid == id && *p >= span.start && *p <= span.end && b.kind() == BlockKind::Text)
            .map(|(_, p, s)| (p, s.to_string()));
        let heading = self.heading_of(env, doc, idx, u);
        let valid = self.blocks[&id].units[u]
            .layout
            .as_ref()
            .is_some_and(|l| l.width == width && l.preedit == pre && l.heading == heading);
        if !valid {
            let map = if b.kind() == BlockKind::Text {
                DisplayMap::build(
                    DisplayInput {
                        text: b.text_str(),
                        start: span.start,
                        end: span.end,
                        mask: env.mask,
                        preedit: pre.as_ref().map(|(p, s)| (*p, s.as_str())),
                        marks: &mark_str,
                    },
                    env.resolve,
                )
            } else {
                let t = placeholder_text(b);
                let n = t.len();
                DisplayMap {
                    text: t,
                    segs: vec![Seg { m0: 0, m1: 0, d0: 0, d1: n, kind: SegKind::Atomic }],
                    m_start: 0,
                    m_end: 0,
                    has_tokens: false,
                }
            };
            let mut inks = vec![];
            if Self::draws_markdown(env, b) {
                let line_start = span.start == 0 || b.text_str().as_bytes()[span.start - 1] == b'\n';
                let (src, body) = md_source(&map);
                md::inks(&src, body, line_start, &mut inks);
            }
            let layout = self.shaper.layout_styled(&map.text, width, heading, &inks);
            let mut h = layout.height();
            // a body line gives the line height used for estimates and paging
            if heading == 0 && layout.len() == 1 && h > 0.0 && map.text.len() < 200 {
                self.line_h = h;
            }
            if h <= 0.0 {
                h = self.line_h;
            }
            let uv = &mut self.blocks.get_mut(&id).expect("view").units[u];
            uv.height = h;
            uv.layout = Some(Box::new(UnitLayout { map, layout, width, preedit: pre, heading }));
        }
        self.blocks[&id].units[u].layout.as_deref().expect("laid out")
    }

    /// Whether block `b` is drawn with light markdown formatting.
    fn draws_markdown(env: &Env<'_>, b: &Block) -> bool {
        env.markdown && !env.mask && b.kind() == BlockKind::Text && b.markup() == Some("markdown")
    }

    /// The heading level of the paragraph unit `u` belongs to (0 when it is
    /// not a heading or markdown is not drawn). Every piece of a long
    /// paragraph gets its paragraph's level, from the paragraph's first bytes.
    fn heading_of(&mut self, env: &Env<'_>, doc: &DocumentV2, idx: usize, u: usize) -> u8 {
        let b = &doc.blocks()[idx];
        if !Self::draws_markdown(env, b) {
            return self.plain_heading;
        }
        let bv = self.block_view(doc, idx);
        let mut k = u;
        while k > 0 && bv.units[k - 1].span.term == 0 {
            k -= 1;
        }
        let s = bv.units[k].span.start;
        let t = b.text_str().as_bytes();
        md::heading_level(&t[s..(s + 10).min(t.len())])
    }

    /// Lays out one line of plain text at a heading level's size (the title
    /// field's placeholder, and its height).
    pub(crate) fn plain_layout(&mut self, text: &str, heading: u8) -> Layout<Brush> {
        self.shaper.layout_styled(text, None, heading, &[])
    }

    pub(crate) fn has_layout(&mut self, doc: &DocumentV2, idx: usize, u: usize) -> bool {
        let uv = &self.block_view(doc, idx).units[u];
        !uv.pending && uv.layout.is_some()
    }

    pub(crate) fn unit_count(&mut self, doc: &DocumentV2, idx: usize) -> usize {
        self.block_view(doc, idx).units.len()
    }

    /// Height of a block (its units), estimated where not laid out.
    fn block_height(&mut self, doc: &DocumentV2, idx: usize) -> f32 {
        self.block_view(doc, idx).units.iter().map(|u| u.height).sum()
    }

    /// Document y of the top of a unit.
    pub(crate) fn unit_y(&mut self, doc: &DocumentV2, idx: usize, u: usize) -> f32 {
        let mut y = PAD;
        for i in 0..idx {
            y += self.block_height(doc, i) + BLOCK_GAP;
        }
        let bv = self.block_view(doc, idx);
        y + bv.units[..u].iter().map(|x| x.height).sum::<f32>()
    }

    /// Calls `f(block, unit, y, height)` for every unit in document order
    /// until it returns `false`.
    pub(crate) fn walk(&mut self, doc: &DocumentV2, mut f: impl FnMut(usize, usize, f32, f32) -> bool) {
        let mut y = PAD;
        for idx in 0..doc.blocks().len() {
            let bv = self.block_view(doc, idx);
            for (u, uv) in bv.units.iter().enumerate() {
                if !f(idx, u, y, uv.height) {
                    return;
                }
                y += uv.height;
            }
            y += BLOCK_GAP;
        }
    }

    // -------------------------------------------------------- geometry

    /// Caret box in unit coordinates for a stored offset. `pre_cursor`: while
    /// composing at this offset, the pre-edit's own cursor (bytes into it).
    pub(crate) fn caret_box(ul: &UnitLayout, offset: usize, aff: Affinity, pre_cursor: Option<usize>) -> (f32, f32, f32) {
        let mut d = ul.map.m2d(offset);
        if let (Some(c), Some((p0, p1))) = (pre_cursor, ul.map.preedit_range()) {
            if d == p0 {
                d = (p0 + c).min(p1);
            }
        }
        let cur = Cursor::from_byte_index(&ul.layout, d, aff);
        let g = cur.geometry(&ul.layout, CARET_W);
        let (mut y0, mut y1) = (g.y0 as f32, g.y1 as f32);
        if y1 <= y0 {
            y0 = 0.0;
            y1 = FONT_SIZE * 1.25;
        }
        (g.x0 as f32, y0, y1)
    }

    /// The line of a laid-out unit holding a displayed offset.
    pub(crate) fn line_of(ul: &UnitLayout, offset: usize, aff: Affinity) -> usize {
        let d = ul.map.m2d(offset);
        let n = ul.layout.len();
        if n == 0 {
            return 0;
        }
        let g = Cursor::from_byte_index(&ul.layout, d, aff).geometry(&ul.layout, CARET_W);
        let mid = ((g.y0 + g.y1) / 2.0) as f32;
        for i in 0..n {
            let m = *ul.layout.get(i).expect("line").metrics();
            if mid < m.block_max_coord || i + 1 == n {
                return i;
            }
        }
        n - 1
    }

    /// The caret places of one line, and whether the line holds right-to-left
    /// text, and whether its paragraph runs right to left.
    pub(crate) fn line_places(&mut self, env: &mut Env<'_>, doc: &DocumentV2, l: LineRef) -> (Vec<Place>, bool, bool) {
        let b = doc.blocks()[l.block].share();
        let ul = self.ensure_layout(env, doc, l.block, l.unit);
        let n = ul.layout.len();
        if b.kind() != BlockKind::Text || n == 0 {
            return (vec![Place { offset: 0, aff: Affinity::Downstream, x: 0.0 }], false, false);
        }
        let line = ul.layout.get(l.line.min(n - 1)).expect("line");
        let lm = *line.metrics();
        let has_rtl = line.runs().any(|r| r.is_rtl());
        let para_rtl = ul.layout.is_rtl();
        let r = line.text_range();
        let text = b.text_str();
        let resolve = env.resolve;
        let is_tok = |id: &str| resolve(id).is_some();
        let m_lo = ul.map.d2m(r.start).min(ul.map.m_end);
        let m_hi = if r.end >= ul.map.text.len() { ul.map.m_end } else { ul.map.d2m(r.end) };
        let mut places = vec![];
        let mut m = text::snap_caret(text, m_lo, Snap::Ceil, &is_tok);
        loop {
            if m > m_hi {
                break;
            }
            let d = ul.map.m2d(m);
            for aff in [Affinity::Downstream, Affinity::Upstream] {
                let g = Cursor::from_byte_index(&ul.layout, d, aff).geometry(&ul.layout, CARET_W);
                let mid = ((g.y0 + g.y1) / 2.0) as f32;
                if mid >= lm.block_min_coord - 0.01 && mid <= lm.block_max_coord + 0.01 {
                    let x = g.x0 as f32;
                    if !places.iter().any(|p: &Place| p.offset == m && (p.x - x).abs() < 0.01) {
                        places.push(Place { offset: m, aff, x });
                    }
                }
            }
            if m >= m_hi {
                break;
            }
            match text::next_caret(text, m, &is_tok) {
                Some(n) => m = n,
                None => break,
            }
        }
        if places.is_empty() {
            places.push(Place { offset: m_lo, aff: Affinity::Downstream, x: 0.0 });
        }
        (places, has_rtl, para_rtl)
    }

    /// The stored range a line covers (for Home / End).
    pub(crate) fn line_range(&mut self, env: &mut Env<'_>, doc: &DocumentV2, l: LineRef) -> (usize, usize) {
        let ul = self.ensure_layout(env, doc, l.block, l.unit);
        let n = ul.layout.len();
        if n == 0 || doc.blocks()[l.block].kind() != BlockKind::Text {
            return (ul.map.m_start, ul.map.m_end);
        }
        let r = ul.layout.get(l.line.min(n - 1)).expect("line").text_range();
        let lo = ul.map.d2m(r.start);
        let hi = if l.line + 1 >= n { ul.map.m_end } else { ul.map.d2m(r.end) };
        (lo, hi)
    }

    pub(crate) fn line_count(&mut self, env: &mut Env<'_>, doc: &DocumentV2, idx: usize, u: usize) -> usize {
        self.ensure_layout(env, doc, idx, u).layout.len().max(1)
    }

    pub(crate) fn next_line(&mut self, env: &mut Env<'_>, doc: &DocumentV2, l: LineRef) -> Option<LineRef> {
        if l.line + 1 < self.line_count(env, doc, l.block, l.unit) {
            return Some(LineRef { line: l.line + 1, ..l });
        }
        if l.unit + 1 < self.unit_count(doc, l.block) {
            return Some(LineRef { block: l.block, unit: l.unit + 1, line: 0 });
        }
        (l.block + 1 < doc.blocks().len()).then_some(LineRef { block: l.block + 1, unit: 0, line: 0 })
    }

    pub(crate) fn prev_line(&mut self, env: &mut Env<'_>, doc: &DocumentV2, l: LineRef) -> Option<LineRef> {
        if l.line > 0 {
            return Some(LineRef { line: l.line - 1, ..l });
        }
        let (block, unit) = if l.unit > 0 {
            (l.block, l.unit - 1)
        } else if l.block > 0 {
            let b = l.block - 1;
            let last = self.unit_count(doc, b) - 1;
            (b, self.cut_all(env, doc, b, last))
        } else {
            return None;
        };
        let line = self.line_count(env, doc, block, unit) - 1;
        Some(LineRef { block, unit, line })
    }

    /// Hit test inside a laid-out unit: the stored offset (snapped to a caret
    /// place) and affinity at unit coordinates (x, y).
    pub(crate) fn hit(ul: &UnitLayout, b: &Block, x: f32, y: f32, resolve: Resolve<'_>) -> (usize, Affinity) {
        if b.kind() != BlockKind::Text {
            return (0, Affinity::Downstream);
        }
        let cur = Cursor::from_point(&ul.layout, x, y);
        let m = ul.map.d2m(cur.index());
        let is_tok = |id: &str| resolve(id).is_some();
        let m = text::snap_caret(b.text_str(), m.clamp(ul.map.m_start, ul.map.m_end), Snap::Nearest, &is_tok);
        (m, cur.affinity())
    }

    /// Selection rectangles of a unit for the stored range `a..b`.
    pub(crate) fn selection_rects(ul: &UnitLayout, a: usize, b: usize) -> Vec<(f32, f32, f32, f32)> {
        let da = ul.map.m2d(a.clamp(ul.map.m_start, ul.map.m_end));
        let db = ul.map.m2d(b.clamp(ul.map.m_start, ul.map.m_end));
        if da >= db {
            return vec![];
        }
        let sel = parley::Selection::new(
            Cursor::from_byte_index(&ul.layout, da, Affinity::Downstream),
            Cursor::from_byte_index(&ul.layout, db, Affinity::Upstream),
        );
        sel.geometry(&ul.layout).into_iter().map(|(r, _)| (r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)).collect()
    }
}

// ------------------------------------------------------------ painting

/// The colours the editor paints with. The host passes its theme's with
/// [`crate::Editor::set_palette`] (and again when the theme changes); until
/// then the editor uses [`Palette::DARK`]. Start from [`Palette::DARK`] or
/// [`Palette::LIGHT`] and set the fields to change.
///
/// What each colour must hold for WCAG 2.1 AA (spec ch. 12, standing policy):
/// `text`, `muted` and `link` at least 4.5:1 against `background` (`muted`
/// also against `placeholder_background`); `selection_text` at least 4.5:1
/// against `selection`; `caret` and `selection` at least 3:1 against
/// `background` (non-text contrast). [`Palette::DARK`] and
/// [`Palette::LIGHT`] meet them (unit tests).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Palette {
    /// Behind the text.
    pub background: Color,
    /// Body text.
    pub text: Color,
    /// Markdown markup (`#`, `*`, `[`…`](url)`, list markers) and the labels of
    /// blocks this version does not edit (images, video, tables).
    pub muted: Color,
    /// Link text and URLs (drawn underlined).
    pub link: Color,
    /// The selection highlight, drawn opaque under the text.
    pub selection: Color,
    /// Selected text, whatever its colour outside the selection.
    pub selection_text: Color,
    /// The caret.
    pub caret: Color,
    /// The box of a block this version does not edit.
    pub placeholder_background: Color,
    pub placeholder_border: Color,
    /// The provenance accent at the editor's left edge (ch. 12): owned,
    /// external, undetermined.
    pub owned: Color,
    pub external: Color,
    pub undetermined: Color,
}

impl Palette {
    /// The editor's own dark colours (its look before hosts passed theirs).
    pub const DARK: Palette = Palette {
        background: Color::from_rgb8(0x1e, 0x1f, 0x22),
        text: Color::from_rgb8(0xe6, 0xe6, 0xe6),
        muted: Color::from_rgb8(0xa0, 0xa4, 0xab),
        link: Color::from_rgb8(0x7a, 0xb2, 0xe6),
        selection: Color::from_rgb8(0x5a, 0x9f, 0xd4),
        selection_text: Color::from_rgb8(0x14, 0x14, 0x1a),
        caret: Color::from_rgb8(0xf5, 0x9e, 0x0b),
        placeholder_background: Color::from_rgb8(0x2a, 0x2c, 0x30),
        placeholder_border: Color::from_rgb8(0x55, 0x58, 0x5e),
        owned: Color::from_rgb8(0x5a, 0x9f, 0xd4),
        external: Color::from_rgb8(0xe0, 0x7c, 0x6a),
        undetermined: Color::from_rgb8(0x8a, 0x8d, 0x93),
    };

    /// Light colours (dark text on white).
    pub const LIGHT: Palette = Palette {
        background: Color::from_rgb8(0xff, 0xff, 0xff),
        text: Color::from_rgb8(0x1a, 0x1a, 0x20),
        muted: Color::from_rgb8(0x6e, 0x6e, 0x6e),
        link: Color::from_rgb8(0x1f, 0x5f, 0xaa),
        selection: Color::from_rgb8(0x5f, 0x91, 0xcd),
        selection_text: Color::from_rgb8(0x1a, 0x1a, 0x20),
        caret: Color::from_rgb8(0xd9, 0x77, 0x06),
        placeholder_background: Color::from_rgb8(0xff, 0xff, 0xff),
        placeholder_border: Color::from_rgb8(0xc8, 0xc8, 0xb8),
        owned: Color::from_rgb8(0x3a, 0x7f, 0xc4),
        external: Color::from_rgb8(0xc0, 0x55, 0x45),
        undetermined: Color::from_rgb8(0x8a, 0x8d, 0x93),
    };

    /// The colour a glyph of `ink` is drawn with outside the selection.
    pub(crate) fn ink(&self, ink: Ink, base: Color) -> Color {
        match ink.role() {
            Ink::MARK => self.muted,
            Ink::LINK => self.link,
            _ => base,
        }
    }
}

impl Default for Palette {
    fn default() -> Palette {
        Palette::DARK
    }
}

/// WCAG 2.x contrast ratio between two opaque colours (relative luminance,
/// sRGB): 1.0 to 21.0. Alpha is ignored.
pub fn contrast_ratio(a: Color, b: Color) -> f64 {
    fn lum(c: Color) -> f64 {
        let [r, g, b, _] = c.to_rgba8().to_u8_array();
        let ch = |v: u8| {
            let s = v as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
    }
    let (x, y) = (lum(a), lum(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// The displayed text of a unit as the markdown rules see it (`md::inks`):
/// tokens and pre-edit replaced by letters (they take part as text, their
/// own characters never as markup), invisible marks by spaces; and where the
/// unit's own text starts.
fn md_source(map: &DisplayMap) -> (Vec<u8>, usize) {
    let mut src = map.text.as_bytes().to_vec();
    for s in &map.segs {
        let fill = match s.kind {
            SegKind::Plain => continue,
            SegKind::Mark => b' ',
            SegKind::Atomic | SegKind::Preedit => b'a',
        };
        for x in &mut src[s.d0..s.d1] {
            *x = fill;
        }
    }
    let body = map.segs.iter().take_while(|s| s.kind == SegKind::Mark).last().map(|s| s.d1).unwrap_or(0);
    (src, body)
}

pub(crate) fn fill(scene: &mut Scene, color: Color, x0: f64, y0: f64, x1: f64, y1: f64) {
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &KRect::new(x0, y0, x1, y1));
}

pub(crate) fn placeholder_box(scene: &mut Scene, pal: &Palette, x0: f64, y0: f64, x1: f64, y1: f64) {
    let r = RoundedRect::new(x0, y0, x1, y1, 4.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal.placeholder_background, None, &r);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal.placeholder_border, None, &r);
}

/// Slant of synthesised italics (tan 12°), as parley's own synthesis does.
const EM_SKEW: f64 = 0.2126;

/// Paints a laid-out unit at (dx, dy). Each glyph run's colour comes from
/// its ink through `color`; strong text is emboldened and emphasised text
/// slanted **when painting** (and where the font itself had to be
/// synthesised), links underlined — the layout, hence every line break and
/// caret place, is the plain source's.
pub(crate) fn draw_layout(scene: &mut Scene, layout: &Layout<Brush>, dx: f64, dy: f64, color: impl Fn(Ink) -> Color) {
    let transform = Affine::translate((dx, dy));
    for line in layout.lines() {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(gr) = item else { continue };
            let run = gr.run();
            let ink = gr.style().brush;
            let brush = color(ink);
            let synth = run.synthesis();
            let size = run.font_size();
            let bold = ink.has(Ink::STRONG) || synth.embolden();
            let skew = if ink.has(Ink::EM) {
                Some(EM_SKEW)
            } else {
                synth.skew().map(|deg| (deg as f64).to_radians().tan())
            };
            let glyphs = || {
                let mut gx = gr.offset();
                let gy = gr.baseline();
                gr.glyphs().map(move |g| {
                    let x = gx + g.x;
                    let y = gy - g.y;
                    gx += g.advance;
                    Glyph { id: g.id, x, y }
                })
            };
            // the fill, then for bold an outline stroke on top (about 3% of
            // the size, each side)
            for pass in 0..if bold { 2 } else { 1 } {
                let b = scene
                    .draw_glyphs(run.font())
                    .font_size(size)
                    .brush(brush)
                    .transform(transform)
                    .glyph_transform(skew.map(|k| Affine::skew(k, 0.0)))
                    .normalized_coords(run.normalized_coords());
                if pass == 0 {
                    b.draw(Fill::NonZero, glyphs());
                } else {
                    b.draw(&Stroke::new((size * 0.035) as f64), glyphs());
                }
            }
            if ink.has(Ink::UNDERLINE) {
                let m = run.metrics();
                let y = (gr.baseline() - m.underline_offset) as f64 + dy;
                let h = (m.underline_size as f64).max(1.0);
                let x0 = dx + gr.offset() as f64;
                fill(scene, brush, x0, y, x0 + gr.advance() as f64, y + h);
            }
        }
    }
}

/// Paints the glyphs of a unit that lie in `rects` (unit coordinates, the
/// selection) once more, in `color`, clipped to the rects: selected text has
/// one colour whatever its ink, so it keeps its contrast on the selection.
pub(crate) fn draw_layout_clipped(scene: &mut Scene, layout: &Layout<Brush>, dx: f64, dy: f64, rects: &[(f64, f64, f64, f64)], color: Color) {
    if rects.is_empty() {
        return;
    }
    let mut clip = vello::kurbo::BezPath::new();
    for &(x0, y0, x1, y1) in rects {
        clip.move_to((dx + x0, dy + y0));
        clip.line_to((dx + x1, dy + y0));
        clip.line_to((dx + x1, dy + y1));
        clip.line_to((dx + x0, dy + y1));
        clip.close_path();
    }
    scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &clip);
    draw_layout(scene, layout, dx, dy, |_| color);
    scene.pop_layer();
}

/// The unit for a paragraph (or the rest of one): pending if long. A free
/// function so it can run while a block view is borrowed.
fn self_paragraph_unit(width: Option<f32>, line_h: f32, t: &str, span: UnitSpan, marks: Option<(bool, Option<Strong>)>) -> UnitView {
    let long = width.is_some() && span.end - span.start > text::MAX_UNIT;
    let marks = match marks {
        Some(m) => Some(m),
        None if long => {
            let rtl = matches!(text::first_strong(&t[span.start..span.end]), Some(Strong::R | Strong::Al));
            Some((rtl, None))
        }
        None => None,
    };
    let per_line = width.map(|w| (w / EST_ADVANCE).max(1.0)).unwrap_or(f32::MAX);
    let height = (((span.end - span.start) as f32 / per_line).ceil().max(1.0)) * line_h;
    UnitView { span, pending: long, marks, layout: None, height, a11y: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line: stored start, stored end, advance, and what else is compared.
    type Line = (usize, usize, i64, String);

    const FONT: &[u8] = include_bytes!("../tests/fixtures/Cantarell-Regular.ttf");

    fn doc_of(text: &str) -> DocumentV2 {
        let mut lit = String::new();
        crate::json::encode_string(text, &mut lit);
        let s = format!(
            r#"{{"format":"sovereign.doc","v":2,"blocks":[{{"type":"text","id":"0123456789abcdef0123456789abcdef","markup":"markdown","text":{lit}}}]}}"#
        );
        DocumentV2::read(&s).unwrap().0
    }

    /// Lines as (stored start, stored end, advance, break reason), for one
    /// laid-out unit.
    fn lines_of(ul: &UnitLayout) -> Vec<Line> {
        ul.layout
            .lines()
            .map(|l| {
                let r = l.text_range();
                let m = l.metrics();
                (
                    ul.map.d2m(r.start),
                    if r.end >= ul.map.text.len() { ul.map.m_end } else { ul.map.d2m(r.end) },
                    (m.advance * 100.0).round() as i64,
                    format!("{:?} offset {} height {}", l.break_reason(), (m.offset * 100.0).round(), (m.line_height * 100.0).round()),
                )
            })
            .collect()
    }

    /// Lays the paragraph out whole, and piece by piece as the view does;
    /// returns both line lists (and how many pieces there were).
    fn whole_and_pieces(text: &str, width: f32) -> (Vec<Line>, Vec<Line>, usize) {
        let fonts = crate::Fonts::Only(vec![FONT.to_vec()]);
        let doc = doc_of(text);
        let resolve = |_: &str| None;
        let mut env = Env { resolve: &resolve, preedit: None, mask: false, markdown: true };

        let mut whole_view = View::new(&fonts);
        whole_view.width = Some(width);
        let map = DisplayMap::build(
            DisplayInput { text, start: 0, end: text.len(), mask: false, preedit: None, marks: "" },
            env.resolve,
        );
        let layout = whole_view.shaper.layout(&map.text, Some(width));
        let whole = lines_of(&UnitLayout { map, layout, width: Some(width), preedit: None, heading: 0 });

        let mut view = View::new(&fonts);
        view.set_width(Some(width));
        let pieces = piece_lines(&mut view, &mut env, &doc);
        let last = view.unit_count(&doc, 0) - 1;
        // The last line of a piece ends because the piece ends; in the whole
        // layout it ends at a soft break: break reasons are not compared
        // (alignment offset and line height are).
        let norm = |v: Vec<Line>| -> Vec<Line> {
            v.into_iter().map(|(a, b, w, r)| (a, b, w, r.split_once(' ').map(|x| x.1.to_string()).unwrap_or_default())).collect()
        };
        (norm(whole), norm(pieces), last + 1)
    }

    fn piece_lines(view: &mut View, env: &mut Env<'_>, doc: &DocumentV2) -> Vec<Line> {
        let mut pieces = vec![];
        let mut u = 0;
        while u < view.unit_count(doc, 0) {
            let ul = view.ensure_layout(env, doc, 0, u);
            pieces.extend(lines_of(ul));
            u += 1;
        }
        pieces
    }

    /// Units tile the block's text: each starts where the previous one's
    /// line break ends.
    fn check_units(view: &mut View, doc: &DocumentV2, when: &str) {
        let len = doc.blocks()[0].text_str().len();
        let spans: Vec<UnitSpan> = view.block_view(doc, 0).units.iter().map(|u| u.span).collect();
        let mut at = 0;
        for (i, sp) in spans.iter().enumerate() {
            assert_eq!(sp.start, at, "{when}: unit {i} of {} starts at {} not {at}: {spans:?}", spans.len(), sp.start);
            at = sp.end + sp.term;
        }
        assert_eq!(at, len, "{when}: units end at {at}, text at {len}");
    }

    fn assert_same(whole: &[Line], pieces: &[Line], what: &str) {
        if let Some(i) = (0..whole.len().min(pieces.len())).find(|&i| whole[i] != pieces[i]) {
            let lo = i.saturating_sub(2);
            panic!(
                "{what}: line {i} differs
 whole  {:?}
 pieces {:?}",
                &whole[lo..(i + 3).min(whole.len())],
                &pieces[lo..(i + 3).min(pieces.len())]
            );
        }
        assert_eq!(whole.len(), pieces.len(), "{what}: line count");
    }

    fn words(seed: u64, n: usize, extra: &[&str]) -> String {
        let mut x = seed;
        let mut s = String::new();
        for i in 0..n {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            if !extra.is_empty() && i % 7 == 3 {
                s.push_str(extra[(x >> 40) as usize % extra.len()]);
            } else {
                let len = 1 + (x >> 59) as usize;
                for k in 0..len {
                    s.push((b'a' + ((x >> (8 + k)) % 26) as u8) as char);
                }
            }
            s.push(if (x >> 30).is_multiple_of(11) { ',' } else { ' ' });
        }
        s
    }

    // A long paragraph is shown exactly as if laid out whole: same lines, at
    // the same places, with the same widths — no line break that is not in
    // the text or forced by the width.
    #[test]
    fn a_long_paragraph_lays_out_as_if_whole() {
        // under 64 KiB: parley 0.10 cannot lay out more as one text (its line
        // ranges wrap at 65,536 bytes), which is one more reason for pieces
        let prose = words(1, 2500, &[]);
        assert!(prose.len() > 3 * text::MAX_UNIT);
        for width in [180.0, 500.0, 1500.0] {
            let (whole, pieces, n) = whole_and_pieces(&prose, width);
            assert!(n > 2, "the paragraph was cut into pieces ({n})");
            assert_same(&whole, &pieces, &format!("width {width}"));
        }
    }

    // Same with right-to-left words, digits and Arabic in the paragraph: the
    // pieces resolve their direction as inside the paragraph.
    #[test]
    fn a_long_mixed_direction_paragraph_lays_out_as_if_whole() {
        let ltr = words(2, 3000, &["שלום", "עולם 42", "مرحبا", "بالعالم 7", "(abc)"]);
        let rtl = format!("שלום {}", words(3, 3000, &["שלום", "עולם", "مرحبا 12", "x"]));
        for text in [ltr, rtl] {
            let (whole, pieces, n) = whole_and_pieces(&text, 420.0);
            assert!(n > 2);
            assert_same(&whole, &pieces, "mixed");
        }
    }

    // A word longer than the line, and text with no break opportunity at all,
    // wrap at the width (overflow-wrap: anywhere), whole and in pieces alike.
    #[test]
    fn unbreakable_text_wraps_the_same_in_pieces() {
        let long_word = format!("{} {} {}", words(4, 300, &[]), "x".repeat(9000), words(5, 300, &[]));
        let accents = "é".repeat(12_000);
        for text in [long_word, accents] {
            let (whole, pieces, n) = whole_and_pieces(&text, 300.0);
            assert!(n > 1);
            assert_same(&whole, &pieces, "unbreakable");
        }
    }

    // Markdown inks are brushes: the layout of a unit drawn with them has the
    // same lines, glyph positions and caret places as the plain source.
    #[test]
    fn markdown_inks_do_not_move_anything() {
        let fonts = crate::Fonts::Only(vec![FONT.to_vec()]);
        let mut view = View::new(&fonts);
        let text = "**bold** and *em* with [a link](https://example.org/x_y) `co*de*` - and __more__ text, wrapping \
                    over several lines at this width so that line breaks are compared too";
        let mut inks = vec![];
        md::inks(text.as_bytes(), 0, true, &mut inks);
        assert!(inks.len() > 10);
        for width in [Some(160.0), Some(330.0), None] {
            let plain = view.shaper.layout(text, width);
            let styled = view.shaper.layout_styled(text, width, 0, &inks);
            assert_eq!(plain.len(), styled.len());
            for (a, b) in plain.lines().zip(styled.lines()) {
                assert_eq!(a.text_range(), b.text_range());
                assert_eq!(a.metrics().advance, b.metrics().advance);
            }
            for o in (0..=text.len()).filter(|&o| text.is_char_boundary(o)) {
                for aff in [Affinity::Downstream, Affinity::Upstream] {
                    let ga = Cursor::from_byte_index(&plain, o, aff).geometry(&plain, CARET_W);
                    let gb = Cursor::from_byte_index(&styled, o, aff).geometry(&styled, CARET_W);
                    assert_eq!((ga.x0, ga.y0, ga.y1), (gb.x0, gb.y0, gb.y1), "offset {o}");
                }
            }
        }
    }

    // A long heading paragraph, cut into pieces, still lays out as if whole:
    // every piece gets the paragraph's heading style.
    #[test]
    fn a_long_heading_lays_out_as_if_whole() {
        let text = format!("# {}", words(9, 2000, &[]));
        let fonts = crate::Fonts::Only(vec![FONT.to_vec()]);
        let doc = doc_of(&text);
        let resolve = |_: &str| None;
        let mut env = Env { resolve: &resolve, preedit: None, mask: false, markdown: true };
        let width = 400.0;
        let mut view = View::new(&fonts);
        view.set_width(Some(width));
        let pieces = piece_lines(&mut view, &mut env, &doc);
        assert!(view.unit_count(&doc, 0) > 2);
        let map = DisplayMap::build(DisplayInput { text: &text, start: 0, end: text.len(), mask: false, preedit: None, marks: "" }, env.resolve);
        let layout = view.shaper.layout_styled(&map.text, Some(width), 1, &[]);
        let norm = |v: Vec<Line>| -> Vec<Line> {
            v.into_iter().map(|(a, b, w, r)| (a, b, w, r.split_once(' ').map(|x| x.1.to_string()).unwrap_or_default())).collect()
        };
        let whole = norm(lines_of(&UnitLayout { map, layout, width: Some(width), preedit: None, heading: 1 }));
        assert_same(&whole, &norm(pieces), "heading");
    }

    // The two built-in palettes meet WCAG 2.1 AA (text 4.5:1, non-text 3:1).
    #[test]
    fn built_in_palettes_meet_wcag_aa() {
        for (name, p) in [("dark", Palette::DARK), ("light", Palette::LIGHT)] {
            let text = [
                ("text", p.text, p.background),
                ("muted", p.muted, p.background),
                ("muted on placeholder", p.muted, p.placeholder_background),
                ("link", p.link, p.background),
                ("selected text", p.selection_text, p.selection),
            ];
            for (what, fg, bg) in text {
                let r = contrast_ratio(fg, bg);
                assert!(r >= 4.5, "{name} {what}: {r:.2}");
            }
            for (what, fg, bg) in [("caret", p.caret, p.background), ("selection", p.selection, p.background)] {
                let r = contrast_ratio(fg, bg);
                assert!(r >= 3.0, "{name} {what}: {r:.2}");
            }
        }
        assert!((contrast_ratio(Color::from_rgb8(0, 0, 0), Color::from_rgb8(255, 255, 255)) - 21.0).abs() < 0.01);
    }

    // Han text: a character the first CJK fallback font lacks is drawn with
    // another system font that has it, never as a box (glyph 0). 佢 (U+4F62)
    // is the case seen in the manual run on Windows. Only checkable against
    // the machine's fonts: when no system CJK font maps it, the test says so
    // and checks nothing.
    #[test]
    fn han_characters_fall_back_per_character_to_a_font_that_has_them() {
        use parley::fontique::{FallbackKey, Language, Script};
        let text = "我徹底咁測試佢。";
        // What the system offers for Han, asked independently of the editor:
        // its answer with no language and for each CJK language.
        let mut plain = FontContext::new();
        let hani = Script::from_bytes(*b"Hani");
        let mut keys = vec![FallbackKey::new(hani, None)];
        for l in ["zh-CN", "zh-TW", "zh-HK", "ja", "ko"] {
            keys.push(FallbackKey::new(hani, Some(&Language::parse(l).unwrap())));
        }
        let mut with_glyph = vec![];
        let mut first = None;
        for key in keys {
            let fams: Vec<_> = plain.collection.fallback_families(key).collect();
            for id in fams {
                let Some(fam) = plain.collection.family(id) else { continue };
                let has = fam.fonts().iter().any(|fi| {
                    fi.load(Some(&mut plain.source_cache))
                        .is_some_and(|blob| fi.charmap_index().charmap(blob.as_ref()).and_then(|c| c.map('佢')).is_some_and(|g| g != 0))
                });
                first.get_or_insert((fam.name().to_string(), has));
                if has {
                    with_glyph.push(fam.name().to_string());
                }
            }
        }
        // (no printing here: tests/g4 forbids stdout/stderr in this crate's
        // source; the families are in the failure message)
        if with_glyph.is_empty() {
            return; // no system CJK font has U+4F62 here: nothing to check
        }
        let mut shaper = Shaper::new(&crate::Fonts::System);
        let layout = shaper.layout(text, None);
        let mut missing = vec![];
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(gr) = item else { continue };
                for cl in gr.run().visual_clusters() {
                    if cl.glyphs().any(|g| g.id == 0) {
                        missing.push(text[cl.text_range()].to_string());
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "drawn as boxes: {missing:?} (Han fallback with no language: {first:?}; fonts with U+4F62: {with_glyph:?})"
        );
    }

    // After an edit inside a long paragraph — at a piece start, in a piece,
    // across a piece boundary — the pieces cut again still show the lines of
    // the whole paragraph.
    #[test]
    fn edits_in_a_long_paragraph_keep_it_as_if_whole() {
        let fonts = crate::Fonts::Only(vec![FONT.to_vec()]);
        let mut doc = doc_of(&words(7, 2500, &[]));
        let resolve = |_: &str| None;
        let mut env = Env { resolve: &resolve, preedit: None, mask: false, markdown: true };
        let width = 360.0;
        let mut view = View::new(&fonts);
        view.set_width(Some(width));
        let _ = piece_lines(&mut view, &mut env, &doc);
        let starts: Vec<usize> = view.block_view(&doc, 0).units.iter().map(|u| u.span.start).collect();
        assert!(starts.len() > 3);
        let edits: [(usize, usize, &str); 4] = [
            (starts[2], 0, "WIDE "),
            (starts[2] + 10, 0, "inserted words here "),
            (starts[1] - 3, 6, ""),
            (starts[3], 1, ""),
        ];
        for (at, del, ins) in edits {
            let old_rev = doc.blocks()[0].rev();
            let t = doc.blocks_mut()[0].text_mut();
            let at = (at..).find(|&i| t.is_char_boundary(i)).unwrap();
            t.replace_range(at..at + del, ins);
            let b = doc.blocks()[0].share();
            view.edited(&b, old_rev, at, del, ins.len());
            check_units(&mut view, &doc, "after edited");
            let pieces = piece_lines(&mut view, &mut env, &doc);
            check_units(&mut view, &doc, "after cutting");
            let text = doc.blocks()[0].text_str().to_string();
            let map = DisplayMap::build(
                DisplayInput { text: &text, start: 0, end: text.len(), mask: false, preedit: None, marks: "" },
                env.resolve,
            );
            let layout = view.shaper.layout(&map.text, Some(width));
            let norm = |v: Vec<Line>| -> Vec<Line> {
                v.into_iter().map(|(a, b, w, r)| (a, b, w, r.split_once(' ').map(|x| x.1.to_string()).unwrap_or_default())).collect()
            };
            let whole = norm(lines_of(&UnitLayout { map, layout, width: Some(width), preedit: None, heading: 0 }));
            assert_same(&whole, &norm(pieces), &format!("edit at {at}"));
        }
    }
}
