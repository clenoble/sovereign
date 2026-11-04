//! The spatial-canvas data model + world-space painters: synthetic + real
//! workspace loading, cards/links/minimap, the link edges, the adaptive time
//! axis, and the minimap overlay.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{Datelike, Days, Duration, Local, Months, NaiveDate, TimeZone, Timelike};
use parley::Layout;
use vello::kurbo::{Affine, BezPath, Line, Point, Rect, Stroke};
use vello::peniko::{Color, Fill};
use vello::kurbo::CubicBez;
use vello::Scene;

use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::schema::thing_to_raw;
use sovereign_db::traits::GraphDB;
use sovereign_core::auth::PersonaKind as CorePersona;

use crate::camera::Camera;
use crate::crypto::persona_raw_db_path;
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::pal;

// ---- Layout constants ---------------------------------------------------

pub(crate) const LANE_H: f64 = 96.0;

/// The number of hues lanes are coloured with (spec ch. 12, N7): a thread's
/// hue is one of these, derived from its id ([`lane_hues`]).
pub(crate) const HUE_COUNT: usize = 12;

/// The time axis is calibrated: one world unit = `1/WORLD_PER_DAY` of a day, so a
/// card's X is a real position in time. At the default camera zoom (0.6) this
/// puts ~130 px/day on screen → a day-grained ruler; zooming in narrows the
/// interval to hours/minutes, zooming out widens it to months/years.
pub(crate) const WORLD_PER_DAY: f64 = 220.0;
const SECS_PER_DAY: f64 = 86_400.0;

/// World-X for a unix timestamp, relative to `ref_ts` (the time at world-X = 0).
pub(crate) fn x_of_ts(ts: i64, ref_ts: i64) -> f64 {
    (ts - ref_ts) as f64 / SECS_PER_DAY * WORLD_PER_DAY
}
/// The unix timestamp at a world-X (inverse of `x_of_ts`).
pub(crate) fn ts_of_x(x: f64, ref_ts: i64) -> i64 {
    ref_ts + (x / WORLD_PER_DAY * SECS_PER_DAY) as i64
}

pub(crate) const CARD_W: f64 = 176.0;
pub(crate) const CARD_H: f64 = 64.0;
pub(crate) const PAD: f64 = 9.0;
pub(crate) const TITLE_PX: f32 = 13.0;
pub(crate) const AXIS_H: f64 = 24.0; // top time-ruler height (screen px)
pub(crate) const MM_W: f64 = 300.0; // minimap (screen px)
pub(crate) const MM_H: f64 = 150.0;
pub(crate) const MM_MARGIN: f64 = 18.0;
pub(crate) const MM_BUCKETS: usize = 160;
pub(crate) const STATUS_H: f64 = 26.0; // bottom status-bar height (screen px)
pub(crate) const TASKBAR_H: f64 = 56.0; // bottom taskbar/dock height (screen px), above the status bar
/// Total bottom chrome (taskbar + status bar) — the band reserved below the canvas.
pub(crate) const BOTTOM_CHROME: f64 = STATUS_H + TASKBAR_H;

pub(crate) const TITLES: &[&str] = &[
    "Q3 planning notes",
    "Meeting with Alex about the encryption rework",
    "Reachy Mini gestures",
    "Social recovery — key splitting design",
    "Grocery list",
    "Prompt injection is an architecture problem",
    "Invoice — March",
    "Spatial canvas: replacing folders",
    "Call mom",
    "P2P sync: all-tables reconciliation plan and open questions",
    "Vello vs WebKitGTK",
    "Onboarding copy v2",
    "Threat model: data at rest",
    "Weekend trip itinerary",
    "Why I'm building a personal OS that doesn't trust the cloud",
    "Bug: duress timing leak",
    "Contacts merge dedupe",
    "Daily journal",
];

pub(crate) struct Card {
    pub(crate) id: String, // document record id (for pin/delete/open)
    pub(crate) x: f64,
    pub(crate) lane: usize,
    pub(crate) external: bool, // external/untrusted content -> parallelogram (provenance cue)
    pub(crate) pinned: bool,   // user-pinned (marker on the card)
    /// The title as shown outside the editor: as stored, its personal-data
    /// tokens as their labels ([`shown_title`], item 8, L2) — never the
    /// token's stored form. The editor opens on the stored title, read from
    /// the store (`v2_open`). Re-shaped larger for the document window.
    pub(crate) title: String,
    /// What the document window shows: the document's text read through the
    /// v2 model ([`display_text`]), or a lock placeholder when encrypted.
    pub(crate) body: String,
    /// The stored content as read from the database — what today's (pre-v2)
    /// edit buffer starts from and what a skill's write verdict inspects. Never
    /// shown as such (adoption step 1: readers use the typed view; the editor
    /// save path is step 2).
    pub(crate) stored: String,
    pub(crate) layout: Layout<Brush>, // card-title shaped once, redrawn each frame
}

impl Card {
    /// Sets the stored content and the text shown for it.
    pub(crate) fn set_stored(&mut self, stored: String) {
        self.body = display_text(&stored);
        self.stored = stored;
    }

    /// Sets the title the card shows (after a save changed it: title contract
    /// T2), re-shaping its layout as cards are shaped when loaded. Called by
    /// the editor's save path.
    pub(crate) fn set_title(&mut self, title: &str, shaper: &mut TextShaper) {
        self.title = title.to_string();
        self.layout = shaper.shape(display_title(title), (CARD_W - 2.0 * PAD) as f32, TITLE_PX);
    }
}

/// How a document's title is displayed (title contract T10): an empty title
/// reads *Untitled*; any other is shown as stored. The one helper every
/// surface that shows a document title uses (card, window bar, trash, search,
/// taskbar, lists); the stored title stays empty, and the editor's title field
/// keeps its own dimmed placeholder.
pub(crate) fn display_title(title: &str) -> &str {
    if title.is_empty() {
        "Untitled"
    } else {
        title
    }
}

/// The stored form of a personal-data token starts so (`[pii:<record id>]`).
const TOKEN_PREFIX: &str = "[pii:";

/// Document `id`'s title `title` as the shell shows it outside the editor
/// (item 8, L2): each personal-data token as its label, as the document
/// service labels it (`DocumentView::labelled_title`, the resolver rule) —
/// never its stored form. Token-form text that is not a token in the document
/// stays as typed, as in the editor. A title holding no token form is shown
/// as stored, with no read. Without a service, or when the document can't be
/// read, each token form shows as the generic label (`[PII]`): in doubt, the
/// stored form is not shown. The card, the window bar and what reads the
/// card's title (History, the taskbar, search…), the trash rows and the
/// orphans' lines take their title from here.
pub(crate) async fn shown_title(docs: Option<&sovereign_docs::DocumentService>, id: &str, title: &str) -> String {
    if !title.contains(TOKEN_PREFIX) {
        return title.to_string();
    }
    if let Some(docs) = docs {
        if let Ok(view) = docs.read(id).await {
            return view.labelled_title();
        }
    }
    let generic = sovereign_ai::pii::resolve::preview_label_for_kind(&sovereign_db::schema::PiiKind::Other);
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(at) = rest.find(TOKEN_PREFIX) {
        let Some(len) = rest[at..].find(']') else { break };
        out.push_str(&rest[..at]);
        out.push_str(generic);
        rest = &rest[at + len + 1..];
    }
    out.push_str(rest);
    out
}

