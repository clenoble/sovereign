/// Level-of-detail zoom thresholds for the canvas.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomLevel {
    /// zoom < 0.3 — show colored dots only
    Dots,
    /// 0.3 ≤ zoom < 0.6 — simplified card (shape + color, no text)
    Simplified,
    /// zoom ≥ 0.6 — full card with title and badge
    Full,
}

pub fn zoom_level(zoom: f64) -> ZoomLevel {
    if zoom < 0.3 {
        ZoomLevel::Dots
    } else if zoom < 0.6 {
        ZoomLevel::Simplified
    } else {
        ZoomLevel::Full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_at_very_low_zoom() {
        assert_eq!(zoom_level(0.02), ZoomLevel::Dots);
        assert_eq!(zoom_level(0.1), ZoomLevel::Dots);
        assert_eq!(zoom_level(0.29), ZoomLevel::Dots);
    }

    #[test]
    fn simplified_at_mid_zoom() {
        assert_eq!(zoom_level(0.3), ZoomLevel::Simplified);
        assert_eq!(zoom_level(0.45), ZoomLevel::Simplified);
        assert_eq!(zoom_level(0.59), ZoomLevel::Simplified);
    }

    #[test]
    fn full_at_high_zoom() {
        assert_eq!(zoom_level(0.6), ZoomLevel::Full);
        assert_eq!(zoom_level(1.0), ZoomLevel::Full);
        assert_eq!(zoom_level(5.0), ZoomLevel::Full);
    }

    #[test]
    fn boundary_values() {
        assert_eq!(zoom_level(0.3), ZoomLevel::Simplified);
        assert_eq!(zoom_level(0.6), ZoomLevel::Full);
    }
}
