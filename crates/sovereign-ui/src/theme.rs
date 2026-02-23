use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

// ── Theme mode ──────────────────────────────────────────────────────────────

static IS_LIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

pub fn current_mode() -> ThemeMode {
    if IS_LIGHT.load(Ordering::Relaxed) {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    }
}

pub fn toggle_theme() {
    let prev = IS_LIGHT.load(Ordering::Relaxed);
    IS_LIGHT.store(!prev, Ordering::Relaxed);
}

pub fn iced_theme() -> Theme {
    match current_mode() {
        ThemeMode::Dark => Theme::Dark,
        ThemeMode::Light => Theme::Light,
    }
}

/// Pick between dark and light color values.
fn pick(dark: Color, light: Color) -> Color {
    match current_mode() {
        ThemeMode::Dark => dark,
        ThemeMode::Light => light,
    }
}

// ── Color palette (functions) ───────────────────────────────────────────────

pub fn bg_window() -> Color {
    pick(
        Color::from_rgb(0.055, 0.055, 0.063),  // #0e0e10
        Color::from_rgb(0.941, 0.941, 0.953),  // #f0f0f3
    )
}

pub fn bg_panel() -> Color {
    pick(
        Color::from_rgb(0.102, 0.102, 0.125),  // #1a1a20
        Color::from_rgb(1.0, 1.0, 1.0),        // #ffffff
    )
}

pub fn bg_taskbar() -> Color {
    pick(
        Color::from_rgb(0.078, 0.078, 0.094),  // #141418
        Color::from_rgb(0.894, 0.894, 0.910),  // #e4e4e8
    )
}

pub fn bg_skill_panel() -> Color {
    pick(
        Color::from_rgb(0.118, 0.118, 0.141),  // #1e1e24
        Color::from_rgb(0.961, 0.961, 0.973),  // #f5f5f8
    )
}

pub fn bg_button() -> Color {
    pick(
        Color::from_rgb(0.165, 0.165, 0.196),  // #2a2a32
        Color::from_rgb(0.847, 0.847, 0.878),  // #d8d8e0
    )
}

pub fn bg_button_hover() -> Color {
    pick(
        Color::from_rgb(0.227, 0.227, 0.259),  // #3a3a42
        Color::from_rgb(0.784, 0.784, 0.816),  // #c8c8d0
    )
}

pub fn border_dim() -> Color {
    pick(
        Color::from_rgb(0.165, 0.165, 0.188),  // #2a2a30
        Color::from_rgb(0.753, 0.753, 0.784),  // #c0c0c8
    )
}

pub fn border_accent() -> Color {
    pick(
        Color::from_rgb(0.353, 0.624, 0.831),  // #5a9fd4
        Color::from_rgb(0.165, 0.435, 0.706),  // #2a6fb4
    )
}

pub fn text_primary() -> Color {
    pick(
        Color::from_rgb(0.878, 0.878, 0.878),  // #e0e0e0
        Color::from_rgb(0.102, 0.102, 0.125),  // #1a1a20
    )
}

pub fn text_dim() -> Color {
    pick(
        Color::from_rgb(0.600, 0.600, 0.600),  // #999 — 6.9:1 on #0e0e10 (WCAG AA)
        Color::from_rgb(0.314, 0.314, 0.345),  // #505058 — 5.5:1 on #f0f0f3 (WCAG AA)
    )
}

pub fn text_label() -> Color {
    pick(
        Color::from_rgb(0.816, 0.816, 0.816),  // #d0d0d0
        Color::from_rgb(0.165, 0.165, 0.208),  // #2a2a35
    )
}

// Accent colors — same in both themes (used on colored backgrounds / as highlights)
pub fn bubble_idle() -> Color {
    Color::from_rgb(0.227, 0.227, 0.957) // #3a3af4
}
pub fn bubble_processing_owned() -> Color {
    Color::from_rgb(0.165, 0.353, 0.957) // #2a5af4
}
pub fn bubble_processing_ext() -> Color {
    Color::from_rgb(0.831, 0.627, 0.353) // #d4a05a
}
pub fn bubble_executing() -> Color {
    Color::from_rgb(0.227, 0.831, 0.478) // #3ad47a
}
pub fn bubble_suggesting() -> Color {
    Color::from_rgb(0.420, 0.353, 0.831) // #6b5ad4
}

pub fn approve_green() -> Color {
    Color::from_rgb(0.290, 0.729, 0.353) // #4aba5a — 5.8:1 on dark bg (WCAG AA)
}
pub fn reject_red() -> Color {
    Color::from_rgb(0.831, 0.314, 0.314) // #d45050 — 5.2:1 on dark bg (WCAG AA)
}
pub fn owned_blue() -> Color {
    pick(
        Color::from_rgb(0.353, 0.624, 0.831),  // #5a9fd4
        Color::from_rgb(0.165, 0.435, 0.706),  // #2a6fb4
    )
}
pub fn external_orange() -> Color {
    pick(
        Color::from_rgb(0.878, 0.486, 0.416),  // #e07c6a
        Color::from_rgb(0.753, 0.353, 0.251),  // #c05a40
    )
}

// ── Container styles ─────────────────────────────────────────────────────────

pub fn dark_background(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_window())),
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn taskbar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_taskbar())),
        border: Border {
            color: border_dim(),
            width: 1.0,
            ..Default::default()
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn search_overlay_style(_theme: &Theme) -> container::Style {
    let bg = match current_mode() {
        ThemeMode::Dark => Color::from_rgba(0.055, 0.055, 0.063, 0.95),
        ThemeMode::Light => Color::from_rgba(0.941, 0.941, 0.953, 0.95),
    };
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn skill_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_skill_panel())),
        border: Border {
            color: border_dim(),
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn chat_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_skill_panel())),
        border: Border {
            color: border_accent(),
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn confirmation_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_skill_panel())),
        border: Border {
            color: bubble_processing_ext(),
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn suggestion_tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_skill_panel())),
        border: Border {
            color: bubble_suggesting(),
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(text_label()),
        ..Default::default()
    }
}

pub fn document_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg_panel())),
        border: Border {
            color: border_dim(),
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(text_primary()),
        ..Default::default()
    }
}

pub fn bubble_style(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: 28.0.into(),
            ..Default::default()
        },
        text_color: Some(Color::WHITE),
        ..Default::default()
    }
}

// ── Button styles ────────────────────────────────────────────────────────────

pub fn skill_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => bg_button_hover(),
        _ => bg_button(),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: text_primary(),
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn approve_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.290, 0.667, 0.353),
        _ => approve_green(),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn reject_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.667, 0.290, 0.290),
        _ => reject_red(),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn taskbar_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => pick(
            Color::from_rgb(0.133, 0.133, 0.157),
            Color::from_rgb(0.816, 0.816, 0.847),
        ),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: text_label(),
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ── Text input styles ────────────────────────────────────────────────────────

pub fn search_input_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let (border_color, border_width) = match status {
        text_input::Status::Focused { .. } => (border_accent(), 2.0),
        text_input::Status::Hovered => (border_accent(), 1.0),
        _ => (border_dim(), 1.0),
    };
    text_input::Style {
        background: Background::Color(bg_panel()),
        border: Border {
            color: border_color,
            width: border_width,
            radius: 8.0.into(),
        },
        icon: text_dim(),
        placeholder: text_dim(),
        value: text_primary(),
        selection: border_accent(),
    }
}