/// What the shell shows for a stored content string (C-DOC adoption step 1):
/// its text read through the v2 model — a v2 document, a v1 envelope or raw
/// text — never the stored JSON. Personal-data tokens stay in their stored
/// form `[pii:<id>]` (labels need the document service's token status; see
/// `doc/plans/C-DOC-adoption.md`).
pub(crate) fn display_text(stored: &str) -> String {
    use sovereign_editor::doc::DocumentV2;
    match DocumentV2::read(stored) {
        Ok((doc, _)) => {
            let text = doc.plain_text();
            if text.is_empty() && !doc.blocks().is_empty() {
                "(This document has no text.)".to_string()
            } else if text.is_empty() {
                "(empty document)".to_string()
            } else {
                text
            }
        }
        Err(_) => "(This document's format can't be read here. It is kept unchanged.)".to_string(),
    }
}

// ---- Deck layout (zoom-reactive, screen-space) --------------------------
// Same model as the Tauri canvas (computeDeckRoles): within a lane, cards whose
// on-screen X gap is under CARD_W collapse into a deck — the most-recent card is
// the front, older cards peek up-left behind it, overflow hides behind a count
// badge. The grouping threshold is CARD_W / zoom world px (== CARD_W screen px),
// so decks dissolve as you zoom in, down to cards sharing an identical time.

/// World-px offset per peeking card. In world units (scales with zoom) so the
/// fan stays a constant fraction of the lane at every zoom. Contained iff
/// (DECK_VISIBLE - 1) * DECK_PEEK <= (LANE_H - CARD_H) / 2  (= 15 <= 16 here).
pub(crate) const DECK_PEEK: f64 = 5.0;
/// Front card + up to (DECK_VISIBLE - 1) peeks are drawn; the rest hide.
pub(crate) const DECK_VISIBLE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeckRole {
    pub(crate) count: u32,
    pub(crate) peek: usize,      // 0 = front (at true position), 1.. = peeks behind
    pub(crate) hidden: bool,     // peek >= DECK_VISIBLE -> not drawn
    pub(crate) front_idx: usize, // index (into the input slice) of this deck's front
}
impl DeckRole {
    pub(crate) fn is_front(&self) -> bool {
        self.peek == 0
    }
}

/// Assign a [`DeckRole`] to each card (indexed identically). Convenience wrapper
/// over [`deck_roles`] that projects cards to their (x, lane).
pub(crate) fn compute_deck_roles(cards: &[Card], zoom: f64) -> Vec<DeckRole> {
    let pos: Vec<(f64, usize)> = cards.iter().map(|c| (c.x, c.lane)).collect();
    deck_roles(&pos, zoom)
}

/// Pure decking core over (world-x, lane) pairs — unit-testable without shaping
/// real `Card`s. Returns roles indexed to `pos`.
pub(crate) fn deck_roles(pos: &[(f64, usize)], zoom: f64) -> Vec<DeckRole> {
    let mut roles = vec![DeckRole { count: 1, peek: 0, hidden: false, front_idx: 0 }; pos.len()];
    // world gap that projects to exactly CARD_W screen px at this zoom
    let threshold = CARD_W / zoom.max(1e-6);

    let mut by_lane: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, (_, lane)) in pos.iter().enumerate() {
        by_lane.entry(*lane).or_default().push(i);
    }
    for idxs in by_lane.values_mut() {
        idxs.sort_by(|&a, &b| {
            pos[a].0.partial_cmp(&pos[b].0).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut i = 0;
        while i < idxs.len() {
            let anchor_x = pos[idxs[i]].0;
            let mut j = i + 1;
            while j < idxs.len() && pos[idxs[j]].0 - anchor_x < threshold {
                j += 1;
            }
            let count = (j - i) as u32;
            let front_idx = idxs[j - 1]; // max x = most recent = front
            for k in i..j {
                // ascending by x -> last (max x, most recent) is the front
                let peek = j - 1 - k;
                roles[idxs[k]] =
                    DeckRole { count, peek, hidden: peek >= DECK_VISIBLE, front_idx };
            }
            i = j;
        }
    }
    roles
}

// ---- Synthetic data -----------------------------------------------------

pub(crate) struct Lcg(pub(crate) u64);
impl Lcg {
    pub(crate) fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as f64 / (1u64 << 31) as f64
    }
}

/// Synthetic body for a card (no real db) — long enough to exercise scrolling.
pub(crate) fn synthetic_body(i: usize) -> String {
    let lead = TITLES[i % TITLES.len()];
    format!(
        "{lead}\n\n\
         This document window is rendered natively with Vello + parley — no \
         WebKitGTK and no Tauri IPC. The body text wraps to the panel width, \
         scrolls with the mouse wheel, and is clipped to the viewport.\n\n\
         Click any card on the canvas to open it here. Click outside the panel, \
         press the \u{00d7}, or hit Esc to close. Owned and external documents \
         keep their provenance cue.\n\n\
         The spatial canvas behind this window stays live — time on the X axis, \
         thread lanes on the Y axis, cross-thread links, the adaptive ruler, and \
         the minimap. This is the first chrome panel of the native-shell \
         migration: a roll-our-own widget layer on the vello 0.9 stack rather \
         than a forked toolkit.\n\n\
         Scroll down to confirm clipping and the scroll clamp hold. \
         Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do \
         eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim \
         ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut \
         aliquip ex ea commodo consequat. Duis aute irure dolor in \
         reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla \
         pariatur. Excepteur sint occaecat cupidatat non proident, sunt in \
         culpa qui officia deserunt mollit anim id est laborum."
    )
}

/// `n` synthetic cards over `lanes` lanes (the synthetic workspace's threads).
pub(crate) fn make_cards(n: usize, world_w: f64, lanes: usize, rng: &mut Lcg, shaper: &mut TextShaper) -> Vec<Card> {
    let max_w = (CARD_W - 2.0 * PAD) as f32;
    let lanes = lanes.max(1);
    let mut cards: Vec<Card> = (0..n)
        .map(|i| {
            let x = rng.next() * world_w;
            let lane = (rng.next() * lanes as f64) as usize % lanes;
            let external = rng.next() < 0.30; // ~30% external/untrusted
            let title = TITLES[i % TITLES.len()];
            Card {
                id: format!("synthetic:{i}"),
                x,
                lane,
                external,
                pinned: i % 7 == 0, // a few pinned in synthetic data to show the marker
                title: title.to_string(),
                body: synthetic_body(i),
                stored: synthetic_body(i),
                layout: shaper.shape(title, max_w, TITLE_PX),
            }
        })
        .collect();
    // Sort by time so links connect temporally-near docs + culling is a range.
    cards.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    cards
}

