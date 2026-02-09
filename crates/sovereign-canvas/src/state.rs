use skia_safe::gpu;

use crate::camera::Camera;
use crate::layout::CanvasLayout;

/// All mutable state for the canvas, held in Rc<RefCell<>>.
pub struct CanvasState {
    pub camera: Camera,
    pub layout: CanvasLayout,
    pub mouse_x: f64,
    pub mouse_y: f64,
    pub hovered: Option<usize>,
    pub selected: Option<usize>,
    pub highlighted: Vec<String>,
    pub frame_times: Vec<f64>,
    pub gr_context: Option<gpu::DirectContext>,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

impl CanvasState {
    pub fn new(layout: CanvasLayout) -> Self {
        Self {
            camera: Camera::new(),
            layout,
            mouse_x: 0.0,
            mouse_y: 0.0,
            hovered: None,
            selected: None,
            highlighted: Vec::new(),
            frame_times: Vec::with_capacity(300),
            gr_context: None,
            viewport_width: 1280.0,
            viewport_height: 720.0,
        }
    }

    pub fn avg_fps(&self) -> f64 {
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
                    x: 100.0,
                    y: 100.0,
                    w: 200.0,
                    h: 80.0,
                },
                CardLayout {
                    doc_id: "d2".into(),
                    title: "Card 2".into(),
                    is_owned: false,
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
}
