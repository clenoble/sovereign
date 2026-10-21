//! Roll-our-own chrome panels on the vello 0.9 stack: the shared centered-panel
//! geometry + scrollbar, plus the document window, inbox, chat composer,
//! settings, and the full-screen auth gate.

use std::collections::HashMap;
use std::sync::Arc;

use parley::Layout;
use vello::kurbo::{BezPath, Circle, Ellipse, Line, Point, Rect, RoundedRect, Stroke};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill, Mix};
use vello::Scene;

use sovereign_core::profile::BubbleStyle;
use sovereign_db::schema::{MessageDirection, PiiKind, ReviewState};
use sovereign_db::traits::GraphDB;

use crate::canvas::{lane_color, Card};
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::pal;

// ---- Document window (first chrome panel — roll-our-own on vello 0.9) ----

pub(crate) const DW_TITLE_H: f64 = 50.0;
pub(crate) const DW_PAD: f64 = 22.0;
pub(crate) const DW_CLOSE: f64 = 30.0;

/// Chrome geometry for a floating window of arbitrary `bounds` (logical px),
/// shared by every panel's painter and hit-testing so they never drift.
/// Returns (header/title-bar, close-button, body-viewport).
///   - header = the top `DW_TITLE_H` of `bounds`
///   - close  = a `DW_CLOSE`-square at the header's right inset 12px, centered vertically
///   - body   = below the header, inset by `DW_PAD`
pub(crate) fn window_chrome(bounds: Rect) -> (Rect, Rect, Rect) {
    let header = Rect::new(bounds.x0, bounds.y0, bounds.x1, bounds.y0 + DW_TITLE_H);
    let cy = bounds.y0 + (DW_TITLE_H - DW_CLOSE) * 0.5;
    let close = Rect::new(bounds.x1 - DW_CLOSE - 12.0, cy, bounds.x1 - 12.0, cy + DW_CLOSE);
    let body = Rect::new(
        bounds.x0 + DW_PAD,
        bounds.y0 + DW_TITLE_H + DW_PAD * 0.5,
        bounds.x1 - DW_PAD,
        bounds.y1 - DW_PAD * 0.5,
    );
    (header, close, body)
}

/// Split a chat body into (message viewport, composer input rect): the input is
/// the bottom `CHAT_INPUT_H` of the body; the messages get the rest (minus a gap).
pub(crate) fn chat_split(body: Rect) -> (Rect, Rect) {
    let input_rect = Rect::new(body.x0, body.y1 - CHAT_INPUT_H, body.x1, body.y1);
    let msg_vp = Rect::new(body.x0, body.y0, body.x1, body.y1 - CHAT_INPUT_H - 10.0);
    (msg_vp, input_rect)
}

/// Split a search body into (query input, results): the input is the top 40px,
/// the results take everything below (with a 10px gap).
pub(crate) fn search_split(body: Rect) -> (Rect, Rect) {
    let input = Rect::new(body.x0, body.y0, body.x1, body.y0 + 40.0);
    let results = Rect::new(body.x0, body.y0 + 50.0, body.x1, body.y1);
    (input, results)
}

/// A header button in the doc window (right-aligned before the × close). The
/// editor edits in place and saves by itself: History is the only one
/// (contract E2).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum DocBtn {
    History,
}

/// An open document's window: its bar (title, History, ×) around the editor,
/// which draws and edits the body (`app/editor_v2.rs`).
pub(crate) struct DocWindow {
    pub(crate) doc_id: String, // the card's document id (open-or-focus dedupe)
    pub(crate) external: bool,
    pub(crate) title: Layout<Brush>,
    // Pre-shaped header-button label (no shaper at draw time).
    lbl_history: Layout<Brush>,
    /// "…", drawn where a bar title too long for the bar is cut (T8).
    lbl_ellipsis: Layout<Brush>,
    /// The document editor. `App::open_doc_window` opens a window only with
    /// it (E1, E3): `None` exists only for a window built outside the app
    /// (tests), which then has no body.
    pub(crate) v2: Option<Box<crate::app::editor_v2::EditorPane>>,
}
/// The size of a document window's bar title (title contract T8: small, the
/// page carries the large one) [choice].
const BAR_TITLE_PX: f32 = 14.0;

/// A document window's bar title: 14 px, on one line (never wrapped; cut
/// with an ellipsis where the bar ends, `draw_doc_window`).
fn bar_title(title: &str, shaper: &mut TextShaper) -> Layout<Brush> {
    shaper.shape(crate::canvas::display_title(title), 100_000.0, BAR_TITLE_PX)
}

impl DocWindow {
    /// The title in the window's bar follows a saved title (title contract
    /// T8: 14 px, one line). Called by the editor's save path.
    pub(crate) fn set_title(&mut self, title: &str, shaper: &mut TextShaper) {
        self.title = bar_title(title, shaper);
    }

    /// A window for `card`'s document, without its editor yet (the app sets
    /// `v2` before it shows the window). `_body_w`: the body's width, which
    /// the editor lays out itself (kept for the callers).
    pub(crate) fn new(card: &Card, shaper: &mut TextShaper, _body_w: f64) -> Self {
        Self {
            doc_id: card.id.clone(),
            title: bar_title(&card.title, shaper),
            lbl_ellipsis: shaper.shape("\u{2026}", 100.0, BAR_TITLE_PX),
            external: card.external,
            lbl_history: shaper.shape("History", 100.0, 12.5),
            v2: None,
        }
    }
    /// The document editor this window holds (seam read by the contract E1-E9
    /// tests, `src/editor_default_tests.rs`).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn editor_pane(&self) -> Option<&crate::app::editor_v2::EditorPane> {
        self.v2.as_deref()
    }
    /// [`editor_pane`](Self::editor_pane), mutable.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn editor_pane_mut(&mut self) -> Option<&mut crate::app::editor_v2::EditorPane> {
        self.v2.as_deref_mut()
    }
    /// The header buttons this window shows: [History] (the editor edits in
    /// place and saves by itself).
    pub(crate) fn buttons(&self, bounds: Rect) -> Vec<(Rect, DocBtn)> {
        doc_buttons(bounds)
    }
    /// True when closing the window now would lose typed text (lot 0.3), body
    /// or title: the editor holds edits not yet saved. Any path that would
    /// discard the window must not lose them (E4).
    pub(crate) fn has_unsaved_edits(&self) -> bool {
        self.v2.as_ref().is_some_and(|p| p.has_unsaved())
    }
}

/// Doc-window header buttons, right-aligned before the × close: [History].
/// Shared by draw + click hit-testing.
pub(crate) fn doc_buttons(bounds: Rect) -> Vec<(Rect, DocBtn)> {
    let (header, close, _body) = window_chrome(bounds);
    let bh = 22.0;
    let by0 = header.y0 + (header.height() - bh) * 0.5;
    // Buttons are placed right→left from the × close, so the slice reads in that
    // order.
    let specs: &[(f64, DocBtn)] = &[(66.0, DocBtn::History)];
    let mut x = close.x0 - 8.0;
    let mut out = Vec::new();
    for &(w, b) in specs {
        out.push((Rect::new(x - w, by0, x, by0 + bh), b));
        x -= w + 6.0;
    }
    out
}

/// Shared chrome for a floating window: fills the rounded panel bg over `bounds`,
/// pushes the rounded clip layer (caller must `pop_layer`), paints the header bar
/// with a 2px provenance/category accent, and draws the × close button. Returns
/// the panel rounded rect + the chrome rects so the caller can finish drawing.
/// NOTE: no full-screen dim backdrop — windows float over the live canvas.
fn draw_window_chrome(scene: &mut Scene, bounds: Rect, accent: Color) -> (RoundedRect, Rect, Rect, Rect) {
    let (header, close, body) = window_chrome(bounds);

    let panel_rr = RoundedRect::from_rect(bounds, 11.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().panel, None, &panel_rr);
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &panel_rr);

    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().header, None, &header);
    scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &Rect::new(header.x0, header.y1 - 2.0, header.x1, header.y1));

    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_alt, None, &RoundedRect::from_rect(close, 6.0));
    let m = 9.0;
    let xc = pal().text_body;
    scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, xc, None, &Line::new(Point::new(close.x0 + m, close.y0 + m), Point::new(close.x1 - m, close.y1 - m)));
    scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, xc, None, &Line::new(Point::new(close.x1 - m, close.y0 + m), Point::new(close.x0 + m, close.y1 - m)));

    (panel_rr, header, close, body)
}

pub(crate) fn draw_doc_window(scene: &mut Scene, win: &DocWindow, bounds: Rect) {
    // Title bar provenance accent (owned = trust green, external = warm — the
    // same owned/external language as the canvas parallelogram cue).
    let accent = if win.external { Color::from_rgb8(190, 120, 70) } else { Color::from_rgb8(90, 150, 110) };
    let (panel_rr, header, close, _body) = draw_window_chrome(scene, bounds, accent);

    // Header buttons (History), right-aligned before the ×.
    let buttons = win.buttons(bounds);
    for (r, b) in &buttons {
        let (lbl, fg, bg) = match b {
            DocBtn::History => (&win.lbl_history, Color::from_rgb8(220, 210, 188), Color::from_rgb8(54, 50, 42)),
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 6.0));
        let tx = r.x0 + (r.width() - lbl.width() as f64) * 0.5;
        let ty = r.y0 + (r.height() - lbl.height() as f64) * 0.5;
        draw_text(scene, lbl, Affine::translate((tx, ty)), fg);
    }

    // Title text, on one line, left of the leftmost header button (or the ×
    // close); too long, it is cut and ends with an ellipsis (T8).
    let title_right = buttons.iter().map(|(r, _)| r.x0).fold(close.x0, f64::min) - 8.0;
    let tx = bounds.x0 + DW_PAD;
    let ty = header.y0 + ((header.height() - win.title.height() as f64) * 0.5).max(0.0);
    let cut = tx + win.title.width() as f64 > title_right;
    let text_right = if cut { title_right - win.lbl_ellipsis.width() as f64 } else { title_right };
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(header.x0, header.y0, text_right, header.y1));
    draw_text(scene, &win.title, Affine::translate((tx, ty)), pal().text);
    scene.pop_layer();
    if cut && text_right > tx {
        draw_text(scene, &win.lbl_ellipsis, Affine::translate((text_right, ty)), pal().text);
    }

    // The body is the editor's, drawn by the app over this chrome
    // (`App::v2_draw`).
    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Contact-first inbox -------------------------------------------------
// Sovereign is contacts-centric: the inbox is a list of CONTACTS (sorted by
// unread). Opening a contact shows their addresses, a tab per conversation
// (one per channel — email, signal, …) and the unified message thread. This
// mirrors the Tauri InboxPanel + ContactPanel.

pub(crate) const IB_ROW_H: f64 = 60.0; // contact row height
pub(crate) const IB_MSG_GAP: f64 = 12.0;
pub(crate) const IB_ADDR_H: f64 = 22.0; // address line height
pub(crate) const IB_TAB_H: f64 = 32.0; // conversation tab strip
pub(crate) const IB_AVATAR_R: f64 = 18.0;
const IB_BUBBLE_PAD: f64 = 9.0;
const IB_BUBBLE_FRAC: f64 = 0.82; // bubble max width as a fraction of the thread

pub(crate) fn short_id(s: &str) -> String {
    s.rsplit(':').next().unwrap_or("?").chars().take(6).collect()
}

pub(crate) struct InboxContact {
    pub(crate) name: String,
    pub(crate) initial: char,
    pub(crate) color: Color,                       // avatar hue (per-contact, stable)
    pub(crate) channels: String,                   // "email · signal"
    pub(crate) unread: u32,
    pub(crate) addresses: Vec<(String, String)>,   // (channel, address)
    pub(crate) conv_indices: Vec<usize>,           // into Inbox.convs
}
pub(crate) struct InboxConv {
    pub(crate) tab_label: String,
    pub(crate) unread: u32,
    pub(crate) msg_indices: Vec<usize>, // into Inbox.msgs, chronological
}
pub(crate) struct MsgRow {
    pub(crate) sender: String,
    pub(crate) subject: Option<String>,
    pub(crate) body: String,
    pub(crate) time: String,
    pub(crate) outbound: bool,
}

struct ContactShaped {
    name: Layout<Brush>,
    channels: Layout<Brush>,
    initial: Layout<Brush>,        // avatar letter (white, drawn centered)
    badge: Option<Layout<Brush>>,  // unread count (when > 0)
}
struct AddrShaped {
    channel: Layout<Brush>,
    address: Layout<Brush>,
}
struct MsgShaped {
    head: Layout<Brush>,
    subject: Option<Layout<Brush>>,
    body: Layout<Brush>,
    outbound: bool,
    width: f64,  // bubble width
    height: f64, // full bubble height
}

pub(crate) struct Inbox {
    pub(crate) contacts: Vec<InboxContact>,
    pub(crate) convs: Vec<InboxConv>,
    pub(crate) msgs: Vec<MsgRow>,
    pub(crate) list_scroll: f64,
    pub(crate) thread_scroll: f64,
    pub(crate) selected: Option<usize>, // a contact -> detail view
    pub(crate) active_conv: usize,      // index into the selected contact's conv_indices
    pub(crate) inner_w: f64,
    shaped_list: Vec<ContactShaped>,
    shaped_addrs: Vec<AddrShaped>,
    shaped_tabs: Vec<Layout<Brush>>,
    shaped_thread: Vec<MsgShaped>,
    detail_key: Option<(usize, usize)>, // (contact, active_conv) the detail caches reflect
    label_inbox: Option<Layout<Brush>>,
    label_back: Option<Layout<Brush>>,
    empty_label: Option<Layout<Brush>>,
    empty_thread: Option<Layout<Brush>>,
}
impl Inbox {
    /// The conversation index (into `convs`) the active tab points at, if any.
    fn active_conv_idx(&self, sel: usize) -> Option<usize> {
        self.contacts.get(sel).and_then(|c| c.conv_indices.get(self.active_conv).copied())
    }

    /// Readable lines for the accessibility tree (current view).
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(sel) = self.selected {
            if let Some(c) = self.contacts.get(sel) {
                out.push(c.name.clone());
                for (ch, addr) in &c.addresses {
                    out.push(format!("{ch}: {addr}"));
                }
            }
            if let Some(ci) = self.selected.and_then(|s| self.active_conv_idx(s)) {
                for &mi in &self.convs[ci].msg_indices {
                    let m = &self.msgs[mi];
                    out.push(format!("{} at {}: {}", m.sender, m.time, m.body));
                }
            }
            if out.len() <= 1 {
                out.push("No messages".into());
            }
        } else if self.contacts.is_empty() {
            out.push("No contacts yet".into());
        } else {
            for c in &self.contacts {
                let unread = if c.unread > 0 { format!(" \u{2014} {} unread", c.unread) } else { String::new() };
                out.push(format!("{} \u{2014} {}{}", c.name, c.channels, unread));
            }
        }
        out
    }

    pub(crate) fn ensure_shaped_list(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        if self.label_inbox.is_none() {
            self.label_inbox = Some(shaper.shape("Inbox", 400.0, 18.0));
            self.label_back = Some(shaper.shape("\u{2039} Contacts", 200.0, 14.0));
            self.empty_label = Some(shaper.shape("No contacts yet.", 400.0, 14.0));
            self.empty_thread = Some(shaper.shape("No messages.", 400.0, 13.0));
        }
        if self.shaped_list.len() == self.contacts.len() && (inner_w - self.inner_w).abs() < 0.5 {
            return;
        }
        self.inner_w = inner_w;
        let name_w = (inner_w - 2.0 * IB_AVATAR_R - 28.0 - 44.0) as f32; // leave room for the badge
        self.shaped_list = self
            .contacts
            .iter()
            .map(|c| ContactShaped {
                name: shaper.shape(&c.name, name_w, 14.5),
                channels: shaper.shape(&c.channels, name_w, 12.0),
                initial: shaper.shape(&c.initial.to_string(), 40.0, 15.0),
                badge: (c.unread > 0).then(|| shaper.shape(&c.unread.to_string(), 40.0, 11.5)),
            })
            .collect();
        self.detail_key = None; // width changed -> detail re-shapes too
    }

    /// (Re)shape the selected contact's addresses, conversation tabs, and the
    /// active conversation's message thread as right/left bubbles.
    pub(crate) fn ensure_shaped_detail(&mut self, shaper: &mut TextShaper, body_w: f64, sel: usize) {
        if self.detail_key == Some((sel, self.active_conv)) && (body_w - self.inner_w).abs() < 0.5 {
            return;
        }
        let Some(c) = self.contacts.get(sel) else { return };
        self.shaped_addrs = c
            .addresses
            .iter()
            .map(|(ch, addr)| AddrShaped {
                channel: shaper.shape(ch, 80.0, 12.0),
                address: shaper.shape(addr, (body_w - 90.0) as f32, 12.0),
            })
            .collect();
        self.shaped_tabs = c
            .conv_indices
            .iter()
            .map(|&ci| {
                let conv = &self.convs[ci];
                let label = if conv.unread > 0 {
                    format!("{}  \u{00b7}  {}", conv.tab_label, conv.unread)
                } else {
                    conv.tab_label.clone()
                };
                shaper.shape(&label, 160.0, 13.0)
            })
            .collect();

        let bubble_w = (body_w * IB_BUBBLE_FRAC).max(120.0);
        let text_w = (bubble_w - 2.0 * IB_BUBBLE_PAD) as f32;
        let msg_idxs: Vec<usize> = self
            .active_conv_idx(sel)
            .map(|ci| self.convs[ci].msg_indices.clone())
            .unwrap_or_default();
        self.shaped_thread = msg_idxs
            .iter()
            .map(|&i| {
                let m = &self.msgs[i];
                let head = shaper.shape(&format!("{}   \u{00b7}   {}", m.sender, m.time), text_w, 11.5);
                let subject = m.subject.as_ref().map(|s| shaper.shape(s, text_w, 12.5));
                let body = shaper.shape(&m.body, text_w, 13.5);
                let mut height = 2.0 * IB_BUBBLE_PAD + head.height() as f64 + 3.0 + body.height() as f64;
                if let Some(s) = &subject {
                    height += s.height() as f64 + 3.0;
                }
                MsgShaped { head, subject, body, outbound: m.outbound, width: bubble_w, height }
            })
            .collect();
        self.detail_key = Some((sel, self.active_conv));
    }

    pub(crate) fn thread_content_h(&self) -> f64 {
        self.shaped_thread.iter().map(|m| m.height + IB_MSG_GAP).sum::<f64>() + 8.0
    }
}

/// Vertical regions of the contact-detail body: addresses block, conversation
/// tab strip (zero-height when ≤1 conversation), and the scrolling thread.
/// Shared by the painter + the click hit-test so they never drift.
pub(crate) fn inbox_detail_regions(body: Rect, n_addr: usize, n_conv: usize) -> (Rect, Rect, Rect) {
    let addr_h = if n_addr > 0 { 8.0 + n_addr as f64 * IB_ADDR_H + 6.0 } else { 4.0 };
    let addrs = Rect::new(body.x0, body.y0, body.x1, body.y0 + addr_h);
    let tab_h = if n_conv > 1 { IB_TAB_H } else { 0.0 };
    let tabs = Rect::new(body.x0, addrs.y1, body.x1, addrs.y1 + tab_h);
    let thread = Rect::new(body.x0, tabs.y1, body.x1, body.y1);
    (addrs, tabs, thread)
}

/// Per-conversation tab rects (equal width across the strip).
pub(crate) fn inbox_tab_rects(tabs: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let tw = tabs.width() / n as f64;
    (0..n).map(|i| Rect::new(tabs.x0 + i as f64 * tw, tabs.y0, tabs.x0 + (i as f64 + 1.0) * tw, tabs.y1)).collect()
}

/// Load contacts + their conversations + messages from sovereign-db IN-PROCESS,
/// grouped contact-first. Plaintext fields shown; encrypted ones get a lock
/// placeholder. Always returns Some (an empty workspace opens the empty state).
pub(crate) async fn load_inbox(db: &Arc<dyn GraphDB>) -> Option<Inbox> {
    {
        let convs_raw = db.list_conversations(None).await.unwrap_or_default();
        let msgs_raw = db.list_all_messages().await.unwrap_or_default();
        let contacts_raw = db.list_contacts().await.unwrap_or_default();

        let decrypt_name = |name: &str, nonce: &Option<String>| -> Option<String> {
            if nonce.is_none() && !name.is_empty() {
                Some(name.to_string())
            } else if nonce.is_some() {
                None // encrypted (raw/no-auth) -> caller substitutes a placeholder
            } else {
                None
            }
        };

        let mut name_of: HashMap<String, String> = HashMap::new();
        for c in &contacts_raw {
            if let Some(id) = &c.id {
                let n = decrypt_name(&c.name, &c.name_nonce).unwrap_or_else(|| "\u{1f512}".to_string());
                name_of.insert(id.to_string(), n);
            }
        }

        // Messages -> rows, grouped by conversation (sorted chronological).
        let mut msgs: Vec<MsgRow> = Vec::with_capacity(msgs_raw.len());
        let mut by_conv: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, m) in msgs_raw.iter().enumerate() {
            let outbound = matches!(m.direction, MessageDirection::Outbound);
            let sender = if outbound {
                "You".to_string()
            } else {
                name_of.get(&m.from_contact_id).cloned().unwrap_or_else(|| short_id(&m.from_contact_id))
            };
            let body = if m.body_nonce.is_none() && !m.body.is_empty() {
                m.body.clone()
            } else if !m.body.is_empty() {
                "\u{1f512} (encrypted message)".to_string()
            } else {
                String::new()
            };
            let subject = m.subject.as_ref().filter(|s| !s.is_empty()).map(|s| {
                if m.subject_nonce.is_none() {
                    s.clone()
                } else {
                    "\u{1f512}".to_string()
                }
            });
            let time = m.sent_at.format("%b %-d, %H:%M").to_string();
            msgs.push(MsgRow { sender, subject, body, time, outbound });
            by_conv.entry(m.conversation_id.clone()).or_default().push(i);
        }

        // Conversations (index = position in convs_raw; conv_index map by id).
        let mut convs: Vec<InboxConv> = Vec::with_capacity(convs_raw.len());
        let mut conv_index_of: HashMap<String, usize> = HashMap::new();
        for c in &convs_raw {
            let cid = c.id.as_ref().map(|t| t.to_string()).unwrap_or_default();
            let mut idxs = by_conv.remove(&cid).unwrap_or_default();
            idxs.sort_by_key(|&i| msgs_raw[i].sent_at);
            let tab_label = if c.title_nonce.is_none() && !c.title.is_empty() {
                c.title.clone()
            } else if c.title_nonce.is_some() {
                c.channel.to_string()
            } else {
                c.channel.to_string()
            };
            conv_index_of.insert(cid, convs.len());
            convs.push(InboxConv { tab_label, unread: c.unread_count, msg_indices: idxs });
        }

        // Contacts (skip the owner/self identity) -> their conversations.
        let mut contacts: Vec<InboxContact> = Vec::new();
        for c in &contacts_raw {
            if c.is_owned {
                continue; // your own identity isn't a correspondent
            }
            let id = c.id.as_ref().map(|t| t.to_string()).unwrap_or_default();
            let name = decrypt_name(&c.name, &c.name_nonce)
                .unwrap_or_else(|| format!("\u{1f512} {}", short_id(&id)));
            let name_ok = c.name_nonce.is_none() && !c.name.is_empty();
            let initial = if name_ok {
                name.chars().find(|ch| ch.is_alphanumeric()).unwrap_or('?').to_ascii_uppercase()
            } else {
                '?'
            };
            let color = lane_color(crate::app::avatar_hue_index(&id));

            let mut conv_indices: Vec<usize> = Vec::new();
            let mut unread = 0u32;
            let mut chans: Vec<String> = Vec::new();
            for craw in &convs_raw {
                if craw.participant_contact_ids.iter().any(|p| p == &id) {
                    let cid = craw.id.as_ref().map(|t| t.to_string()).unwrap_or_default();
                    if let Some(&ci) = conv_index_of.get(&cid) {
                        conv_indices.push(ci);
                        unread += craw.unread_count;
                        let ch = craw.channel.to_string();
                        if !chans.contains(&ch) {
                            chans.push(ch);
                        }
                    }
                }
            }
            // Addresses (plaintext only — raw/no-auth can't decrypt them).
            let addresses: Vec<(String, String)> = if c.addresses_nonce.is_none() {
                c.addresses.iter().map(|a| (a.channel.to_string(), a.address.clone())).collect()
            } else {
                Vec::new()
            };
            if chans.is_empty() {
                for (ch, _) in &addresses {
                    if !chans.contains(ch) {
                        chans.push(ch.clone());
                    }
                }
            }
            contacts.push(InboxContact {
                name,
                initial,
                color,
                channels: chans.join("  \u{00b7}  "),
                unread,
                addresses,
                conv_indices,
            });
        }
        // Contact-first ordering: unread first, then alphabetical.
        contacts.sort_by(|a, b| b.unread.cmp(&a.unread).then_with(|| a.name.cmp(&b.name)));

        Some(Inbox {
            contacts,
            convs,
            msgs,
            list_scroll: 0.0,
            thread_scroll: 0.0,
            selected: None,
            active_conv: 0,
            inner_w: 0.0,
            shaped_list: Vec::new(),
            shaped_addrs: Vec::new(),
            shaped_tabs: Vec::new(),
            shaped_thread: Vec::new(),
            detail_key: None,
            label_inbox: None,
            label_back: None,
            empty_label: None,
            empty_thread: None,
        })
    }
}

