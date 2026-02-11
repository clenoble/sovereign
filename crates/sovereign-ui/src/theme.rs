use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

// ── Color palette ────────────────────────────────────────────────────────────

pub const BG_WINDOW: Color = Color::from_rgb(0.055, 0.055, 0.063); // #0e0e10
pub const BG_PANEL: Color = Color::from_rgb(0.102, 0.102, 0.125); // #1a1a20
pub const BG_TASKBAR: Color = Color::from_rgb(0.078, 0.078, 0.094); // #141418
pub const BG_SKILL_PANEL: Color = Color::from_rgb(0.118, 0.118, 0.141); // #1e1e24
pub const BG_BUTTON: Color = Color::from_rgb(0.165, 0.165, 0.196); // #2a2a32
pub const BG_BUTTON_HOVER: Color = Color::from_rgb(0.227, 0.227, 0.259); // #3a3a42

pub const BORDER_DIM: Color = Color::from_rgb(0.165, 0.165, 0.188); // #2a2a30
pub const BORDER_ACCENT: Color = Color::from_rgb(0.353, 0.624, 0.831); // #5a9fd4

pub const TEXT_PRIMARY: Color = Color::from_rgb(0.878, 0.878, 0.878); // #e0e0e0
pub const TEXT_DIM: Color = Color::from_rgb(0.400, 0.400, 0.400); // #666
pub const TEXT_LABEL: Color = Color::from_rgb(0.816, 0.816, 0.816); // #d0d0d0

pub const BUBBLE_IDLE: Color = Color::from_rgb(0.227, 0.227, 0.957); // #3a3af4
pub const BUBBLE_PROCESSING_OWNED: Color = Color::from_rgb(0.165, 0.353, 0.957); // #2a5af4
pub const BUBBLE_PROCESSING_EXT: Color = Color::from_rgb(0.831, 0.627, 0.353); // #d4a05a
pub const BUBBLE_EXECUTING: Color = Color::from_rgb(0.227, 0.831, 0.478); // #3ad47a
pub const BUBBLE_SUGGESTING: Color = Color::from_rgb(0.420, 0.353, 0.831); // #6b5ad4

pub const APPROVE_GREEN: Color = Color::from_rgb(0.227, 0.604, 0.290); // #3a9a4a
pub const REJECT_RED: Color = Color::from_rgb(0.604, 0.227, 0.227); // #9a3a3a
pub const OWNED_BLUE: Color = Color::from_rgb(0.353, 0.624, 0.831); // #5a9fd4
pub const EXTERNAL_ORANGE: Color = Color::from_rgb(0.878, 0.486, 0.416); // #e07c6a

// ── Container styles ─────────────────────────────────────────────────────────

pub fn dark_background(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_WINDOW)),
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn taskbar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_TASKBAR)),
        border: Border {
            color: BORDER_DIM,
            width: 1.0,
            ..Default::default()
        },
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn search_overlay_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.055, 0.055, 0.063, 0.95))),
        border: Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn skill_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SKILL_PANEL)),
        border: Border {
            color: BORDER_DIM,
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn confirmation_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SKILL_PANEL)),
        border: Border {
            color: BUBBLE_PROCESSING_EXT,
            width: 1.0,
            radius: 12.0.into(),
        },
        text_color: Some(TEXT_PRIMARY),
        ..Default::default()
    }
}

pub fn suggestion_tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SKILL_PANEL)),
        border: Border {
            color: BUBBLE_SUGGESTING,
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(TEXT_LABEL),
        ..Default::default()
    }
}

pub fn document_panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_PANEL)),
        border: Border {
            color: BORDER_DIM,
            width: 1.0,
            radius: 8.0.into(),
        },
        text_color: Some(TEXT_PRIMARY),
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
        button::Status::Hovered => BG_BUTTON_HOVER,
        _ => BG_BUTTON,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT_PRIMARY,
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
        _ => APPROVE_GREEN,
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
        _ => REJECT_RED,
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
        button::Status::Hovered => Color::from_rgb(0.133, 0.133, 0.157),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT_LABEL,
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

// ── Text input styles ────────────────────────────────────────────────────────

pub fn search_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(BG_PANEL),
        border: Border {
            color: BORDER_DIM,
            width: 1.0,
            radius: 8.0.into(),
        },
        icon: TEXT_DIM,
        placeholder: TEXT_DIM,
        value: TEXT_PRIMARY,
        selection: BORDER_ACCENT,
    }
}
