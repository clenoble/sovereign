use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::camera::Camera;
use crate::layout::{CardLayout, CanvasLayout};

/// Animation state for a card transitioning from external → owned.
#[derive(Debug, Clone)]
pub struct AdoptionAnim {
    pub start: Instant,
    pub duration: Duration,
}

impl AdoptionAnim {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(600),
        }
    }

    /// Returns 0.0 at start, 1.0 when complete.
    pub fn progress(&self) -> f32 {
        let elapsed = self.start.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();
        (elapsed / total).clamp(0.0, 1.0)
    }

    pub fn is_done(&self) -> bool {
        self.start.elapsed() >= self.duration
    }
}

/// Filter criteria for which cards to display.
#[derive(Debug, Clone)]
pub struct CanvasFilter {
    pub show_owned: bool,
    pub show_external: bool,
    pub thread_ids: Option<Vec<String>>,
}

impl Default for CanvasFilter {
    fn default() -> Self {
        Self {
            show_owned: true,
            show_external: true,
            thread_ids: None,
        }
    }
}

impl CanvasFilter {
    /// Returns true if the given card passes this filter.
    pub fn matches(&self, card: &CardLayout) -> bool {
        if card.is_owned && !self.show_owned {
            return false;
        }
        if !card.is_owned && !self.show_external {
            return false;
        }
        if let Some(ref ids) = self.thread_ids {
            if !ids.contains(&card.thread_id) {
                return false;
            }
        }
        true
    }
}

/// All mutable state for the canvas, held in Arc<Mutex<>>.
pub struct CanvasState {
    pub camera: Camera,
    pub layout: CanvasLayout,
    pub filter: CanvasFilter,
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub hovered: Option<usize>,
    pub selected: Option<usize>,
    pub highlighted: HashSet<String>,
    pub minimap_visible: bool,
    pub adoption_animations: HashMap<String, AdoptionAnim>,
    pub frame_times: VecDeque<f64>,
    pub viewport_width: f64,
    pub viewport_height: f64,
    /// Set by shader on double-click; consumed by the app's Tick handler.
    pub pending_open: Option<String>,

    // ── Render cache (dirty-flag gating) ───────────────────────────
    /// Incremented on any visual change; compared against `cached_gen` to detect staleness.
    pub render_gen: u64,
    /// Pixel buffer from the last Skia render (RGBA, w*h*4 bytes).
    pub cached_pixels: Option<Arc<Vec<u8>>>,
    /// (width, height) at which the cache was produced.
    pub cached_size: (u32, u32),
    /// The `render_gen` value when the cache was last written.
    pub cached_gen: u64,
}