/// Demo contact-first inbox for the no-auth bypass (mirrors the app seed) so the
/// inbox UI is exercisable without a logged-in workspace — the canvas already
/// has an equivalent synthetic fallback.
pub(crate) fn synthetic_inbox() -> Inbox {
    fn m(sender: &str, subject: Option<&str>, body: &str, time: &str, outbound: bool) -> MsgRow {
        MsgRow { sender: sender.into(), subject: subject.map(|s| s.into()), body: body.into(), time: time.into(), outbound }
    }
    let color = |s: &str| lane_color(crate::app::avatar_hue_index(s));
    let msgs = vec![
        // 0..3 — Alice, email
        m("Alice Chen", Some("Architecture discussion"), "I reviewed the architecture doc — the component separation looks solid. Should we keep the DB abstraction as a trait or move to concrete types?", "Jun 16, 09:12", false),
        m("You", None, "Let's keep the trait — it lets us swap SurrealDB for SQLite later, and the mock is useful for tests.", "Jun 16, 09:27", true),
        m("Alice Chen", None, "Makes sense. I'll update the API spec to reference the trait methods.", "Jun 16, 09:41", false),
        // 3..5 — Alice, signal
        m("Alice Chen", None, "Tested the dark theme on my display — contrast ratios look good, WCAG AA compliant.", "Jun 17, 14:02", false),
        m("You", None, "Great! Want to mock up the light-theme colors this week?", "Jun 17, 14:10", true),
        // 5..7 — Bob, whatsapp
        m("Bob Martinez", None, "The $500 infra budget seems low — are we accounting for CI/CD costs?", "Jun 15, 11:00", false),
        m("You", None, "Good point. We self-host CI on the NAS for now, but I'll add a cloud-CI contingency line.", "Jun 15, 11:20", true),
        // 7..9 — Carol, sms
        m("Carol Nguyen", None, "Hey, are we still meeting Thursday?", "Jun 14, 16:30", false),
        m("You", None, "Yes! 2pm at the usual spot.", "Jun 14, 16:35", true),
        // 9 — David, signal
        m("David Park", None, "Sent over the gesture prototypes — let me know what you think.", "Jun 13, 10:05", false),
    ];
    let convs = vec![
        InboxConv { tab_label: "email".into(), unread: 2, msg_indices: vec![0, 1, 2] },
        InboxConv { tab_label: "signal".into(), unread: 1, msg_indices: vec![3, 4] },
        InboxConv { tab_label: "whatsapp".into(), unread: 1, msg_indices: vec![5, 6] },
        InboxConv { tab_label: "sms".into(), unread: 0, msg_indices: vec![7, 8] },
        InboxConv { tab_label: "signal".into(), unread: 2, msg_indices: vec![9] },
    ];
    let mut contacts = vec![
        InboxContact { name: "Alice Chen".into(), initial: 'A', color: color("alice"), channels: "email  \u{00b7}  signal".into(), unread: 3, addresses: vec![("email".into(), "alice.chen@example.com".into()), ("signal".into(), "+1-555-0101".into())], conv_indices: vec![0, 1] },
        InboxContact { name: "Bob Martinez".into(), initial: 'B', color: color("bob"), channels: "whatsapp".into(), unread: 1, addresses: vec![("email".into(), "bob.m@example.com".into()), ("whatsapp".into(), "+1-555-0102".into())], conv_indices: vec![2] },
        InboxContact { name: "Carol Nguyen".into(), initial: 'C', color: color("carol"), channels: "sms".into(), unread: 0, addresses: vec![("email".into(), "carol.n@example.com".into()), ("sms".into(), "+1-555-0103".into())], conv_indices: vec![3] },
        InboxContact { name: "David Park".into(), initial: 'D', color: color("david"), channels: "signal".into(), unread: 2, addresses: vec![("signal".into(), "+1-555-0104".into())], conv_indices: vec![4] },
    ];
    contacts.sort_by(|a, b| b.unread.cmp(&a.unread).then_with(|| a.name.cmp(&b.name)));
    Inbox {
        contacts,
        convs,
        msgs,
        list_scroll: 0.0,
        thread_scroll: 0.0,
        selected: None,
        active_conv: 0,
        inner_w: 0.0,
        shaped_list: Vec::new(),
        shaped_addrs: Vec::new(),
        shaped_tabs: Vec::new(),
        shaped_thread: Vec::new(),
        detail_key: None,
        label_inbox: None,
        label_back: None,
        empty_label: None,
        empty_thread: None,
    }
}

pub(crate) fn draw_inbox(scene: &mut Scene, ib: &Inbox, bounds: Rect) {
    let panel = bounds;
    let (panel_rr, header, close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(90, 120, 160));

    let title_color = pal().text;
    let dim = pal().text_dim;
    let faint = pal().text_faint;
    let accent = pal().accent;

    match ib.selected {
        None => {
            if let Some(l) = &ib.label_inbox {
                draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 13.0)), title_color);
            }
            scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);
            for (i, cs) in ib.shaped_list.iter().enumerate() {
                let ry = body_vp.y0 - ib.list_scroll + i as f64 * IB_ROW_H;
                if ry + IB_ROW_H < body_vp.y0 || ry > body_vp.y1 {
                    continue;
                }
                let contact = &ib.contacts[i];
                // Avatar disc + centered initial.
                let cx = body_vp.x0 + 12.0 + IB_AVATAR_R;
                let cy = ry + IB_ROW_H * 0.5;
                scene.fill(Fill::NonZero, Affine::IDENTITY, contact.color, None, &Circle::new(Point::new(cx, cy), IB_AVATAR_R));
                let iw = cs.initial.width() as f64;
                let ih = cs.initial.height() as f64;
                draw_text(scene, &cs.initial, Affine::translate((cx - iw * 0.5, cy - ih * 0.5)), pal().on_accent);
                // Name + channels.
                let tx = cx + IB_AVATAR_R + 12.0;
                draw_text(scene, &cs.name, Affine::translate((tx, ry + 12.0)), title_color);
                draw_text(scene, &cs.channels, Affine::translate((tx, ry + 33.0)), dim);
                // Unread badge (right-aligned red pill).
                if let Some(badge) = &cs.badge {
                    let bw = badge.width() as f64;
                    let pill_w = (bw + 14.0).max(20.0);
                    let x1 = body_vp.x1 - 12.0;
                    let pill = RoundedRect::new(x1 - pill_w, cy - 9.0, x1, cy + 9.0, 9.0);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(0xEF, 0x44, 0x44), None, &pill);
                    draw_text(scene, badge, Affine::translate((x1 - pill_w + (pill_w - bw) * 0.5, cy - badge.height() as f64 * 0.5)), Color::WHITE);
                }
                scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(body_vp.x0, ry + IB_ROW_H - 1.0, body_vp.x1, ry + IB_ROW_H));
            }
            if ib.contacts.is_empty() {
                if let Some(el) = &ib.empty_label {
                    draw_text(scene, el, Affine::translate((body_vp.x0 + 16.0, body_vp.y0 + 14.0)), dim);
                }
            }
            scene.pop_layer();
            draw_scrollbar(scene, &panel, &body_vp, ib.contacts.len() as f64 * IB_ROW_H, ib.list_scroll);
        }
        Some(sel) => {
            if let Some(l) = &ib.label_back {
                draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 16.0)), accent);
            }
            // Contact name in the header (after the Back link).
            if let Some(cs) = ib.shaped_list.get(sel) {
                let nx = header.x0 + 120.0;
                scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(nx, header.y0, close.x0 - 8.0, header.y1));
                draw_text(scene, &cs.name, Affine::translate((nx, header.y0 + 15.0)), title_color);
                scene.pop_layer();
            }
            let n_conv = ib.contacts.get(sel).map(|c| c.conv_indices.len()).unwrap_or(0);
            let (addrs_r, tabs_r, thread_r) = inbox_detail_regions(body_vp, ib.shaped_addrs.len(), n_conv);

            // Addresses block.
            scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &addrs_r);
            for (k, a) in ib.shaped_addrs.iter().enumerate() {
                let ay = addrs_r.y0 + 8.0 + k as f64 * IB_ADDR_H;
                draw_text(scene, &a.channel, Affine::translate((addrs_r.x0 + 4.0, ay)), faint);
                draw_text(scene, &a.address, Affine::translate((addrs_r.x0 + 84.0, ay)), dim);
            }
            scene.pop_layer();
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(addrs_r.x0, addrs_r.y1 - 1.0, addrs_r.x1, addrs_r.y1));

            // Conversation tabs (only when >1).
            if n_conv > 1 {
                let rects = inbox_tab_rects(tabs_r, n_conv);
                for (k, r) in rects.iter().enumerate() {
                    let active = k == ib.active_conv;
                    if active {
                        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_hover, None, r);
                        scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &Rect::new(r.x0, r.y1 - 2.0, r.x1, r.y1));
                    }
                    if let Some(lay) = ib.shaped_tabs.get(k) {
                        let tw = lay.width() as f64;
                        let tx = r.x0 + (r.width() - tw) * 0.5;
                        let col = if active { title_color } else { dim };
                        draw_text(scene, lay, Affine::translate((tx.max(r.x0 + 6.0), r.y0 + 8.0)), col);
                    }
                }
            }

            // Message thread (bubbles: outbound right + owned tint, inbound left).
            scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &thread_r);
            let mut y = thread_r.y0 + 6.0 - ib.thread_scroll;
            for ms in &ib.shaped_thread {
                if y + ms.height >= thread_r.y0 && y <= thread_r.y1 {
                    let x0 = if ms.outbound {
                        thread_r.x1 - ms.width - 12.0
                    } else {
                        thread_r.x0 + 12.0
                    };
                    let rr = RoundedRect::new(x0, y, x0 + ms.width, y + ms.height, 8.0);
                    let bg = if ms.outbound { pal().card } else { pal().surface };
                    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &rr);
                    let tx = x0 + IB_BUBBLE_PAD;
                    let mut ty = y + IB_BUBBLE_PAD;
                    draw_text(scene, &ms.head, Affine::translate((tx, ty)), faint);
                    ty += ms.head.height() as f64 + 3.0;
                    if let Some(s) = &ms.subject {
                        draw_text(scene, s, Affine::translate((tx, ty)), title_color);
                        ty += s.height() as f64 + 3.0;
                    }
                    draw_text(scene, &ms.body, Affine::translate((tx, ty)), pal().text_body);
                }
                y += ms.height + IB_MSG_GAP;
            }
            if ib.shaped_thread.is_empty() {
                if let Some(el) = &ib.empty_thread {
                    draw_text(scene, el, Affine::translate((thread_r.x0 + 12.0, thread_r.y0 + 12.0)), dim);
                }
            }
            scene.pop_layer();
            draw_scrollbar(scene, &panel, &thread_r, ib.thread_content_h(), ib.thread_scroll);
        }
    }

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Document history (version commits) ---------------------------------
// `list_document_commits` returns encrypted snapshots — only the message +
// timestamp are plaintext, so the history lists versions by when/message and
// offers Restore (which decrypts + MAC-verifies in the DB layer). Restore is
// two-step: select a version, then confirm via the Restore button.

pub(crate) const HIST_ROW_H: f64 = 50.0;
pub(crate) const HIST_ACTION_H: f64 = 48.0; // bottom restore bar (when a row is selected)

/// Split a history body into (list viewport, optional restore-bar rect).
pub(crate) fn hist_split(body: Rect, has_selection: bool) -> (Rect, Option<Rect>) {
    if has_selection {
        let bar = Rect::new(body.x0, body.y1 - HIST_ACTION_H, body.x1, body.y1);
        let list = Rect::new(body.x0, body.y0, body.x1, body.y1 - HIST_ACTION_H - 6.0);
        (list, Some(bar))
    } else {
        (body, None)
    }
}

/// The Restore button rect inside the action bar.
pub(crate) fn hist_restore_rect(bar: Rect) -> Rect {
    let w = 200.0_f64.min(bar.width() - 24.0);
    let x0 = bar.x0 + (bar.width() - w) * 0.5;
    let y0 = bar.y0 + (bar.height() - 30.0) * 0.5;
    Rect::new(x0, y0, x0 + w, y0 + 30.0)
}

pub(crate) struct CommitRow {
    pub(crate) commit_id: String,
    pub(crate) when: String,
    pub(crate) message: String,
}

pub(crate) struct HistoryPanel {
    pub(crate) doc_id: String,
    pub(crate) doc_title: String,
    pub(crate) commits: Vec<CommitRow>,
    pub(crate) scroll: f64,
    pub(crate) selected: Option<usize>,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    restore_lbl: Option<Layout<Brush>>,
    empty_lbl: Option<Layout<Brush>>,
    rows: Vec<(Layout<Brush>, Layout<Brush>)>, // (message, when)
}
impl HistoryPanel {
    /// Readable lines for the accessibility tree.
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        if self.commits.is_empty() {
            return vec!["No saved versions yet".into()];
        }
        self.commits.iter().map(|c| format!("{} \u{2014} {}", c.when, c.message)).collect()
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() || width_changed {
            self.title_lbl = Some(shaper.shape(&format!("History \u{2014} {}", self.doc_title), 4000.0, 16.0));
            self.restore_lbl = Some(shaper.shape("Use this version", 300.0, 13.0));
            self.empty_lbl = Some(shaper.shape("No saved versions yet \u{2014} edits you save appear here.", 600.0, 14.0));
        }
        if self.rows.len() != self.commits.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 20.0) as f32;
            self.rows = self
                .commits
                .iter()
                .map(|c| (shaper.shape(&c.message, tw, 14.0), shaper.shape(&c.when, tw, 12.0)))
                .collect();
        }
    }
    pub(crate) fn content_h(&self) -> f64 {
        self.commits.len() as f64 * HIST_ROW_H
    }
}

/// Load a document's version history (most recent first). Always returns a panel
/// (an empty list still opens, with an empty state).
pub(crate) async fn load_history(db: &Arc<dyn GraphDB>, doc_id: &str, doc_title: &str) -> HistoryPanel {
    let raw = db.list_document_commits(doc_id).await.unwrap_or_default();
    let commits = raw
        .iter()
        .map(|c| CommitRow {
            commit_id: c.id.as_ref().map(|t| t.to_string()).unwrap_or_default(),
            when: c.timestamp.format("%Y-%m-%d %H:%M").to_string(),
            message: if c.message.is_empty() { "(no message)".into() } else { c.message.clone() },
        })
        .collect();
    HistoryPanel {
        doc_id: doc_id.to_string(),
        doc_title: doc_title.to_string(),
        commits,
        scroll: 0.0,
        selected: None,
        inner_w: 0.0,
        title_lbl: None,
        restore_lbl: None,
        empty_lbl: None,
        rows: Vec::new(),
    }
}

pub(crate) fn draw_history(scene: &mut Scene, hp: &HistoryPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(150, 130, 90));
    let title_color = pal().text;
    let dim = pal().text_dim;
    let accent = Color::from_rgb8(205, 180, 120);

    if let Some(l) = &hp.title_lbl {
        scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(header.x0, header.y0, _close.x0 - 8.0, header.y1));
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), title_color);
        scene.pop_layer();
    }

    let (list_vp, bar) = hist_split(body_vp, hp.selected.is_some());

    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &list_vp);
    for (i, (msg, when)) in hp.rows.iter().enumerate() {
        let ry = list_vp.y0 - hp.scroll + i as f64 * HIST_ROW_H;
        if ry + HIST_ROW_H < list_vp.y0 || ry > list_vp.y1 {
            continue;
        }
        if hp.selected == Some(i) {
            scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(54, 50, 40), None, &Rect::new(list_vp.x0, ry, list_vp.x1, ry + HIST_ROW_H - 1.0));
            scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &Rect::new(list_vp.x0, ry, list_vp.x0 + 3.0, ry + HIST_ROW_H - 1.0));
        }
        draw_text(scene, msg, Affine::translate((list_vp.x0 + 14.0, ry + 7.0)), title_color);
        draw_text(scene, when, Affine::translate((list_vp.x0 + 14.0, ry + 27.0)), dim);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(list_vp.x0, ry + HIST_ROW_H - 1.0, list_vp.x1, ry + HIST_ROW_H));
    }
    if hp.commits.is_empty() {
        if let Some(el) = &hp.empty_lbl {
            draw_text(scene, el, Affine::translate((list_vp.x0 + 14.0, list_vp.y0 + 14.0)), dim);
        }
    }
    scene.pop_layer();
    draw_scrollbar(scene, &bounds, &list_vp, hp.content_h(), hp.scroll);

    // Restore action bar (only with a selection).
    if let Some(bar) = bar {
        scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(32, 33, 40), None, &bar);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider_strong, None, &Rect::new(bar.x0, bar.y0, bar.x1, bar.y0 + 1.0));
        let btn = hist_restore_rect(bar);
        scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(96, 78, 44), None, &RoundedRect::from_rect(btn, 6.0));
        if let Some(l) = &hp.restore_lbl {
            let tx = btn.x0 + (btn.width() - l.width() as f64) * 0.5;
            let ty = btn.y0 + (btn.height() - l.height() as f64) * 0.5;
            draw_text(scene, l, Affine::translate((tx, ty)), Color::from_rgb8(240, 232, 214));
        }
    }

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Models + trust (AI control panel) ----------------------------------

pub(crate) const MODEL_ROW_H: f64 = 52.0;
pub(crate) const TRUST_ROW_H: f64 = 36.0;

/// A per-model-row action button.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ModelBtn {
    Router,
    Reasoning,
    Delete,
}

/// Per-model-row buttons (right-aligned): [R] [Q] [Del].
pub(crate) fn model_row_buttons(row: Rect) -> [(Rect, ModelBtn); 3] {
    let bh = 22.0;
    let by0 = row.y0 + (row.height() - bh) * 0.5;
    let (del_w, rq_w, gap) = (42.0, 26.0, 6.0);
    let mut x = row.x1 - 10.0;
    let del = Rect::new(x - del_w, by0, x, by0 + bh);
    x -= del_w + gap;
    let q = Rect::new(x - rq_w, by0, x, by0 + bh);
    x -= rq_w + gap;
    let r = Rect::new(x - rq_w, by0, x, by0 + bh);
    [(r, ModelBtn::Router), (q, ModelBtn::Reasoning), (del, ModelBtn::Delete)]
}

/// Raw model-scan row (filename + size + current role assignment).
pub(crate) struct ModelRow {
    pub(crate) filename: String,
    pub(crate) size_mb: f64,
    pub(crate) is_router: bool,
    pub(crate) is_reasoning: bool,
}

/// A learned-trust entry for display.
pub(crate) struct TrustRow {
    pub(crate) action: String,
    pub(crate) approvals: u32,
    pub(crate) auto: bool,
}

pub(crate) struct ModelsPanel {
    pub(crate) models: Vec<ModelRow>,
    pub(crate) trust: Vec<TrustRow>,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    models_hdr: Option<Layout<Brush>>,
    trust_hdr: Option<Layout<Brush>>,
    empty_models: Option<Layout<Brush>>,
    empty_trust: Option<Layout<Brush>>,
    r_lbl: Option<Layout<Brush>>,
    q_lbl: Option<Layout<Brush>>,
    del_lbl: Option<Layout<Brush>>,
    rows: Vec<(Layout<Brush>, Layout<Brush>)>, // (filename, "1234 MB · ROUTER")
    trust_rows: Vec<Layout<Brush>>,
}
impl ModelsPanel {
    pub(crate) fn new(models: Vec<ModelRow>, trust: Vec<TrustRow>) -> Self {
        Self {
            models,
            trust,
            scroll: 0.0,
            inner_w: 0.0,
            title_lbl: None,
            models_hdr: None,
            trust_hdr: None,
            empty_models: None,
            empty_trust: None,
            r_lbl: None,
            q_lbl: None,
            del_lbl: None,
            rows: Vec::new(),
            trust_rows: Vec::new(),
        }
    }

    /// Readable lines for the accessibility tree.
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        let mut out = vec!["Models".to_string()];
        for m in &self.models {
            let role = if m.is_router {
                " (router)"
            } else if m.is_reasoning {
                " (reasoning)"
            } else {
                ""
            };
            out.push(format!("{} \u{2014} {:.0} MB{role}", m.filename, m.size_mb));
        }
        out.push("Learned trust".to_string());
        for t in &self.trust {
            out.push(format!("{}: {} approvals{}", t.action, t.approvals, if t.auto { ", auto-approved" } else { "" }));
        }
        out
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() {
            self.title_lbl = Some(shaper.shape("Models & trust", 4000.0, 16.0));
            self.models_hdr = Some(shaper.shape("LOCAL MODELS", 400.0, 11.5));
            self.trust_hdr = Some(shaper.shape("LEARNED TRUST", 400.0, 11.5));
            self.empty_models = Some(shaper.shape("No .gguf models found in the model directory.", 600.0, 13.5));
            self.empty_trust = Some(shaper.shape("No learned trust yet — approve AI actions to build it.", 600.0, 13.0));
            self.r_lbl = Some(shaper.shape("R", 40.0, 12.5));
            self.q_lbl = Some(shaper.shape("Q", 40.0, 12.5));
            self.del_lbl = Some(shaper.shape("Del", 60.0, 12.0));
        }
        if self.rows.len() != self.models.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 130.0).max(80.0) as f32;
            self.rows = self
                .models
                .iter()
                .map(|m| {
                    let role = if m.is_router {
                        "  \u{00b7}  ROUTER"
                    } else if m.is_reasoning {
                        "  \u{00b7}  REASON"
                    } else {
                        ""
                    };
                    (
                        shaper.shape(&m.filename, tw, 13.5),
                        shaper.shape(&format!("{:.0} MB{role}", m.size_mb), tw, 11.5),
                    )
                })
                .collect();
        }
        if self.trust_rows.len() != self.trust.len() || width_changed {
            self.trust_rows = self
                .trust
                .iter()
                .map(|t| {
                    let auto = if t.auto { "  \u{00b7}  auto-approved" } else { "" };
                    shaper.shape(&format!("{}   \u{2014}   {} approvals{auto}", t.action, t.approvals), (inner_w - 20.0) as f32, 13.0)
                })
                .collect();
        }
    }

    pub(crate) fn content_h(&self) -> f64 {
        28.0 + self.models.len().max(1) as f64 * MODEL_ROW_H
            + 34.0 + self.trust.len().max(1) as f64 * TRUST_ROW_H
            + 16.0
    }

}