/// Cross-thread relationship edges: each card links forward to a temporally
/// near card, biased to a DIFFERENT lane (like doc relationships / suggested
/// links spanning threads). ~1 link/card.
pub(crate) fn make_links(cards: &[Card], rng: &mut Lcg) -> Vec<(u32, u32)> {
    let n = cards.len();
    let mut links = Vec::with_capacity(n);
    for i in 0..n {
        if rng.next() > 0.72 {
            continue;
        }
        let span = 1 + (rng.next() * 28.0) as usize; // i -> i+span (x-local)
        let j = (i + span).min(n - 1);
        if j != i {
            links.push((i as u32, j as u32));
        }
    }
    links
}

/// Pre-binned minimap density grid [lane][bucket] for `lanes` lanes, so the
/// minimap is cheap to redraw (a production minimap would cache to a texture;
/// this is the same idea).
pub(crate) fn make_minimap(cards: &[Card], world_w: f64, lanes: usize) -> Vec<u16> {
    let mut grid = vec![0u16; lanes * MM_BUCKETS];
    for c in cards {
        if c.lane >= lanes {
            continue;
        }
        let b = ((c.x / world_w) * MM_BUCKETS as f64) as usize;
        let b = b.min(MM_BUCKETS - 1);
        let idx = c.lane * MM_BUCKETS + b;
        grid[idx] = grid[idx].saturating_add(1);
    }
    grid
}

// ---- The lanes: one per live thread (spec ch. 12, C-LANE) ----------------

/// What a lane shows: a live thread (its raw id), or the Unfiled lane, where
/// documents whose thread cannot be resolved are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LaneKind<'a> {
    Thread(&'a str),
    Unfiled,
}

/// The one mapping between lanes and threads (N1): every use — drawing,
/// placing cards, decks, hit-testing, the context menu, drag and drop, "New
/// document", lane rename, the status bar, the accessibility tree, the
/// minimap, home and fit — reads it, and none computes its own.
///
/// Lane `i` shows the `i`-th live thread in creation order (the store's
/// `list_threads` order), no cap (N2); the Unfiled lane, when a document needs
/// it, comes after them (N3). Thread ids are raw (`thing_to_raw`, I1).
#[derive(Clone, Debug, Default)]
pub(crate) struct LaneMap {
    threads: Vec<String>,
    lane_of: HashMap<String, usize>,
    unfiled: bool,
}

impl LaneMap {
    /// The lanes of `thread_ids` (raw, in lane order), without an Unfiled lane.
    pub(crate) fn new(thread_ids: Vec<String>) -> LaneMap {
        let lane_of = thread_ids.iter().enumerate().map(|(i, t)| (t.clone(), i)).collect();
        LaneMap { threads: thread_ids, lane_of, unfiled: false }
    }

    /// The lanes of the store's thread list: one per live thread, in order.
    pub(crate) fn from_threads(threads: &[sovereign_db::schema::Thread]) -> LaneMap {
        LaneMap::new(threads.iter().filter(|t| t.deleted_at.is_none()).filter_map(|t| t.id_string()).collect())
    }

    /// With (or without) the Unfiled lane after the threads' lanes.
    pub(crate) fn with_unfiled(mut self, unfiled: bool) -> LaneMap {
        self.unfiled = unfiled;
        self
    }

    /// The lane of thread `thread_id` (raw), if it is a live thread.
    pub(crate) fn lane_of_thread(&self, thread_id: &str) -> Option<usize> {
        self.lane_of.get(thread_id).copied()
    }

    /// What lane `lane` shows (`None`: no such lane).
    pub(crate) fn lane(&self, lane: usize) -> Option<LaneKind<'_>> {
        match self.threads.get(lane) {
            Some(t) => Some(LaneKind::Thread(t)),
            None if self.unfiled && lane == self.threads.len() => Some(LaneKind::Unfiled),
            None => None,
        }
    }

    /// The thread lane `lane` shows (`None` for the Unfiled lane or no lane).
    pub(crate) fn thread_of_lane(&self, lane: usize) -> Option<&str> {
        match self.lane(lane) {
            Some(LaneKind::Thread(t)) => Some(t),
            _ => None,
        }
    }

    /// The Unfiled lane, while a document needs it.
    pub(crate) fn unfiled_lane(&self) -> Option<usize> {
        self.unfiled.then_some(self.threads.len())
    }

    /// The number of lanes drawn (the threads' and Unfiled).
    pub(crate) fn len(&self) -> usize {
        self.threads.len() + usize::from(self.unfiled)
    }

    /// The number of thread lanes (N5: Unfiled is not one).
    pub(crate) fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// The threads' raw ids, in lane order.
    pub(crate) fn thread_ids(&self) -> &[String] {
        &self.threads
    }
}

/// A thread's own hue, from its raw id: FNV-1a 64 with a final mix, so ids
/// that differ in one character still spread over the hues.
fn own_hue(thread_id: &str) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in thread_id.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    (h % HUE_COUNT as u64) as usize
}

/// The hue (an index into the [`HUE_COUNT`] hues) of each lane, for the live
/// threads in lane order (N7). A thread's hue is derived from its id; when it
/// equals the hue derived from the id of the thread just above, it moves to
/// the next hue. Compared with the upper thread's *derived* hue, not the hue
/// that lane is drawn in, so there is no cascade: a thread's hue depends only
/// on its own id and its upper neighbour's, and deleting a thread changes at
/// most the hue of the thread that gets a new upper neighbour.
pub(crate) fn lane_hues(thread_ids: &[String]) -> Vec<usize> {
    let own: Vec<usize> = thread_ids.iter().map(|t| own_hue(t)).collect();
    own.iter()
        .enumerate()
        .map(|(i, &h)| if i > 0 && own[i - 1] == h { (h + 1) % HUE_COUNT } else { h })
        .collect()
}

/// The camera's vertical offset with the stop applied (N4): for `lanes` lanes
/// at `zoom` in a window `h` high, the first lane never below the canvas top
/// (the time ruler's bottom) and the last never above the canvas bottom (the
/// taskbar's top); when every lane fits, the whole block lies in the canvas.
pub(crate) fn lanes_stop(offset_y: f64, lanes: usize, zoom: f64, h: f64) -> f64 {
    let (top, bottom) = (AXIS_H, h - BOTTOM_CHROME);
    let block = lanes as f64 * LANE_H * zoom;
    let (a, b) = (top, bottom - block);
    offset_y.clamp(a.min(b), a.max(b))
}

/// Whether `lanes` lanes at `zoom` all fit in the canvas of a window `h` high
/// (then there is no vertical scroll, N4).
pub(crate) fn lanes_fit(lanes: usize, zoom: f64, h: f64) -> bool {
    lanes as f64 * LANE_H * zoom <= h - BOTTOM_CHROME - AXIS_H
}

/// The lanes with at least one pixel between screen `y0` and `y1` (N8: only
/// they are drawn), as a range of lane indices within `0..lanes`.
pub(crate) fn lanes_between(cam: &Camera, lanes: usize, y0: f64, y1: f64) -> std::ops::Range<usize> {
    let lane_h = LANE_H * cam.zoom;
    if lanes == 0 || lane_h <= 0.0 {
        return 0..0;
    }
    let first = ((y0 - cam.offset_y) / lane_h).floor().max(0.0);
    let last = ((y1 - cam.offset_y) / lane_h).ceil().max(0.0);
    let first = (first as usize).min(lanes);
    let last = (last as usize).min(lanes);
    first..last.max(first)
}