impl CanvasState {
    pub fn new(layout: CanvasLayout) -> Self {
        Self {
            camera: Camera::new(),
            layout,
            filter: CanvasFilter::default(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            hovered: None,
            selected: None,
            highlighted: HashSet::new(),
            minimap_visible: true,
            adoption_animations: HashMap::new(),
            frame_times: VecDeque::with_capacity(120),
            viewport_width: 1280.0,
            viewport_height: 720.0,
            pending_open: None,
            render_gen: 1,
            cached_pixels: None,
            cached_size: (0, 0),
            cached_gen: 0,
        }
    }

    /// Mark the canvas as needing a fresh Skia render on the next draw().
    pub fn mark_dirty(&mut self) {
        self.render_gen = self.render_gen.wrapping_add(1);
    }

    pub fn avg_fps(&self) -> f64 {
        if self.frame_times.len() < 2 {
            return 0.0;
        }
        let n = self.frame_times.len().min(60);
        let avg_ms: f64 = self.frame_times.iter().rev().take(n).sum::<f64>() / n as f64;
        if avg_ms > 0.0 {
            1000.0 / avg_ms
        } else {
            0.0
        }
    }

    /// Record a frame time, keeping the deque bounded at 120 entries.
    pub fn push_frame_time(&mut self, ms: f64) {
        if self.frame_times.len() >= 120 {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(ms);
    }
}

/// Hit-test: find which card (if any) is under the given screen coordinates.
/// Checks in reverse order so last-drawn (topmost) cards are found first.
pub fn hit_test(state: &CanvasState, screen_x: f64, screen_y: f64) -> Option<usize> {
    let (wx, wy) = state.camera.screen_to_world(screen_x, screen_y);
    for (i, card) in state.layout.cards.iter().enumerate().rev() {
        if wx >= card.x as f64
            && wx <= (card.x + card.w) as f64
            && wy >= card.y as f64
            && wy <= (card.y + card.h) as f64
        {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{CardLayout, CanvasLayout, LaneLayout};

    fn test_state() -> CanvasState {
        let layout = CanvasLayout {
            cards: vec![
                CardLayout {
                    doc_id: "d1".into(),
                    title: "Card 1".into(),
                    is_owned: true,
                    thread_id: "t1".into(),
                    created_at_ts: 1000,
                    x: 100.0,
                    y: 100.0,
                    w: 200.0,
                    h: 80.0,
                },
                CardLayout {
                    doc_id: "d2".into(),
                    title: "Card 2".into(),
                    is_owned: false,
                    thread_id: "t1".into(),
                    created_at_ts: 2000,
                    x: 320.0,
                    y: 100.0,
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
            document_edges: vec![],
        };
        CanvasState::new(layout)
    }

    #[test]
    fn hit_inside_card_returns_index() {
        let state = test_state();
        // Default camera: pan_x=-200, pan_y=-100, zoom=1.0
        // screen_to_world(sx, sy) = (sx/1.0 + (-200), sy/1.0 + (-100))
        // To hit card at (100, 100): need world = (150, 130)
        // screen = (150 - (-200), 130 - (-100)) = (350, 230)
        let result = hit_test(&state, 350.0, 230.0);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn hit_outside_all_cards_returns_none() {
        let state = test_state();
        // World (0,0) -> screen (200, 100) with default camera
        let result = hit_test(&state, 200.0, 100.0);
        assert_eq!(result, None);
    }

    #[test]
    fn hit_second_card() {
        let state = test_state();
        // Card 2 at (320, 100). Center = (420, 140).
        // screen = (420 + 200, 140 + 100) = (620, 240)
        let result = hit_test(&state, 620.0, 240.0);
        assert_eq!(result, Some(1));
    }

    fn make_card(doc_id: &str, is_owned: bool, thread_id: &str) -> CardLayout {
        CardLayout {
            doc_id: doc_id.into(),
            title: "Test".into(),
            is_owned,
            thread_id: thread_id.into(),
            created_at_ts: 0,
            x: 0.0, y: 0.0, w: 200.0, h: 80.0,
        }
    }

    #[test]
    fn filter_default_shows_all() {
        let f = CanvasFilter::default();
        assert!(f.matches(&make_card("d1", true, "t1")));
        assert!(f.matches(&make_card("d2", false, "t1")));
    }

    #[test]
    fn filter_hide_external() {
        let f = CanvasFilter {
            show_owned: true,
            show_external: false,
            thread_ids: None,
        };
        assert!(f.matches(&make_card("d1", true, "t1")));
        assert!(!f.matches(&make_card("d2", false, "t1")));
    }

    #[test]
    fn filter_hide_owned() {
        let f = CanvasFilter {
            show_owned: false,
            show_external: true,
            thread_ids: None,
        };
        assert!(!f.matches(&make_card("d1", true, "t1")));
        assert!(f.matches(&make_card("d2", false, "t1")));
    }

    #[test]
    fn adoption_anim_progress() {
        let anim = AdoptionAnim {
            start: Instant::now() - Duration::from_millis(300),
            duration: Duration::from_millis(600),
        };
        let p = anim.progress();
        assert!(p >= 0.4 && p <= 0.6, "progress was {}", p);
        assert!(!anim.is_done());
    }

    #[test]
    fn adoption_anim_done() {
        let anim = AdoptionAnim {
            start: Instant::now() - Duration::from_millis(700),
            duration: Duration::from_millis(600),
        };
        assert_eq!(anim.progress(), 1.0);
        assert!(anim.is_done());
    }

    #[test]
    fn filter_by_thread_ids() {
        let f = CanvasFilter {
            show_owned: true,
            show_external: true,
            thread_ids: Some(vec!["t1".into()]),
        };
        assert!(f.matches(&make_card("d1", true, "t1")));
        assert!(!f.matches(&make_card("d2", true, "t2")));
    }
}