pub(crate) fn draw_models(scene: &mut Scene, mp: &ModelsPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(120, 130, 165));
    let title_c = pal().text;
    let dim = pal().text_dim;
    let hdr_c = Color::from_rgb8(140, 150, 175);
    let router_c = Color::from_rgb8(120, 175, 135);
    let reason_c = Color::from_rgb8(150, 160, 215);

    if let Some(l) = &mp.title_lbl {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), title_c);
    }

    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);
    let mut y = body_vp.y0 - mp.scroll;

    // Models section.
    if let Some(h) = &mp.models_hdr {
        draw_text(scene, h, Affine::translate((body_vp.x0 + 4.0, y + 6.0)), hdr_c);
    }
    y += 28.0;
    if mp.models.is_empty() {
        if let Some(e) = &mp.empty_models {
            draw_text(scene, e, Affine::translate((body_vp.x0 + 4.0, y + 6.0)), dim);
        }
        y += MODEL_ROW_H;
    }
    for (i, (name, meta)) in mp.rows.iter().enumerate() {
        let row = Rect::new(body_vp.x0, y, body_vp.x1, y + MODEL_ROW_H);
        let m = &mp.models[i];
        if row.y1 >= body_vp.y0 && row.y0 <= body_vp.y1 {
            if m.is_router || m.is_reasoning {
                let accent = if m.is_router { router_c } else { reason_c };
                scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &Rect::new(row.x0, row.y0 + 6.0, row.x0 + 3.0, row.y1 - 6.0));
            }
            draw_text(scene, name, Affine::translate((row.x0 + 12.0, row.y0 + 8.0)), title_c);
            let meta_c = if m.is_router { router_c } else if m.is_reasoning { reason_c } else { dim };
            draw_text(scene, meta, Affine::translate((row.x0 + 12.0, row.y0 + 28.0)), meta_c);

            for (br, kind) in model_row_buttons(row) {
                let (lbl, bg) = match kind {
                    ModelBtn::Router => (&mp.r_lbl, if m.is_router { router_c.with_alpha(0.5) } else { Color::from_rgb8(50, 52, 62) }),
                    ModelBtn::Reasoning => (&mp.q_lbl, if m.is_reasoning { reason_c.with_alpha(0.5) } else { Color::from_rgb8(50, 52, 62) }),
                    ModelBtn::Delete => (&mp.del_lbl, Color::from_rgb8(66, 50, 52)),
                };
                scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(br, 5.0));
                if let Some(l) = lbl {
                    let tx = br.x0 + (br.width() - l.width() as f64) * 0.5;
                    let ty = br.y0 + (br.height() - l.height() as f64) * 0.5;
                    let fg = if matches!(kind, ModelBtn::Delete) { Color::from_rgb8(225, 160, 150) } else { pal().text };
                    draw_text(scene, l, Affine::translate((tx, ty)), fg);
                }
            }
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &Rect::new(row.x0, row.y1 - 1.0, row.x1, row.y1));
        }
        y += MODEL_ROW_H;
    }

    // Trust section.
    if let Some(h) = &mp.trust_hdr {
        draw_text(scene, h, Affine::translate((body_vp.x0 + 4.0, y + 12.0)), hdr_c);
    }
    y += 34.0;
    if mp.trust.is_empty() {
        if let Some(e) = &mp.empty_trust {
            draw_text(scene, e, Affine::translate((body_vp.x0 + 4.0, y + 4.0)), dim);
        }
    }
    for t in &mp.trust_rows {
        if y + TRUST_ROW_H >= body_vp.y0 && y <= body_vp.y1 {
            draw_text(scene, t, Affine::translate((body_vp.x0 + 6.0, y + 4.0)), pal().text_body);
        }
        y += TRUST_ROW_H;
    }

    scene.pop_layer();
    draw_scrollbar(scene, &bounds, &body_vp, mp.content_h(), mp.scroll);
    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Browser (chrome around a wry child WebView) ------------------------
// The web page is rendered by a native wry WebView overlaid on the body rect
// (positioned by app.rs). Here we only draw the chrome: title bar, a nav row
// (Back / Fwd / Reload / URL / Go) and an info strip (reliability + actions).

pub(crate) const BR_NAV_H: f64 = 40.0;
pub(crate) const BR_INFO_H: f64 = 30.0;

/// Browser chrome geometry: nav-bar control rects + the URL field + the body
/// rect the WebView is sized to. Shared by draw + click + webview positioning.
pub(crate) struct BrowserChrome {
    pub(crate) close: Rect,
    pub(crate) back: Rect,
    pub(crate) forward: Rect,
    pub(crate) reload: Rect,
    pub(crate) url: Rect,
    pub(crate) go: Rect,
    pub(crate) assess: Rect,
    pub(crate) save: Rect,
    pub(crate) body: Rect, // where the WebView goes
}

pub(crate) fn browser_chrome(bounds: Rect) -> BrowserChrome {
    let (header, close, _b) = window_chrome(bounds);
    let nav_y0 = header.y1;
    let nav = Rect::new(bounds.x0, nav_y0, bounds.x1, nav_y0 + BR_NAV_H);
    let info = Rect::new(bounds.x0, nav.y1, bounds.x1, nav.y1 + BR_INFO_H);
    let body = Rect::new(bounds.x0 + 1.0, info.y1, bounds.x1 - 1.0, bounds.y1 - 1.0);

    let by = nav.y0 + (BR_NAV_H - 26.0) * 0.5;
    let bw = 30.0;
    let mut x = nav.x0 + 10.0;
    let back = Rect::new(x, by, x + bw, by + 26.0);
    x += bw + 4.0;
    let forward = Rect::new(x, by, x + bw, by + 26.0);
    x += bw + 4.0;
    let reload = Rect::new(x, by, x + bw, by + 26.0);
    x += bw + 8.0;
    let go = Rect::new(nav.x1 - 12.0 - 44.0, by, nav.x1 - 12.0, by + 26.0);
    let url = Rect::new(x, by, go.x0 - 8.0, by + 26.0);

    // Info strip actions (right-aligned): [Assess] [Save].
    let iy = info.y0 + (BR_INFO_H - 22.0) * 0.5;
    let save = Rect::new(info.x1 - 12.0 - 64.0, iy, info.x1 - 12.0, iy + 22.0);
    let assess = Rect::new(save.x0 - 8.0 - 92.0, iy, save.x0 - 8.0, iy + 22.0);
    BrowserChrome { close, back, forward, reload, url, go, assess, save, body }
}

pub(crate) struct BrowserPanel {
    pub(crate) url: String,           // editable URL-bar text
    pub(crate) loaded_url: String,    // current page (from the webview / IPC)
    pub(crate) title: String,         // page title (from IPC)
    pub(crate) page_text: String,     // extracted page text (IPC) for save + reliability
    pub(crate) reliability: Option<String>, // "Factual · 4.2 / 5"
    pub(crate) assessing: bool,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    url_layout: Option<Layout<Brush>>,
    info_layout: Option<Layout<Brush>>,
    back_lbl: Option<Layout<Brush>>,
    fwd_lbl: Option<Layout<Brush>>,
    reload_lbl: Option<Layout<Brush>>,
    go_lbl: Option<Layout<Brush>>,
    assess_lbl: Option<Layout<Brush>>,
    save_lbl: Option<Layout<Brush>>,
}
impl BrowserPanel {
    pub(crate) fn new() -> Self {
        Self {
            url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
            loaded_url: String::new(),
            title: "New tab".to_string(),
            page_text: String::new(),
            reliability: None,
            assessing: false,
            inner_w: 0.0,
            title_lbl: None,
            url_layout: None,
            info_layout: None,
            back_lbl: None,
            fwd_lbl: None,
            reload_lbl: None,
            go_lbl: None,
            assess_lbl: None,
            save_lbl: None,
        }
    }
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        let mut out = vec![format!("Browser: {}", self.title), format!("URL: {}", self.url)];
        if let Some(r) = &self.reliability {
            out.push(format!("Reliability: {r}"));
        }
        out
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        if self.back_lbl.is_none() {
            self.back_lbl = Some(shaper.shape("\u{2039}", 40.0, 18.0));
            self.fwd_lbl = Some(shaper.shape("\u{203a}", 40.0, 18.0));
            self.reload_lbl = Some(shaper.shape("\u{21bb}", 40.0, 15.0));
            self.go_lbl = Some(shaper.shape("Go", 60.0, 13.0));
            self.assess_lbl = Some(shaper.shape("Assess", 100.0, 12.0));
            self.save_lbl = Some(shaper.shape("Save doc", 100.0, 12.0));
        }
        self.inner_w = inner_w;
        self.title_lbl = Some(shaper.shape(&self.title, 4000.0, 15.0));
        self.url_layout = Some(shaper.shape(&self.url, (inner_w + 4000.0) as f32, 13.0));
        let info = match &self.reliability {
            Some(r) => format!("Reliability: {r}"),
            None if self.assessing => "Assessing reliability\u{2026}".to_string(),
            None => "Not assessed".to_string(),
        };
        self.info_layout = Some(shaper.shape(&info, (inner_w - 180.0).max(80.0) as f32, 12.5));
    }
}

pub(crate) fn draw_browser(scene: &mut Scene, br: &BrowserPanel, bounds: Rect) {
    let (panel_rr, header, _close, _b) = draw_window_chrome(scene, bounds, Color::from_rgb8(110, 140, 170));
    let c = browser_chrome(bounds);
    if let Some(l) = &br.title_lbl {
        scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(header.x0, header.y0, c.close.x0 - 8.0, header.y1));
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 13.0)), pal().text);
        scene.pop_layer();
    }

    // Nav buttons.
    let draw_btn = |scene: &mut Scene, r: Rect, lbl: &Option<Layout<Brush>>, font_drop: f64| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(r, 5.0));
        if let Some(l) = lbl {
            let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
            let ty = r.y0 + (r.height() - l.height() as f64) * 0.5 - font_drop;
            draw_text(scene, l, Affine::translate((tx, ty)), pal().text_body);
        }
    };
    draw_btn(scene, c.back, &br.back_lbl, 2.0);
    draw_btn(scene, c.forward, &br.fwd_lbl, 2.0);
    draw_btn(scene, c.reload, &br.reload_lbl, 1.0);
    draw_btn(scene, c.go, &br.go_lbl, 0.0);

    // URL field.
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().input, None, &RoundedRect::from_rect(c.url, 6.0));
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().input_border, None, &RoundedRect::from_rect(c.url, 6.0));
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &c.url);
    if let Some(l) = &br.url_layout {
        let ty = c.url.y0 + (c.url.height() - l.height() as f64) * 0.5;
        draw_text(scene, l, Affine::translate((c.url.x0 + 8.0, ty)), pal().text);
        let caret_x = c.url.x0 + 8.0 + l.width() as f64 + 1.0;
        scene.stroke(&Stroke::new(1.3), Affine::IDENTITY, pal().caret, None, &Line::new(Point::new(caret_x, c.url.y0 + 5.0), Point::new(caret_x, c.url.y1 - 5.0)));
    }
    scene.pop_layer();

    // Info strip + actions.
    if let Some(l) = &br.info_layout {
        let iy = c.assess.y0 - (BR_INFO_H - 22.0) * 0.5;
        draw_text(scene, l, Affine::translate((bounds.x0 + DW_PAD, iy + 8.0)), pal().text_dim);
    }
    draw_btn(scene, c.assess, &br.assess_lbl, 0.0);
    draw_btn(scene, c.save, &br.save_lbl, 0.0);

    // Body: the WebView overlays here. Draw a placeholder so it's not blank
    // before the webview shows / when it's hidden behind another window.
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().base, None, &c.body);

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- PII dashboard ------------------------------------------------------
// Lists detected/stored PII (plaintext metadata: kind, label, review state,
// sources). Values stay masked — decryption is an L3-gated reveal through the
// orchestrator's account key (a follow-up). Per row: Confirm / Dismiss / Delete.

pub(crate) const PII_ROW_H: f64 = 50.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PiiBtn {
    Confirm,
    Dismiss,
    Delete,
}

/// Per-row buttons (right-aligned): [Keep] [Dismiss] [Del].
pub(crate) fn pii_row_buttons(row: Rect) -> [(Rect, PiiBtn); 3] {
    let bh = 22.0;
    let by0 = row.y0 + (row.height() - bh) * 0.5;
    let (keep_w, dis_w, del_w, gap) = (44.0, 58.0, 42.0, 6.0);
    let mut x = row.x1 - 10.0;
    let del = Rect::new(x - del_w, by0, x, by0 + bh);
    x -= del_w + gap;
    let dis = Rect::new(x - dis_w, by0, x, by0 + bh);
    x -= dis_w + gap;
    let keep = Rect::new(x - keep_w, by0, x, by0 + bh);
    [(keep, PiiBtn::Confirm), (dis, PiiBtn::Dismiss), (del, PiiBtn::Delete)]
}

pub(crate) struct PiiRow {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) headline: String, // label, else "(unlabeled <kind>)"
    pub(crate) meta: String,     // "•••• · confidence 95% · 2 sources · vault"
    pub(crate) review: ReviewState,
}

pub(crate) struct PiiPanel {
    pub(crate) rows: Vec<PiiRow>,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    empty_lbl: Option<Layout<Brush>>,
    keep_lbl: Option<Layout<Brush>>,
    dis_lbl: Option<Layout<Brush>>,
    del_lbl: Option<Layout<Brush>>,
    shaped: Vec<(Layout<Brush>, Layout<Brush>)>, // (headline, meta)
}
impl PiiPanel {
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        if self.rows.is_empty() {
            return vec!["No PII detected or stored yet".into()];
        }
        self.rows
            .iter()
            .map(|r| format!("{} \u{2014} {} \u{2014} {} \u{2014} {:?}", r.kind, r.headline, r.meta, r.review))
            .collect()
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() {
            self.title_lbl = Some(shaper.shape("PII dashboard", 4000.0, 16.0));
            self.empty_lbl = Some(shaper.shape("No PII detected or stored yet \u{2014} scanned documents + vault entries appear here.", 600.0, 13.5));
            self.keep_lbl = Some(shaper.shape("Keep", 60.0, 12.0));
            self.dis_lbl = Some(shaper.shape("Dismiss", 80.0, 12.0));
            self.del_lbl = Some(shaper.shape("Del", 60.0, 12.0));
        }
        if self.shaped.len() != self.rows.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 175.0).max(80.0) as f32;
            self.shaped = self
                .rows
                .iter()
                .map(|r| (shaper.shape(&r.headline, tw, 13.5), shaper.shape(&r.meta, tw, 11.5)))
                .collect();
        }
    }
    pub(crate) fn content_h(&self) -> f64 {
        self.rows.len().max(1) as f64 * PII_ROW_H + 8.0
    }
}

/// Human label for a PII kind.
fn pii_kind_label(k: &PiiKind) -> &'static str {
    match k {
        PiiKind::Email => "Email",
        PiiKind::Phone => "Phone",
        PiiKind::Ssn => "SSN",
        PiiKind::CreditCard => "Credit card",
        PiiKind::Ipv4 => "IP address",
        PiiKind::Avs => "AVS/AHV",
        PiiKind::Iban => "IBAN",
        PiiKind::Passport => "Passport",
        PiiKind::Dob => "Date of birth",
        PiiKind::Address => "Address",
        PiiKind::PersonName => "Name",
        PiiKind::OrgName => "Organization",
        PiiKind::Password => "Password",
        PiiKind::ApiToken => "API token",
        PiiKind::BankAccount => "Bank account",
        PiiKind::DocumentId => "Document ID",
        _ => "Other",
    }
}

/// Load all PII records (detected + vault) into a dashboard panel. Values stay
/// encrypted/masked; only plaintext metadata is shown.
pub(crate) async fn load_pii(db: &Arc<dyn GraphDB>) -> PiiPanel {
    let records = db.list_pii_records(None, None, None).await.unwrap_or_default();
    let rows = records
        .iter()
        .map(|r| {
            let kind = pii_kind_label(&r.kind).to_string();
            let headline = r.label.clone().filter(|l| !l.is_empty()).unwrap_or_else(|| format!("(unlabeled {})", kind.to_lowercase()));
            let origin = if r.stored_secret { "vault" } else { "detected" };
            let meta = format!(
                "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}   \u{00b7}   {:.0}% confidence   \u{00b7}   {} source{}   \u{00b7}   {origin}",
                r.confidence * 100.0,
                r.sources.len(),
                if r.sources.len() == 1 { "" } else { "s" },
            );
            PiiRow { id: r.id_string().unwrap_or_default(), kind, headline, meta, review: r.review_state.clone() }
        })
        .collect();
    PiiPanel { rows, scroll: 0.0, inner_w: 0.0, title_lbl: None, empty_lbl: None, keep_lbl: None, dis_lbl: None, del_lbl: None, shaped: Vec::new() }
}

pub(crate) fn draw_pii(scene: &mut Scene, pp: &PiiPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(180, 130, 150));
    let dim = pal().text_dim;
    if let Some(l) = &pp.title_lbl {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), pal().text);
    }

    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);
    if pp.rows.is_empty() {
        if let Some(e) = &pp.empty_lbl {
            draw_text(scene, e, Affine::translate((body_vp.x0 + 4.0, body_vp.y0 + 8.0)), dim);
        }
    }
    let unreviewed = Color::from_rgb8(214, 168, 92);
    let confirmed = Color::from_rgb8(120, 175, 135);
    let dismissed = Color::from_rgb8(120, 124, 134);
    for (i, (headline, meta)) in pp.shaped.iter().enumerate() {
        let y = body_vp.y0 - pp.scroll + i as f64 * PII_ROW_H;
        let row = Rect::new(body_vp.x0, y, body_vp.x1, y + PII_ROW_H);
        if row.y1 < body_vp.y0 || row.y0 > body_vp.y1 {
            continue;
        }
        let rev = &pp.rows[i].review;
        let badge = match rev {
            ReviewState::Unreviewed => unreviewed,
            ReviewState::Confirmed => confirmed,
            ReviewState::Dismissed => dismissed,
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, badge, None, &Rect::new(row.x0, row.y0 + 6.0, row.x0 + 3.0, row.y1 - 6.0));
        draw_text(scene, headline, Affine::translate((row.x0 + 12.0, row.y0 + 7.0)), pal().text);
        draw_text(scene, meta, Affine::translate((row.x0 + 12.0, row.y0 + 27.0)), dim);

        for (br, kind) in pii_row_buttons(row) {
            let (lbl, fg, bg) = match kind {
                PiiBtn::Confirm => (&pp.keep_lbl, Color::from_rgb8(214, 234, 220), Color::from_rgb8(46, 70, 56)),
                PiiBtn::Dismiss => (&pp.dis_lbl, pal().text_body, pal().surface),
                PiiBtn::Delete => (&pp.del_lbl, Color::from_rgb8(225, 160, 150), Color::from_rgb8(66, 50, 52)),
            };
            scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(br, 5.0));
            if let Some(l) = lbl {
                let tx = br.x0 + (br.width() - l.width() as f64) * 0.5;
                let ty = br.y0 + (br.height() - l.height() as f64) * 0.5;
                draw_text(scene, l, Affine::translate((tx, ty)), fg);
            }
        }
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(row.x0, row.y1 - 1.0, row.x1, row.y1));
    }
    scene.pop_layer();
    draw_scrollbar(scene, &bounds, &body_vp, pp.content_h(), pp.scroll);
    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Alerts on changes from paired devices (C-WIRE 13) --------------------
// A change from one of the user's paired devices applies without asking (the
// state it replaced is in History). Every received document change is
// analysed; only a detection is listed here, until the user dismisses it.
// Dismissing changes nothing in the document; going back is the ordinary
// "Use this version" in History. Spec ch. 05, decided by Céline 2026-10-15.

pub(crate) const ALERT_ROW_H: f64 = 104.0;

/// Primary button (white text on indigo-600: 6.3:1).
const PRIMARY_BTN: Color = Color::from_rgb8(79, 70, 229);
/// The "looks suspicious" pill (dark text on amber: well above 4.5:1).
const ALERT_PILL: Color = Color::from_rgb8(0xF5, 0x9E, 0x0B);
const ALERT_PILL_TEXT: Color = Color::from_rgb8(26, 26, 26);

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum AlertBtn {
    Open,
    Previous,
    Dismiss,
}

/// The per-row buttons, left to right along the row's bottom: [Open the
/// document] [Open the previous version] [Dismiss]. "Previous version" is
/// left out for a first arrival (there is none).
pub(crate) fn alert_row_buttons(row: Rect, has_previous: bool) -> Vec<(Rect, AlertBtn)> {
    let (bh, gap) = (26.0, 8.0);
    let y0 = row.y1 - bh - 10.0;
    let mut x = row.x0 + 12.0;
    let mut out = Vec::new();
    for (w, b) in [(150.0, AlertBtn::Open), (196.0, AlertBtn::Previous), (84.0, AlertBtn::Dismiss)] {
        if b == AlertBtn::Previous && !has_previous {
            continue;
        }
        out.push((Rect::new(x, y0, x + w, y0 + bh), b));
        x += w + gap;
    }
    out
}

pub(crate) struct AlertRow {
    pub(crate) doc_id: String,
    pub(crate) title: String,
    /// The device's name, or its id when it is not a paired device.
    pub(crate) device: String,
    pub(crate) finding: String,
    pub(crate) previous_version: Option<String>,
}

impl AlertRow {
    /// The notice, in words: "A change from <device> to <document> looks suspicious".
    pub(crate) fn notice(&self) -> String {
        let title = if self.title.is_empty() { "a document" } else { &self.title };
        let device = if self.device.is_empty() { "a paired device" } else { &self.device };
        format!("A change from {device} to \u{201c}{title}\u{201d} looks suspicious")
    }
}

struct AlertShaped {
    notice: Layout<Brush>,
    finding: Layout<Brush>,
}

pub(crate) struct AlertsPanel {
    pub(crate) rows: Vec<AlertRow>,
    /// The row the keyboard acts on.
    pub(crate) selected: usize,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    empty_lbl: Option<Layout<Brush>>,
    hint_lbl: Option<Layout<Brush>>,
    pill_lbl: Option<Layout<Brush>>,
    open_lbl: Option<Layout<Brush>>,
    prev_lbl: Option<Layout<Brush>>,
    dismiss_lbl: Option<Layout<Brush>>,
    shaped: Vec<AlertShaped>,
    /// Documents whose thread was deleted (S5-12 W5), below the alerts.
    orphans: crate::orphans::OrphansSection,
    orphan_shaped: Option<OrphanShaped>,
}

/// The section's shaped text: heading, each line's notice and origin, each
/// group's label, and the buttons' labels (line, then group; in `ACTIONS`
/// order).
struct OrphanShaped {
    ids: Vec<String>,
    heading: Layout<Brush>,
    lines: Vec<(Layout<Brush>, Layout<Brush>)>,
    groups: Vec<Layout<Brush>>,
    line_btns: Vec<Layout<Brush>>,
    group_btns: Vec<Layout<Brush>>,
}

/// What the keys do in the Alerts window (said in the window and to a screen reader).
pub(crate) const ALERTS_KEYS: &str =
    "Up and Down choose an alert. Enter or O opens the document, V opens the previous version, D dismisses, Escape closes.";

impl AlertsPanel {
    pub(crate) fn new(rows: Vec<AlertRow>) -> Self {
        Self {
            rows,
            selected: 0,
            scroll: 0.0,
            inner_w: 0.0,
            title_lbl: None,
            empty_lbl: None,
            hint_lbl: None,
            pill_lbl: None,
            open_lbl: None,
            prev_lbl: None,
            dismiss_lbl: None,
            shaped: Vec::new(),
            orphans: crate::orphans::OrphansSection::empty(),
            orphan_shaped: None,
        }
    }

    /// The panel with the section "Documents whose thread was deleted".
    pub(crate) fn with_orphans(mut self, orphans: crate::orphans::OrphansSection) -> Self {
        self.orphans = orphans;
        self
    }

    /// The alert row the keyboard acts on (none while the keyboard is in the
    /// section of documents whose thread was deleted).
    pub(crate) fn selected_row(&self) -> Option<&AlertRow> {
        if self.orphans.selected.is_some() {
            return None;
        }
        self.rows.get(self.selected)
    }

    /// The section "Documents whose thread was deleted" (S5-12 W5), loaded
    /// with the alerts by [`load_alerts`].
    pub(crate) fn orphans(&self) -> &crate::orphans::OrphansSection {
        &self.orphans
    }

    pub(crate) fn orphans_mut(&mut self) -> &mut crate::orphans::OrphansSection {
        &mut self.orphans
    }

    /// Hands the section over (when the window is rebuilt).
    pub(crate) fn take_orphans(&mut self) -> crate::orphans::OrphansSection {
        std::mem::replace(&mut self.orphans, crate::orphans::OrphansSection::empty())
    }

    /// Where the section starts in the list's content (below the alerts).
    pub(crate) fn orphans_top(&self) -> f64 {
        self.rows.len() as f64 * ALERT_ROW_H
    }

    /// The keyboard's Up / Down across the alerts and then the section's
    /// documents, keeping the focused row in view.
    pub(crate) fn move_focus(&mut self, down: bool, view_h: f64) {
        let in_section = self.orphans.selected.is_some();
        let last_alert = self.rows.is_empty() || self.selected + 1 >= self.rows.len();
        if in_section || (down && last_alert && self.orphans.count() > 0) {
            let stayed = self.orphans.move_selection(down);
            if !stayed && self.rows.is_empty() {
                // Nothing above the section: stay on its first line.
                self.orphans.move_selection(true);
            }
            if let Some(top) = self.orphans.selected.and_then(|i| self.orphans.line_top(i)) {
                let top = self.orphans_top() + top;
                let h = crate::orphans::LINE_H;
                if top < self.scroll {
                    self.scroll = top;
                } else if top + h > self.scroll + view_h {
                    self.scroll = (top + h - view_h).max(0.0);
                }
                return;
            }
        }
        self.move_selection(down, view_h);
    }