/// Phase 0b: load the real workspace from sovereign-db IN-PROCESS (no IPC).
/// Positions (`modified_at`), threads, links (relationships), and provenance
/// (`is_owned`) are plaintext metadata; titles are shown when plaintext (seed
/// data) and a lock placeholder when encrypted (decryption needs the login key
/// — a later phase). Returns None on any failure (db absent, locked by a
/// running app, or empty) → the shell falls back to synthetic data.
/// Open a persistent SurrealGraphDB at `path` (RocksDB is single-process
/// exclusive — only one open per path at a time across the whole shell).
pub(crate) async fn open_db_at(path: &Path) -> Option<Arc<dyn GraphDB>> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = SurrealGraphDB::new(StorageMode::Persistent(path.to_string_lossy().into_owned()))
        .await
        .ok()?;
    db.connect().await.ok()?;
    db.init_schema().await.ok()?;
    Some(Arc::new(db) as Arc<dyn GraphDB>)
}

/// The default (primary-persona) workspace DB path.
pub(crate) async fn open_db() -> Option<Arc<dyn GraphDB>> {
    open_db_at(&persona_raw_db_path(CorePersona::Primary)).await
}

/// The name of the lane where documents of a missing thread are drawn (spec
/// ch. 03, *Thread deletion and the fallback lane*). The trash view names a
/// document's thread so when the thread is gone.
pub(crate) const FALLBACK_LANE_NAME: &str = "Unfiled";

/// A lane's name label (the pill at the canvas's left edge), in screen
/// coordinates, for a lane whose top is at screen y `lane_top` and a label
/// text `label_w` wide. Shared by drawing and hit-testing, so a double-click
/// lands where the label is drawn (`THREAD-name.md` R1). The pill sticks
/// below the time ruler when its lane is scrolled partly under it; the text
/// is drawn [`LANE_LABEL_TEXT_INSET`] inside it.
pub(crate) fn lane_label_rect(lane_top: f64, label_w: f64) -> Rect {
    let py = (lane_top + 6.0).max(AXIS_H + 4.0);
    Rect::new(6.0, py, 6.0 + label_w + 18.0, py + 20.0)
}

/// Where a lane label's text sits inside its pill (x, y).
pub(crate) const LANE_LABEL_TEXT_INSET: (f64, f64) = (9.0, 4.0);

/// The canvas's workspace, as loaded from the store.
pub(crate) struct Workspace {
    pub(crate) cards: Vec<Card>,
    pub(crate) links: Vec<(u32, u32)>,
    pub(crate) world_w: f64,
    /// The name each lane shows (index = lane): its thread's name, a lock while
    /// the name is sealed, empty for a thread with no name; then Unfiled.
    pub(crate) lane_names: Vec<String>,
    /// The unix time at world-X = 0 (calibrates the time ruler).
    pub(crate) ref_ts: i64,
    /// The one lane mapping (N1) these cards were placed by.
    pub(crate) lanes: LaneMap,
    /// The store's live threads, as read for this load (the renamable labels
    /// are derived from the same list, `lane_rename::lane_labels`).
    pub(crate) threads: Vec<sovereign_db::schema::Thread>,
}

/// Returns `(cards, links, world_w, lane_names, ref_ts)` — [`load_workspace_full`]
/// in the tuple form the tests read.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn load_workspace(
    db: &Arc<dyn GraphDB>,
    shaper: &mut TextShaper,
) -> Option<(Vec<Card>, Vec<(u32, u32)>, f64, Vec<String>, i64)> {
    let ws = load_workspace_full(db, None, shaper).await?;
    Some((ws.cards, ws.links, ws.world_w, ws.lane_names, ws.ref_ts))
}

/// Loads the workspace: one lane per live thread in creation order (N2),
/// documents placed in their thread's lane, and the Unfiled lane after them
/// only while a document's thread cannot be resolved (N3). Every id is raw
/// (I1). `None` only when there is nothing at all (no docs *and* no threads)
/// — a workspace with labeled-but-empty lanes still returns (so the lanes
/// render and the `+` button has structure). A card's title is the title as
/// shown, its tokens labelled through `service` ([`shown_title`], item 8, L2).
pub(crate) async fn load_workspace_full(
    db: &Arc<dyn GraphDB>,
    service: Option<&sovereign_docs::DocumentService>,
    shaper: &mut TextShaper,
) -> Option<Workspace> {
    let docs = db.list_documents(None).await.ok()?;
    let threads: Vec<sovereign_db::schema::Thread> =
        db.list_threads().await.unwrap_or_default().into_iter().filter(|t| t.deleted_at.is_none()).collect();
    if docs.is_empty() && threads.is_empty() {
        return None;
    }
    let rels = db.list_all_relationships().await.unwrap_or_default();

    let lanes = LaneMap::from_threads(&threads);
    // The per-lane display name, read for the thread each lane shows (the
    // same threads, in the same order, as `LaneMap::from_threads`).
    let mut lane_names: Vec<String> = Vec::with_capacity(lanes.thread_count() + 1);
    for t in threads.iter().filter(|t| t.id.is_some()) {
        // Plaintext name when decrypted (real app clears the nonce on read);
        // a lock placeholder in raw/no-auth mode where it stays ciphertext.
        if t.name_nonce.is_none() {
            lane_names.push(t.name.clone());
        } else {
            let id = t.id_string().unwrap_or_default();
            let short: String = id.rsplit(':').next().unwrap_or("?").chars().take(4).collect();
            lane_names.push(format!("\u{1f512} {short}"));
        }
    }

    // Calibrate the time axis: ref = oldest doc (so world-X = 0 sits at the
    // start of the data); world_w spans from there to a few days past "now"
    // (or past the newest doc). An empty-but-labeled workspace shows the last
    // ~week so the "now" line + ruler are meaningful.
    let now = chrono::Utc::now().timestamp();
    let (ref_ts, end_ts) = if docs.is_empty() {
        (now - 7 * 86_400, now + 3 * 86_400)
    } else {
        let min_t = docs.iter().map(|d| d.modified_at.timestamp()).min().unwrap_or(now);
        let max_t = docs.iter().map(|d| d.modified_at.timestamp()).max().unwrap_or(now);
        (min_t, max_t.max(now) + 3 * 86_400)
    };
    let world_w = x_of_ts(end_ts, ref_ts).max(WORLD_PER_DAY);
    let max_w = (CARD_W - 2.0 * PAD) as f32;

    // Docs whose thread_id doesn't resolve to a live thread go to the Unfiled
    // lane, after every thread's lane (spec ch. 03, *Thread deletion and the
    // fallback lane*), rather than silently piling into another thread's.
    let unfiled_lane = lanes.thread_count();
    let mut used_unfiled = false;
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();
    let mut cards = Vec::with_capacity(docs.len());
    for (i, d) in docs.iter().enumerate() {
        let id = d.id_string().unwrap_or_default();
        if !id.is_empty() {
            id_to_idx.insert(id.clone(), i);
        }
        let lane = match lanes.lane_of_thread(&d.thread_id) {
            Some(l) => l,
            None => {
                used_unfiled = true;
                unfiled_lane
            }
        };
        let x = x_of_ts(d.modified_at.timestamp(), ref_ts);
        // A title still under its nonce is ciphertext (raw / no-auth
        // mode): a lock placeholder. Read through the encrypted store the
        // nonce is cleared, and the title is the plaintext — possibly
        // empty, which is a title like any other (T10: shown Untitled) —
        // its tokens shown as their labels (L2).
        let title = if d.title_nonce.is_none() {
            shown_title(service, &id, &d.title).await
        } else {
            let short: String = id.rsplit(':').next().unwrap_or("?").chars().take(6).collect();
            format!("\u{1f512} {short}")
        };
        // Adoption step 1: the card shows the document's text read through
        // the v2 model (a v1 envelope used to show as JSON, a v2 document
        // as its blocks' JSON); the stored string is kept for the pre-v2
        // edit buffer only.
        let (body, stored) = if d.encryption_nonce.is_none() && !d.content.is_empty() {
            (display_text(&d.content), d.content.clone())
        } else if !d.content.is_empty() {
            let lock = "\u{1f512} Encrypted — this document unlocks after login.".to_string();
            (lock.clone(), lock)
        } else {
            ("(empty document)".to_string(), String::new())
        };
        cards.push(Card {
            id,
            x,
            lane,
            external: !d.is_owned,
            pinned: d.pinned,
            title: title.clone(),
            body,
            stored,
            layout: shaper.shape(display_title(&title), max_w, TITLE_PX),
        });
    }

    let lanes = lanes.with_unfiled(used_unfiled);
    if used_unfiled {
        lane_names.push(FALLBACK_LANE_NAME.to_string());
    }

    let mut links = Vec::new();
    for r in &rels {
        let (Some(a), Some(b)) = (r.in_.as_ref().map(thing_to_raw), r.out.as_ref().map(thing_to_raw)) else {
            continue;
        };
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&a), id_to_idx.get(&b)) {
            links.push((a as u32, b as u32));
        }
    }
    Some(Workspace { cards, links, world_w, lane_names, ref_ts, lanes, threads })
}

