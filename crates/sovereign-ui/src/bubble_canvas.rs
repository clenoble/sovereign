//! Animated bubble canvas rendering for the 9 AI bubble styles.
//!
//! Each style implements procedural drawing via Iced's `canvas` widget,
//! driven by elapsed time from the app's 60fps Tick subscription.

use std::f32::consts::PI;

use iced::widget::canvas::{self, Frame, Path, Stroke, stroke};
use iced::{Color, Point, Rectangle, mouse};

use sovereign_core::profile::BubbleStyle;

// ── Color constants (from AnimatedBubbles.tsx) ─────────────────────────

const WAVE_BLUE: Color = Color { r: 0.376, g: 0.647, b: 0.980, a: 1.0 };
const WAVE_BLUE_DARK: Color = Color { r: 0.231, g: 0.510, b: 0.965, a: 1.0 };
const SPIN_PURPLE: Color = Color { r: 0.545, g: 0.361, b: 0.965, a: 1.0 };
const PULSE_PINK: Color = Color { r: 0.925, g: 0.282, b: 0.600, a: 1.0 };
const PULSE_PINK_LIGHT: Color = Color { r: 0.957, g: 0.447, b: 0.714, a: 1.0 };
const BLINK_GOLD: Color = Color { r: 0.988, g: 0.827, b: 0.302, a: 1.0 };
const BLINK_ORANGE: Color = Color { r: 0.961, g: 0.620, b: 0.043, a: 1.0 };
const RINGS_GREEN: Color = Color { r: 0.063, g: 0.725, b: 0.506, a: 1.0 };
const RINGS_GREEN_LIGHT: Color = Color { r: 0.204, g: 0.827, b: 0.600, a: 1.0 };
const ORBIT_DARK: Color = Color { r: 0.851, g: 0.467, b: 0.024, a: 1.0 };
const ICON_GOLD: Color = Color { r: 0.988, g: 0.827, b: 0.302, a: 1.0 };
const ICON_BRONZE: Color = Color { r: 0.804, g: 0.498, b: 0.196, a: 1.0 };
const MORPH_BLUE: Color = Color { r: 0.376, g: 0.647, b: 0.980, a: 1.0 };
const MORPH_PURPLE: Color = Color { r: 0.545, g: 0.361, b: 0.965, a: 1.0 };
const MORPH_PINK: Color = Color { r: 0.925, g: 0.282, b: 0.600, a: 1.0 };

// ── BubbleProgram ──────────────────────────────────────────────────────

/// Canvas program that draws an animated AI bubble.
pub struct BubbleProgram {
    pub style: BubbleStyle,
    pub state_color: Color,
    pub elapsed: f32,
}

impl<Message> canvas::Program<Message> for BubbleProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let r = bounds.width.min(bounds.height) / 2.0;
        let center = Point::new(r, r);

        // Background circle
        let bg = Path::circle(center, r - 1.0);
        frame.fill(&bg, Color { r: 0.1, g: 0.1, b: 0.15, a: 0.85 });

        match self.style {
            BubbleStyle::Icon => draw_icon(&mut frame, center, r),
            BubbleStyle::Wave => draw_wave(&mut frame, center, r, self.elapsed),
            BubbleStyle::Spin => draw_spin(&mut frame, center, r, self.elapsed),
            BubbleStyle::Pulse => draw_pulse(&mut frame, center, r, self.elapsed),
            BubbleStyle::Blink => draw_blink(&mut frame, center, r, self.elapsed),
            BubbleStyle::Rings => draw_rings(&mut frame, center, r, self.elapsed),
            BubbleStyle::Matrix => draw_matrix(&mut frame, center, r, self.elapsed),
            BubbleStyle::Orbit => draw_orbit(&mut frame, center, r, self.elapsed),
            BubbleStyle::Morph => draw_morph(&mut frame, center, r, self.elapsed),
        }

        // State border ring
        let ring = Path::circle(center, r - 0.5);
        frame.stroke(
            &ring,
            Stroke {
                style: stroke::Style::Solid(self.state_color),
                width: 2.0,
                ..Stroke::default()
            },
        );

        vec![frame.into_geometry()]
    }
}

// ── Drawing functions ──────────────────────────────────────────────────

/// Icon: static gold circle with a crown shape.
fn draw_icon(frame: &mut Frame, center: Point, r: f32) {
    let inner = Path::circle(center, r * 0.55);
    frame.fill(&inner, ICON_GOLD);

    // 3-peak crown
    let cr = r * 0.3;
    let cx = center.x;
    let cy = center.y;
    let crown = Path::new(|b| {
        b.move_to(Point::new(cx - cr, cy + cr * 0.3));
        b.line_to(Point::new(cx - cr * 0.7, cy - cr * 0.5));
        b.line_to(Point::new(cx - cr * 0.3, cy - cr * 0.1));
        b.line_to(Point::new(cx, cy - cr * 0.7));
        b.line_to(Point::new(cx + cr * 0.3, cy - cr * 0.1));
        b.line_to(Point::new(cx + cr * 0.7, cy - cr * 0.5));
        b.line_to(Point::new(cx + cr, cy + cr * 0.3));
        b.close();
    });
    frame.fill(&crown, ICON_BRONZE);
}