    pub(crate) fn move_selection(&mut self, down: bool, view_h: f64) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = if down { (self.selected + 1).min(self.rows.len() - 1) } else { self.selected.saturating_sub(1) };
        // Keep the selected row in view.
        let top = self.selected as f64 * ALERT_ROW_H;
        if top < self.scroll {
            self.scroll = top;
        } else if top + ALERT_ROW_H > self.scroll + view_h {
            self.scroll = (top + ALERT_ROW_H - view_h).max(0.0);
        }
    }

    /// Readable lines for the accessibility tree: each alert, which one is
    /// selected, its actions, and the keys.
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        let mut out = self.alert_a11y_lines();
        out.extend(self.orphans.a11y_lines());
        out
    }

    fn alert_a11y_lines(&self) -> Vec<String> {
        if self.rows.is_empty() {
            return vec!["No alerts. Changes from your devices that look suspicious show up here.".into()];
        }
        let n = self.rows.len();
        let mut out: Vec<String> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let sel = if i == self.selected && self.orphans.selected.is_none() { ", selected" } else { "" };
                let prev = if r.previous_version.is_some() { "Open the previous version, " } else { "" };
                format!(
                    "Alert {} of {n}{sel}: {}. Found: {}. Actions: Open the document, {prev}Dismiss.",
                    i + 1,
                    r.notice(),
                    r.finding
                )
            })
            .collect();
        out.push(ALERTS_KEYS.into());
        out
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() {
            self.title_lbl = Some(shaper.shape("Alerts", 4000.0, 16.0));
            self.empty_lbl = Some(shaper.shape(
                "No alerts. Changes from your devices apply on their own; one that looks suspicious shows up here.",
                600.0,
                13.5,
            ));
            self.pill_lbl = Some(shaper.shape("Looks suspicious", 200.0, 11.0));
            self.open_lbl = Some(shaper.shape("Open the document", 200.0, 12.0));
            self.prev_lbl = Some(shaper.shape("Open the previous version", 220.0, 12.0));
            self.dismiss_lbl = Some(shaper.shape("Dismiss", 120.0, 12.0));
        }
        let orphan_ids: Vec<String> = self.orphans.lines().iter().map(|l| l.id.clone()).collect();
        if width_changed || self.hint_lbl.is_none() || self.orphan_shaped.as_ref().map(|o| &o.ids) != Some(&orphan_ids) {
            let tw = (inner_w - 24.0).max(120.0) as f32;
            let keys = match (self.rows.is_empty(), orphan_ids.is_empty()) {
                (_, true) => ALERTS_KEYS.to_string(),
                (true, false) => crate::orphans::ORPHAN_KEYS.to_string(),
                (false, false) => format!("{ALERTS_KEYS} {}", crate::orphans::ORPHAN_KEYS),
            };
            self.hint_lbl = Some(shaper.shape(&keys, tw, 11.5));
            self.orphan_shaped = Some(OrphanShaped {
                ids: orphan_ids,
                heading: shaper.shape(crate::orphans::HEADING, tw, 14.5),
                lines: self
                    .orphans
                    .lines()
                    .iter()
                    .map(|l| (shaper.shape(&l.notice(), tw, 13.5), shaper.shape(&l.origin(), tw, 12.0)))
                    .collect(),
                groups: self
                    .orphans
                    .groups()
                    .iter()
                    .map(|g| {
                        let n = g.ids.len();
                        let text = if n > 1 { format!("{} ({n} documents)", g.label) } else { g.label.clone() };
                        shaper.shape(&text, tw, 12.5)
                    })
                    .collect(),
                line_btns: crate::orphans::ACTIONS.iter().map(|a| shaper.shape(a.label(), 200.0, 12.0)).collect(),
                group_btns: crate::orphans::ACTIONS.iter().map(|a| shaper.shape(a.group_label(), 220.0, 12.0)).collect(),
            });
        }
        if self.shaped.len() != self.rows.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 24.0).max(120.0) as f32;
            self.shaped = self
                .rows
                .iter()
                .map(|r| AlertShaped {
                    notice: shaper.shape(&r.notice(), tw, 13.5),
                    finding: shaper.shape(&format!("Found: {}", r.finding), tw, 12.0),
                })
                .collect();
        }
    }

    /// The height of the keys line above the list.
    pub(crate) fn hint_h(&self) -> f64 {
        self.hint_lbl.as_ref().map(|l| l.height() as f64 + 10.0).unwrap_or(24.0)
    }

    pub(crate) fn content_h(&self) -> f64 {
        if self.orphans.count() == 0 {
            return self.rows.len().max(1) as f64 * ALERT_ROW_H + 8.0;
        }
        self.orphans_top() + self.orphans.height() + 8.0
    }
}

/// The open alerts, with each document's title and each device's name, and
/// the documents whose thread was deleted (S5-12). A section that cannot be
/// read is left empty: the alerts load as before.
pub(crate) async fn load_alerts(
    docs: &sovereign_docs::DocumentService,
    device_names: &HashMap<String, String>,
) -> AlertsPanel {
    let mut rows = Vec::new();
    for a in docs.alerts().await.unwrap_or_default() {
        let title = match docs.read(&a.id).await {
            Ok(v) => v.labelled_title(),
            Err(_) => String::new(),
        };
        let device = device_names.get(&a.device).cloned().unwrap_or_else(|| a.device.clone());
        rows.push(AlertRow { doc_id: a.id, title, device, finding: a.finding, previous_version: a.previous_version });
    }
    let orphans = crate::orphans::OrphansSection::load(docs, device_names)
        .await
        .unwrap_or_else(|_| crate::orphans::OrphansSection::empty());
    AlertsPanel::new(rows).with_orphans(orphans)
}

/// Sample alerts for screenshot/debug (SHELL_OPEN_ALERTS=1).
pub(crate) fn synthetic_alerts() -> AlertsPanel {
    AlertsPanel::new(vec![
        AlertRow {
            doc_id: "document:a1".into(),
            title: "Q3 strategy notes".into(),
            device: "Laptop".into(),
            finding: "injected instructions in the new content".into(),
            previous_version: Some("commit:p1".into()),
        },
        AlertRow {
            doc_id: "document:a2".into(),
            title: "Reading list".into(),
            device: "Phone".into(),
            finding: "the model judged the change suspicious".into(),
            previous_version: None,
        },
    ])
}

fn draw_button(scene: &mut Scene, br: Rect, lbl: &Option<Layout<Brush>>, primary: bool) {
    let (fg, bg) = if primary { (Color::WHITE, PRIMARY_BTN) } else { (pal().text_body, pal().surface) };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(br, 5.0));
    if !primary {
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &RoundedRect::from_rect(br, 5.0));
    }
    if let Some(l) = lbl {
        let tx = br.x0 + (br.width() - l.width() as f64) * 0.5;
        let ty = br.y0 + (br.height() - l.height() as f64) * 0.5;
        draw_text(scene, l, Affine::translate((tx, ty)), fg);
    }
}

/// The list viewport below the keys line.
pub(crate) fn alerts_list_vp(body_vp: Rect, hint_h: f64) -> Rect {
    Rect::new(body_vp.x0, body_vp.y0 + hint_h, body_vp.x1, body_vp.y1)
}

pub(crate) fn draw_alerts(scene: &mut Scene, ap: &AlertsPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) = draw_window_chrome(scene, bounds, ALERT_PILL);
    let dim = pal().text_dim;
    if let Some(l) = &ap.title_lbl {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), pal().text);
    }
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);
    if ap.rows.is_empty() && ap.orphans.count() == 0 {
        if let Some(e) = &ap.empty_lbl {
            draw_text(scene, e, Affine::translate((body_vp.x0 + 4.0, body_vp.y0 + 8.0)), dim);
        }
        scene.pop_layer();
        scene.pop_layer();
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
        return;
    }
    if let Some(h) = &ap.hint_lbl {
        draw_text(scene, h, Affine::translate((body_vp.x0 + 4.0, body_vp.y0 + 4.0)), dim);
    }
    let list_vp = alerts_list_vp(body_vp, ap.hint_h());
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &list_vp);
    for (i, sh) in ap.shaped.iter().enumerate() {
        let y = list_vp.y0 - ap.scroll + i as f64 * ALERT_ROW_H;
        let row = Rect::new(list_vp.x0, y, list_vp.x1, y + ALERT_ROW_H);
        if row.y1 < list_vp.y0 || row.y0 > list_vp.y1 {
            continue;
        }
        let r = &ap.rows[i];
        scene.fill(Fill::NonZero, Affine::IDENTITY, ALERT_PILL, None, &Rect::new(row.x0, row.y0 + 8.0, row.x0 + 3.0, row.y1 - 8.0));
        if let Some(p) = &ap.pill_lbl {
            let pill = Rect::new(row.x0 + 12.0, row.y0 + 8.0, row.x0 + 12.0 + p.width() as f64 + 16.0, row.y0 + 26.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, ALERT_PILL, None, &RoundedRect::from_rect(pill, 5.0));
            draw_text(scene, p, Affine::translate((pill.x0 + 8.0, pill.y0 + 3.0)), ALERT_PILL_TEXT);
        }
        draw_text(scene, &sh.notice, Affine::translate((row.x0 + 12.0, row.y0 + 30.0)), pal().text);
        draw_text(scene, &sh.finding, Affine::translate((row.x0 + 12.0, row.y0 + 50.0)), pal().text_body);
        for (br, b) in alert_row_buttons(row, r.previous_version.is_some()) {
            let lbl = match b {
                AlertBtn::Open => &ap.open_lbl,
                AlertBtn::Previous => &ap.prev_lbl,
                AlertBtn::Dismiss => &ap.dismiss_lbl,
            };
            draw_button(scene, br, lbl, b == AlertBtn::Open);
        }
        // The keyboard's row: a visible focus outline (WCAG 2.4.7).
        if i == ap.selected && ap.orphans.selected.is_none() {
            scene.stroke(
                &Stroke::new(2.0),
                Affine::IDENTITY,
                pal().accent,
                None,
                &RoundedRect::from_rect(Rect::new(row.x0 + 1.0, row.y0 + 2.0, row.x1 - 1.0, row.y1 - 2.0), 6.0),
            );
        }
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(row.x0, row.y1 - 1.0, row.x1, row.y1));
    }
    draw_orphans_section(scene, ap, list_vp);
    scene.pop_layer();
    scene.pop_layer();
    draw_scrollbar(scene, &bounds, &list_vp, ap.content_h(), ap.scroll);
    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

/// The section "Documents whose thread was deleted" (S5-12 W5), below the
/// alerts in the list: its heading, each deleted thread's row (with the "all"
/// buttons when it holds several documents), and each document with its
/// three ways out. No document text.
fn draw_orphans_section(scene: &mut Scene, ap: &AlertsPanel, list_vp: Rect) {
    use crate::orphans::{group_buttons, line_buttons, SectionRow};
    let Some(sh) = &ap.orphan_shaped else { return };
    if ap.orphans.count() == 0 || sh.lines.len() != ap.orphans.count() {
        return;
    }
    let top = list_vp.y0 - ap.scroll + ap.orphans_top();
    let groups = ap.orphans.groups();
    for (row, y, h) in ap.orphans.layout() {
        let r = Rect::new(list_vp.x0, top + y, list_vp.x1, top + y + h);
        if r.y1 < list_vp.y0 || r.y0 > list_vp.y1 {
            continue;
        }
        match row {
            SectionRow::Heading => {
                scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(r.x0, r.y0 + 6.0, r.x1, r.y0 + 7.0));
                draw_text(scene, &sh.heading, Affine::translate((r.x0 + 4.0, r.y0 + 14.0)), pal().text);
            }
            SectionRow::Group(g) => {
                if let Some(l) = sh.groups.get(g) {
                    draw_text(scene, l, Affine::translate((r.x0 + 4.0, r.y0 + 6.0)), pal().text_body);
                }
                if groups.get(g).is_some_and(|g| g.ids.len() > 1) {
                    for (i, (br, _)) in group_buttons(r).into_iter().enumerate() {
                        draw_button(scene, br, &sh.group_btns.get(i).cloned(), false);
                    }
                }
            }
            SectionRow::Line(i) => {
                scene.fill(Fill::NonZero, Affine::IDENTITY, pal().accent, None, &Rect::new(r.x0, r.y0 + 8.0, r.x0 + 3.0, r.y1 - 8.0));
                if let Some((notice, origin)) = sh.lines.get(i) {
                    draw_text(scene, notice, Affine::translate((r.x0 + 12.0, r.y0 + 8.0)), pal().text);
                    draw_text(scene, origin, Affine::translate((r.x0 + 12.0, r.y0 + 28.0)), pal().text_dim);
                }
                for (bi, (br, _)) in line_buttons(r).into_iter().enumerate() {
                    draw_button(scene, br, &sh.line_btns.get(bi).cloned(), bi == 0);
                }
                // The keyboard's line: a visible focus outline (WCAG 2.4.7).
                if ap.orphans.selected == Some(i) {
                    scene.stroke(
                        &Stroke::new(2.0),
                        Affine::IDENTITY,
                        pal().accent,
                        None,
                        &RoundedRect::from_rect(Rect::new(r.x0 + 1.0, r.y0 + 2.0, r.x1 - 1.0, r.y1 - 2.0), 6.0),
                    );
                }
                scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(r.x0, r.y1 - 1.0, r.x1, r.y1));
            }
        }
    }
}

// ---- Previous values of synced items (C-WIRE 13) ---------------------------
// A paired device's sync can replace a row (a thread, a contact, a
// personal-data record…). Its previous value is kept, sealed, and can be put
// back from this list. No pending badge: nothing here waits on the user.

pub(crate) const PV_ROW_H: f64 = 78.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum PrevBtn {
    Restore,
    Forget,
}

/// Per-row buttons (right-aligned, stacked): [Restore] over [Remove from list].
/// Restore is left out for an item that cannot be put back.
pub(crate) fn previous_value_buttons(row: Rect, can_restore: bool) -> Vec<(Rect, PrevBtn)> {
    let (bw, bh, gap) = (132.0, 24.0, 8.0);
    let x1 = row.x1 - 12.0;
    let x0 = x1 - bw;
    if can_restore {
        let total = bh * 2.0 + gap;
        let y0 = row.y0 + (row.height() - total) * 0.5;
        vec![
            (Rect::new(x0, y0, x1, y0 + bh), PrevBtn::Restore),
            (Rect::new(x0, y0 + bh + gap, x1, y0 + bh + gap + bh), PrevBtn::Forget),
        ]
    } else {
        let y0 = row.y0 + (row.height() - bh) * 0.5;
        vec![(Rect::new(x0, y0, x1, y0 + bh), PrevBtn::Forget)]
    }
}

pub(crate) struct PreviousValueRow {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) device: String,
    pub(crate) when: String,
    pub(crate) can_restore: bool,
}

pub(crate) const PREVIOUS_KEYS: &str =
    "Up and Down choose an item. Enter or R restores its previous value, X removes it from the list, Escape closes.";

pub(crate) struct PreviousValuesPanel {
    pub(crate) rows: Vec<PreviousValueRow>,
    pub(crate) selected: usize,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    empty_lbl: Option<Layout<Brush>>,
    hint_lbl: Option<Layout<Brush>>,
    restore_lbl: Option<Layout<Brush>>,
    forget_lbl: Option<Layout<Brush>>,
    shaped: Vec<(Layout<Brush>, Layout<Brush>)>,
}

impl PreviousValuesPanel {
    pub(crate) fn new(rows: Vec<PreviousValueRow>) -> Self {
        Self {
            rows,
            selected: 0,
            scroll: 0.0,
            inner_w: 0.0,
            title_lbl: None,
            empty_lbl: None,
            hint_lbl: None,
            restore_lbl: None,
            forget_lbl: None,
            shaped: Vec::new(),
        }
    }

    pub(crate) fn selected_row(&self) -> Option<&PreviousValueRow> {
        self.rows.get(self.selected)
    }

    pub(crate) fn move_selection(&mut self, down: bool, view_h: f64) {
        if self.rows.is_empty() {
            return;
        }
        self.selected = if down { (self.selected + 1).min(self.rows.len() - 1) } else { self.selected.saturating_sub(1) };
        let top = self.selected as f64 * PV_ROW_H;
        if top < self.scroll {
            self.scroll = top;
        } else if top + PV_ROW_H > self.scroll + view_h {
            self.scroll = (top + PV_ROW_H - view_h).max(0.0);
        }
    }

    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        if self.rows.is_empty() {
            return vec!["No previous values. When a sync replaces an item, its earlier value is kept here.".into()];
        }
        let n = self.rows.len();
        let mut out: Vec<String> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let sel = if i == self.selected { ", selected" } else { "" };
                let restore = if r.can_restore { "Restore, " } else { "" };
                format!(
                    "Item {} of {n}{sel}: {}, replaced by a sync from {} on {}. Actions: {restore}Remove from list.",
                    i + 1,
                    r.label,
                    if r.device.is_empty() { "a paired device" } else { &r.device },
                    r.when
                )
            })
            .collect();
        out.push(PREVIOUS_KEYS.into());
        out
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() {
            self.title_lbl = Some(shaper.shape("Previous values", 4000.0, 16.0));
            self.empty_lbl = Some(shaper.shape(
                "No previous values. When a sync from one of your devices replaces an item, its earlier value is kept here.",
                600.0,
                13.5,
            ));
            self.restore_lbl = Some(shaper.shape("Restore", 140.0, 12.0));
            self.forget_lbl = Some(shaper.shape("Remove from list", 140.0, 12.0));
        }
        if self.shaped.len() != self.rows.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 168.0).max(120.0) as f32;
            self.hint_lbl = Some(shaper.shape(PREVIOUS_KEYS, (inner_w - 24.0).max(120.0) as f32, 11.5));
            self.shaped = self
                .rows
                .iter()
                .map(|r| {
                    let meta = format!(
                        "Replaced by a sync from {} \u{00b7} {}",
                        if r.device.is_empty() { "a paired device" } else { &r.device },
                        r.when
                    );
                    (shaper.shape(&r.label, tw, 13.5), shaper.shape(&meta, tw, 11.5))
                })
                .collect();
        }
    }

    pub(crate) fn hint_h(&self) -> f64 {
        self.hint_lbl.as_ref().map(|l| l.height() as f64 + 10.0).unwrap_or(24.0)
    }

    pub(crate) fn content_h(&self) -> f64 {
        self.rows.len().max(1) as f64 * PV_ROW_H + 8.0
    }
}

/// A row's label in words, from its current value.
async fn previous_value_label(db: &Arc<dyn GraphDB>, table: &str, row_id: &str) -> String {
    match table {
        "thread" => match db.get_thread(row_id).await {
            Ok(t) => format!("Thread: {}", t.name),
            Err(_) => "A thread".into(),
        },
        "contact" => match db.get_contact(row_id).await {
            Ok(c) => format!("Contact: {}", c.name),
            Err(_) => "A contact".into(),
        },
        "pii_record" => match db.get_pii_record(row_id).await {
            Ok(p) => match p.label {
                Some(l) if !l.is_empty() => format!("Personal data: {l}"),
                _ => "A personal-data record".into(),
            },
            Err(_) => "A personal-data record".into(),
        },
        "entity" => "A person or organisation".into(),
        "message" => "A message".into(),
        "conversation" => "A conversation".into(),
        "suggested_link" => "A suggested link".into(),
        other => format!("An item ({other})"),
    }
}

/// The previous values a sync replaced, newest first.
pub(crate) async fn load_previous_values(
    db: &Arc<dyn GraphDB>,
    device_names: &HashMap<String, String>,
) -> PreviousValuesPanel {
    let mut recs = db.list_pending_row_recoveries().await.unwrap_or_default();
    recs.sort_by(|a, b| b.overwritten_at.cmp(&a.overwritten_at));
    let mut rows = Vec::with_capacity(recs.len());
    for r in recs {
        rows.push(PreviousValueRow {
            id: r.id_string().unwrap_or_default(),
            label: previous_value_label(db, &r.table, &r.row_id).await,
            device: device_names.get(&r.peer).cloned().unwrap_or_else(|| r.peer.clone()),
            when: r.overwritten_at.format("%Y-%m-%d %H:%M").to_string(),
            // Restore is wired for these tables (SyncService::restore_row_recovery).
            can_restore: matches!(r.table.as_str(), "thread" | "contact" | "pii_record"),
        });
    }
    PreviousValuesPanel::new(rows)
}

pub(crate) fn draw_previous_values(scene: &mut Scene, pv: &PreviousValuesPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) = draw_window_chrome(scene, bounds, Color::from_rgb8(120, 130, 200));
    let dim = pal().text_dim;
    if let Some(l) = &pv.title_lbl {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), pal().text);
    }
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);
    if pv.rows.is_empty() {
        if let Some(e) = &pv.empty_lbl {
            draw_text(scene, e, Affine::translate((body_vp.x0 + 4.0, body_vp.y0 + 8.0)), dim);
        }
        scene.pop_layer();
        scene.pop_layer();
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
        return;
    }
    if let Some(h) = &pv.hint_lbl {
        draw_text(scene, h, Affine::translate((body_vp.x0 + 4.0, body_vp.y0 + 4.0)), dim);
    }
    let list_vp = alerts_list_vp(body_vp, pv.hint_h());
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &list_vp);
    for (i, (label, meta)) in pv.shaped.iter().enumerate() {
        let y = list_vp.y0 - pv.scroll + i as f64 * PV_ROW_H;
        let row = Rect::new(list_vp.x0, y, list_vp.x1, y + PV_ROW_H);
        if row.y1 < list_vp.y0 || row.y0 > list_vp.y1 {
            continue;
        }
        let r = &pv.rows[i];
        draw_text(scene, label, Affine::translate((row.x0 + 12.0, row.y0 + 16.0)), pal().text);
        draw_text(scene, meta, Affine::translate((row.x0 + 12.0, row.y0 + 38.0)), dim);
        for (br, b) in previous_value_buttons(row, r.can_restore) {
            let lbl = match b {
                PrevBtn::Restore => &pv.restore_lbl,
                PrevBtn::Forget => &pv.forget_lbl,
            };
            draw_button(scene, br, lbl, b == PrevBtn::Restore);
        }
        if i == pv.selected {
            scene.stroke(
                &Stroke::new(2.0),
                Affine::IDENTITY,
                pal().accent,
                None,
                &RoundedRect::from_rect(Rect::new(row.x0 + 1.0, row.y0 + 2.0, row.x1 - 1.0, row.y1 - 2.0), 6.0),
            );
        }
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(row.x0, row.y1 - 1.0, row.x1, row.y1));
    }
    scene.pop_layer();
    scene.pop_layer();
    draw_scrollbar(scene, &bounds, &list_vp, pv.content_h(), pv.scroll);
    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

#[cfg(test)]
mod alerts_panel_tests {
    use super::*;

    fn row(title: &str, device: &str, prev: bool) -> AlertRow {
        AlertRow {
            doc_id: "document:x".into(),
            title: title.into(),
            device: device.into(),
            finding: "injected instructions in the new content".into(),
            previous_version: prev.then(|| "commit:p".into()),
        }
    }

    #[test]
    fn the_notice_names_the_device_and_the_document_in_plain_words() {
        assert_eq!(row("Plan", "Laptop", true).notice(), "A change from Laptop to \u{201c}Plan\u{201d} looks suspicious");
        assert_eq!(row("", "", true).notice(), "A change from a paired device to \u{201c}a document\u{201d} looks suspicious");
    }

    #[test]
    fn a_first_arrival_has_no_previous_version_button() {
        let r = Rect::new(0.0, 0.0, 600.0, ALERT_ROW_H);
        let with: Vec<_> = alert_row_buttons(r, true).into_iter().map(|(_, b)| b).collect();
        assert_eq!(with, vec![AlertBtn::Open, AlertBtn::Previous, AlertBtn::Dismiss]);
        let without: Vec<_> = alert_row_buttons(r, false).into_iter().map(|(_, b)| b).collect();
        assert_eq!(without, vec![AlertBtn::Open, AlertBtn::Dismiss]);
        for (b, _) in alert_row_buttons(r, true) {
            assert!(b.y1 <= r.y1 && b.y0 >= r.y0 && b.x1 <= r.x1, "inside the row");
        }
    }

    #[test]
    fn the_accessibility_lines_say_which_alert_is_selected_its_actions_and_the_keys() {
        let mut p = AlertsPanel::new(vec![row("A", "Laptop", true), row("B", "Phone", false)]);
        p.move_selection(true, 1000.0);
        let lines = p.a11y_lines();
        assert!(lines[0].starts_with("Alert 1 of 2: A change from Laptop"));
        assert!(lines[0].contains("Open the previous version"));
        assert!(lines[1].contains("Alert 2 of 2, selected"));
        assert!(!lines[1].contains("previous version"), "no previous version for a first arrival");
        assert_eq!(lines.last().unwrap(), ALERTS_KEYS);
        assert!(AlertsPanel::new(vec![]).a11y_lines()[0].starts_with("No alerts"));
    }

    #[test]
    fn selection_stays_in_range_and_in_view() {
        let mut p = AlertsPanel::new((0..5).map(|i| row(&i.to_string(), "d", true)).collect());
        for _ in 0..10 {
            p.move_selection(true, ALERT_ROW_H * 2.0);
        }
        assert_eq!(p.selected, 4);
        assert!(p.scroll >= ALERT_ROW_H * 3.0 - 0.5);
        for _ in 0..10 {
            p.move_selection(false, ALERT_ROW_H * 2.0);
        }
        assert_eq!((p.selected, p.scroll), (0, 0.0));
    }

