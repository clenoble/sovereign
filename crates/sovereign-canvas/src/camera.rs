use crate::layout::CanvasLayout;

/// 2-D camera: world_pos = screen_pos / zoom + pan
#[derive(Debug, Clone)]
pub struct Camera {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

/// Compute the "home" position: pan so the newest card (rightmost X) is
/// visible near the right edge of the viewport.
/// Returns (pan_x, pan_y). If there are no cards, returns the default pan.
pub fn home_position(layout: &CanvasLayout) -> (f64, f64) {
    if layout.cards.is_empty() {
        return (-200.0, -100.0);
    }
    let max_x = layout
        .cards
        .iter()
        .map(|c| c.x + c.w)
        .fold(f32::NEG_INFINITY, f32::max);
    // Pan so the rightmost card sits ~400px from the left edge of the viewport
    let pan_x = (max_x as f64) - 400.0;
    let pan_y = -100.0;
    (pan_x, pan_y)
}

impl Camera {
    pub fn new() -> Self {
        Self {
            pan_x: -200.0,
            pan_y: -100.0,
            zoom: 1.0,
        }
    }

    pub fn to_matrix(&self) -> skia_safe::Matrix {
        let mut m = skia_safe::Matrix::new_identity();
        m.pre_scale((self.zoom as f32, self.zoom as f32), None);
        m.pre_translate((-self.pan_x as f32, -self.pan_y as f32));
        m
    }

    pub fn screen_to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        (sx / self.zoom + self.pan_x, sy / self.zoom + self.pan_y)
    }

    /// World-space bounding box visible in the viewport: (left, top, right, bottom).
    pub fn visible_rect(&self, vp_w: f64, vp_h: f64) -> (f64, f64, f64, f64) {
        let left = self.pan_x;
        let top = self.pan_y;
        let right = self.pan_x + vp_w / self.zoom;
        let bottom = self.pan_y + vp_h / self.zoom;
        (left, top, right, bottom)
    }

    pub fn zoom_at(&mut self, screen_x: f64, screen_y: f64, factor: f64) {
        let old = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.02, 20.0);
        self.pan_x += screen_x * (1.0 / old - 1.0 / self.zoom);
        self.pan_y += screen_y * (1.0 / old - 1.0 / self.zoom);
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{CardLayout, CanvasLayout, LaneLayout};

    #[test]
    fn home_position_empty_layout() {
        let layout = CanvasLayout {
            cards: vec![],
            lanes: vec![],
            timeline_markers: vec![],
            branch_edges: vec![],
            document_edges: vec![],
        };
        let (px, py) = home_position(&layout);
        assert_eq!(px, -200.0);
        assert_eq!(py, -100.0);
    }

    #[test]
    fn home_position_pans_to_rightmost_card() {
        let layout = CanvasLayout {
            cards: vec![
                CardLayout {
                    doc_id: "d1".into(),
                    title: "A".into(),
                    is_owned: true,
                    thread_id: "t1".into(),
                    created_at_ts: 1000,
                    x: 100.0,
                    y: 30.0,
                    w: 200.0,
                    h: 80.0,
                },
                CardLayout {
                    doc_id: "d2".into(),
                    title: "B".into(),
                    is_owned: true,
                    thread_id: "t1".into(),
                    created_at_ts: 2000,
                    x: 500.0,
                    y: 30.0,
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
        let (px, _py) = home_position(&layout);
        // rightmost card edge = 500 + 200 = 700. home = 700 - 400 = 300.
        assert_eq!(px, 300.0);
    }

    #[test]
    fn default_values() {
        let cam = Camera::new();
        assert_eq!(cam.pan_x, -200.0);
        assert_eq!(cam.pan_y, -100.0);
        assert_eq!(cam.zoom, 1.0);
    }

    #[test]
    fn screen_to_world_identity_zoom() {
        let mut cam = Camera::new();
        cam.pan_x = 0.0;
        cam.pan_y = 0.0;
        cam.zoom = 1.0;
        let (wx, wy) = cam.screen_to_world(100.0, 200.0);
        assert_eq!(wx, 100.0);
        assert_eq!(wy, 200.0);
    }

    #[test]
    fn screen_to_world_with_zoom() {
        let mut cam = Camera::new();
        cam.pan_x = 0.0;
        cam.pan_y = 0.0;
        cam.zoom = 2.0;
        let (wx, wy) = cam.screen_to_world(100.0, 200.0);
        assert_eq!(wx, 50.0);
        assert_eq!(wy, 100.0);
    }

    #[test]
    fn zoom_at_clamps_min() {
        let mut cam = Camera::new();
        cam.zoom = 0.03;
        cam.zoom_at(0.0, 0.0, 0.5);
        assert!(cam.zoom >= 0.02);
    }

    #[test]
    fn zoom_at_clamps_max() {
        let mut cam = Camera::new();
        cam.zoom = 19.0;
        cam.zoom_at(0.0, 0.0, 2.0);
        assert!(cam.zoom <= 20.0);
    }
}
