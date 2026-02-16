use iced::widget::{button, column, container, row, text, text_input, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    DeviceName,
    ThemeSelect,
    SampleData,
    Complete,
}

impl OnboardingStep {
    fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::DeviceName => 1,
            Self::ThemeSelect => 2,
            Self::SampleData => 3,
            Self::Complete => 4,
        }
    }

    fn total() -> usize {
        4 // Welcome through SampleData (Complete is the finish action)
    }
}

/// State for the first-launch onboarding wizard.
pub struct OnboardingState {
    pub step: OnboardingStep,
    pub device_name: String,
    pub seed_sample_data: bool,
}

impl OnboardingState {
    pub fn new() -> Self {
        Self {
            step: OnboardingStep::Welcome,
            device_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "My Device".into()),
            seed_sample_data: true,
        }
    }

    pub fn next_step(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::DeviceName,
            OnboardingStep::DeviceName => OnboardingStep::ThemeSelect,
            OnboardingStep::ThemeSelect => OnboardingStep::SampleData,
            OnboardingStep::SampleData => OnboardingStep::Complete,
            OnboardingStep::Complete => OnboardingStep::Complete,
        };
    }

    pub fn prev_step(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::Welcome,
            OnboardingStep::DeviceName => OnboardingStep::Welcome,
            OnboardingStep::ThemeSelect => OnboardingStep::DeviceName,
            OnboardingStep::SampleData => OnboardingStep::ThemeSelect,
            OnboardingStep::Complete => OnboardingStep::SampleData,
        };
    }

    pub fn view(&self) -> Element<'_, Message> {
        let step_body: Element<'_, Message> = match self.step {
            OnboardingStep::Welcome => self.view_welcome(),
            OnboardingStep::DeviceName => self.view_device_name(),
            OnboardingStep::ThemeSelect => self.view_theme_select(),
            OnboardingStep::SampleData => self.view_sample_data(),
            OnboardingStep::Complete => self.view_welcome(), // unreachable
        };

        // Progress indicator
        let step_idx = self.step.index();
        let total = OnboardingStep::total();
        let progress = text(format!("Step {} of {}", step_idx + 1, total))
            .size(12)
            .color(theme::text_dim());

        // Navigation buttons
        let mut nav = row![].spacing(12);

        if step_idx > 0 {
            nav = nav.push(
                button(text("Back").size(14))
                    .on_press(Message::OnboardingBack)
                    .style(theme::skill_button_style)
                    .padding(Padding::from([10, 24])),
            );
        }

        nav = nav.push(Space::new().width(Length::Fill));

        if self.step == OnboardingStep::SampleData {
            nav = nav.push(
                button(text("Get Started").size(14))
                    .on_press(Message::OnboardingComplete)
                    .style(theme::approve_button_style)
                    .padding(Padding::from([10, 24])),
            );
        } else {
            nav = nav.push(
                button(text("Next").size(14))
                    .on_press(Message::OnboardingNext)
                    .style(theme::approve_button_style)
                    .padding(Padding::from([10, 24])),
            );
        }

        let content = column![
            progress,
            Space::new().height(16),
            step_body,
            Space::new().height(24),
            nav,
        ]
        .spacing(0)
        .padding(40)
        .width(500);

        container(
            container(content).style(theme::skill_panel_style),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::dark_background)
        .into()
    }

    fn view_welcome(&self) -> Element<'_, Message> {
        column![
            text("Welcome to Sovereign OS").size(24).color(theme::text_primary()),
            Space::new().height(12),
            text("Your personal operating system for documents, knowledge, and communication.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(8),
            text("This wizard will help you set up your workspace in a few quick steps.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
        ]
        .spacing(0)
        .into()
    }

    fn view_device_name(&self) -> Element<'_, Message> {
        column![
            text("Name this device").size(20).color(theme::text_primary()),
            Space::new().height(12),
            text("Choose a name to identify this device when syncing with others.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text_input("Device name", &self.device_name)
                .on_input(Message::OnboardingDeviceNameChanged)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
        ]
        .spacing(0)
        .into()
    }

    fn view_theme_select(&self) -> Element<'_, Message> {
        let current = theme::current_mode();
        let is_light = current == theme::ThemeMode::Light;

        let dark_btn = button(
            text("Dark").size(14).color(if !is_light {
                theme::border_accent()
            } else {
                theme::text_dim()
            }),
        )
        .on_press(if is_light {
            Message::OnboardingThemeToggled
        } else {
            Message::Ignore
        })
        .style(theme::skill_button_style)
        .padding(Padding::from([12, 32]));

        let light_btn = button(
            text("Light").size(14).color(if is_light {
                theme::border_accent()
            } else {
                theme::text_dim()
            }),
        )
        .on_press(if !is_light {
            Message::OnboardingThemeToggled
        } else {
            Message::Ignore
        })
        .style(theme::skill_button_style)
        .padding(Padding::from([12, 32]));

        column![
            text("Choose your theme").size(20).color(theme::text_primary()),
            Space::new().height(12),
            text("You can change this anytime from the taskbar.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            row![dark_btn, light_btn].spacing(12),
        ]
        .spacing(0)
        .into()
    }

    fn view_sample_data(&self) -> Element<'_, Message> {
        let toggle_label = if self.seed_sample_data {
            "Sample data: ON"
        } else {
            "Sample data: OFF"
        };
        let toggle_btn = button(text(toggle_label).size(14))
            .on_press(Message::OnboardingSeedToggled)
            .style(if self.seed_sample_data {
                theme::approve_button_style
            } else {
                theme::skill_button_style
            })
            .padding(Padding::from([10, 20]));

        column![
            text("Sample data").size(20).color(theme::text_primary()),
            Space::new().height(12),
            text("Sovereign OS can create sample documents, threads, contacts, and conversations so you can explore the interface right away.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(8),
            text("You can delete these later — they're just to get you started.")
                .size(13)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            toggle_btn,
        ]
        .spacing(0)
        .into()
    }
}
