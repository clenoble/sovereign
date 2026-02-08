/// 2-D camera: world_pos = screen_pos / zoom + pan
#[derive(Debug, Clone)]
pub struct Camera {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
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