    #[test]
    fn previous_values_have_no_pending_badge_and_say_their_actions() {
        let p = PreviousValuesPanel::new(vec![PreviousValueRow {
            id: "row_recovery:1".into(),
            label: "Thread: Work".into(),
            device: "Phone".into(),
            when: "2026-10-15 10:00".into(),
            can_restore: true,
        }]);
        let l = p.a11y_lines();
        assert!(l[0].contains("Thread: Work") && l[0].contains("Restore, Remove from list"));
        assert!(!l[0].to_lowercase().contains("review") && !l[0].to_lowercase().contains("pending"));
    }
}

// ---- Devices & Sync (P2P, Batch 6c) --------------------------------------
// Identity block (this device's peer id + listen address + a Sync-now button)
// on top, then the scrolled list of paired devices (each with a Forget button).

/// Fixed (non-scrolled) identity-block height at the top of the body.
pub(crate) const DEVICES_HEADER_H: f64 = 124.0;
/// Per-paired-device row height.
pub(crate) const DEVICE_ROW_H: f64 = 46.0;

/// The "Sync now" button, top-right of the identity block.
pub(crate) fn devices_sync_btn(body: Rect) -> Rect {
    let (bw, bh) = (104.0, 28.0);
    let x1 = body.x1 - 12.0;
    Rect::new(x1 - bw, body.y0 + 14.0, x1, body.y0 + 14.0 + bh)
}

/// The "Pair a device…" button, just left of Sync now.
pub(crate) fn devices_pair_btn(body: Rect) -> Rect {
    let sync = devices_sync_btn(body);
    let (bw, gap) = (124.0, 8.0);
    Rect::new(sync.x0 - gap - bw, sync.y0, sync.x0 - gap, sync.y1)
}

/// Per-row "Forget" button (right-aligned).
pub(crate) fn device_forget_btn(row: Rect) -> Rect {
    let (bw, bh) = (70.0, 22.0);
    let by0 = row.y0 + (row.height() - bh) * 0.5;
    let x1 = row.x1 - 10.0;
    Rect::new(x1 - bw, by0, x1, by0 + bh)
}

pub(crate) struct DeviceRow {
    pub(crate) peer_id: String,
    pub(crate) name: String,
    pub(crate) status: String, // last sync status, else "paired"
}

pub(crate) struct DevicesPanel {
    pub(crate) enabled: bool, // is the node running?
    pub(crate) local_peer_id: String,
    pub(crate) device_name: String,
    pub(crate) listen: String,
    pub(crate) rows: Vec<DeviceRow>,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    title_lbl: Option<Layout<Brush>>,
    name_lbl: Option<Layout<Brush>>,
    id_lbl: Option<Layout<Brush>>,
    listen_lbl: Option<Layout<Brush>>,
    sync_lbl: Option<Layout<Brush>>,
    pair_lbl: Option<Layout<Brush>>,
    forget_lbl: Option<Layout<Brush>>,
    empty_lbl: Option<Layout<Brush>>,
    rows_shaped: Vec<(Layout<Brush>, Layout<Brush>)>, // (name, peer+status meta)
    dirty: bool,                                       // identity strings changed
}

impl DevicesPanel {
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        let mut v = vec![format!(
            "This device {} \u{2014} {} \u{2014} {}",
            self.device_name,
            if self.enabled { "sync running" } else { "sync off" },
            self.local_peer_id
        )];
        if self.rows.is_empty() {
            v.push("No paired devices yet".into());
        } else {
            v.extend(self.rows.iter().map(|r| format!("{} \u{2014} {} \u{2014} {}", r.name, r.peer_id, r.status)));
        }
        v
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        let width_changed = (inner_w - self.inner_w).abs() > 0.5;
        if self.title_lbl.is_none() {
            self.title_lbl = Some(shaper.shape("Devices & Sync", 4000.0, 16.0));
            self.sync_lbl = Some(shaper.shape("Sync now", 200.0, 12.5));
            self.pair_lbl = Some(shaper.shape("Pair a device\u{2026}", 200.0, 12.5));
            self.forget_lbl = Some(shaper.shape("Forget", 80.0, 12.0));
            self.empty_lbl = Some(shaper.shape(
                "No paired devices yet \u{2014} \u{201c}Pair a device\u{2026}\u{201d} shows a code + QR another device scans to join your encrypted sync.",
                560.0,
                13.0,
            ));
        }
        if self.dirty || self.name_lbl.is_none() {
            let status = if self.enabled { "sync running" } else { "sync off" };
            self.name_lbl = Some(shaper.shape(&format!("{}  \u{00b7}  {status}", self.device_name), 4000.0, 14.0));
            self.id_lbl = Some(shaper.shape(&format!("peer  {}", self.local_peer_id), 4000.0, 11.5));
            let listen = if self.listen.is_empty() { "listening\u{2026}".to_string() } else { format!("on  {}", self.listen) };
            self.listen_lbl = Some(shaper.shape(&listen, 4000.0, 11.5));
            self.dirty = false;
        }
        if self.rows_shaped.len() != self.rows.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 110.0).max(80.0) as f32;
            self.rows_shaped = self
                .rows
                .iter()
                .map(|r| {
                    let meta = format!("{}   \u{00b7}   {}", short_peer(&r.peer_id), r.status);
                    (shaper.shape(&r.name, tw, 13.5), shaper.shape(&meta, tw, 11.5))
                })
                .collect();
        }
    }
}

/// Shorten a libp2p PeerId for display: keep the first 8 + last 6 chars.
fn short_peer(id: &str) -> String {
    if id.len() > 18 {
        format!("{}\u{2026}{}", &id[..8], &id[id.len() - 6..])
    } else {
        id.to_string()
    }
}

/// Build an empty/initial Devices panel from the live node identity.
pub(crate) fn devices_panel(
    enabled: bool,
    local_peer_id: String,
    device_name: String,
    listen: String,
    rows: Vec<DeviceRow>,
) -> DevicesPanel {
    DevicesPanel {
        enabled,
        local_peer_id,
        device_name,
        listen,
        rows,
        scroll: 0.0,
        inner_w: 0.0,
        title_lbl: None,
        name_lbl: None,
        id_lbl: None,
        listen_lbl: None,
        sync_lbl: None,
        pair_lbl: None,
        forget_lbl: None,
        empty_lbl: None,
        rows_shaped: Vec::new(),
        dirty: true,
    }
}

pub(crate) fn draw_devices(scene: &mut Scene, dp: &DevicesPanel, bounds: Rect) {
    let (panel_rr, header, _close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(120, 165, 195));
    let dim = pal().text_dim;
    if let Some(l) = &dp.title_lbl {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 14.0)), pal().text);
    }

    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &body_vp);

    // Identity block (fixed, not scrolled).
    let id_block = Rect::new(body_vp.x0, body_vp.y0, body_vp.x1, body_vp.y0 + DEVICES_HEADER_H);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(id_block, 8.0));
    let dot = if dp.enabled { Color::from_rgb8(120, 175, 135) } else { Color::from_rgb8(150, 120, 120) };
    scene.fill(Fill::NonZero, Affine::IDENTITY, dot, None, &vello::kurbo::Circle::new((id_block.x0 + 16.0, id_block.y0 + 24.0), 5.0));
    if let Some(l) = &dp.name_lbl {
        draw_text(scene, l, Affine::translate((id_block.x0 + 30.0, id_block.y0 + 14.0)), pal().text);
    }
    if let Some(l) = &dp.id_lbl {
        draw_text(scene, l, Affine::translate((id_block.x0 + 30.0, id_block.y0 + 42.0)), dim);
    }
    if let Some(l) = &dp.listen_lbl {
        draw_text(scene, l, Affine::translate((id_block.x0 + 30.0, id_block.y0 + 62.0)), dim);
    }

    // Sync now + Pair buttons (top-right of the identity block).
    let sync = devices_sync_btn(body_vp);
    let (sbg, sfg) = if dp.enabled {
        (Color::from_rgb8(46, 64, 78), Color::from_rgb8(214, 230, 240))
    } else {
        (pal().surface, pal().text_dim)
    };
    scene.fill(Fill::NonZero, Affine::IDENTITY, sbg, None, &RoundedRect::from_rect(sync, 6.0));
    if let Some(l) = &dp.sync_lbl {
        let tx = sync.x0 + (sync.width() - l.width() as f64) * 0.5;
        let ty = sync.y0 + (sync.height() - l.height() as f64) * 0.5;
        draw_text(scene, l, Affine::translate((tx, ty)), sfg);
    }
    let pair = devices_pair_btn(body_vp);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(pair, 6.0));
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &RoundedRect::from_rect(pair, 6.0));
    if let Some(l) = &dp.pair_lbl {
        let tx = pair.x0 + (pair.width() - l.width() as f64) * 0.5;
        let ty = pair.y0 + (pair.height() - l.height() as f64) * 0.5;
        draw_text(scene, l, Affine::translate((tx, ty)), pal().text_body);
    }
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(id_block.x0, id_block.y1 - 1.0, id_block.x1, id_block.y1));

    // Paired-device list (scrolled, clipped to below the identity block).
    let list_vp = Rect::new(body_vp.x0, id_block.y1, body_vp.x1, body_vp.y1);
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &list_vp);
    if dp.rows.is_empty() {
        if let Some(e) = &dp.empty_lbl {
            draw_text(scene, e, Affine::translate((list_vp.x0 + 8.0, list_vp.y0 + 10.0)), dim);
        }
    }
    for (i, (name, meta)) in dp.rows_shaped.iter().enumerate() {
        let y = list_vp.y0 - dp.scroll + i as f64 * DEVICE_ROW_H;
        let row = Rect::new(list_vp.x0, y, list_vp.x1, y + DEVICE_ROW_H);
        if row.y1 < list_vp.y0 || row.y0 > list_vp.y1 {
            continue;
        }
        scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(120, 165, 195), None, &Rect::new(row.x0, row.y0 + 6.0, row.x0 + 3.0, row.y1 - 6.0));
        draw_text(scene, name, Affine::translate((row.x0 + 12.0, row.y0 + 6.0)), pal().text);
        draw_text(scene, meta, Affine::translate((row.x0 + 12.0, row.y0 + 25.0)), dim);
        let fb = device_forget_btn(row);
        scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(66, 50, 52), None, &RoundedRect::from_rect(fb, 5.0));
        if let Some(l) = &dp.forget_lbl {
            let tx = fb.x0 + (fb.width() - l.width() as f64) * 0.5;
            let ty = fb.y0 + (fb.height() - l.height() as f64) * 0.5;
            draw_text(scene, l, Affine::translate((tx, ty)), Color::from_rgb8(225, 160, 150));
        }
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().divider, None, &Rect::new(row.x0, row.y1 - 1.0, row.x1, row.y1));
    }
    scene.pop_layer(); // list clip
    draw_scrollbar(scene, &bounds, &list_vp, dp.rows.len().max(1) as f64 * DEVICE_ROW_H + 8.0, dp.scroll);
    scene.pop_layer(); // body clip
    scene.pop_layer(); // panel clip (pushed by draw_window_chrome) — MUST pop or
                       // everything drawn afterwards (taskbar, modals like the
                       // pairing QR) is clipped to this window's rect.
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

/// Shared vertical scrollbar for a panel body viewport.
pub(crate) fn draw_scrollbar(scene: &mut Scene, panel: &Rect, body_vp: &Rect, content_h: f64, scroll: f64) {
    let view_h = body_vp.height();
    if content_h <= view_h + 1.0 {
        return;
    }
    let frac = (view_h / content_h).clamp(0.06, 1.0);
    let sb_h = view_h * frac;
    let max_scroll = (content_h - view_h).max(1.0);
    let sb_y = body_vp.y0 + (scroll / max_scroll) * (view_h - sb_h);
    let sb_x = panel.x1 - 9.0;
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrollbar, None, &RoundedRect::new(sb_x - 2.0, sb_y, sb_x + 2.0, sb_y + sb_h, 2.0));
}

// ---- Chat (native transcript + the first text-input widget) --------------
// Phase 1d.1: the chat UI + composer. The local-model orchestrator is wired
// in-process in 1d.2 (handle_query + the OrchestratorEvent channel).

pub(crate) const CHAT_INPUT_H: f64 = 46.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Role {
    User,
    Assistant,
}
pub(crate) struct ChatMsg {
    pub(crate) role: Role,
    pub(crate) text: String,
}

/// Minimal text-input state: an append/backspace buffer with the caret pinned
/// to the end (arrow-key cursor movement + selection + IME are later polish).
pub(crate) struct ChatPanel {
    pub(crate) msgs: Vec<ChatMsg>,
    pub(crate) input: String,
    pub(crate) scroll: f64,
    pub(crate) inner_w: f64,
    pub(crate) shaped: Vec<(Role, Layout<Brush>, Layout<Brush>)>, // role, "You"/"Assistant", body
    pub(crate) shaped_len: usize,
    pub(crate) input_layout: Option<Layout<Brush>>,
    pub(crate) placeholder: Option<Layout<Brush>>,
    pub(crate) label: Option<Layout<Brush>>,
    pub(crate) pending: bool, // awaiting an orchestrator reply
    pub(crate) loading: bool, // the pending reply is the FIRST one (model loading)
    pub(crate) pending_label: Option<Layout<Brush>>,
    pub(crate) loading_label: Option<Layout<Brush>>,
}
impl ChatPanel {
    pub(crate) fn new() -> Self {
        Self {
            msgs: vec![ChatMsg {
                role: Role::Assistant,
                text: "Native chat — rendered with Vello + parley, no WebView. The local \
                       Qwen orchestrator runs in-process (no Tauri IPC). Type below and press \
                       Enter; the first reply loads the model on CPU, so it can take a moment."
                    .to_string(),
            }],
            input: String::new(),
            scroll: f64::MAX, // start pinned to the bottom
            inner_w: 0.0,
            shaped: Vec::new(),
            shaped_len: 0,
            input_layout: None,
            placeholder: None,
            label: None,
            pending: false,
            loading: false,
            pending_label: None,
            loading_label: None,
        }
    }
    /// Append the user's message and mark a reply pending; returns the text to
    /// hand to the orchestrator (None if the input was blank).
    pub(crate) fn submit(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.msgs.push(ChatMsg { role: Role::User, text: text.clone() });
        self.pending = true;
        self.input.clear();
        self.scroll = f64::MAX; // jump to bottom
        Some(text)
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        if self.label.is_none() {
            self.label = Some(shaper.shape("Chat", 400.0, 18.0));
            self.placeholder = Some(shaper.shape("Type a message\u{2026}   (Enter to send)", 400.0, 14.5));
            self.pending_label = Some(shaper.shape("Assistant is thinking\u{2026}", 400.0, 13.0));
            self.loading_label =
                Some(shaper.shape("Loading the model\u{2026} the first reply takes a moment (CPU)", 400.0, 13.0));
        }
        let width_changed = (inner_w - self.inner_w).abs() >= 0.5;
        if self.shaped_len != self.msgs.len() || width_changed {
            self.inner_w = inner_w;
            let tw = (inner_w - 16.0) as f32;
            self.shaped = self
                .msgs
                .iter()
                .map(|m| {
                    let label = shaper.shape(if m.role == Role::User { "You" } else { "Assistant" }, 200.0, 12.0);
                    (m.role, label, shaper.shape(&m.text, tw, 14.5))
                })
                .collect();
            self.shaped_len = self.msgs.len();
        }
        // The input changes per keystroke — re-shape it every frame (short, cheap).
        self.input_layout = Some(shaper.shape(&self.input, (inner_w - 24.0) as f32, 14.5));
    }
    pub(crate) fn content_h(&self) -> f64 {
        let msgs: f64 = self
            .shaped
            .iter()
            .map(|(_, l, b)| l.height() as f64 + b.height() as f64 + IB_MSG_GAP)
            .sum();
        msgs + if self.pending { 28.0 } else { 0.0 }
    }
}

pub(crate) fn draw_chat(scene: &mut Scene, chat: &ChatPanel, bounds: Rect) {
    let panel = bounds;
    let (panel_rr, header, _close, body) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(110, 150, 120));
    let (msg_vp, input_rect) = chat_split(body);

    if let Some(l) = &chat.label {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 13.0)), pal().text);
    }

    // Transcript (role label colored by speaker: You=blue, Assistant=green).
    let user_c = Color::from_rgb8(130, 175, 215);
    let asst_c = Color::from_rgb8(120, 175, 135);
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &msg_vp);
    let mut y = msg_vp.y0 - chat.scroll;
    for (role, label, body) in &chat.shaped {
        let hh = label.height() as f64;
        let bh = body.height() as f64;
        if y + hh + bh + IB_MSG_GAP >= msg_vp.y0 && y <= msg_vp.y1 {
            let rc = if *role == Role::User { user_c } else { asst_c };
            draw_text(scene, label, Affine::translate((msg_vp.x0, y)), rc);
            draw_text(scene, body, Affine::translate((msg_vp.x0, y + hh + 2.0)), pal().text_body);
        }
        y += hh + bh + IB_MSG_GAP;
    }
    if chat.pending {
        let label = if chat.loading { &chat.loading_label } else { &chat.pending_label };
        if let Some(pl) = label {
            draw_text(scene, pl, Affine::translate((msg_vp.x0, y + 2.0)), Color::from_rgb8(120, 175, 135));
        }
    }
    scene.pop_layer();
    draw_scrollbar(scene, &panel, &msg_vp, chat.content_h(), chat.scroll);

    // Composer (the text input).
    let input_rr = RoundedRect::from_rect(input_rect, 8.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().input, None, &input_rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().input_border, None, &input_rr);
    let ty = input_rect.y0 + (CHAT_INPUT_H - 18.0) * 0.5;
    let tx = input_rect.x0 + 12.0;
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &input_rect);
    if chat.input.is_empty() {
        if let Some(ph) = &chat.placeholder {
            draw_text(scene, ph, Affine::translate((tx, ty)), pal().text_faint);
        }
    } else if let Some(il) = &chat.input_layout {
        draw_text(scene, il, Affine::translate((tx, ty)), pal().text);
    }
    // Caret pinned to the end of the input text.
    let caret_x = tx + chat.input_layout.as_ref().map(|l| l.width() as f64).unwrap_or(0.0);
    scene.stroke(
        &Stroke::new(1.5),
        Affine::IDENTITY,
        pal().caret,
        None,
        &Line::new(Point::new(caret_x + 1.0, input_rect.y0 + 11.0), Point::new(caret_x + 1.0, input_rect.y1 - 11.0)),
    );
    scene.pop_layer();

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Settings (read-only config / stats / keybindings) -------------------

pub(crate) const SET_ROW_H: f64 = 26.0;
pub(crate) const SET_HDR_H: f64 = 38.0;
pub(crate) const SET_TAB_H: f64 = 34.0; // tab-bar strip below the title

pub(crate) struct SettingsRow {
    pub(crate) header: bool,
    pub(crate) key: Layout<Brush>,
    pub(crate) val: Option<Layout<Brush>>,
}
pub(crate) struct SettingsPanel {
    pub(crate) tabs: Vec<String>,                       // tab labels
    pub(crate) active: usize,                           // selected tab
    pub(crate) tab_rows: Vec<Vec<(bool, String, String)>>, // rows per tab
    pub(crate) rows_src: Vec<(bool, String, String)>,  // = tab_rows[active]
    pub(crate) rows: Vec<SettingsRow>,
    pub(crate) tab_layouts: Vec<Layout<Brush>>,        // shaped tab labels
    pub(crate) inner_w: f64,
    pub(crate) scroll: f64,
    pub(crate) label: Option<Layout<Brush>>,
}
impl SettingsPanel {
    /// Build from tabs: each `(label, rows)`. Rows are `(is_header, key, value)`.
    pub(crate) fn new(tabs: Vec<(String, Vec<(bool, String, String)>)>) -> Self {
        let mut labels = Vec::with_capacity(tabs.len());
        let mut rowsets = Vec::with_capacity(tabs.len());
        for (l, r) in tabs {
            labels.push(l);
            rowsets.push(r);
        }
        let rows_src = rowsets.first().cloned().unwrap_or_default();
        Self {
            tabs: labels,
            active: 0,
            tab_rows: rowsets,
            rows_src,
            rows: Vec::new(),
            tab_layouts: Vec::new(),
            inner_w: 0.0,
            scroll: 0.0,
            label: None,
        }
    }
    /// Switch the active tab: swap in its rows, reset scroll, force a reshape.
    pub(crate) fn set_active(&mut self, i: usize) {
        if i < self.tab_rows.len() && i != self.active {
            self.active = i;
            self.rows_src = self.tab_rows[i].clone();
            self.rows.clear();
            self.scroll = 0.0;
            self.inner_w = 0.0;
        }
    }
    /// Readable lines for the accessibility tree.
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        self.rows_src
            .iter()
            .map(|(header, key, val)| {
                if *header || val.is_empty() {
                    key.clone()
                } else {
                    format!("{key}: {val}")
                }
            })
            .collect()
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        if self.label.is_none() {
            self.label = Some(shaper.shape("Settings", 400.0, 18.0));
        }
        if self.tab_layouts.is_empty() {
            self.tab_layouts = self.tabs.iter().map(|t| shaper.shape(t, 160.0, 13.0)).collect();
        }
        if !self.rows.is_empty() && (inner_w - self.inner_w).abs() < 0.5 {
            return;
        }
        self.inner_w = inner_w;
        let val_w = (inner_w - 220.0).max(120.0) as f32;
        self.rows = self
            .rows_src
            .iter()
            .map(|(header, key, val)| {
                if *header {
                    SettingsRow { header: true, key: shaper.shape(key, inner_w as f32, 14.5), val: None }
                } else {
                    SettingsRow {
                        header: false,
                        key: shaper.shape(key, 200.0, 13.0),
                        val: Some(shaper.shape(val, val_w, 13.0)),
                    }
                }
            })
            .collect();
    }
    pub(crate) fn content_h(&self) -> f64 {
        self.rows_src.iter().map(|(h, _, _)| if *h { SET_HDR_H } else { SET_ROW_H }).sum()
    }
}

/// Per-tab clickable rects across the top of the settings body (shared by the
/// painter + the hit-test). Tabs share the width equally.
pub(crate) fn settings_tab_rects(body: Rect, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let tw = body.width() / n as f64;
    (0..n)
        .map(|i| {
            let x0 = body.x0 + i as f64 * tw;
            Rect::new(x0, body.y0, x0 + tw, body.y0 + SET_TAB_H)
        })
        .collect()
}

pub(crate) fn draw_settings(scene: &mut Scene, set: &SettingsPanel, bounds: Rect) {
    let panel = bounds;
    let (panel_rr, header, _close, body_vp) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(150, 140, 110));

    if let Some(l) = &set.label {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 13.0)), pal().text);
    }

    let accent = pal().accent;
    let dim = Color::from_rgb8(140, 146, 158);
    let bright = pal().text;

    // --- Tab bar ---
    let tabs = settings_tab_rects(body_vp, set.tabs.len());
    for (i, r) in tabs.iter().enumerate() {
        let active = i == set.active;
        if active {
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_hover, None, r);
            // accent underline for the active tab
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                accent,
                None,
                &Rect::new(r.x0, r.y1 - 2.0, r.x1, r.y1),
            );
        }
        if let Some(lay) = set.tab_layouts.get(i) {
            let tw = lay.width() as f64;
            let tx = r.x0 + (r.width() - tw) * 0.5;
            let col = if active { pal().text } else { dim };
            draw_text(scene, lay, Affine::translate((tx, r.y0 + 9.0)), col);
        }
    }
    // separator under the tab bar
    scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        pal().divider,
        None,
        &Line::new(Point::new(body_vp.x0, body_vp.y0 + SET_TAB_H), Point::new(body_vp.x1, body_vp.y0 + SET_TAB_H)),
    );

    // --- Content (below the tab bar) ---
    let content_vp = Rect::new(body_vp.x0, body_vp.y0 + SET_TAB_H, body_vp.x1, body_vp.y1);
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &content_vp);
    let mut y = content_vp.y0 + 4.0 - set.scroll;
    for row in &set.rows {
        let rh = if row.header { SET_HDR_H } else { SET_ROW_H };
        if y + rh >= content_vp.y0 && y <= content_vp.y1 {
            if row.header {
                draw_text(scene, &row.key, Affine::translate((content_vp.x0, y + 15.0)), accent);
            } else {
                draw_text(scene, &row.key, Affine::translate((content_vp.x0 + 4.0, y + 4.0)), dim);
                if let Some(v) = &row.val {
                    draw_text(scene, v, Affine::translate((content_vp.x0 + 210.0, y + 4.0)), bright);
                }
            }
        }
        y += rh;
    }
    scene.pop_layer();
    draw_scrollbar(scene, &panel, &content_vp, set.content_h(), set.scroll);

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

// ---- Auth screen (full-screen gate: onboarding / login) ------------------
// The first SECURE text input — password fields render masked. While locked,
// this gate owns the keyboard and the canvas/panels are not drawn.

