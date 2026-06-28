//! The spatial-canvas data model + world-space painters: synthetic + real
//! workspace loading, cards/links/minimap, the link edges, the adaptive time
//! axis, and the minimap overlay.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{Datelike, Duration, Local, Months, TimeZone, Timelike};
use parley::Layout;
use vello::kurbo::{Affine, BezPath, Line, Point, Rect, Stroke};
use vello::peniko::{Color, Fill};
use vello::kurbo::CubicBez;
use vello::Scene;

use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_core::auth::PersonaKind as CorePersona;

use crate::camera::Camera;
use crate::crypto::persona_raw_db_path;
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::pal;

// ---- Layout constants ---------------------------------------------------

pub(crate) const LANES: usize = 12;
pub(crate) const LANE_H: f64 = 96.0;

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
    pub(crate) title: String,  // raw title, re-shaped larger for the document window
    pub(crate) body: String,   // raw content (or a lock placeholder when encrypted)
    pub(crate) layout: Layout<Brush>, // card-title shaped once, redrawn each frame
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

pub(crate) fn make_cards(n: usize, world_w: f64, rng: &mut Lcg, shaper: &mut TextShaper) -> Vec<Card> {
    let max_w = (CARD_W - 2.0 * PAD) as f32;
    let mut cards: Vec<Card> = (0..n)
        .map(|i| {
            let x = rng.next() * world_w;
            let lane = (rng.next() * LANES as f64) as usize % LANES;
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

/// Pre-binned minimap density grid [lane][bucket] so the minimap is cheap to
/// redraw (a production minimap would cache to a texture; this is the same idea).
pub(crate) fn make_minimap(cards: &[Card], world_w: f64) -> Vec<u16> {
    let mut grid = vec![0u16; LANES * MM_BUCKETS];
    for c in cards {
        let b = ((c.x / world_w) * MM_BUCKETS as f64) as usize;
        let b = b.min(MM_BUCKETS - 1);
        let idx = c.lane * MM_BUCKETS + b;
        grid[idx] = grid[idx].saturating_add(1);
    }
    grid
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

/// Returns `(cards, links, world_w, lane_names, ref_ts)`. `ref_ts` is the unix
/// time at world-X = 0 (calibrates the time ruler). `None` only when there is
/// nothing at all (no docs *and* no threads) — a workspace with labeled-but-empty
/// lanes still returns (so the lanes render and the `+` button has structure).
pub(crate) async fn load_workspace(
    db: &Arc<dyn GraphDB>,
    shaper: &mut TextShaper,
) -> Option<(Vec<Card>, Vec<(u32, u32)>, f64, Vec<String>, i64)> {
    {
        let docs = db.list_documents(None).await.ok()?;
        let threads = db.list_threads().await.unwrap_or_default();
        if docs.is_empty() && threads.is_empty() {
            return None;
        }
        let rels = db.list_all_relationships().await.unwrap_or_default();

        // thread record-id -> lane (collapsed onto LANES; a per-thread dynamic
        // lane count is a later refinement) + the per-lane display name.
        let mut lane_of: HashMap<String, usize> = HashMap::new();
        let mut lane_names: Vec<String> = vec![String::new(); LANES];
        for (i, t) in threads.iter().enumerate() {
            let lane = i % LANES;
            if let Some(id) = &t.id {
                lane_of.insert(id.to_string(), lane);
            }
            // Plaintext name when decrypted (real app clears the nonce on read);
            // a lock placeholder in raw/no-auth mode where it stays ciphertext.
            if t.name_nonce.is_none() && !t.name.is_empty() {
                lane_names[lane] = t.name.clone();
            } else if t.name_nonce.is_some() {
                let id = t.id.as_ref().map(|x| x.to_string()).unwrap_or_default();
                let short: String = id.rsplit(':').next().unwrap_or("?").chars().take(4).collect();
                lane_names[lane] = format!("\u{1f512} {short}");
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

        // Docs whose thread_id doesn't resolve to a lane go to a dedicated
        // "Unfiled" lane rather than silently piling into lane 0 (where they'd
        // stack on top of that lane's real docs and look like a single card).
        // This currently happens to P2P-synced docs: sync doesn't transport
        // thread membership yet, so they arrive with thread_id "default".
        let unfiled_lane = threads.len().min(LANES - 1);
        let mut used_unfiled = false;
        let mut id_to_idx: HashMap<String, usize> = HashMap::new();
        let mut cards = Vec::with_capacity(docs.len());
        for (i, d) in docs.iter().enumerate() {
            if let Some(id) = &d.id {
                id_to_idx.insert(id.to_string(), i);
            }
            let lane = match lane_of.get(&d.thread_id) {
                Some(&l) => l,
                None => {
                    used_unfiled = true;
                    unfiled_lane
                }
            };
            let x = x_of_ts(d.modified_at.timestamp(), ref_ts);
            let title = if d.title_nonce.is_none() && !d.title.is_empty() {
                d.title.clone()
            } else {
                let id = d.id.as_ref().map(|t| t.to_string()).unwrap_or_default();
                let short: String =
                    id.rsplit(':').next().unwrap_or("?").chars().take(6).collect();
                format!("\u{1f512} {short}")
            };
            let body = if d.encryption_nonce.is_none() && !d.content.is_empty() {
                d.content.clone()
            } else if !d.content.is_empty() {
                "\u{1f512} Encrypted — this document unlocks after login.".to_string()
            } else {
                "(empty document)".to_string()
            };
            cards.push(Card {
                id: d.id.as_ref().map(|t| t.to_string()).unwrap_or_default(),
                x,
                lane,
                external: !d.is_owned,
                pinned: d.pinned,
                title: title.clone(),
                body,
                layout: shaper.shape(&title, max_w, TITLE_PX),
            });
        }

        if used_unfiled && lane_names.get(unfiled_lane).is_some_and(|s| s.is_empty()) {
            lane_names[unfiled_lane] = "Unfiled".to_string();
        }

        let mut links = Vec::new();
        for r in &rels {
            if let (Some(in_), Some(out)) = (&r.in_, &r.out) {
                if let (Some(&a), Some(&b)) =
                    (id_to_idx.get(&in_.to_string()), id_to_idx.get(&out.to_string()))
                {
                    links.push((a as u32, b as u32));
                }
            }
        }
        Some((cards, links, world_w, lane_names, ref_ts))
    }
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

pub(crate) fn lane_color(lane: usize) -> Color {
    const HUES: [(u8, u8, u8); 12] = [
        (64, 110, 180), (90, 150, 110), (170, 120, 70), (150, 90, 150),
        (70, 150, 160), (180, 100, 100), (110, 120, 170), (140, 150, 80),
        (100, 140, 160), (160, 110, 130), (90, 130, 90), (130, 110, 160),
    ];
    let (r, g, b) = HUES[lane % HUES.len()];
    Color::from_rgb8(r, g, b)
}

// ---- Overlays (free fns: borrow only the fields they need) --------------

/// Cross-thread link edges as cubic beziers (culled to the visible x-range).
pub(crate) fn draw_links(scene: &mut Scene, cards: &[Card], links: &[(u32, u32)], cam: &Camera, w: f64) -> usize {
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
            lane_color(ca.lane).with_alpha(0.30),
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

fn local_dt(ts: i64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(ts, 0).single().unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap())
}

fn axis_label(ts: i64, fmt: AxisFmt) -> String {
    let dt = local_dt(ts);
    match fmt {
        AxisFmt::Hm => format!("{:02}:{:02}", dt.hour(), dt.minute()),
        AxisFmt::Day => format!("{} {}", MONTHS[dt.month0() as usize], dt.day()),
        AxisFmt::Month => format!("{} {:02}", MONTHS[dt.month0() as usize], dt.year().rem_euclid(100)),
        AxisFmt::Year => format!("{}", dt.year()),
    }
}

/// Ticks at a fixed-duration step (minute/hour/day), starting at an aligned
/// boundary `start` (already snapped to a local minute/hour/midnight).
fn ticks_fixed(t0: i64, t1: i64, start: chrono::DateTime<Local>, step: Duration) -> Vec<i64> {
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

fn ticks_months(t0: i64, t1: i64, step: u32) -> Vec<i64> {
    let base = local_dt(t0);
    let mut dt = Local
        .with_ymd_and_hms(base.year(), base.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(base);
    let off = dt.month0() % step; // align to a multiple-of-step month from January
    if off != 0 {
        dt = dt.checked_sub_months(Months::new(off)).unwrap_or(dt);
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

fn ticks_years(t0: i64, t1: i64, step: i32) -> Vec<i64> {
    let y0 = local_dt(t0).year();
    let mut y = y0 - y0.rem_euclid(step);
    let mut out = Vec::new();
    let mut guard = 0;
    while guard < 2000 {
        let ts = match Local.with_ymd_and_hms(y, 1, 1, 0, 0, 0).single() {
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
    const MIN_PX: f64 = 76.0;
    let want = (MIN_PX / px_per_day.max(1e-6)) * SECS_PER_DAY; // min seconds per tick
    let base = local_dt(t0);
    let z = |dt: chrono::DateTime<Local>| dt.with_second(0).unwrap().with_nanosecond(0).unwrap();
    let aligned_min = |m: u32| {
        let b = z(base);
        b - Duration::minutes((b.minute() % m) as i64)
    };
    let aligned_hour = |hh: u32| {
        let b = z(base).with_minute(0).unwrap();
        b - Duration::hours((b.hour() % hh) as i64)
    };
    let midnight = z(base).with_minute(0).unwrap().with_hour(0).unwrap();
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
        (ticks_fixed(t0, t1, aligned_hour(1), Duration::hours(1)), AxisFmt::Hm)
    } else if want <= 2.0 * hr {
        (ticks_fixed(t0, t1, aligned_hour(2), Duration::hours(2)), AxisFmt::Hm)
    } else if want <= 3.0 * hr {
        (ticks_fixed(t0, t1, aligned_hour(3), Duration::hours(3)), AxisFmt::Hm)
    } else if want <= 6.0 * hr {
        (ticks_fixed(t0, t1, aligned_hour(6), Duration::hours(6)), AxisFmt::Hm)
    } else if want <= 12.0 * hr {
        (ticks_fixed(t0, t1, aligned_hour(12), Duration::hours(12)), AxisFmt::Hm)
    } else if want <= day {
        (ticks_fixed(t0, t1, midnight, Duration::days(1)), AxisFmt::Day)
    } else if want <= 2.0 * day {
        (ticks_fixed(t0, t1, midnight, Duration::days(2)), AxisFmt::Day)
    } else if want <= 7.0 * day {
        (ticks_fixed(t0, t1, midnight, Duration::days(7)), AxisFmt::Day)
    } else if want <= 31.0 * day {
        (ticks_months(t0, t1, 1), AxisFmt::Month)
    } else if want <= 93.0 * day {
        (ticks_months(t0, t1, 3), AxisFmt::Month)
    } else if want <= 186.0 * day {
        (ticks_months(t0, t1, 6), AxisFmt::Month)
    } else if want <= 366.0 * day {
        (ticks_years(t0, t1, 1), AxisFmt::Year)
    } else if want <= 2.0 * 366.0 * day {
        (ticks_years(t0, t1, 2), AxisFmt::Year)
    } else if want <= 5.0 * 366.0 * day {
        (ticks_years(t0, t1, 5), AxisFmt::Year)
    } else {
        (ticks_years(t0, t1, 10), AxisFmt::Year)
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

/// Minimap with a live viewport box (needs world_w to map world→minimap).
pub(crate) fn draw_minimap_world(scene: &mut Scene, grid: &[u16], cam: &Camera, world_w: f64, w: f64, h: f64) {
    // Top-right corner, just below the time axis.
    let (x0, y0) = (w - MM_W - MM_MARGIN, AXIS_H + MM_MARGIN);
    let (x1, y1) = (x0 + MM_W, y0 + MM_H);
    let panel = vello::kurbo::RoundedRect::new(x0, y0, x1, y1, 6.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().minimap_bg.with_alpha(0.92), None, &panel);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border_soft, None, &panel);

    let cell_w = MM_W / MM_BUCKETS as f64;
    let cell_h = MM_H / LANES as f64;
    let max_c = grid.iter().copied().max().unwrap_or(1).max(1);
    for lane in 0..LANES {
        for b in 0..MM_BUCKETS {
            let c = grid[lane * MM_BUCKETS + b];
            if c == 0 {
                continue;
            }
            let a = 0.25 + 0.75 * (c as f32 / max_c as f32);
            let mx = x0 + b as f64 * cell_w;
            let my = y0 + lane as f64 * cell_h;
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                lane_color(lane).with_alpha(a),
                None,
                &Rect::new(mx, my, mx + cell_w.max(1.0), my + cell_h + 0.5),
            );
        }
    }

    // Live viewport box: visible world range → minimap fractions.
    let world_h = LANES as f64 * LANE_H;
    let (vx0, vx1) = cam.visible_x(w);
    let (vy0, vy1) = cam.visible_y(h);
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
}
