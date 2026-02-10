use skia_safe::Color4f;

pub const BG: Color4f = Color4f::new(0.055, 0.055, 0.063, 1.0); // #0e0e10
pub const OWNED_FILL: Color4f = Color4f::new(0.106, 0.165, 0.227, 1.0); // #1b2a3a
pub const OWNED_BORDER: Color4f = Color4f::new(0.353, 0.624, 0.831, 1.0); // #5a9fd4
pub const EXT_FILL: Color4f = Color4f::new(0.227, 0.125, 0.125, 1.0); // #3a2020
pub const EXT_BORDER: Color4f = Color4f::new(0.878, 0.486, 0.416, 1.0); // #e07c6a
pub const ACCENT: Color4f = Color4f::new(0.420, 0.639, 0.839, 1.0); // #6ba3d6
pub const TEXT_PRIMARY: Color4f = Color4f::new(0.9, 0.9, 0.9, 1.0);
pub const TEXT_DIM: Color4f = Color4f::new(0.6, 0.6, 0.6, 1.0);
pub const GRID_LINE: Color4f = Color4f::new(0.15, 0.15, 0.17, 1.0);
pub const TIMELINE_LINE: Color4f = Color4f::new(0.22, 0.22, 0.25, 1.0);
pub const LANE_HEADER_BG: Color4f = Color4f::new(0.08, 0.08, 0.10, 0.85);

pub const LANE_COLORS: [Color4f; 8] = [
    Color4f::new(0.10, 0.12, 0.18, 0.35), // blue tint
    Color4f::new(0.18, 0.10, 0.12, 0.35), // red tint
    Color4f::new(0.10, 0.18, 0.12, 0.35), // green tint
    Color4f::new(0.18, 0.16, 0.10, 0.35), // amber tint
    Color4f::new(0.14, 0.10, 0.18, 0.35), // purple tint
    Color4f::new(0.10, 0.17, 0.17, 0.35), // teal tint
    Color4f::new(0.18, 0.12, 0.16, 0.35), // pink tint
    Color4f::new(0.12, 0.14, 0.18, 0.35), // steel tint
];