/// Geometry for the centered auth card + its field boxes (shared by paint + hit-test).
pub(crate) fn auth_layout(w: f64, h: f64, n_fields: usize) -> (Rect, Vec<Rect>) {
    let cw = 480.0_f64.min(w - 80.0);
    let ch = 150.0 + n_fields as f64 * 78.0 + 44.0;
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round();
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let mut fields = Vec::new();
    let mut fy = y0 + 112.0;
    for _ in 0..n_fields {
        fields.push(Rect::new(x0 + 28.0, fy, x0 + cw - 28.0, fy + 38.0));
        fy += 78.0;
    }
    (card, fields)
}

/// The "Recover with guardians" link on the login screen, just below the auth
/// card. Same width as a field row. (F1 Surface 2.)
pub(crate) fn recovery_link_rect(w: f64, h: f64, n_fields: usize) -> Rect {
    let (card, _fields) = auth_layout(w, h, n_fields);
    Rect::new(card.x0 + 28.0, card.y1 + 14.0, card.x1 - 28.0, card.y1 + 40.0)
}

/// The show/hide ("eye") toggle inside a masked field, right-aligned.
pub(crate) fn auth_reveal_btn(field: Rect) -> Rect {
    let bw = 48.0;
    Rect::new(field.x1 - bw - 4.0, field.y0 + 5.0, field.x1 - 4.0, field.y1 - 5.0)
}

pub(crate) struct AuthForm {
    pub(crate) onboarding: bool,
    pub(crate) joining: bool, // Phase 2b: join an existing account from another device
    pub(crate) fields: Vec<String>,
    pub(crate) labels: Vec<&'static str>,
    pub(crate) mask: Vec<bool>, // per-field: render as dots (secrets) vs visible
    pub(crate) reveal: bool,    // show masked fields as plaintext (eye toggle)
    pub(crate) busy: bool,      // pairing handshake in flight — input suppressed
    pub(crate) focus: usize,
    pub(crate) error: Option<String>,
    pub(crate) title: Option<Layout<Brush>>,
    pub(crate) subtitle: Option<Layout<Brush>>,
    pub(crate) hint: Option<Layout<Brush>>,
    pub(crate) busy_lbl: Option<Layout<Brush>>,
    pub(crate) show_lbl: Option<Layout<Brush>>,
    pub(crate) hide_lbl: Option<Layout<Brush>>,
    pub(crate) label_layouts: Vec<Layout<Brush>>,
    pub(crate) field_layouts: Vec<Layout<Brush>>,
    pub(crate) error_layout: Option<Layout<Brush>>,
    pub(crate) error_text: String, // tracks what error_layout was shaped for
}
impl AuthForm {
    pub(crate) fn login() -> Self {
        Self::make(false, false, vec![String::new()], vec!["Password"], vec![true])
    }
    pub(crate) fn onboarding() -> Self {
        Self::make(
            true,
            false,
            vec![String::new(), String::new(), String::new(), String::new()],
            vec![
                "Password",
                "Confirm password",
                "Duress password",
                "Confirm duress password",
            ],
            vec![true, true, true, true],
        )
    }
    /// Phase 2b: join an existing account using a pairing code + PIN from
    /// another device, setting a fresh local password here.
    pub(crate) fn join() -> Self {
        Self::make(
            false,
            true,
            vec![String::new(), String::new(), String::new(), String::new()],
            vec![
                "Offer from the other device (paste: Ctrl+V)",
                "Pairing code",
                "New password for this device",
                "Duress password (optional)",
            ],
            vec![false, false, true, true],
        )
    }
    fn make(
        onboarding: bool,
        joining: bool,
        fields: Vec<String>,
        labels: Vec<&'static str>,
        mask: Vec<bool>,
    ) -> Self {
        Self {
            onboarding,
            joining,
            fields,
            labels,
            mask,
            reveal: false,
            busy: false,
            focus: 0,
            error: None,
            title: None,
            subtitle: None,
            hint: None,
            busy_lbl: None,
            show_lbl: None,
            hide_lbl: None,
            label_layouts: Vec::new(),
            field_layouts: Vec::new(),
            error_layout: None,
            error_text: String::new(),
        }
    }

    /// What to render inside field `i`: dots for secrets, the decoded source
    /// name for the join offer field (not the 200-char code), else the value.
    fn field_display(&self, i: usize) -> String {
        let f = &self.fields[i];
        if self.joining && i == 0 {
            if f.trim().is_empty() {
                return String::new();
            }
            return match crate::p2p::preview_offer(f) {
                Ok(name) => format!("\u{2713} pairing with {name}"),
                Err(e) if e.contains("expired") => {
                    "\u{26a0} offer expired \u{2014} arm a fresh one".into()
                }
                Err(_) => "\u{26a0} not a valid offer \u{2014} copy + paste again".into(),
            };
        }
        if self.mask.get(i).copied().unwrap_or(true) && !self.reveal {
            "\u{2022}".repeat(f.chars().count())
        } else {
            f.clone()
        }
    }

    /// Whether field `i` is a secret (gets a show/hide eye toggle).
    pub(crate) fn is_masked(&self, i: usize) -> bool {
        // The join offer field (i==0) shows a decoded status, never dots.
        if self.joining && i == 0 {
            return false;
        }
        self.mask.get(i).copied().unwrap_or(true)
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper) {
        if self.title.is_none() {
            let (t, s) = if self.joining {
                (
                    "Join from another device",
                    "Paste the code from your other device, enter its PIN, and pick a \
                     password for THIS device.",
                )
            } else if self.onboarding {
                (
                    "Welcome to Sovereign",
                    "Set a password (12+ chars, mixed case, digit, symbol). The duress \
                     password unlocks a separate, empty workspace under coercion.",
                )
            } else {
                ("Unlock Sovereign", "Enter your password to decrypt your workspace.")
            };
            self.title = Some(shaper.shape(t, 460.0, 22.0));
            self.subtitle = Some(shaper.shape(s, 424.0, 13.0));
            let hint = if self.joining {
                "Ctrl+V paste  \u{00b7}  show to reveal  \u{00b7}  Tab to switch  \u{00b7}  Enter to join  \u{00b7}  click below to create a new account instead"
            } else if self.onboarding {
                "Ctrl+V paste  \u{00b7}  show to reveal  \u{00b7}  Tab to switch  \u{00b7}  Enter to create  \u{00b7}  click below to join from another device"
            } else {
                "Ctrl+V to paste your password  \u{00b7}  show to reveal  \u{00b7}  Enter to unlock"
            };
            self.hint = Some(shaper.shape(hint, 460.0, 12.0));
            self.show_lbl = Some(shaper.shape("show", 60.0, 11.5));
            self.hide_lbl = Some(shaper.shape("hide", 60.0, 11.5));
            self.busy_lbl = Some(shaper.shape(
                "Pairing\u{2026}  keep the other device on its pairing screen.",
                424.0,
                13.0,
            ));
            self.label_layouts = self.labels.iter().map(|l| shaper.shape(l, 400.0, 12.0)).collect();
        }
        // Field contents change per keystroke (masked / visible / offer status).
        self.field_layouts = (0..self.fields.len())
            .map(|i| shaper.shape(&self.field_display(i), 400.0, 16.0))
            .collect();
        // Error re-shaped only when it changes.
        let err = self.error.clone().unwrap_or_default();
        if err != self.error_text {
            self.error_text = err.clone();
            self.error_layout = if err.is_empty() { None } else { Some(shaper.shape(&err, 424.0, 12.5)) };
        }
    }
}

pub(crate) fn draw_auth(scene: &mut Scene, form: &AuthForm, w: f64, h: f64) {
    let (card, field_rects) = auth_layout(w, h, form.fields.len());
    // Full-window background.
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(15, 16, 21), None, &Rect::new(0.0, 0.0, w, h));
    // Card.
    let card_rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().panel, None, &card_rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &card_rr);

    let title_c = pal().text;
    let dim = pal().text_dim;
    let accent = pal().accent;
    if let Some(t) = &form.title {
        draw_text(scene, t, Affine::translate((card.x0 + 28.0, card.y0 + 30.0)), title_c);
    }
    if let Some(s) = &form.subtitle {
        draw_text(scene, s, Affine::translate((card.x0 + 28.0, card.y0 + 62.0)), dim);
    }

    for (i, fr) in field_rects.iter().enumerate() {
        if let Some(lbl) = form.label_layouts.get(i) {
            draw_text(scene, lbl, Affine::translate((fr.x0, fr.y0 - 18.0)), dim);
        }
        let focused = i == form.focus;
        let frr = RoundedRect::from_rect(*fr, 8.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, crate::theme::field_fill(&pal()), None, &frr);
        scene.stroke(
            &Stroke::new(if focused { 1.6 } else { 1.0 }),
            Affine::IDENTITY,
            if focused { accent } else { pal().input_border },
            None,
            &frr,
        );
        let tx = fr.x0 + 12.0;
        let ty = fr.y0 + (fr.height() - 16.0) * 0.5;
        if let Some(fl) = form.field_layouts.get(i) {
            draw_text(scene, fl, Affine::translate((tx, ty)), title_c);
            if focused {
                let cx = tx + fl.width() as f64 + 1.0;
                scene.stroke(
                    &Stroke::new(1.5),
                    Affine::IDENTITY,
                    pal().caret,
                    None,
                    &Line::new(Point::new(cx, fr.y0 + 8.0), Point::new(cx, fr.y1 - 8.0)),
                );
            }
        }
        // Secrets get a show/hide reveal toggle at the right edge.
        if form.is_masked(i) {
            let btn = auth_reveal_btn(*fr);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(btn, 5.0));
            let lbl = if form.reveal { &form.hide_lbl } else { &form.show_lbl };
            if let Some(l) = lbl {
                let lx = btn.x0 + (btn.width() - l.width() as f64) * 0.5;
                let ly = btn.y0 + (btn.height() - l.height() as f64) * 0.5;
                draw_text(scene, l, Affine::translate((lx, ly)), pal().text_dim);
            }
        }
    }

    // Busy (pairing handshake) takes the error slot; else error + hint.
    if form.busy {
        if let Some(b) = &form.busy_lbl {
            draw_text(scene, b, Affine::translate((card.x0 + 28.0, card.y1 - 50.0)), accent);
        }
    } else if let Some(e) = &form.error_layout {
        draw_text(scene, e, Affine::translate((card.x0 + 28.0, card.y1 - 50.0)), Color::from_rgb8(220, 110, 110));
    }
    if let Some(hint) = &form.hint {
        draw_text(scene, hint, Affine::translate((card.x0 + 28.0, card.y1 - 26.0)), pal().text_faint);
    }
}

// ---- Email setup form (Batch 6) -----------------------------------------
// A centered modal (not a z-stack window) to enter IMAP/SMTP credentials, save
// the config, and trigger a sync. Field 5 (password) renders masked.

pub(crate) const COMMS_PASSWORD_FIELD: usize = 5;

/// Layout for the email-setup card: (card, field rects, sync btn, save btn,
/// cancel btn). Shared by paint + hit-test.
pub(crate) fn comms_form_layout(w: f64, h: f64) -> (Rect, Vec<Rect>, Rect, Rect, Rect) {
    let n = 6;
    let cw = 520.0_f64.min(w - 80.0);
    let ch = 144.0 + n as f64 * 64.0 + 70.0;
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(36.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let mut fields = Vec::new();
    let mut fy = y0 + 118.0;
    for _ in 0..n {
        fields.push(Rect::new(x0 + 28.0, fy, x0 + cw - 28.0, fy + 34.0));
        fy += 64.0;
    }
    let bw = 96.0;
    let by = card.y1 - 50.0;
    let sync = Rect::new(card.x1 - 28.0 - bw, by, card.x1 - 28.0, by + 32.0);
    let save = Rect::new(sync.x0 - 10.0 - bw, by, sync.x0 - 10.0, by + 32.0);
    let cancel = Rect::new(card.x0 + 28.0, by, card.x0 + 28.0 + bw, by + 32.0);
    (card, fields, sync, save, cancel)
}

pub(crate) struct CommsForm {
    pub(crate) fields: Vec<String>, // imap_host, imap_port, smtp_host, smtp_port, username, password
    pub(crate) labels: Vec<&'static str>,
    pub(crate) focus: usize,
    pub(crate) status: Option<String>,
    pub(crate) syncing: bool,
    title: Option<Layout<Brush>>,
    subtitle: Option<Layout<Brush>>,
    label_layouts: Vec<Layout<Brush>>,
    field_layouts: Vec<Layout<Brush>>,
    sync_lbl: Option<Layout<Brush>>,
    save_lbl: Option<Layout<Brush>>,
    cancel_lbl: Option<Layout<Brush>>,
    status_layout: Option<Layout<Brush>>,
    status_shaped: String,
}
impl CommsForm {
    pub(crate) fn new(prefill: Option<(&str, u16, &str, u16, &str)>) -> Self {
        let (ih, ip, sh, sp, user) = prefill.unwrap_or(("", 993, "", 587, ""));
        Self {
            fields: vec![
                ih.to_string(),
                ip.to_string(),
                sh.to_string(),
                sp.to_string(),
                user.to_string(),
                String::new(),
            ],
            labels: vec!["IMAP host", "IMAP port", "SMTP host", "SMTP port", "Username (email)", "Password (use an app-password if 2FA is on)"],
            focus: 0,
            status: None,
            syncing: false,
            title: None,
            subtitle: None,
            label_layouts: Vec::new(),
            field_layouts: Vec::new(),
            sync_lbl: None,
            save_lbl: None,
            cancel_lbl: None,
            status_layout: None,
            status_shaped: "\0".into(),
        }
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper) {
        if self.title.is_none() {
            self.title = Some(shaper.shape("Email setup", 460.0, 21.0));
            self.subtitle = Some(shaper.shape(
                "IMAP fetches mail; SMTP sends it. The password is kept for this session only \
                 (not written to disk). Tab to switch field.",
                460.0,
                12.5,
            ));
            self.label_layouts = self.labels.iter().map(|l| shaper.shape(l, 400.0, 11.5)).collect();
            self.sync_lbl = Some(shaper.shape("Save & sync", 200.0, 13.0));
            self.save_lbl = Some(shaper.shape("Save", 100.0, 13.0));
            self.cancel_lbl = Some(shaper.shape("Cancel", 100.0, 13.0));
        }
        self.field_layouts = self
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == COMMS_PASSWORD_FIELD {
                    shaper.shape(&"\u{2022}".repeat(f.chars().count()), 400.0, 15.0)
                } else {
                    shaper.shape(f, 400.0, 15.0)
                }
            })
            .collect();
        let st = self.status.clone().unwrap_or_default();
        if st != self.status_shaped {
            self.status_shaped = st.clone();
            self.status_layout = if st.is_empty() { None } else { Some(shaper.shape(&st, 460.0, 12.5)) };
        }
    }
}

pub(crate) fn draw_comms_form(scene: &mut Scene, form: &CommsForm, w: f64, h: f64) {
    let (card, fields, sync, save, cancel) = comms_form_layout(w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);
    let accent = pal().accent;

    if let Some(t) = &form.title {
        draw_text(scene, t, Affine::translate((card.x0 + 28.0, card.y0 + 26.0)), pal().text);
    }
    if let Some(s) = &form.subtitle {
        draw_text(scene, s, Affine::translate((card.x0 + 28.0, card.y0 + 56.0)), pal().text_dim);
    }
    for (i, fr) in fields.iter().enumerate() {
        if let Some(l) = form.label_layouts.get(i) {
            draw_text(scene, l, Affine::translate((fr.x0, fr.y0 - 16.0)), pal().text_dim);
        }
        let focused = i == form.focus;
        let frr = RoundedRect::from_rect(*fr, 7.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().input, None, &frr);
        scene.stroke(&Stroke::new(if focused { 1.6 } else { 1.0 }), Affine::IDENTITY, if focused { accent } else { pal().input_border }, None, &frr);
        let tx = fr.x0 + 11.0;
        let ty = fr.y0 + (fr.height() - 16.0) * 0.5;
        if let Some(fl) = form.field_layouts.get(i) {
            draw_text(scene, fl, Affine::translate((tx, ty)), pal().text);
            if focused {
                let cx = tx + fl.width() as f64 + 1.0;
                scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, pal().caret, None, &Line::new(Point::new(cx, fr.y0 + 7.0), Point::new(cx, fr.y1 - 7.0)));
            }
        }
    }
    if let Some(st) = &form.status_layout {
        let col = if form.syncing { pal().text_dim } else { Color::from_rgb8(150, 185, 150) };
        draw_text(scene, st, Affine::translate((card.x0 + 28.0, card.y1 - 76.0)), col);
    }

    // Buttons.
    let btn = |scene: &mut Scene, r: Rect, lbl: &Option<Layout<Brush>>, bg: Color, fg: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(r, 7.0));
        if let Some(l) = lbl {
            let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
            let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
            draw_text(scene, l, Affine::translate((tx, ty)), fg);
        }
    };
    btn(scene, sync, &form.sync_lbl, accent.with_alpha(0.85), pal().on_accent);
    btn(scene, save, &form.save_lbl, pal().surface_alt, pal().text);
    btn(scene, cancel, &form.cancel_lbl, pal().surface, pal().text_body);
}

// ---- Pairing offer (Batch 6c, Phase 2) ----------------------------------
// Existing-device side: show the armed offer as a scannable QR (the SAME
// base64url PairingOffer the app's generate_pair_qr produces — so a mobile
// device scans the shell to join) + the pairing code (proven live, never in the
// QR). A centered modal; the node runs the wire handshake + emits
// PairingCompleted, which the event translator persists.

/// Layout: (card, qr square, copy button, done button). QR is centered;
/// high-contrast black-on-white (theme colors would break scanners).
pub(crate) fn pairing_modal_layout(w: f64, h: f64) -> (Rect, Rect, Rect, Rect) {
    let cw = 460.0_f64.min(w - 80.0);
    let ch = 564.0_f64.min(h - 60.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(30.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let qr_side = 268.0_f64.min(cw - 96.0);
    let qx0 = (x0 + (cw - qr_side) * 0.5).round();
    let qy0 = y0 + 150.0;
    let qr = Rect::new(qx0, qy0, qx0 + qr_side, qy0 + qr_side);
    let bh = 34.0;
    let by = card.y1 - 50.0;
    let copy = Rect::new(card.x0 + 28.0, by, card.x0 + 28.0 + 130.0, by + bh);
    let done = Rect::new(card.x1 - 28.0 - 110.0, by, card.x1 - 28.0, by + bh);
    (card, qr, copy, done)
}

pub(crate) struct PairingModal {
    pub(crate) pin: String,
    pub(crate) code: String, // base64url offer (Copy button → clipboard for shell↔shell)
    pub(crate) copied: bool, // transient: Copy button flips the label to "Copied!"
    qr_dark: Vec<bool>,      // row-major, width*width; true = dark module
    qr_width: usize,
    title: Option<Layout<Brush>>,
    instr: Option<Layout<Brush>>,
    pin_lbl: Option<Layout<Brush>>,
    footer: Option<Layout<Brush>>,
    copy_lbl: Option<Layout<Brush>>,
    copied_lbl: Option<Layout<Brush>>,
    close_lbl: Option<Layout<Brush>>,
    shaped: bool,
}
impl PairingModal {
    pub(crate) fn new(code: String, pin: String) -> Self {
        let (qr_dark, qr_width) = match qrcode::QrCode::new(code.as_bytes()) {
            Ok(c) => {
                let w = c.width();
                let dark = c.to_colors().into_iter().map(|m| m == qrcode::Color::Dark).collect();
                (dark, w)
            }
            Err(_) => (Vec::new(), 0),
        };
        Self {
            pin,
            code,
            copied: false,
            qr_dark,
            qr_width,
            title: None,
            instr: None,
            pin_lbl: None,
            footer: None,
            copy_lbl: None,
            copied_lbl: None,
            close_lbl: None,
            shaped: false,
        }
    }

    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        vec![
            "Pair a device".into(),
            format!("Pairing code {}", self.pin),
            "Scan the QR on the other device, then enter the pairing code".into(),
        ]
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper) {
        if self.shaped {
            return;
        }
        // Single-use pairing code (10-char, already grouped XXXXX-XXXXX) — show
        // it as-is; the dash makes it readable without per-char spacing.
        self.title = Some(shaper.shape("Pair a device", 4000.0, 18.0));
        self.instr = Some(shaper.shape(
            "On the other device, scan this QR (mobile) or paste the code, then enter this code:",
            400.0,
            13.0,
        ));
        self.pin_lbl = Some(shaper.shape(&self.pin, 4000.0, 30.0));
        self.footer = Some(shaper.shape(
            "Valid for 10 minutes. The code is verified live and never travels in the QR.",
            400.0,
            11.5,
        ));
        self.copy_lbl = Some(shaper.shape("Copy code", 130.0, 13.0));
        self.copied_lbl = Some(shaper.shape("Copied!", 130.0, 13.0));
        self.close_lbl = Some(shaper.shape("Done", 120.0, 13.0));
        self.shaped = true;
    }
}

pub(crate) fn draw_pairing_modal(scene: &mut Scene, m: &PairingModal, w: f64, h: f64) {
    let (card, qr, copy, close) = pairing_modal_layout(w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);

    if let Some(t) = &m.title {
        draw_text(scene, t, Affine::translate((card.x0 + 28.0, card.y0 + 22.0)), pal().text);
    }
    if let Some(i) = &m.instr {
        draw_text(scene, i, Affine::translate((card.x0 + 28.0, card.y0 + 52.0)), pal().text_dim);
    }
    // PIN, centered above the QR.
    if let Some(p) = &m.pin_lbl {
        let tx = card.x0 + (card.width() - p.width() as f64) * 0.5;
        draw_text(scene, p, Affine::translate((tx, card.y0 + 92.0)), Color::from_rgb8(120, 200, 235));
    }

    // QR: black modules on a white card (high contrast for scanners).
    draw_qr_modules(scene, qr, &m.qr_dark, m.qr_width);

    if let Some(f) = &m.footer {
        draw_text(scene, f, Affine::translate((card.x0 + 28.0, qr.y1 + 14.0)), pal().text_dim);
    }
    // Buttons: Copy code (for shell↔shell paste) + Done.
    let btn = |scene: &mut Scene, r: Rect, lbl: &Option<Layout<Brush>>, bg: Color, fg: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(r, 7.0));
        if let Some(l) = lbl {
            let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
            let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
            draw_text(scene, l, Affine::translate((tx, ty)), fg);
        }
    };
    let (copy_lbl, copy_fg) = if m.copied {
        (&m.copied_lbl, Color::from_rgb8(150, 200, 150))
    } else {
        (&m.copy_lbl, pal().text_body)
    };
    btn(scene, copy, copy_lbl, pal().surface, copy_fg);
    btn(scene, close, &m.close_lbl, pal().surface_alt, pal().text);
}

/// Draw a QR as black modules on a white card, with a 3-module white quiet zone.
/// Shared by the pairing and guardian-enrollment modals so the module rendering
/// lives in one place. `qr_dark` is row-major, `width*width`, `true` = dark.
fn draw_qr_modules(scene: &mut Scene, qr: Rect, qr_dark: &[bool], qr_width: usize) {
    let white = Color::from_rgb8(255, 255, 255);
    let black = Color::from_rgb8(0, 0, 0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, white, None, &RoundedRect::from_rect(qr, 6.0));
    if qr_width == 0 {
        return;
    }
    let quiet = 3.0; // modules of white margin on each side
    let modules = qr_width as f64 + quiet * 2.0;
    let cell = qr.width() / modules;
    let origin_x = qr.x0 + quiet * cell;
    let origin_y = qr.y0 + quiet * cell;
    for row in 0..qr_width {
        for col in 0..qr_width {
            if qr_dark[row * qr_width + col] {
                let cx = origin_x + col as f64 * cell;
                let cy = origin_y + row as f64 * cell;
                // +0.5 overdraw avoids hairline seams between cells.
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    black,
                    None,
                    &Rect::new(cx, cy, cx + cell + 0.5, cy + cell + 0.5),
                );
            }
        }
    }
}

/// Build the (row-major dark-module vector, width) for a QR of `payload`.
/// `(Vec::new(), 0)` if encoding fails, which the draw path renders as a blank
/// white card rather than panicking.
fn build_qr(payload: &str) -> (Vec<bool>, usize) {
    match qrcode::QrCode::new(payload.as_bytes()) {
        Ok(c) => {
            let w = c.width();
            let dark = c.to_colors().into_iter().map(|m| m == qrcode::Color::Dark).collect();
            (dark, w)
        }
        Err(_) => (Vec::new(), 0),
    }
}