/// Wave: 3 animated sine waves with a pulsing center dot.
fn draw_wave(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let waves: [(Color, f32, f32, f32); 3] = [
        (WAVE_BLUE, 0.6, 2.0, 3.0),
        (WAVE_BLUE_DARK, 0.4, 2.5, 2.5),
        (WAVE_BLUE, 0.3, 3.0, 3.5),
    ];

    for (i, &(base_color, alpha, freq, speed)) in waves.iter().enumerate() {
        let color = Color { a: alpha, ..base_color };
        let amp = r * 0.15 * (1.0 - i as f32 * 0.2);
        let phase = t * speed + i as f32 * 0.8;

        let path = Path::new(|b| {
            let steps = 40;
            for s in 0..=steps {
                let frac = s as f32 / steps as f32;
                let x = center.x - r * 0.7 + frac * r * 1.4;
                let y = center.y + amp * (frac * freq * PI + phase).sin();
                if s == 0 {
                    b.move_to(Point::new(x, y));
                } else {
                    b.line_to(Point::new(x, y));
                }
            }
        });
        frame.stroke(
            &path,
            Stroke {
                style: stroke::Style::Solid(color),
                width: 1.5,
                ..Stroke::default()
            },
        );
    }

    // Center dot
    let dot_alpha = 0.5 + 0.5 * (t * 4.2).sin();
    let dot = Path::circle(center, r * 0.06);
    frame.fill(&dot, Color { a: dot_alpha, ..WAVE_BLUE });
}

/// Spin: 16 rotating petals at radial positions.
fn draw_spin(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let count = 16;
    for i in 0..count {
        let base_angle = i as f32 * (2.0 * PI / count as f32);
        let speed = 0.3 + i as f32 * 0.05;
        let angle = base_angle + t * speed;
        let dist = r * 0.45;
        let px = center.x + dist * angle.cos();
        let py = center.y + dist * angle.sin();
        let petal_r = r * 0.08;
        let alpha = 0.3 + 0.4 * ((i as f32 / count as f32) * PI).sin();

        let petal = Path::circle(Point::new(px, py), petal_r);
        frame.fill(&petal, Color { a: alpha, ..SPIN_PURPLE });
    }
}

/// Pulse: concentric pulsing rings.
fn draw_pulse(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let rings: [(f32, f32, f32, Color); 4] = [
        (0.35, 1.8, 0.0, PULSE_PINK),
        (0.50, 1.5, 0.3, PULSE_PINK_LIGHT),
        (0.62, 1.2, 0.7, PULSE_PINK),
        (0.72, 1.0, 1.0, PULSE_PINK_LIGHT),
    ];

    for &(base_frac, freq, offset, base_color) in &rings {
        let pulse = 0.5 + 0.5 * (t * freq + offset).sin();
        let ring_r = r * (base_frac + 0.05 * pulse);
        let alpha = 0.2 + 0.4 * pulse;
        let ring = Path::circle(center, ring_r);
        frame.stroke(
            &ring,
            Stroke {
                style: stroke::Style::Solid(Color { a: alpha, ..base_color }),
                width: 1.5 + pulse,
                ..Stroke::default()
            },
        );
    }
}

/// Blink: 8 blinking dots at octagonal positions.
fn draw_blink(frame: &mut Frame, center: Point, r: f32, t: f32) {
    for i in 0..8 {
        let angle = i as f32 * (2.0 * PI / 8.0) - PI / 2.0;
        let dist = r * 0.6;
        let px = center.x + dist * angle.cos();
        let py = center.y + dist * angle.sin();

        let freq = 2.0 + i as f32 * 0.1;
        let offset = i as f32 * 0.3;
        let alpha = 0.2 + 0.6 * (0.5 + 0.5 * (t * freq + offset).sin());
        let dot_r = r * 0.07;

        let color = if i % 2 == 0 { BLINK_GOLD } else { BLINK_ORANGE };
        let dot = Path::circle(Point::new(px, py), dot_r);
        frame.fill(&dot, Color { a: alpha, ..color });
    }
}