/// External/untrusted content renders as a right-leaning parallelogram — a
/// pre-attentive shape cue the visual system discriminates without a judgment
/// call (Sovereignty Halo / provenance). Owned content stays a rounded rect.
/// `lean` is the half-shift of the top edge vs the bottom.
pub(crate) fn parallelogram(sx: f64, sy: f64, sw: f64, sh: f64, lean: f64) -> BezPath {
    let mut p = BezPath::new();
    p.move_to((sx + lean, sy));
    p.line_to((sx + sw + lean, sy));
    p.line_to((sx + sw - lean, sy + sh));
    p.line_to((sx - lean, sy + sh));
    p.close_path();
    p
}

/// One of the [`HUE_COUNT`] hues (a lane's, by [`lane_hues`]; a contact
/// avatar's, by its own index).
pub(crate) fn hue_color(hue: usize) -> Color {
    const HUES: [(u8, u8, u8); HUE_COUNT] = [
        (64, 110, 180), (90, 150, 110), (170, 120, 70), (150, 90, 150),
        (70, 150, 160), (180, 100, 100), (110, 120, 170), (140, 150, 80),
        (100, 140, 160), (160, 110, 130), (90, 130, 90), (130, 110, 160),
    ];
    let (r, g, b) = HUES[hue % HUES.len()];
    Color::from_rgb8(r, g, b)
}

/// The Unfiled lane's colour: a neutral grey, no thread's hue.
pub(crate) const UNFILED_COLOR: Color = Color::from_rgb8(128, 128, 128);

/// Each lane's colour (index = lane), for the lanes of `lanes`: a thread's
/// lane in its hue (N7, [`lane_hues`]), the Unfiled lane in grey. The band,
/// the label, the links and the minimap row all read it.
pub(crate) fn lane_colors(lanes: &LaneMap) -> Vec<Color> {
    let mut out: Vec<Color> = lane_hues(lanes.thread_ids()).into_iter().map(hue_color).collect();
    if lanes.unfiled_lane().is_some() {
        out.push(UNFILED_COLOR);
    }
    out
}

// ---- Overlays (free fns: borrow only the fields they need) --------------

/// Cross-thread link edges as cubic beziers (culled to the visible x-range,
/// and to the window's height: two ends both above it or both below it are
/// not drawn). Coloured by the source card's lane (`lane_colors`).
pub(crate) fn draw_links(
    scene: &mut Scene,
    cards: &[Card],
    links: &[(u32, u32)],
    lane_colors: &[Color],
    cam: &Camera,
    w: f64,
    h: f64,
) -> usize {
    let (vx0, vx1) = cam.visible_x(w);
    let mut drawn = 0;
    for &(a, b) in links {
        let (ca, cb) = (&cards[a as usize], &cards[b as usize]);
        if ca.x.max(cb.x) + CARD_W < vx0 || ca.x.min(cb.x) > vx1 {
            continue;
        }
        let from = Point::new(
            cam.w2s_x(ca.x + CARD_W),
            cam.w2s_y(ca.lane as f64 * LANE_H + LANE_H * 0.5),
        );
        let to = Point::new(
            cam.w2s_x(cb.x),
            cam.w2s_y(cb.lane as f64 * LANE_H + LANE_H * 0.5),
        );
        if (from.y < AXIS_H && to.y < AXIS_H) || (from.y > h && to.y > h) {
            continue;
        }
        let color = lane_colors.get(ca.lane).copied().unwrap_or(UNFILED_COLOR);
        let dx = (to.x - from.x) * 0.45;
        let curve = CubicBez::new(
            from,
            Point::new(from.x + dx, from.y),
            Point::new(to.x - dx, to.y),
            to,
        );
        scene.stroke(
            &Stroke::new(1.1),
            Affine::IDENTITY,
            color.with_alpha(0.30),
            None,
            &curve,
        );
        drawn += 1;
    }
    drawn
}

// ---- Adaptive, calibrated time ruler ------------------------------------

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Clone, Copy)]
enum AxisFmt {
    Hm,    // "14:30" — minute/hour ticks (zoomed in)
    Day,   // "Apr 23" — day/week ticks (default)
    Month, // "Apr 26" — month ticks (zoomed out)
    Year,  // "2026" — year ticks (far out)
}

/// `ts` read in `tz`. v0.0.12, ruler days: every local reading of the ruler goes through the
/// zone it is given (production passes `Local`), so the tests can bring a 23- and a 25-hour day.
fn dt_in<Tz: TimeZone>(tz: &Tz, ts: i64) -> chrono::DateTime<Tz> {
    tz.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(|| tz.from_utc_datetime(&chrono::DateTime::UNIX_EPOCH.naive_utc()))
}

fn axis_label(ts: i64, fmt: AxisFmt) -> String {
    axis_label_in(&Local, ts, fmt)
}