// ---- Guardian enrollment (F1 Surface 1b) --------------------------------
// The owner arms an in-person offer for one guardian: a QR (the base64url
// GuardianEnrollOffer) + a spoken code the owner reads aloud. The guardian's
// device scans + hears the code and completes the handshake, at which point it
// receives one Shamir share of the Recovery Key. The distinguishing copy — and
// the reason this is not just the pairing modal — is the RECOGNITION PROOF:
// each owner<->guardian pair agrees, out of band, something only they know, so
// the guardian can tell it is really the owner at recovery time. Sovereign
// stores nothing about it (spec §Identity proof). It is the anti-impersonation
// control, not flavour text, so it is required on this screen.

pub(crate) fn guardian_modal_layout(w: f64, h: f64) -> (Rect, Rect, Rect) {
    let cw = 470.0_f64.min(w - 80.0);
    let ch = 648.0_f64.min(h - 40.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(24.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let qr_side = 240.0_f64.min(cw - 120.0);
    let qx0 = (x0 + (cw - qr_side) * 0.5).round();
    let qy0 = y0 + 132.0;
    let qr = Rect::new(qx0, qy0, qx0 + qr_side, qy0 + qr_side);
    let bh = 34.0;
    let done = Rect::new(card.x1 - 28.0 - 120.0, card.y1 - 46.0, card.x1 - 28.0, card.y1 - 46.0 + bh);
    (card, qr, done)
}

pub(crate) struct GuardianEnrollModal {
    pub(crate) code: String, // spoken code (XXXXX-XXXXX)
    qr_dark: Vec<bool>,
    qr_width: usize,
    /// `enrolled_before + 1` .. of 5 — which guardian this offer is for.
    slot_ordinal: usize,
    title: Option<Layout<Brush>>,
    instr: Option<Layout<Brush>>,
    code_lbl: Option<Layout<Brush>>,
    proof_head: Option<Layout<Brush>>,
    proof_body: Option<Layout<Brush>>,
    footer: Option<Layout<Brush>>,
    close_lbl: Option<Layout<Brush>>,
    shaped: bool,
}
impl GuardianEnrollModal {
    /// `qr_payload` is the base64url `GuardianEnrollOffer`; `code` the spoken
    /// code; `slot_ordinal` which of the 5 this is (1-based).
    pub(crate) fn new(qr_payload: String, code: String, slot_ordinal: usize) -> Self {
        let (qr_dark, qr_width) = build_qr(&qr_payload);
        Self {
            code,
            qr_dark,
            qr_width,
            slot_ordinal,
            title: None,
            instr: None,
            code_lbl: None,
            proof_head: None,
            proof_body: None,
            footer: None,
            close_lbl: None,
            shaped: false,
        }
    }

    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        vec![
            format!("Enroll guardian {} of 5", self.slot_ordinal),
            format!("Spoken code {}", self.code),
            "Have them scan the QR, then read the code aloud".into(),
            "Agree a recognition proof out loud: something only the two of you \
             know, that Sovereign never stores"
                .into(),
        ]
    }

    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper) {
        if self.shaped {
            return;
        }
        self.title = Some(shaper.shape(
            &format!("Enroll guardian {} of 5", self.slot_ordinal),
            4000.0,
            18.0,
        ));
        self.instr = Some(shaper.shape(
            "In person: have them scan this QR in their guardian app, then read this code aloud.",
            414.0,
            13.0,
        ));
        self.code_lbl = Some(shaper.shape(&self.code, 4000.0, 30.0));
        self.proof_head = Some(shaper.shape("Agree a recognition proof, out loud", 414.0, 13.5));
        self.proof_body = Some(shaper.shape(
            "Decide together on something only the two of you know \u{2014} a shared \
             memory, a private question, an object \u{2014} that isn't findable online. \
             It is how they will know it is really you if you ever recover. Sovereign \
             never stores it; it lives only between you.",
            414.0,
            12.5,
        ));
        self.footer = Some(shaper.shape(
            "The share goes to them over the live connection, never in the QR. Valid 10 minutes.",
            414.0,
            11.5,
        ));
        self.close_lbl = Some(shaper.shape("Done", 120.0, 13.0));
        self.shaped = true;
    }
}

pub(crate) fn draw_guardian_enroll_modal(scene: &mut Scene, m: &GuardianEnrollModal, w: f64, h: f64) {
    let (card, qr, close) = guardian_modal_layout(w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);

    if let Some(t) = &m.title {
        draw_text(scene, t, Affine::translate((card.x0 + 28.0, card.y0 + 22.0)), pal().text);
    }
    if let Some(i) = &m.instr {
        draw_text(scene, i, Affine::translate((card.x0 + 28.0, card.y0 + 52.0)), pal().text_dim);
    }
    // Spoken code, centered above the QR.
    if let Some(c) = &m.code_lbl {
        let tx = card.x0 + (card.width() - c.width() as f64) * 0.5;
        draw_text(scene, c, Affine::translate((tx, card.y0 + 92.0)), Color::from_rgb8(120, 200, 235));
    }
    draw_qr_modules(scene, qr, &m.qr_dark, m.qr_width);

    // Recognition proof — the required anti-impersonation copy.
    let mut y = qr.y1 + 16.0;
    if let Some(ph) = &m.proof_head {
        draw_text(scene, ph, Affine::translate((card.x0 + 28.0, y)), Color::from_rgb8(210, 180, 120));
        y += 22.0;
    }
    if let Some(pb) = &m.proof_body {
        draw_text(scene, pb, Affine::translate((card.x0 + 28.0, y)), pal().text_body);
        y += 74.0;
    }
    if let Some(f) = &m.footer {
        draw_text(scene, f, Affine::translate((card.x0 + 28.0, y)), pal().text_dim);
    }

    // Done button.
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_alt, None, &RoundedRect::from_rect(close, 7.0));
    if let Some(l) = &m.close_lbl {
        let tx = close.x0 + (close.width() - l.width() as f64) * 0.5;
        let ty = close.y0 + (close.height() - l.height() as f64) * 0.5;
        draw_text(scene, l, Affine::translate((tx, ty)), pal().text);
    }
}

// ---- Guardian access recovery wizard (F1 Surface 2) ---------------------
// Pre-login. The user forgot their passphrase; their guardians release shares
// of the Recovery Key over a multi-day (72h/guardian) window; once ≥threshold
// are in, the user sets a NEW passphrase and the account is re-wrapped +
// unlocked. NO old-password field, NO data-fragment badge (that is Feature 2,
// deferred). The waiting screen is deliberately legible — a live countdown, a
// "checking…" state, and a manual "Check now" — because the first version of
// this wizard (Svelte) polled silently every 45s and read as "stuck" for a
// minute after approvals (0050 #6). Do not rebuild the silent version.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryPhase {
    /// FIRST: set the new password, BEFORE any guardian is contacted
    /// (RECOVERY-001, SEAM A). The collected passphrase seals each guardian
    /// share the moment it arrives, so `access_recovery.json` is never a
    /// plaintext-reconstructable file — the vulnerable window is zero, not 72h.
    /// On resume (relaunch mid-recovery) this phase re-prompts for the same
    /// passphrase to re-derive the sealing key.
    SetPassword,
    /// Waiting for ≥threshold guardians to approve + release.
    Awaiting,
    /// Enough shares — confirm and recover (the password was already set).
    Ready,
    /// Hard failure (bad shares / bundle / at-rest verify); see `error`.
    Failed,
}

pub(crate) struct RecoveryWizard {
    pub(crate) phase: RecoveryPhase,
    /// True when this is a resume (recovery already in progress on disk) — the
    /// SetPassword phase re-prompts for the SAME passphrase to re-derive the
    /// sealing key, rather than starting fresh.
    pub(crate) resuming: bool,
    pub(crate) shares: u8,
    pub(crate) threshold: u8,
    /// Released state per guardian, in roster order.
    pub(crate) released: Vec<bool>,
    /// The new passphrase, collected UP FRONT in the SetPassword phase and held
    /// for the rest of the session (it seals the shares on arrival and unseals
    /// them at finalize).
    pub(crate) new_password: String,
    pub(crate) reveal: bool,
    /// A poll round is in flight — the UI says "checking…" instead of looking
    /// stuck.
    pub(crate) polling: bool,
    /// Seconds until the next automatic poll (App recomputes each frame). `None`
    /// while polling or on a terminal phase.
    pub(crate) next_poll_secs: Option<u64>,
    /// A finalize (reconstruct + re-install) is running.
    pub(crate) finalizing: bool,
    pub(crate) error: Option<String>,
}

impl RecoveryWizard {
    fn blank() -> Self {
        Self {
            phase: RecoveryPhase::SetPassword,
            resuming: false,
            shares: 0,
            threshold: 0,
            released: Vec::new(),
            new_password: String::new(),
            reveal: false,
            polling: false,
            next_poll_secs: None,
            finalizing: false,
            error: None,
        }
    }

    /// Open the wizard for a NEW recovery: set the password first, then start.
    pub(crate) fn fresh() -> Self {
        Self::blank()
    }

    /// Open the wizard to RESUME an in-progress recovery: re-prompt for the same
    /// passphrase (to re-derive the sealing key) before showing the wait again.
    pub(crate) fn resume() -> Self {
        Self { resuming: true, ..Self::blank() }
    }

    /// Fold a fresh status in — called AFTER start/resume succeeds (never while
    /// still on SetPassword). Preserves user-entered fields (new_password).
    pub(crate) fn adopt(&mut self, status: &crate::recovery::RecoveryStatus) {
        use sovereign_p2p::access_recovery::AccessRecoveryPhase as P;
        self.shares = status.shares_collected;
        self.threshold = status.threshold;
        self.released = status.guardians.iter().map(|(_, r)| *r).collect();
        self.error = status.error.clone();
        self.phase = match status.phase {
            P::Failed => RecoveryPhase::Failed,
            _ if status.ready_to_finalize() => RecoveryPhase::Ready,
            _ => RecoveryPhase::Awaiting,
        };
    }

    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        match self.phase {
            RecoveryPhase::SetPassword => vec![
                if self.resuming {
                    "Resume recovery: re-enter the new password you chose".into()
                } else {
                    "Recover access: first, set a new password".into()
                },
                "Your guardians are contacted only after this — the password \
                 protects the collected shares at rest"
                    .into(),
            ],
            RecoveryPhase::Awaiting => vec![
                "Recovering access with guardians".into(),
                format!("{} of {} guardians have approved", self.shares, self.threshold),
                "Each guardian has a 72-hour window to approve or refuse".into(),
                if self.polling { "Checking now".into() } else { "Waiting for the next check".into() },
            ],
            RecoveryPhase::Ready => vec![
                "Enough guardians have approved".into(),
                "Recover now to unlock your account under the new password".into(),
            ],
            RecoveryPhase::Failed => vec![
                "Recovery could not be completed".into(),
                self.error.clone().unwrap_or_default(),
            ],
        }
    }
}

/// (card, password field, primary button, secondary button, check-now button,
/// reveal eye). `check_now` is empty on non-Awaiting phases; `field`/`reveal`
/// are empty off the Ready phase.
pub(crate) fn recovery_wizard_layout(
    w: f64,
    h: f64,
    phase: RecoveryPhase,
) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    let cw = 480.0_f64.min(w - 80.0);
    let ch = 452.0_f64.min(h - 40.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(24.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let bh = 36.0;
    let by = card.y1 - 52.0;
    // Primary right, secondary left of it.
    let primary = Rect::new(card.x1 - 28.0 - 140.0, by, card.x1 - 28.0, by + bh);
    let secondary = Rect::new(card.x0 + 28.0, by, card.x0 + 28.0 + 110.0, by + bh);
    let (field, reveal, check_now) = match phase {
        // Password entry lives here now (up front), not on Ready.
        RecoveryPhase::SetPassword => {
            let f = Rect::new(card.x0 + 28.0, card.y0 + 150.0, card.x1 - 28.0, card.y0 + 188.0);
            let eye = Rect::new(f.x1 - 52.0, f.y0 + 5.0, f.x1 - 4.0, f.y1 - 5.0);
            (f, eye, Rect::ZERO)
        }
        RecoveryPhase::Awaiting => {
            // "Check now" sits inline with the countdown, above the buttons.
            let c = Rect::new(card.x1 - 28.0 - 120.0, card.y1 - 104.0, card.x1 - 28.0, card.y1 - 104.0 + 30.0);
            (Rect::ZERO, Rect::ZERO, c)
        }
        // Ready is now a confirm — the password was already set up front.
        RecoveryPhase::Ready | RecoveryPhase::Failed => (Rect::ZERO, Rect::ZERO, Rect::ZERO),
    };
    (card, field, primary, secondary, check_now, reveal)
}

pub(crate) fn draw_recovery_wizard(
    scene: &mut Scene,
    shaper: &mut TextShaper,
    wiz: &RecoveryWizard,
    w: f64,
    h: f64,
) {
    let (card, field, primary, secondary, check_now, reveal) =
        recovery_wizard_layout(w, h, wiz.phase);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);

    let ix = card.x0 + 28.0;
    let text = |scene: &mut Scene, shaper: &mut TextShaper, s: &str, size: f32, y: f64, c: Color, maxw: f64| {
        let l = shaper.shape(s, maxw as f32, size);
        draw_text(scene, &l, Affine::translate((ix, y)), c);
    };
    let btn = |scene: &mut Scene, shaper: &mut TextShaper, r: Rect, label: &str, bg: Color, fg: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(r, 7.0));
        let l = shaper.shape(label, r.width() as f32, 13.0);
        let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
        let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
        draw_text(scene, &l, Affine::translate((tx, ty)), fg);
    };
    let maxw = card.width() - 56.0;
    let accent = Color::from_rgb8(120, 200, 235);

    match wiz.phase {
        RecoveryPhase::Awaiting => {
            text(scene, shaper, "Recover access with guardians", 18.0, card.y0 + 22.0, pal().text, maxw);
            text(
                scene, shaper,
                "Your guardians are being asked to approve. Each has a 72-hour window; you can close this and come back — recovery keeps running.",
                13.0, card.y0 + 52.0, pal().text_dim, maxw,
            );
            // Big progress line.
            text(
                scene, shaper,
                &format!("{} of {} approvals", wiz.shares, wiz.threshold),
                26.0, card.y0 + 108.0, accent, maxw,
            );
            // Per-guardian dots.
            let mut gy = card.y0 + 160.0;
            for (i, released) in wiz.released.iter().enumerate() {
                let (mark, c) = if *released {
                    ("\u{2713} released", Color::from_rgb8(150, 200, 150))
                } else {
                    ("waiting", pal().text_dim)
                };
                text(scene, shaper, &format!("Guardian {}  \u{00b7}  {mark}", i + 1), 13.0, gy, c, maxw);
                gy += 24.0;
            }
            // Countdown / checking — the legibility fix.
            let status_line = if wiz.polling {
                "checking now\u{2026}".to_string()
            } else if let Some(s) = wiz.next_poll_secs {
                format!("next check in {s}s")
            } else {
                "waiting".to_string()
            };
            text(scene, shaper, &status_line, 12.5, card.y1 - 98.0, pal().text_dim, maxw - 130.0);
            btn(scene, shaper, check_now, if wiz.polling { "Checking…" } else { "Check now" }, pal().surface, pal().text_body);
            btn(scene, shaper, primary, "Close", pal().surface_alt, pal().text);
            // (secondary = Cancel recovery)
            btn(scene, shaper, secondary, "Cancel", pal().surface, pal().text_dim);
        }
        RecoveryPhase::SetPassword => {
            let (title, instr) = if wiz.resuming {
                (
                    "Resume recovery",
                    "Re-enter the new password you chose. Your guardians pick up where they left off.",
                )
            } else {
                (
                    "Recover access — set a new password",
                    "Choose the password you'll recover into. Your guardians are contacted next; \
                     this password protects the shares they send, at rest.",
                )
            };
            text(scene, shaper, title, 18.0, card.y0 + 22.0, pal().text, maxw);
            text(scene, shaper, instr, 13.0, card.y0 + 52.0, pal().text_dim, maxw);
            text(scene, shaper, "New password", 12.0, card.y0 + 126.0, pal().text_dim, maxw);
            // The password field (moved up front from the old Ready phase).
            let fr = RoundedRect::from_rect(field, 7.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &fr);
            scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &fr);
            let shown = if wiz.reveal {
                wiz.new_password.clone()
            } else {
                "\u{2022}".repeat(wiz.new_password.chars().count())
            };
            let fl = shaper.shape(&shown, (field.width() - 60.0) as f32, 15.0);
            draw_text(scene, &fl, Affine::translate((field.x0 + 12.0, field.y0 + 9.0)), pal().text);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_alt, None, &RoundedRect::from_rect(reveal, 5.0));
            let eye = shaper.shape(if wiz.reveal { "hide" } else { "show" }, reveal.width() as f32, 11.0);
            draw_text(scene, &eye, Affine::translate((reveal.x0 + 6.0, reveal.y0 + 6.0)), pal().text_dim);

            if let Some(e) = &wiz.error {
                text(scene, shaper, e, 12.0, card.y0 + 210.0, Color::from_rgb8(230, 130, 130), maxw);
            }
            let plabel = if wiz.finalizing {
                "Working…"
            } else if wiz.resuming {
                "Resume"
            } else {
                "Start recovery"
            };
            btn(scene, shaper, primary, plabel, accent, Color::from_rgb8(10, 20, 26));
            btn(scene, shaper, secondary, "Cancel", pal().surface, pal().text_dim);
        }
        RecoveryPhase::Ready => {
            text(scene, shaper, "Recover now", 18.0, card.y0 + 22.0, pal().text, maxw);
            text(
                scene, shaper,
                &format!("{} of {} guardians approved. Recover to unlock your account under the password you already set.", wiz.shares, wiz.threshold),
                13.0, card.y0 + 52.0, pal().text_dim, maxw,
            );
            if let Some(e) = &wiz.error {
                text(scene, shaper, e, 12.0, card.y0 + 120.0, Color::from_rgb8(230, 130, 130), maxw);
            }
            btn(
                scene, shaper, primary,
                if wiz.finalizing { "Recovering…" } else { "Recover" },
                accent, Color::from_rgb8(10, 20, 26),
            );
            btn(scene, shaper, secondary, "Cancel", pal().surface, pal().text_dim);
        }
        RecoveryPhase::Failed => {
            text(scene, shaper, "Recovery couldn't finish", 18.0, card.y0 + 22.0, pal().text, maxw);
            text(
                scene, shaper,
                wiz.error.as_deref().unwrap_or("Something went wrong reaching your guardians."),
                13.0, card.y0 + 56.0, Color::from_rgb8(230, 130, 130), maxw,
            );
            text(
                scene, shaper,
                "Your account was left unchanged — your old password still works if you remember it.",
                12.5, card.y0 + 120.0, pal().text_dim, maxw,
            );
            btn(scene, shaper, primary, "Start over", pal().surface_alt, pal().text);
            btn(scene, shaper, secondary, "Close", pal().surface, pal().text_dim);
        }
    }
}

// ---- Injection-decision gate (INJECTION-002) ----------------------------
// A high-severity injection was found in agent-loop tool output; the loop is
// PAUSED and the user chooses. The `preview` is UNTRUSTED attacker content shown
// verbatim so the user can judge — it is rendered as PLAIN TEXT (never markup,
// no interpretation) and clipped, so the preview itself can do nothing (the
// render-path concern Fable 5 flagged in 0071).

pub(crate) struct InjectionPrompt {
    pub(crate) source: String,
    pub(crate) severity: u8,
    pub(crate) indicators: Vec<String>,
    pub(crate) preview: String,
}

/// (panel, redact btn, pass-through btn, abort btn).
pub(crate) fn injection_prompt_geom(w: f64, h: f64) -> (Rect, Rect, Rect, Rect) {
    let cw = 540.0_f64.min(w - 80.0);
    let ch = 440.0_f64.min(h - 40.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(24.0);
    let panel = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let bh = 36.0;
    let by = panel.y1 - 52.0;
    let gap = 8.0;
    let bw = ((cw - 56.0) - gap * 2.0) / 3.0;
    let redact = Rect::new(panel.x0 + 28.0, by, panel.x0 + 28.0 + bw, by + bh);
    let pass = Rect::new(redact.x1 + gap, by, redact.x1 + gap + bw, by + bh);
    let abort = Rect::new(pass.x1 + gap, by, panel.x1 - 28.0, by + bh);
    (panel, redact, pass, abort)
}

/// Collapse to a single line and clip so untrusted preview text can't blow the
/// layout or inject control characters into the render.
fn one_line_clip(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let keep: String = flat.chars().take(max.saturating_sub(1)).collect();
        format!("{keep}\u{2026}")
    }
}

pub(crate) fn draw_injection_prompt(
    scene: &mut Scene,
    shaper: &mut TextShaper,
    p: &InjectionPrompt,
    w: f64,
    h: f64,
) {
    let (panel, redact, pass, abort) = injection_prompt_geom(w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(panel, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);

    let ix = panel.x0 + 28.0;
    let maxw = (panel.width() - 56.0) as f32;
    let mut line = |scene: &mut Scene, shaper: &mut TextShaper, s: &str, size: f32, y: f64, c: Color| {
        let l = shaper.shape(s, maxw, size);
        draw_text(scene, &l, Affine::translate((ix, y)), c);
    };
    let warn = Color::from_rgb8(230, 160, 90);

    line(scene, shaper, "Possible prompt injection in tool output", 18.0, panel.y0 + 22.0, warn);
    line(
        scene, shaper,
        &format!(
            "From {}  \u{00b7}  severity {}/10. It may be trying to steer the assistant. Choose what reaches the model.",
            one_line_clip(&p.source, 48),
            p.severity
        ),
        13.0, panel.y0 + 52.0, pal().text_dim,
    );
    // Indicators (bounded).
    let ind = if p.indicators.is_empty() {
        "flagged by the injection scanner".to_string()
    } else {
        one_line_clip(&p.indicators.join(", "), 70)
    };
    line(scene, shaper, &format!("Signals: {ind}"), 12.5, panel.y0 + 88.0, pal().text_body);

    // Untrusted preview — plain text, boxed + clipped.
    line(scene, shaper, "Flagged content (preview):", 12.0, panel.y0 + 122.0, pal().text_dim);
    let pv = Rect::new(panel.x0 + 28.0, panel.y0 + 144.0, panel.x1 - 28.0, panel.y0 + 300.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(pv, 7.0));
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &RoundedRect::from_rect(pv, 7.0));
    // Wrap the preview to the box width, a few clipped lines. shape() wraps at
    // maxw; draw_text renders it as text only (no markup interpretation).
    let pv_layout = shaper.shape(&one_line_clip(&p.preview, 500), (pv.width() - 24.0) as f32, 12.5);
    draw_text(scene, &pv_layout, Affine::translate((pv.x0 + 12.0, pv.y0 + 10.0)), pal().text_body);

    let btn = |scene: &mut Scene, shaper: &mut TextShaper, r: Rect, label: &str, bg: Color, fg: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(r, 7.0));
        let l = shaper.shape(label, r.width() as f32, 12.5);
        let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
        let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
        draw_text(scene, &l, Affine::translate((tx, ty)), fg);
    };
    // Redact is the safe default (accent); pass-through is the riskier choice.
    btn(scene, shaper, redact, "Redact & continue", Color::from_rgb8(120, 200, 235), Color::from_rgb8(10, 20, 26));
    btn(scene, shaper, pass, "Pass through", pal().surface_alt, pal().text);
    btn(scene, shaper, abort, "Abort turn", pal().surface, pal().text_dim);
}

// ---- Email compose (Batch 6b) -------------------------------------------
// A centered modal: To / Subject (single-line) + Body (multi-line, end caret),
// with Send / Cancel. Sends via SMTP through comms::send_email.