/// Rings: 3 rotating elliptical rings.
fn draw_rings(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let ring_params: [(f32, f32, f32, Color); 3] = [
        (0.7, 0.3, 0.5, RINGS_GREEN),
        (0.65, 0.35, -0.35, RINGS_GREEN_LIGHT),
        (0.6, 0.4, 0.7, RINGS_GREEN),
    ];

    for &(rx_frac, ry_frac, speed, base_color) in &ring_params {
        let angle = t * speed;
        let steps = 48;
        let rx = r * rx_frac;
        let ry = r * ry_frac;

        let path = Path::new(|b| {
            for s in 0..=steps {
                let theta = s as f32 * (2.0 * PI / steps as f32);
                let ex = rx * theta.cos();
                let ey = ry * theta.sin();
                let x = center.x + ex * angle.cos() - ey * angle.sin();
                let y = center.y + ex * angle.sin() + ey * angle.cos();
                if s == 0 {
                    b.move_to(Point::new(x, y));
                } else {
                    b.line_to(Point::new(x, y));
                }
            }
        });
        frame.stroke(
            &path,
            Stroke {
                style: stroke::Style::Solid(Color { a: 0.6, ..base_color }),
                width: 1.2,
                ..Stroke::default()
            },
        );
    }
}

/// Matrix: scrolling binary rain (green dots in columns).
fn draw_matrix(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let cols = 9;
    let col_spacing = (r * 1.4) / cols as f32;
    let rows_per_col = 12;
    let row_height = r * 0.12;

    for col in 0..cols {
        let x = center.x - r * 0.7 + (col as f32 + 0.5) * col_spacing;
        let speed = 0.6 + col as f32 * 0.15;
        let y_offset = (t * speed * r * 0.3) % (rows_per_col as f32 * row_height);

        for row in 0..rows_per_col {
            let y = center.y - r * 0.6 + row as f32 * row_height + y_offset;
            let dx = x - center.x;
            let dy = y - center.y;
            if dx * dx + dy * dy < (r * 0.85) * (r * 0.85) {
                let hash = ((col * 7 + row * 13) as f32 * 0.37).fract();
                let alpha = 0.2 + 0.6 * hash;
                let color = if hash > 0.5 { RINGS_GREEN } else { RINGS_GREEN_LIGHT };
                let dot = Path::circle(Point::new(x, y), r * 0.06);
                frame.fill(&dot, Color { a: alpha, ..color });
            }
        }
    }
}

/// Orbit: particles orbiting at 3 radii.
fn draw_orbit(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let tiers: [(f32, i32, f32, Color); 3] = [
        (0.25, 3, 1.2, BLINK_GOLD),
        (0.45, 3, 0.8, BLINK_ORANGE),
        (0.65, 3, 0.5, ORBIT_DARK),
    ];

    for &(radius_frac, count, speed, base_color) in &tiers {
        let orbit_r = r * radius_frac;
        for j in 0..count {
            let phase = j as f32 * (2.0 * PI / count as f32);
            let angle = phase + t * speed;
            let px = center.x + orbit_r * angle.cos();
            let py = center.y + orbit_r * angle.sin();
            let dot_r = r * (0.04 + 0.02 * (t * 2.0 + phase).sin().abs());
            let alpha = 0.5 + 0.3 * (t * 1.5 + phase).sin();

            let dot = Path::circle(Point::new(px, py), dot_r);
            frame.fill(&dot, Color { a: alpha, ..base_color });
        }
    }
}

/// Morph: shape-morphing bezier blob with color cycling.
fn draw_morph(frame: &mut Frame, center: Point, r: f32, t: f32) {
    let blend = 0.5 + 0.5 * (t * 0.6).sin();
    let cr = r * 0.55;

    let path = Path::new(|b| {
        let points = 8;
        for i in 0..points {
            let base_angle = i as f32 * (2.0 * PI / points as f32);
            let wobble = 1.0
                + 0.25 * (base_angle * 2.0 + t * 0.8).sin() * blend
                + 0.15 * (base_angle * 3.0 - t * 1.2).cos() * (1.0 - blend);
            let pr = cr * wobble;
            let x = center.x + pr * base_angle.cos();
            let y = center.y + pr * base_angle.sin();
            if i == 0 {
                b.move_to(Point::new(x, y));
            } else {
                b.line_to(Point::new(x, y));
            }
        }
        b.close();
    });

    // Color cycles through 3 hues
    let hue_t = (t * 0.4).sin();
    let color = if hue_t > 0.3 {
        MORPH_BLUE
    } else if hue_t > -0.3 {
        MORPH_PURPLE
    } else {
        MORPH_PINK
    };
    frame.fill(&path, Color { a: 0.5, ..color });
    frame.stroke(
        &path,
        Stroke {
            style: stroke::Style::Solid(Color { a: 0.7, ..color }),
            width: 1.5,
            ..Stroke::default()
        },
    );

    // Outer ring
    let outer = Path::circle(center, r * 0.75);
    frame.stroke(
        &outer,
        Stroke {
            style: stroke::Style::Solid(Color { a: 0.3, ..color }),
            width: 1.0,
            ..Stroke::default()
        },
    );
}
