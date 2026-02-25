use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// State for the login screen (full-screen overlay).
pub struct LoginState {
    pub password_input: String,
    pub error_message: Option<String>,
    pub attempts: u32,
    pub max_attempts: u32,
    pub locked_until: Option<u64>,
    /// Keystroke timing data captured during this login attempt.
    pub keystrokes: Vec<KeystrokeSampleUi>,
}

/// UI-side keystroke sample (converted to crypto types on submission).
#[derive(Debug, Clone)]
pub struct KeystrokeSampleUi {
    pub key: String,
    pub press_ms: u64,
    pub release_ms: u64,
}

impl LoginState {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            password_input: String::new(),
            error_message: None,
            attempts: 0,
            max_attempts,
            locked_until: None,
            keystrokes: Vec::new(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // Check lockout
        if let Some(until) = self.locked_until {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now < until {
                let remaining = until - now;
                return self.view_locked(remaining);
            }
        }

        let title = text("Sovereign OS")
            .size(28)
            .color(theme::text_primary());

        let subtitle = text("Enter your password to unlock")
            .size(14)
            .color(theme::text_dim());

        let password_field = text_input("Password", &self.password_input)
            .on_input(Message::LoginPasswordChanged)
            .on_submit(Message::LoginSubmit)
            .secure(true)
            .style(theme::search_input_style)
            .padding(Padding::from([12, 14]))
            .size(15);

        let submit_enabled = !self.password_input.is_empty();
        let submit_btn = if submit_enabled {
            button(text("Unlock").size(14))
                .on_press(Message::LoginSubmit)
                .style(theme::approve_button_style)
                .padding(Padding::from([10, 32]))
        } else {
            button(text("Unlock").size(14))
                .style(theme::skill_button_style)
                .padding(Padding::from([10, 32]))
        };

        let mut content = column![
            title,
            Space::new().height(4),
            subtitle,
            Space::new().height(24),
            password_field,
            Space::new().height(16),
            row![Space::new().width(Length::Fill), submit_btn].spacing(0),
        ]
        .spacing(0)
        .padding(40)
        .width(420);

        if let Some(ref err) = self.error_message {
            content = content.push(Space::new().height(12));
            content = content.push(
                text(err.as_str())
                    .size(13)
                    .color(theme::reject_red()),
            );
        }

        if self.attempts > 0 {
            let remaining = self.max_attempts.saturating_sub(self.attempts);
            content = content.push(Space::new().height(4));
            content = content.push(
                text(format!("{} attempts remaining", remaining))
                    .size(12)
                    .color(theme::text_dim()),
            );
        }

        container(container(content).style(theme::skill_panel_style))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(theme::dark_background)
            .into()
    }

    fn view_locked(&self, remaining_secs: u64) -> Element<'_, Message> {
        let minutes = remaining_secs / 60;
        let seconds = remaining_secs % 60;

        let content = column![
            text("Account Locked")
                .size(24)
                .color(theme::reject_red()),
            Space::new().height(12),
            text("Too many failed attempts.")
                .size(14)
                .color(theme::text_dim()),
            Space::new().height(8),
            text(format!("Try again in {}:{:02}", minutes, seconds))
                .size(16)
                .color(theme::text_primary()),
        ]
        .spacing(0)
        .padding(40)
        .width(420);

        container(container(content).style(theme::skill_panel_style))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(theme::dark_background)
            .into()
    }
}