/// Layout for the compose card: (card, to, subject, body, send btn, cancel btn).
pub(crate) fn compose_form_layout(w: f64, h: f64) -> (Rect, Rect, Rect, Rect, Rect, Rect) {
    let cw = 600.0_f64.min(w - 80.0);
    let ch = 470.0_f64.min(h - 80.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(36.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let ix0 = x0 + 28.0;
    let ix1 = x0 + cw - 28.0;
    let to = Rect::new(ix0, y0 + 80.0, ix1, y0 + 114.0);
    let subject = Rect::new(ix0, y0 + 148.0, ix1, y0 + 182.0);
    let body = Rect::new(ix0, y0 + 216.0, ix1, card.y1 - 64.0);
    let bw = 96.0;
    let by = card.y1 - 48.0;
    let send = Rect::new(card.x1 - 28.0 - bw, by, card.x1 - 28.0, by + 32.0);
    let cancel = Rect::new(card.x0 + 28.0, by, card.x0 + 28.0 + bw, by + 32.0);
    (card, to, subject, body, send, cancel)
}

pub(crate) struct ComposeForm {
    pub(crate) to: String,
    pub(crate) subject: String,
    pub(crate) body: String,
    pub(crate) focus: usize, // 0 = to, 1 = subject, 2 = body
    pub(crate) status: Option<String>,
    pub(crate) sending: bool,
    title: Option<Layout<Brush>>,
    to_lbl: Option<Layout<Brush>>,
    subject_lbl: Option<Layout<Brush>>,
    body_lbl: Option<Layout<Brush>>,
    send_lbl: Option<Layout<Brush>>,
    cancel_lbl: Option<Layout<Brush>>,
    to_layout: Option<Layout<Brush>>,
    subject_layout: Option<Layout<Brush>>,
    body_layout: Option<Layout<Brush>>,
    status_layout: Option<Layout<Brush>>,
    status_shaped: String,
    inner_w: f64,
}
impl ComposeForm {
    pub(crate) fn new(to: String, subject: String) -> Self {
        Self {
            to,
            subject,
            body: String::new(),
            focus: 0,
            status: None,
            sending: false,
            title: None,
            to_lbl: None,
            subject_lbl: None,
            body_lbl: None,
            send_lbl: None,
            cancel_lbl: None,
            to_layout: None,
            subject_layout: None,
            body_layout: None,
            status_layout: None,
            status_shaped: "\0".into(),
            inner_w: 0.0,
        }
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, body_w: f64) {
        if self.title.is_none() {
            self.title = Some(shaper.shape("New message", 460.0, 21.0));
            self.to_lbl = Some(shaper.shape("To", 200.0, 11.5));
            self.subject_lbl = Some(shaper.shape("Subject", 200.0, 11.5));
            self.body_lbl = Some(shaper.shape("Message  (Tab to switch field \u{00b7} Ctrl+Enter to send)", 400.0, 11.5));
            self.send_lbl = Some(shaper.shape("Send", 100.0, 13.0));
            self.cancel_lbl = Some(shaper.shape("Cancel", 100.0, 13.0));
        }
        self.inner_w = body_w;
        self.to_layout = Some(shaper.shape(&self.to, 4000.0, 14.5));
        self.subject_layout = Some(shaper.shape(&self.subject, 4000.0, 14.5));
        self.body_layout = Some(shaper.shape(&self.body, (body_w - 20.0).max(60.0) as f32, 14.5));
        let st = self.status.clone().unwrap_or_default();
        if st != self.status_shaped {
            self.status_shaped = st.clone();
            self.status_layout = if st.is_empty() { None } else { Some(shaper.shape(&st, 460.0, 12.5)) };
        }
    }
}

pub(crate) fn draw_compose_form(scene: &mut Scene, form: &ComposeForm, w: f64, h: f64) {
    let (card, to, subject, body, send, cancel) = compose_form_layout(w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);
    let accent = pal().accent;
    if let Some(t) = &form.title {
        draw_text(scene, t, Affine::translate((card.x0 + 28.0, card.y0 + 26.0)), pal().text);
    }

    let field = |scene: &mut Scene, r: Rect, lbl: &Option<Layout<Brush>>, layout: &Option<Layout<Brush>>, idx: usize, multiline: bool| {
        if let Some(l) = lbl {
            draw_text(scene, l, Affine::translate((r.x0, r.y0 - 16.0)), pal().text_dim);
        }
        let focused = idx == form.focus;
        let frr = RoundedRect::from_rect(r, 7.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().input, None, &frr);
        scene.stroke(&Stroke::new(if focused { 1.6 } else { 1.0 }), Affine::IDENTITY, if focused { accent } else { pal().input_border }, None, &frr);
        scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &r);
        if let Some(fl) = layout {
            let ty = if multiline { r.y0 + 7.0 } else { r.y0 + (r.height() - 16.0) * 0.5 };
            draw_text(scene, fl, Affine::translate((r.x0 + 11.0, ty)), pal().text);
            if focused {
                let (cx, cy) = crate::text::end_caret(fl, 14.5);
                let caret_x = r.x0 + 11.0 + if multiline { cx } else { fl.width() as f64 };
                let caret_y = if multiline { ty + cy } else { r.y0 + r.height() * 0.5 };
                let half = if multiline { 8.0 } else { (r.height() * 0.5 - 7.0).max(6.0) };
                scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, pal().caret, None, &Line::new(Point::new(caret_x + 1.0, caret_y - half), Point::new(caret_x + 1.0, caret_y + 4.0)));
            }
        }
        scene.pop_layer();
    };
    field(scene, to, &form.to_lbl, &form.to_layout, 0, false);
    field(scene, subject, &form.subject_lbl, &form.subject_layout, 1, false);
    field(scene, body, &form.body_lbl, &form.body_layout, 2, true);

    if let Some(st) = &form.status_layout {
        let col = if form.sending { pal().text_dim } else { Color::from_rgb8(150, 185, 150) };
        draw_text(scene, st, Affine::translate((card.x0 + 28.0, send.y0 - 22.0)), col);
    }
    let btn = |scene: &mut Scene, r: Rect, lbl: &Option<Layout<Brush>>, bg: Color, fg: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(r, 7.0));
        if let Some(l) = lbl {
            let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
            let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
            draw_text(scene, l, Affine::translate((tx, ty)), fg);
        }
    };
    btn(scene, send, &form.send_lbl, accent.with_alpha(0.85), pal().on_accent);
    btn(scene, cancel, &form.cancel_lbl, pal().surface, pal().text_body);
}

// ---- Search (title filter over loaded cards) -----------------------------

pub(crate) const SEARCH_ROW_H: f64 = 30.0;

pub(crate) struct SearchPanel {
    pub(crate) query: String,
    pub(crate) results: Vec<(usize, String)>, // (card index, title)
    pub(crate) scroll: f64,
    inner_w: f64,
    results_len_shaped: usize,
    query_shaped: String,
    label: Option<Layout<Brush>>,
    placeholder: Option<Layout<Brush>>,
    hint: Option<Layout<Brush>>,
    query_layout: Option<Layout<Brush>>,
    result_layouts: Vec<Layout<Brush>>,
}
impl SearchPanel {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            scroll: 0.0,
            inner_w: 0.0,
            results_len_shaped: usize::MAX,
            query_shaped: "\0".into(),
            label: None,
            placeholder: None,
            hint: None,
            query_layout: None,
            result_layouts: Vec::new(),
        }
    }
    pub(crate) fn ensure_shaped(&mut self, shaper: &mut TextShaper, inner_w: f64) {
        if self.label.is_none() {
            self.label = Some(shaper.shape("Search", 400.0, 18.0));
            self.placeholder = Some(shaper.shape("Search documents by title\u{2026}  (Enter to ask the AI)", 600.0, 14.5));
            self.hint = Some(shaper.shape("\u{2191}/\u{2193} unused \u{00b7} click to open \u{00b7} Enter \u{2192} ask AI \u{00b7} Esc to close", 600.0, 12.0));
        }
        if self.query != self.query_shaped {
            self.query_shaped = self.query.clone();
            self.query_layout = Some(shaper.shape(&self.query, (inner_w - 24.0).max(50.0) as f32, 14.5));
        }
        if self.results.len() != self.results_len_shaped || (inner_w - self.inner_w).abs() >= 0.5 {
            self.inner_w = inner_w;
            self.results_len_shaped = self.results.len();
            let tw = (inner_w - 16.0) as f32;
            self.result_layouts = self.results.iter().map(|(_, t)| shaper.shape(t, tw, 13.5)).collect();
        }
    }
    pub(crate) fn content_h(&self) -> f64 {
        self.results.len() as f64 * SEARCH_ROW_H
    }
}

pub(crate) fn draw_search(scene: &mut Scene, sp: &SearchPanel, bounds: Rect) {
    let panel = bounds;
    let (panel_rr, header, _close, body) =
        draw_window_chrome(scene, bounds, Color::from_rgb8(120, 150, 180));
    let (input, results) = search_split(body);

    if let Some(l) = &sp.label {
        draw_text(scene, l, Affine::translate((header.x0 + DW_PAD, header.y0 + 13.0)), pal().text);
    }

    // Query input box.
    let input_rr = RoundedRect::from_rect(input, 8.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, crate::theme::field_fill(&pal()), None, &input_rr);
    scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, Color::from_rgb8(120, 150, 180), None, &input_rr);
    let ty = input.y0 + (input.height() - 16.0) * 0.5;
    let tx = input.x0 + 12.0;
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &input);
    if sp.query.is_empty() {
        if let Some(ph) = &sp.placeholder {
            draw_text(scene, ph, Affine::translate((tx, ty)), pal().text_faint);
        }
    } else if let Some(q) = &sp.query_layout {
        draw_text(scene, q, Affine::translate((tx, ty)), pal().text);
    }
    let caret_x = tx + sp.query_layout.as_ref().map(|l| l.width() as f64).unwrap_or(0.0);
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, pal().caret, None, &Line::new(Point::new(caret_x + 1.0, input.y0 + 10.0), Point::new(caret_x + 1.0, input.y1 - 10.0)));
    scene.pop_layer();

    // Results.
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &results);
    if sp.results.is_empty() {
        let msg = if sp.query.is_empty() { sp.placeholder.as_ref() } else { sp.hint.as_ref() };
        // (placeholder already shown in the box; for empty results just show the hint)
        if !sp.query.is_empty() {
            if let Some(h) = &sp.hint {
                let _ = msg;
                draw_text(scene, h, Affine::translate((results.x0 + 4.0, results.y0 + 8.0)), pal().text_faint);
            }
        }
    } else {
        for (i, rl) in sp.result_layouts.iter().enumerate() {
            let ry = results.y0 - sp.scroll + i as f64 * SEARCH_ROW_H;
            if ry + SEARCH_ROW_H < results.y0 || ry > results.y1 {
                continue;
            }
            draw_text(scene, rl, Affine::translate((results.x0 + 8.0, ry + 6.0)), pal().text);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &Rect::new(results.x0, ry + SEARCH_ROW_H - 1.0, results.x1, ry + SEARCH_ROW_H));
        }
    }
    scene.pop_layer();
    draw_scrollbar(scene, &panel, &results, sp.content_h(), sp.scroll);

    scene.pop_layer(); // panel clip
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &panel_rr);
}

#[cfg(test)]
mod settings_tab_tests {
    use super::*;

    fn panel() -> SettingsPanel {
        SettingsPanel::new(vec![
            ("Profile".into(), vec![(true, "A".into(), String::new()), (false, "x".into(), "1".into())]),
            ("AI".into(), vec![(false, "y".into(), "2".into())]),
            ("Security".into(), vec![(false, "z".into(), "3".into())]),
        ])
    }

    #[test]
    fn starts_on_first_tab() {
        let p = panel();
        assert_eq!(p.active, 0);
        assert_eq!(p.tabs.len(), 3);
        assert_eq!(p.rows_src.len(), 2); // Profile's rows
    }

    #[test]
    fn set_active_swaps_rows_and_resets_scroll() {
        let mut p = panel();
        p.scroll = 40.0;
        p.set_active(1);
        assert_eq!(p.active, 1);
        assert_eq!(p.rows_src, p.tab_rows[1]);
        assert_eq!(p.rows_src.first().map(|(_, k, _)| k.as_str()), Some("y"));
        assert_eq!(p.scroll, 0.0, "switching tabs resets scroll");
    }

    #[test]
    fn set_active_ignores_out_of_range() {
        let mut p = panel();
        p.set_active(99);
        assert_eq!(p.active, 0);
    }

    #[test]
    fn tab_rects_cover_body_width_in_order() {
        let body = Rect::new(10.0, 20.0, 710.0, 600.0);
        let rects = settings_tab_rects(body, 7);
        assert_eq!(rects.len(), 7);
        assert!((rects[0].x0 - body.x0).abs() < 1e-9);
        assert!((rects[6].x1 - body.x1).abs() < 1e-6);
        assert!(rects.windows(2).all(|w| (w[0].x1 - w[1].x0).abs() < 1e-6), "tabs are contiguous");
        assert!(rects.iter().all(|r| (r.y1 - r.y0 - SET_TAB_H).abs() < 1e-9));
    }
}

#[cfg(test)]
mod inbox_layout_tests {
    use super::*;

    #[test]
    fn detail_regions_are_contiguous_and_in_body() {
        let body = Rect::new(0.0, 100.0, 480.0, 600.0);
        let (addrs, tabs, thread) = inbox_detail_regions(body, 2, 2);
        // top→bottom, contiguous, fully inside the body
        assert!((addrs.y0 - body.y0).abs() < 1e-9);
        assert!((tabs.y0 - addrs.y1).abs() < 1e-9);
        assert!((thread.y0 - tabs.y1).abs() < 1e-9);
        assert!((thread.y1 - body.y1).abs() < 1e-9);
        assert!((tabs.height() - IB_TAB_H).abs() < 1e-9, "2 convs -> a tab strip");
    }

    #[test]
    fn single_conversation_has_no_tab_strip() {
        let body = Rect::new(0.0, 0.0, 480.0, 500.0);
        let (_a, tabs, _t) = inbox_detail_regions(body, 1, 1);
        assert_eq!(tabs.height(), 0.0, "1 conv -> no tabs");
    }

    #[test]
    fn tab_rects_split_strip_equally() {
        let tabs = Rect::new(0.0, 0.0, 300.0, IB_TAB_H);
        let rects = inbox_tab_rects(tabs, 3);
        assert_eq!(rects.len(), 3);
        assert!((rects[0].width() - 100.0).abs() < 1e-6);
        assert!((rects[2].x1 - tabs.x1).abs() < 1e-6);
        assert!(rects.windows(2).all(|w| (w[0].x1 - w[1].x0).abs() < 1e-6));
    }
}

#[cfg(test)]
mod doc_edit_tests {
    use super::*;

    #[test]
    fn pii_row_buttons_are_ordered_and_in_bounds() {
        let row = Rect::new(0.0, 0.0, 600.0, 50.0);
        let [(keep, kk), (dis, dk), (del, xk)] = pii_row_buttons(row);
        assert_eq!(kk, PiiBtn::Confirm);
        assert_eq!(dk, PiiBtn::Dismiss);
        assert_eq!(xk, PiiBtn::Delete);
        assert!(keep.x1 <= dis.x0 && dis.x1 <= del.x0, "Keep < Dismiss < Del");
        assert!(keep.x0 >= row.x0 && del.x1 <= row.x1, "within row");
    }

    #[test]
    fn model_row_buttons_are_ordered_and_in_bounds() {
        let row = Rect::new(0.0, 100.0, 500.0, 152.0);
        let [(r, rk), (q, qk), (del, dk)] = model_row_buttons(row);
        assert_eq!(rk, ModelBtn::Router);
        assert_eq!(qk, ModelBtn::Reasoning);
        assert_eq!(dk, ModelBtn::Delete);
        // Left→right: R, then Q, then Del; none overlap; all inside the row.
        assert!(r.x1 <= q.x0, "R is left of Q");
        assert!(q.x1 <= del.x0, "Q is left of Del");
        assert!(r.x0 >= row.x0 && del.x1 <= row.x1, "buttons stay within the row");
        // Vertically centered within the row.
        assert!((r.y0 - row.y0 - (del.y0 - row.y0)).abs() < 0.01);
        assert!(r.y0 > row.y0 && r.y1 < row.y1);
    }
}

// ---- Orchestrator bubble styles + picker (Batch 6c cosmetic) -------------
// The 9 BubbleStyle variants. The web renders them as SMIL-animated SVGs; Vello
// has no SVG-animation runtime, so these are STATIC re-creations for now (the
// procedural-animation pass is deferred). Drawn within radius `r` at center `c`
// in `accent` + `on` (highlight) palette colors, so they theme with the light
// theme later. Shared by the live bubble (app::draw_bubble) + the picker swatches.
/// Each bubble style's signature color, lifted from the Tauri BubblePreview SVGs.
/// These ENCODE the style (like lane hues), so they're constant across themes.
pub(crate) fn bubble_style_color(style: BubbleStyle) -> Color {
    match style {
        BubbleStyle::Icon => Color::from_rgb8(217, 119, 6),    // #D97706 brand orange
        BubbleStyle::Wave => Color::from_rgb8(59, 130, 246),   // #3B82F6 blue
        BubbleStyle::Spin => Color::from_rgb8(124, 58, 237),   // #7C3AED violet
        BubbleStyle::Pulse => Color::from_rgb8(219, 39, 119),  // #DB2777 pink
        BubbleStyle::Blink => Color::from_rgb8(245, 158, 11),  // #F59E0B amber
        BubbleStyle::Rings => Color::from_rgb8(16, 185, 129),  // #10B981 emerald
        BubbleStyle::Matrix => Color::from_rgb8(34, 197, 94),  // #22C55E green
        BubbleStyle::Orbit => Color::from_rgb8(245, 158, 11),  // #F59E0B amber
        BubbleStyle::Morph => Color::from_rgb8(139, 92, 246),  // #8B5CF6 violet
    }
}

pub(crate) fn draw_bubble_style(scene: &mut Scene, style: BubbleStyle, c: Point, r: f64, accent: Color, on: Color) {
    let dot = |scene: &mut Scene, center: Point, rad: f64, col: Color| {
        scene.fill(Fill::NonZero, Affine::IDENTITY, col, None, &Circle::new(center, rad));
    };
    match style {
        BubbleStyle::Icon => {
            dot(scene, c, r, accent);
            // 4-point sparkle.
            let s = r * 0.52;
            let mut p = BezPath::new();
            p.move_to((c.x, c.y - s));
            p.quad_to((c.x + s * 0.16, c.y - s * 0.16), (c.x + s, c.y));
            p.quad_to((c.x + s * 0.16, c.y + s * 0.16), (c.x, c.y + s));
            p.quad_to((c.x - s * 0.16, c.y + s * 0.16), (c.x - s, c.y));
            p.quad_to((c.x - s * 0.16, c.y - s * 0.16), (c.x, c.y - s));
            p.close_path();
            scene.fill(Fill::NonZero, Affine::IDENTITY, on.with_alpha(0.85), None, &p);
        }
        BubbleStyle::Pulse => {
            dot(scene, c, r, accent.with_alpha(0.18));
            dot(scene, c, r * 0.72, accent.with_alpha(0.34));
            dot(scene, c, r * 0.44, accent);
            dot(scene, c, r * 0.2, on.with_alpha(0.5));
        }
        BubbleStyle::Spin => {
            dot(scene, c, r, accent.with_alpha(0.22));
            let n = 8;
            for i in 0..n {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                let d = Point::new(c.x + a.cos() * r * 0.72, c.y + a.sin() * r * 0.72);
                let alpha = 0.25 + 0.75 * (i as f32 / (n - 1) as f32);
                dot(scene, d, r * 0.11, accent.with_alpha(alpha));
            }
        }
        BubbleStyle::Wave => {
            dot(scene, c, r, accent.with_alpha(0.9));
            scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Circle::new(c, r));
            for k in 0..3 {
                let yo = c.y + (k as f64 - 1.0) * r * 0.42;
                let mut p = BezPath::new();
                let steps = 24;
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let x = c.x - r + t * 2.0 * r;
                    let y = yo + (t * std::f64::consts::TAU * 1.5).sin() * r * 0.13;
                    if s == 0 {
                        p.move_to((x, y));
                    } else {
                        p.line_to((x, y));
                    }
                }
                scene.stroke(&Stroke::new(2.2), Affine::IDENTITY, on.with_alpha(0.7), None, &p);
            }
            scene.pop_layer();
        }
        BubbleStyle::Blink => {
            dot(scene, c, r, accent);
            scene.fill(Fill::NonZero, Affine::IDENTITY, on.with_alpha(0.9), None, &Ellipse::new(c, (r * 0.62, r * 0.34), 0.0));
            dot(scene, c, r * 0.2, accent);
        }
        BubbleStyle::Rings => {
            for (rad, wdt, al) in [(0.9, 2.5, 0.5), (0.62, 2.0, 0.7), (0.34, 2.0, 0.9)] {
                scene.stroke(&Stroke::new(wdt), Affine::IDENTITY, accent.with_alpha(al), None, &Circle::new(c, r * rad));
            }
            dot(scene, c, r * 0.12, accent);
        }
        BubbleStyle::Matrix => {
            let bg = RoundedRect::new(c.x - r, c.y - r, c.x + r, c.y + r, r * 0.38);
            scene.fill(Fill::NonZero, Affine::IDENTITY, accent.with_alpha(0.16), None, &bg);
            scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &bg);
            let n = 4;
            let cell = r * 2.0 / n as f64;
            let pad = cell * 0.2;
            for gy in 0..n {
                for gx in 0..n {
                    let al = 0.15 + 0.8 * (((gx * 7 + gy * 13) % 5) as f32 / 4.0);
                    let x0 = c.x - r + gx as f64 * cell + pad;
                    let y0 = c.y - r + gy as f64 * cell + pad;
                    scene.fill(Fill::NonZero, Affine::IDENTITY, accent.with_alpha(al), None, &Rect::new(x0, y0, x0 + cell - 2.0 * pad, y0 + cell - 2.0 * pad));
                }
            }
            scene.pop_layer();
        }
        BubbleStyle::Orbit => {
            dot(scene, c, r, accent.with_alpha(0.12));
            scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, accent.with_alpha(0.5), None, &Ellipse::new(c, (r * 0.85, r * 0.42), 0.6));
            scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, accent.with_alpha(0.5), None, &Ellipse::new(c, (r * 0.85, r * 0.42), -0.6));
            dot(scene, c, r * 0.26, accent);
            dot(scene, Point::new(c.x + r * 0.68, c.y - r * 0.22), r * 0.12, accent);
            dot(scene, Point::new(c.x - r * 0.68, c.y + r * 0.22), r * 0.1, accent.with_alpha(0.7));
        }
        BubbleStyle::Morph => {
            let rr = RoundedRect::new(c.x - r * 0.86, c.y - r * 0.86, c.x + r * 0.86, c.y + r * 0.86, r * 0.6);
            scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &rr);
            dot(scene, Point::new(c.x - r * 0.22, c.y - r * 0.22), r * 0.22, on.with_alpha(0.35));
        }
    }
}

/// Picker modal layout: (card, [(swatch cell, style); 9], done button). 3×3 grid.
pub(crate) fn bubble_picker_layout(w: f64, h: f64) -> (Rect, Vec<(Rect, BubbleStyle)>, Rect) {
    let cw = 540.0_f64.min(w - 60.0);
    let ch = 480.0_f64.min(h - 60.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(24.0);
    let card = Rect::new(x0, y0, x0 + cw, y0 + ch);
    let (cols, rows) = (3usize, 3usize);
    let grid_top = y0 + 60.0;
    let grid_h = ch - 60.0 - 58.0;
    let cellw = cw / cols as f64;
    let cellh = grid_h / rows as f64;
    let mut cells = Vec::new();
    for (i, &st) in BubbleStyle::all().iter().enumerate() {
        let (gx, gy) = (i % cols, i / cols);
        let cx0 = x0 + gx as f64 * cellw;
        let cy0 = grid_top + gy as f64 * cellh;
        cells.push((Rect::new(cx0, cy0, cx0 + cellw, cy0 + cellh), st));
    }
    let bw = 110.0;
    let bh = 32.0;
    let bx = x0 + (cw - bw) * 0.5;
    let by = card.y1 - 44.0;
    (card, cells, Rect::new(bx, by, bx + bw, by + bh))
}

/// The bubble-style picker: a 3×3 grid of static style swatches; the current one
/// is highlighted. Click a swatch to choose it.
pub(crate) fn draw_bubble_picker(scene: &mut Scene, shaper: &mut TextShaper, current: BubbleStyle, w: f64, h: f64) {
    let (card, cells, close) = bubble_picker_layout(w, h);
    let accent = pal().accent;
    let on = pal().on_accent;
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.5), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);

    let title = shaper.shape("Choose your bubble", 4000.0, 16.0);
    draw_text(scene, &title, Affine::translate((card.x0 + 24.0, card.y0 + 20.0)), pal().text);

    for (cell, st) in &cells {
        let cc = Point::new((cell.x0 + cell.x1) * 0.5, cell.y0 + 42.0);
        let sr = 30.0;
        if *st == current {
            let hl = Rect::new(cell.x0 + 8.0, cell.y0 + 4.0, cell.x1 - 8.0, cell.y1 - 4.0);
            let hrr = RoundedRect::from_rect(hl, 10.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, accent.with_alpha(0.16), None, &hrr);
            scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, accent, None, &hrr);
        }
        draw_bubble_style(scene, *st, cc, sr, bubble_style_color(*st), on);
        let lbl = shaper.shape(st.label(), (cell.width() - 8.0) as f32, 12.0);
        let tx = (cell.x0 + cell.x1) * 0.5 - lbl.width() as f64 * 0.5;
        draw_text(scene, &lbl, Affine::translate((tx, cc.y + sr + 8.0)), pal().text_dim);
    }

    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface_alt, None, &RoundedRect::from_rect(close, 7.0));
    let done = shaper.shape("Done", 120.0, 13.0);
    let tx = close.x0 + (close.width() - done.width() as f64) * 0.5;
    let ty = close.y0 + (close.height() - done.height() as f64) * 0.5;
    draw_text(scene, &done, Affine::translate((tx, ty)), pal().text);
}