fn axis_label_in<Tz: TimeZone>(tz: &Tz, ts: i64, fmt: AxisFmt) -> String {
    let dt = dt_in(tz, ts);
    match fmt {
        AxisFmt::Hm => format!("{:02}:{:02}", dt.hour(), dt.minute()),
        AxisFmt::Day => format!("{} {}", MONTHS[dt.month0() as usize], dt.day()),
        AxisFmt::Month => format!("{} {:02}", MONTHS[dt.month0() as usize], dt.year().rem_euclid(100)),
        AxisFmt::Year => format!("{}", dt.year()),
    }
}

/// Ticks at a fixed-duration step (minutes, 1 hour: every real instant on the step, D3),
/// starting at an aligned boundary `start` (already snapped to a local minute/hour).
fn ticks_fixed<Tz: TimeZone>(t0: i64, t1: i64, start: chrono::DateTime<Tz>, step: Duration) -> Vec<i64> {
    let mut out = Vec::new();
    let mut dt = start;
    let mut guard = 0;
    while dt.timestamp() <= t1 && guard < 2000 {
        if dt.timestamp() >= t0 {
            out.push(dt.timestamp());
        }
        dt += step;
        guard += 1;
    }
    out
}

/// The first instant of the local date `d`: its 00:00 (the earlier if it occurs twice), else
/// the first local minute that exists (a change skipped 00:00).
fn first_instant_of<Tz: TimeZone>(tz: &Tz, d: NaiveDate) -> Option<i64> {
    (0u32..24 * 60).find_map(|min| {
        let n = d.and_hms_opt(min / 60, min % 60, 0)?;
        tz.from_local_datetime(&n).earliest().map(|x| x.timestamp())
    })
}

/// Day ticks (v0.0.12, ruler days, D1): the first instant of the left edge's date and of every
/// `step`-th calendar date after it, so a 23- or 25-hour day is labelled once.
fn ticks_days<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u64) -> Vec<i64> {
    let mut out = Vec::new();
    let mut d = dt_in(tz, t0).date_naive();
    let mut guard = 0;
    while guard < 2000 {
        if let Some(ts) = first_instant_of(tz, d) {
            if ts > t1 {
                break;
            }
            if ts >= t0 {
                out.push(ts);
            }
        }
        d = match d.checked_add_days(Days::new(step)) {
            Some(x) => x,
            None => break,
        };
        guard += 1;
    }
    out
}

/// Multi-hour ticks (v0.0.12, ruler days, D2): the instants whose local time is `h:00`, `h` a
/// multiple of `step`, date by date; the earlier when `h:00` occurs twice, none when it is
/// skipped. So the labels read 00, 06, 12, 18 on both sides of a change.
fn ticks_hours<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u32) -> Vec<i64> {
    let mut out = Vec::new();
    // One date either side of the range, then sorted: a change may put a date's hours out of order.
    let (Some(first), Some(last)) = (dt_in(tz, t0).date_naive().pred_opt(), dt_in(tz, t1).date_naive().succ_opt())
    else {
        return out;
    };
    let mut d = first;
    let mut guard = 0;
    while d <= last && guard < 2000 {
        for h in (0..24).step_by(step.max(1) as usize) {
            if let Some(x) = d.and_hms_opt(h, 0, 0).and_then(|n| tz.from_local_datetime(&n).earliest()) {
                let ts = x.timestamp();
                if ts >= t0 && ts <= t1 {
                    out.push(ts);
                }
            }
        }
        d = match d.succ_opt() {
            Some(x) => x,
            None => break,
        };
        guard += 1;
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn ticks_months<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u32) -> Vec<i64> {
    let base = dt_in(tz, t0);
    let mut dt = tz
        .with_ymd_and_hms(base.year(), base.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(base);
    let off = dt.month0() % step; // align to a multiple-of-step month from January
    if off != 0 {
        dt = dt.clone().checked_sub_months(Months::new(off)).unwrap_or(dt);
    }
    let mut out = Vec::new();
    let mut guard = 0;
    while dt.timestamp() <= t1 && guard < 2000 {
        if dt.timestamp() >= t0 {
            out.push(dt.timestamp());
        }
        dt = match dt.checked_add_months(Months::new(step)) {
            Some(x) => x,
            None => break,
        };
        guard += 1;
    }
    out
}

fn ticks_years<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: i32) -> Vec<i64> {
    let y0 = dt_in(tz, t0).year();
    let mut y = y0 - y0.rem_euclid(step);
    let mut out = Vec::new();
    let mut guard = 0;
    while guard < 2000 {
        let ts = match tz.with_ymd_and_hms(y, 1, 1, 0, 0, 0).single() {
            Some(d) => d.timestamp(),
            None => break,
        };
        if ts > t1 {
            break;
        }
        if ts >= t0 {
            out.push(ts);
        }
        y += step;
        guard += 1;
    }
    out
}

/// Pick the calendar interval whose on-screen spacing is at least `MIN_PX`, and
/// the ticks + label format for it. `px_per_day` = screen px for one day.
fn axis_ticks(t0: i64, t1: i64, px_per_day: f64) -> (Vec<i64>, AxisFmt) {
    axis_ticks_in(&Local, t0, t1, px_per_day)
}

/// `axis_ticks` read in `tz`: minutes and 1 hour step by fixed durations (D3), 2 to 12 hours
/// on wall-clock hours (D2), days on calendar dates (D1), months and years on the calendar.
fn axis_ticks_in<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, px_per_day: f64) -> (Vec<i64>, AxisFmt) {
    const MIN_PX: f64 = 76.0;
    let want = (MIN_PX / px_per_day.max(1e-6)) * SECS_PER_DAY; // min seconds per tick
    let base = dt_in(tz, t0);
    // Align by subtracting durations from the instant, never by `with_second/minute/hour`:
    // those return `None` when the local time they land on is ambiguous (the repeated hour
    // when clocks go back at a DST change) or does not exist, and the ruler panicked there.
    let z = |dt: chrono::DateTime<Tz>| {
        let back = Duration::seconds(dt.second() as i64) + Duration::nanoseconds(dt.nanosecond() as i64);
        dt - back
    };
    let aligned_min = |m: u32| {
        let b = z(base.clone());
        let back = Duration::minutes((b.minute() % m) as i64);
        b - back
    };
    let aligned_hour = || {
        let b = z(base.clone());
        let back = Duration::minutes(b.minute() as i64);
        b - back
    };
    let m = 60.0;
    let hr = 3600.0;
    let day = 86_400.0;
    if want <= m {
        (ticks_fixed(t0, t1, aligned_min(1), Duration::minutes(1)), AxisFmt::Hm)
    } else if want <= 2.0 * m {
        (ticks_fixed(t0, t1, aligned_min(2), Duration::minutes(2)), AxisFmt::Hm)
    } else if want <= 5.0 * m {
        (ticks_fixed(t0, t1, aligned_min(5), Duration::minutes(5)), AxisFmt::Hm)
    } else if want <= 10.0 * m {
        (ticks_fixed(t0, t1, aligned_min(10), Duration::minutes(10)), AxisFmt::Hm)
    } else if want <= 15.0 * m {
        (ticks_fixed(t0, t1, aligned_min(15), Duration::minutes(15)), AxisFmt::Hm)
    } else if want <= 30.0 * m {
        (ticks_fixed(t0, t1, aligned_min(30), Duration::minutes(30)), AxisFmt::Hm)
    } else if want <= hr {
        (ticks_fixed(t0, t1, aligned_hour(), Duration::hours(1)), AxisFmt::Hm)
    } else if want <= 2.0 * hr {
        (ticks_hours(tz, t0, t1, 2), AxisFmt::Hm)
    } else if want <= 3.0 * hr {
        (ticks_hours(tz, t0, t1, 3), AxisFmt::Hm)
    } else if want <= 6.0 * hr {
        (ticks_hours(tz, t0, t1, 6), AxisFmt::Hm)
    } else if want <= 12.0 * hr {
        (ticks_hours(tz, t0, t1, 12), AxisFmt::Hm)
    } else if want <= day {
        (ticks_days(tz, t0, t1, 1), AxisFmt::Day)
    } else if want <= 2.0 * day {
        (ticks_days(tz, t0, t1, 2), AxisFmt::Day)
    } else if want <= 7.0 * day {
        (ticks_days(tz, t0, t1, 7), AxisFmt::Day)
    } else if want <= 31.0 * day {
        (ticks_months(tz, t0, t1, 1), AxisFmt::Month)
    } else if want <= 93.0 * day {
        (ticks_months(tz, t0, t1, 3), AxisFmt::Month)
    } else if want <= 186.0 * day {
        (ticks_months(tz, t0, t1, 6), AxisFmt::Month)
    } else if want <= 366.0 * day {
        (ticks_years(tz, t0, t1, 1), AxisFmt::Year)
    } else if want <= 2.0 * 366.0 * day {
        (ticks_years(tz, t0, t1, 2), AxisFmt::Year)
    } else if want <= 5.0 * 366.0 * day {
        (ticks_years(tz, t0, t1, 5), AxisFmt::Year)
    } else {
        (ticks_years(tz, t0, t1, 10), AxisFmt::Year)
    }
}

