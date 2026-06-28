//! Pan/zoom camera: world <-> screen transforms and visible-range queries.

pub(crate) struct Camera {
    pub(crate) offset_x: f64,
    pub(crate) offset_y: f64,
    pub(crate) zoom: f64,
}
impl Camera {
    pub(crate) fn w2s_x(&self, x: f64) -> f64 {
        x * self.zoom + self.offset_x
    }
    pub(crate) fn w2s_y(&self, y: f64) -> f64 {
        y * self.zoom + self.offset_y
    }
    pub(crate) fn visible_x(&self, w: f64) -> (f64, f64) {
        ((-self.offset_x) / self.zoom, (w - self.offset_x) / self.zoom)
    }
    pub(crate) fn visible_y(&self, h: f64) -> (f64, f64) {
        ((-self.offset_y) / self.zoom, (h - self.offset_y) / self.zoom)
    }
    pub(crate) fn zoom_toward(&mut self, cx: f64, cy: f64, factor: f64) {
        let wx = (cx - self.offset_x) / self.zoom;
        let wy = (cy - self.offset_y) / self.zoom;
        // Wide range so the time ruler can span minutes (zoomed in) to years
        // (zoomed far out) — see canvas::axis_ticks.
        self.zoom = (self.zoom * factor).clamp(0.0008, 80.0);
        self.offset_x = cx - wx * self.zoom;
        self.offset_y = cy - wy * self.zoom;
    }
}