/// Calibrated time axis: a top ruler whose tick spacing + labels are REAL dates
/// that adapt to zoom — minutes/hours when zoomed in, days at mid-zoom, months
/// then years when zoomed out. `ref_ts` is the time at world-X = 0.
pub(crate) fn draw_axis(scene: &mut Scene, shaper: &mut TextShaper, cam: &Camera, w: f64, ref_ts: i64) {
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        pal().input,
        None,
        &Rect::new(0.0, 0.0, w, AXIS_H),
    );
    let (vx0, vx1) = cam.visible_x(w);
    let t0 = ts_of_x(vx0, ref_ts);
    let t1 = ts_of_x(vx1, ref_ts);
    if t1 <= t0 {
        return;
    }
    let px_per_day = WORLD_PER_DAY * cam.zoom;
    let (ticks, fmt) = axis_ticks(t0, t1, px_per_day);
    let label_color = pal().text_dim;
    for ts in ticks {
        let sx = cam.w2s_x(x_of_ts(ts, ref_ts));
        if sx < -40.0 || sx > w {
            continue;
        }
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            pal().border_soft,
            None,
            &Line::new(Point::new(sx, 0.0), Point::new(sx, AXIS_H)),
        );
        let label = shaper.shape(&axis_label(ts, fmt), 120.0, 12.0);
        draw_text(scene, &label, Affine::translate((sx + 4.0, 5.0)), label_color);
    }
}

/// The smallest height (minimap px) of a lane's row (N6: at least a hairline).
const MM_HAIRLINE: f64 = 1.0;

/// Minimap with a live viewport box (needs world_w to map world→minimap).
/// Every lane has its row (N6): the `lane_colors.len()` lanes share the
/// minimap's height, one row each, at least a hairline, each in its lane's
/// colour; the viewport box is the canvas area on the same scale.
pub(crate) fn draw_minimap_world(
    scene: &mut Scene,
    grid: &[u16],
    lane_colors: &[Color],
    cam: &Camera,
    world_w: f64,
    w: f64,
    h: f64,
) {
    let lanes = lane_colors.len();
    // Top-right corner, just below the time axis.
    let (x0, y0) = (w - MM_W - MM_MARGIN, AXIS_H + MM_MARGIN);
    let (x1, y1) = (x0 + MM_W, y0 + MM_H);
    let panel = vello::kurbo::RoundedRect::new(x0, y0, x1, y1, 6.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().minimap_bg.with_alpha(0.92), None, &panel);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border_soft, None, &panel);

    let cell_w = MM_W / MM_BUCKETS as f64;
    let cell_h = MM_H / lanes.max(1) as f64;
    let row_h = cell_h.max(MM_HAIRLINE);
    let max_c = grid.iter().copied().max().unwrap_or(1).max(1);
    for (i, &c) in grid.iter().enumerate() {
        if c == 0 {
            continue;
        }
        let (lane, b) = (i / MM_BUCKETS, i % MM_BUCKETS);
        let Some(color) = lane_colors.get(lane) else { continue };
        let a = 0.25 + 0.75 * (c as f32 / max_c as f32);
        let mx = x0 + b as f64 * cell_w;
        let my = (y0 + lane as f64 * cell_h).min(y1 - row_h);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color.with_alpha(a),
            None,
            &Rect::new(mx, my, mx + cell_w.max(1.0), my + row_h + 0.5),
        );
    }

    // Live viewport box: the canvas area's world range → minimap fractions.
    let world_h = lanes.max(1) as f64 * LANE_H;
    let (vx0, vx1) = cam.visible_x(w);
    let vy0 = (AXIS_H - cam.offset_y) / cam.zoom;
    let vy1 = (h - BOTTOM_CHROME - cam.offset_y) / cam.zoom;
    let fx = |x: f64| x0 + (x / world_w).clamp(0.0, 1.0) * MM_W;
    let fy = |y: f64| y0 + (y / world_h).clamp(0.0, 1.0) * MM_H;
    let vp = Rect::new(fx(vx0), fy(vy0), fx(vx1), fy(vy1));
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, pal().text, None, &vp);
}

#[cfg(test)]
mod time_axis_tests {
    use super::*;

    #[test]
    fn x_ts_scale_and_roundtrip() {
        let r = 1_700_000_000i64;
        // One day forward maps to exactly WORLD_PER_DAY world units.
        assert!((x_of_ts(r + 86_400, r) - WORLD_PER_DAY).abs() < 1e-6);
        // World-X 0 is the reference time.
        assert!(x_of_ts(r, r).abs() < 1e-9);
        // Roundtrip within a second (ts_of_x truncates to whole seconds).
        let x = x_of_ts(r + 12_345, r);
        assert!((ts_of_x(x, r) - (r + 12_345)).abs() <= 1);
    }

    // v0.0.12, the ruler across a DST change (PANDA2, 2026-10-24; found by the item 8 test
    // writer: `axis_ticks` panicked on `with_second(0).unwrap()`). `with_second`, `with_minute`
    // and `with_hour` on a `DateTime<Local>` give `None` when the local time they land on is
    // ambiguous (the repeated hour when clocks go back) or does not exist. The ruler must never
    // panic, whatever its left edge. This walks every quarter hour of 2026 at several zooms, so
    // on a machine whose zone has DST it crosses both changes; on a UTC machine it passes
    // without crossing one (the count printed says which).
    #[test]
    fn the_ruler_never_panics_across_a_year_including_dst_changes() {
        let start = Local.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).earliest().unwrap().timestamp();
        let end = Local.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).earliest().unwrap().timestamp();
        let zooms = [WORLD_PER_DAY * 400.0, WORLD_PER_DAY * 40.0, WORLD_PER_DAY * 4.0, WORLD_PER_DAY * 0.6];
        let mut ambiguous = 0usize;
        let mut t0 = start;
        while t0 < end {
            let local = Local.timestamp_opt(t0, 0).unwrap().naive_local();
            if !matches!(Local.from_local_datetime(&local), chrono::LocalResult::Single(_)) {
                ambiguous += 1;
            }
            for &z in &zooms {
                let (ticks, _) = axis_ticks(t0, t0 + 6 * 3600, z);
                // Ascending, and none far before the left edge.
                assert!(ticks.windows(2).all(|w| w[0] < w[1]), "ticks out of order at t0={t0}, zoom={z}");
                assert!(ticks.iter().all(|&t| t >= t0 - 2 * 86_400), "a tick far before t0={t0}");
            }
            t0 += 900;
        }
        eprintln!("the ruler test crossed {ambiguous} quarter hours whose local time is ambiguous");
    }

    // No `with_second` / `with_minute` / `with_hour` result is unwrapped in this file's
    // production code (a source scan: the test module and comments are not read).
    #[test]
    fn no_local_time_component_is_unwrapped() {
        let src = include_str!("canvas.rs");
        let prod = src.split("mod time_axis_tests").next().unwrap();
        for (n, line) in prod.lines().enumerate() {
            let code = line.split("//").next().unwrap();
            for f in [".with_second(", ".with_minute(", ".with_hour(", ".with_nanosecond("] {
                if let Some(i) = code.find(f) {
                    let rest = &code[i..];
                    let close = rest.find(')').map(|j| &rest[j + 1..]).unwrap_or("");
                    assert!(
                        !close.trim_start().starts_with(".unwrap()"),
                        "canvas.rs:{}: {} … .unwrap() can panic on an ambiguous local time",
                        n + 1,
                        f
                    );
                }
            }
        }
    }

    fn fmt_at(px_per_day: f64) -> AxisFmt {
        let t0 = 1_700_000_000i64;
        axis_ticks(t0, t0 + 86_400, px_per_day).1
    }

    #[test]
    fn interval_adapts_to_zoom() {
        // Default camera zoom (0.6) -> ~132 px/day -> day-grained ruler.
        assert!(matches!(fmt_at(WORLD_PER_DAY * 0.6), AxisFmt::Day));
        // Zoomed in -> hours/minutes.
        assert!(matches!(fmt_at(WORLD_PER_DAY * 40.0), AxisFmt::Hm));
        // Zoomed far out -> months or years.
        assert!(matches!(fmt_at(WORLD_PER_DAY * 0.004), AxisFmt::Year | AxisFmt::Month));
    }

    #[test]
    fn ticks_are_ordered_and_in_range() {
        let t0 = 1_700_000_000i64;
        let t1 = t0 + 7 * 86_400;
        let (ticks, _) = axis_ticks(t0, t1, WORLD_PER_DAY * 0.6);
        assert!(ticks.len() >= 5, "expected several day ticks over a week");
        assert!(ticks.iter().all(|&t| t >= t0 && t <= t1));
        assert!(ticks.windows(2).all(|w| w[0] < w[1]));
    }

    // ---- deck layout --------------------------------------------------

    #[test]
    fn deck_fan_stays_within_lane_top_margin() {
        // Containment: the deepest peek must fit the lane's top slack.
        assert!((DECK_VISIBLE - 1) as f64 * DECK_PEEK <= (LANE_H - CARD_H) / 2.0);
    }

    #[test]
    fn lone_card_is_its_own_front() {
        let roles = deck_roles(&[(0.0, 0)], 1.0);
        assert_eq!(roles[0], DeckRole { count: 1, peek: 0, hidden: false, front_idx: 0 });
    }

    #[test]
    fn deck_members_point_at_the_front_index() {
        // Three cards in one lane, close in x -> one deck; front is the max-x (idx 2).
        let roles = deck_roles(&[(0.0, 0), (10.0, 0), (20.0, 0)], 1.0);
        assert!(roles.iter().all(|r| r.front_idx == 2));
    }

    #[test]
    fn same_position_cards_form_one_deck_with_hidden_overflow() {
        let pos: Vec<(f64, usize)> = (0..7).map(|_| (0.0, 0)).collect();
        let roles = deck_roles(&pos, 1.0);
        assert!(roles.iter().all(|r| r.count == 7));
        assert_eq!(roles.iter().filter(|r| r.is_front()).count(), 1);
        assert_eq!(roles.iter().filter(|r| !r.hidden).count(), DECK_VISIBLE);
        assert_eq!(roles.iter().filter(|r| r.hidden).count(), 7 - DECK_VISIBLE);
    }

    #[test]
    fn most_recent_card_is_front() {
        let roles = deck_roles(&[(0.0, 0), (10.0, 0), (20.0, 0)], 1.0);
        assert!(roles[2].is_front()); // max x
        assert_eq!(roles[0].peek, 2); // oldest deepest
    }

    #[test]
    fn cards_farther_than_a_card_width_do_not_deck() {
        let roles = deck_roles(&[(0.0, 0), (CARD_W + 1.0, 0)], 1.0);
        assert_eq!(roles[0].count, 1);
        assert_eq!(roles[1].count, 1);
    }

    #[test]
    fn zooming_in_dissolves_a_deck() {
        // Gap 150 world px: < CARD_W(176) at zoom 1 → decked; screen gap 150*z,
        // so at zoom 2 it is 300 > 176 → separate.
        let pos = [(0.0, 0), (150.0, 0)];
        assert_eq!(deck_roles(&pos, 1.0)[0].count, 2);
        assert_eq!(deck_roles(&pos, 2.0)[0].count, 1);
    }

    #[test]
    fn identical_position_stays_decked_at_any_zoom() {
        let pos = [(42.0, 0), (42.0, 0)];
        assert_eq!(deck_roles(&pos, 80.0)[0].count, 2);
    }

    #[test]
    fn different_lanes_never_merge() {
        let roles = deck_roles(&[(0.0, 0), (0.0, 1)], 1.0);
        assert_eq!(roles[0].count, 1);
        assert_eq!(roles[1].count, 1);
    }
}

// v0.0.12, the ruler's days follow the calendar (doc/plans/V0012-ruler-calendar-days.md, D1-D6).
// Declared after the module above, so the source scan's production half does not include it.
#[cfg(test)]
#[path = "ruler_days_tests.rs"]
mod ruler_days_tests;
