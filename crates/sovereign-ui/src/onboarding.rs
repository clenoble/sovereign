use iced::widget::{button, canvas, column, container, row, text, text_input, Id, Space};
use iced::{Element, Length, Padding, Task};

use sovereign_core::profile::{self, BubbleStyle};
use sovereign_crypto::auth::PasswordPolicy;

use crate::app::Message;
use crate::bubble_canvas::BubbleProgram;
use crate::login::KeystrokeSampleUi;
use crate::theme;

// Text-input field IDs for focus management (Tab / Enter navigation).
const ID_NICKNAME: &str = "ob_nickname";
const ID_PW: &str = "ob_pw";
const ID_PW_CONFIRM: &str = "ob_pw_confirm";
const ID_DURESS: &str = "ob_duress";
const ID_DURESS_CONFIRM: &str = "ob_duress_confirm";
const ID_CANARY: &str = "ob_canary";
const ID_CANARY_CONFIRM: &str = "ob_canary_confirm";
const ID_ENROLL: &str = "ob_enroll";

/// Returns a [`Task`] that focuses the first input field of the given step.
pub fn focus_first_input(step: OnboardingStep) -> Task<Message> {
    let id = match step {
        OnboardingStep::Nickname => Some(ID_NICKNAME),
        OnboardingStep::SetPassword => Some(ID_PW),
        OnboardingStep::SetDuressPassword => Some(ID_DURESS),
        OnboardingStep::SetCanaryPhrase => Some(ID_CANARY),
        OnboardingStep::EnrollKeystrokes => Some(ID_ENROLL),
        _ => None,
    };
    match id {
        Some(name) => iced::widget::operation::focus(Id::new(name)),
        None => Task::none(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    Nickname,
    BubbleSelect,
    ThemeSelect,
    SampleData,
    // Auth setup
    SetPassword,
    SetDuressPassword,
    SetCanaryPhrase,
    EnrollKeystrokes,
    Complete,
}

impl OnboardingStep {
    fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Nickname => 1,
            Self::BubbleSelect => 2,
            Self::ThemeSelect => 3,
            Self::SampleData => 4,
            Self::SetPassword => 5,
            Self::SetDuressPassword => 6,
            Self::SetCanaryPhrase => 7,
            Self::EnrollKeystrokes => 8,
            Self::Complete => 9,
        }
    }

    fn total() -> usize {
        9 // Welcome through EnrollKeystrokes
    }
}

/// State for the first-launch onboarding wizard.
pub struct OnboardingState {
    pub step: OnboardingStep,
    pub designation: String,
    pub nickname: String,
    pub bubble_style: BubbleStyle,
    pub elapsed: f32,
    pub seed_sample_data: bool,
    // Auth setup fields
    pub primary_password: String,
    pub primary_confirm: String,
    pub primary_validation_errors: Vec<String>,
    pub duress_password: String,
    pub duress_confirm: String,
    pub duress_validation_errors: Vec<String>,
    pub canary_phrase: String,
    pub canary_confirm: String,
    // Keystroke enrollment
    pub enrollment_samples: Vec<Vec<KeystrokeSampleUi>>,
    pub enrollment_target: usize,
    pub current_enrollment_input: String,
    pub current_enrollment_keystrokes: Vec<KeystrokeSampleUi>,
    pub enrollment_error: Option<String>,
    policy: PasswordPolicy,
}

impl OnboardingState {
    pub fn new() -> Self {
        Self {
            step: OnboardingStep::Welcome,
            designation: profile::generate_designation(),
            nickname: String::new(),
            bubble_style: BubbleStyle::default(),
            elapsed: 0.0,
            seed_sample_data: true,
            primary_password: String::new(),
            primary_confirm: String::new(),
            primary_validation_errors: Vec::new(),
            duress_password: String::new(),
            duress_confirm: String::new(),
            duress_validation_errors: Vec::new(),
            canary_phrase: String::new(),
            canary_confirm: String::new(),
            enrollment_samples: Vec::new(),
            enrollment_target: 5,
            current_enrollment_input: String::new(),
            current_enrollment_keystrokes: Vec::new(),
            enrollment_error: None,
            policy: PasswordPolicy::default_policy(),
        }
    }

    pub fn next_step(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::Nickname,
            OnboardingStep::Nickname => OnboardingStep::BubbleSelect,
            OnboardingStep::BubbleSelect => OnboardingStep::ThemeSelect,
            OnboardingStep::ThemeSelect => OnboardingStep::SampleData,
            OnboardingStep::SampleData => OnboardingStep::SetPassword,
            OnboardingStep::SetPassword => OnboardingStep::SetDuressPassword,
            OnboardingStep::SetDuressPassword => OnboardingStep::SetCanaryPhrase,
            OnboardingStep::SetCanaryPhrase => OnboardingStep::EnrollKeystrokes,
            OnboardingStep::EnrollKeystrokes => OnboardingStep::Complete,
            OnboardingStep::Complete => OnboardingStep::Complete,
        };
    }

    pub fn prev_step(&mut self) {
        self.step = match self.step {
            OnboardingStep::Welcome => OnboardingStep::Welcome,
            OnboardingStep::Nickname => OnboardingStep::Welcome,
            OnboardingStep::BubbleSelect => OnboardingStep::Nickname,
            OnboardingStep::ThemeSelect => OnboardingStep::BubbleSelect,
            OnboardingStep::SampleData => OnboardingStep::ThemeSelect,
            OnboardingStep::SetPassword => OnboardingStep::SampleData,
            OnboardingStep::SetDuressPassword => OnboardingStep::SetPassword,
            OnboardingStep::SetCanaryPhrase => OnboardingStep::SetDuressPassword,
            OnboardingStep::EnrollKeystrokes => OnboardingStep::SetCanaryPhrase,
            OnboardingStep::Complete => OnboardingStep::EnrollKeystrokes,
        };
    }

    /// Whether the "Next" button should be enabled for the current step.
    pub fn can_advance(&self) -> bool {
        match self.step {
            OnboardingStep::SetPassword => {
                let v = self.policy.validate(&self.primary_password);
                v.valid && self.primary_password == self.primary_confirm
            }
            OnboardingStep::SetDuressPassword => {
                let v = self.policy.validate(&self.duress_password);
                v.valid
                    && self.duress_password == self.duress_confirm
                    && self.duress_password != self.primary_password
            }
            OnboardingStep::SetCanaryPhrase => {
                self.canary_phrase.len() >= 4 && self.canary_phrase == self.canary_confirm
            }
            OnboardingStep::EnrollKeystrokes => {
                self.enrollment_samples.len() >= self.enrollment_target
            }
            _ => true,
        }
    }

    /// Validate primary password and update error list.
    pub fn validate_primary(&mut self) {
        let v = self.policy.validate(&self.primary_password);
        self.primary_validation_errors = v.errors;
        if !self.primary_confirm.is_empty() && self.primary_password != self.primary_confirm {
            self.primary_validation_errors
                .push("Passwords do not match".into());
        }
    }

    /// Validate duress password and update error list.
    pub fn validate_duress(&mut self) {
        let v = self.policy.validate(&self.duress_password);
        self.duress_validation_errors = v.errors;
        if !self.duress_confirm.is_empty() && self.duress_password != self.duress_confirm {
            self.duress_validation_errors
                .push("Passwords do not match".into());
        }
        if !self.duress_password.is_empty() && self.duress_password == self.primary_password {
            self.duress_validation_errors
                .push("Must differ from your main password".into());
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let step_body: Element<'_, Message> = match self.step {
            OnboardingStep::Welcome => self.view_welcome(),
            OnboardingStep::Nickname => self.view_nickname(),
            OnboardingStep::BubbleSelect => self.view_bubble_select(),
            OnboardingStep::ThemeSelect => self.view_theme_select(),
            OnboardingStep::SampleData => self.view_sample_data(),
            OnboardingStep::SetPassword => self.view_set_password(),
            OnboardingStep::SetDuressPassword => self.view_set_duress_password(),
            OnboardingStep::SetCanaryPhrase => self.view_set_canary_phrase(),
            OnboardingStep::EnrollKeystrokes => self.view_enroll_keystrokes(),
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

        let is_last = self.step == OnboardingStep::EnrollKeystrokes;
        let can_go = self.can_advance();

        let next_label = if is_last { "Get Started" } else { "Next" };
        let next_btn = if can_go {
            button(text(next_label).size(14))
                .on_press(if is_last {
                    Message::OnboardingComplete
                } else {
                    Message::OnboardingNext
                })
                .style(theme::approve_button_style)
                .padding(Padding::from([10, 24]))
        } else {
            button(text(next_label).size(14))
                .style(theme::skill_button_style)
                .padding(Padding::from([10, 24]))
        };
        nav = nav.push(next_btn);

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

        container(container(content).style(theme::skill_panel_style))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(theme::dark_background)
            .into()
    }

    // ── Existing step views ──────────────────────────────────────────

    fn view_welcome(&self) -> Element<'_, Message> {
        column![
            text("Welcome to Sovereign GE")
                .size(24)
                .color(theme::text_primary()),
            Space::new().height(12),
            text("Your personal graphic environment for documents, knowledge, and communication.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text("Your AI orchestrator has been assigned:")
                .size(14)
                .color(theme::text_dim()),
            Space::new().height(8),
            text(&self.designation)
                .size(22)
                .color(theme::border_accent()),
            Space::new().height(16),
            text("This is its unique designation — a serial identity for your personal AI assistant. In the next step, you can give it a shorter nickname.")
                .size(13)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
        ]
        .spacing(0)
        .into()
    }

    fn view_nickname(&self) -> Element<'_, Message> {
        column![
            text("What would you like to call me?")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text(format!("My full designation is {}, but that's a bit of a mouthful.", &self.designation))
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(4),
            text("Give me a short nickname — something easy to say.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text_input("e.g. Ike, T-Nine, B4...", &self.nickname)
                .id(Id::new(ID_NICKNAME))
                .on_input(Message::OnboardingNicknameChanged)
                .on_submit(Message::OnboardingTryAdvance)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
        ]
        .spacing(0)
        .into()
    }

    fn view_bubble_select(&self) -> Element<'_, Message> {
        let mut col = column![
            text("Choose my look")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text("Pick a visual style for my bubble avatar. You'll see it in the corner of your workspace.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
        ]
        .spacing(0);

        // 3×3 grid of animated bubble previews
        let styles = BubbleStyle::all();
        for chunk in styles.chunks(3) {
            let mut grid_row = row![].spacing(12);
            for &style in chunk {
                let is_selected = style == self.bubble_style;
                let border_color = if is_selected {
                    theme::border_accent()
                } else {
                    iced::Color { r: 0.3, g: 0.3, b: 0.3, a: 1.0 }
                };

                let bubble_preview = canvas(BubbleProgram {
                    style,
                    state_color: border_color,
                    elapsed: self.elapsed,
                })
                .width(Length::Fixed(80.0))
                .height(Length::Fixed(80.0));

                let label = text(style.label())
                    .size(11)
                    .color(if is_selected {
                        theme::border_accent()
                    } else {
                        theme::text_dim()
                    })
                    .align_x(iced::alignment::Horizontal::Center);

                let cell = column![bubble_preview, label]
                    .spacing(4)
                    .align_x(iced::alignment::Horizontal::Center);

                grid_row = grid_row.push(
                    iced::widget::mouse_area(cell)
                        .on_press(Message::OnboardingBubbleSelected(style)),
                );
            }
            col = col.push(grid_row);
            col = col.push(Space::new().height(8));
        }

        col.into()
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
            text("Choose your theme")
                .size(20)
                .color(theme::text_primary()),
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
            text("Sample data")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(12),
            text("Sovereign GE can create sample documents, threads, contacts, and conversations so you can explore the interface right away.")
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

    // ── Auth setup step views ────────────────────────────────────────

    fn view_set_password(&self) -> Element<'_, Message> {
        let mut col = column![
            text("Create your password")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text("This password protects all your data. Choose something strong.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text_input("Password", &self.primary_password)
                .id(Id::new(ID_PW))
                .on_input(Message::OnboardingPrimaryChanged)
                .on_submit(Message::OnboardingFocusField(ID_PW_CONFIRM))
                .secure(true)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
            Space::new().height(8),
            text_input("Confirm password", &self.primary_confirm)
                .id(Id::new(ID_PW_CONFIRM))
                .on_input(Message::OnboardingPrimaryConfirmChanged)
                .on_submit(Message::OnboardingTryAdvance)
                .secure(true)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
        ]
        .spacing(0);

        // Password strength bar
        col = col.push(Space::new().height(12));
        col = col.push(self.view_strength_bar(&self.primary_password));

        // Validation errors
        for err in &self.primary_validation_errors {
            col = col.push(Space::new().height(4));
            col = col.push(text(err.as_str()).size(12).color(theme::reject_red()));
        }

        col.into()
    }

    fn view_set_duress_password(&self) -> Element<'_, Message> {
        let mut col = column![
            text("Set a duress password")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text("If you're ever forced to unlock your device, enter this password instead. It opens a convincing decoy workspace with no real data.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(4),
            text("Choose something different from your main password.")
                .size(13)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text_input("Duress password", &self.duress_password)
                .id(Id::new(ID_DURESS))
                .on_input(Message::OnboardingDuressChanged)
                .on_submit(Message::OnboardingFocusField(ID_DURESS_CONFIRM))
                .secure(true)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
            Space::new().height(8),
            text_input("Confirm duress password", &self.duress_confirm)
                .id(Id::new(ID_DURESS_CONFIRM))
                .on_input(Message::OnboardingDuressConfirmChanged)
                .on_submit(Message::OnboardingTryAdvance)
                .secure(true)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
        ]
        .spacing(0);

        for err in &self.duress_validation_errors {
            col = col.push(Space::new().height(4));
            col = col.push(text(err.as_str()).size(12).color(theme::reject_red()));
        }

        col = col.push(Space::new().height(16));
        col = col.push(
            button(
                text("Skip, set up later")
                    .size(12)
                    .color(theme::text_dim()),
            )
            .on_press(Message::OnboardingSkipAuth)
            .style(|_theme: &iced::Theme, _status: button::Status| button::Style {
                background: None,
                text_color: theme::text_dim(),
                ..Default::default()
            })
            .padding(Padding::from([4, 0])),
        );

        col.into()
    }

    fn view_set_canary_phrase(&self) -> Element<'_, Message> {
        let match_ok =
            !self.canary_phrase.is_empty() && self.canary_phrase == self.canary_confirm;
        let too_short = !self.canary_phrase.is_empty() && self.canary_phrase.len() < 4;

        let mut col = column![
            text("Set a canary phrase")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text("If you type this exact phrase anywhere in the app, it will silently lock down: all keys are wiped from memory and the session ends.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(4),
            text("Choose something you'd never type by accident, but could work into a sentence naturally.")
                .size(13)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text_input("Canary phrase", &self.canary_phrase)
                .id(Id::new(ID_CANARY))
                .on_input(Message::OnboardingCanaryChanged)
                .on_submit(Message::OnboardingFocusField(ID_CANARY_CONFIRM))
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
            Space::new().height(8),
            text_input("Confirm canary phrase", &self.canary_confirm)
                .id(Id::new(ID_CANARY_CONFIRM))
                .on_input(Message::OnboardingCanaryConfirmChanged)
                .on_submit(Message::OnboardingTryAdvance)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
        ]
        .spacing(0);

        if too_short {
            col = col.push(Space::new().height(4));
            col = col.push(
                text("At least 4 characters")
                    .size(12)
                    .color(theme::reject_red()),
            );
        }
        if !self.canary_confirm.is_empty() && !match_ok && !too_short {
            col = col.push(Space::new().height(4));
            col = col.push(
                text("Phrases do not match")
                    .size(12)
                    .color(theme::reject_red()),
            );
        }

        col = col.push(Space::new().height(12));
        col = col.push(
            text("e.g. \"the weather in zurich\" — something mundane you could type in chat")
                .size(12)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
        );

        col = col.push(Space::new().height(16));
        col = col.push(
            button(
                text("Skip, set up later")
                    .size(12)
                    .color(theme::text_dim()),
            )
            .on_press(Message::OnboardingSkipAuth)
            .style(|_theme: &iced::Theme, _status: button::Status| button::Style {
                background: None,
                text_color: theme::text_dim(),
                ..Default::default()
            })
            .padding(Padding::from([4, 0])),
        );

        col.into()
    }

    fn view_enroll_keystrokes(&self) -> Element<'_, Message> {
        let done = self.enrollment_samples.len();
        let target = self.enrollment_target;

        let mut col = column![
            text("Learn your typing style")
                .size(20)
                .color(theme::text_primary()),
            Space::new().height(8),
            text("Type your password 5 times so we can recognize how you type. If someone else enters your password with a different rhythm, they'll be asked to verify.")
                .size(14)
                .color(theme::text_dim())
                .wrapping(text::Wrapping::Word),
            Space::new().height(16),
            text(format!("Attempt {} of {}", (done + 1).min(target), target))
                .size(14)
                .color(theme::border_accent()),
            Space::new().height(8),
            text_input("Type your password", &self.current_enrollment_input)
                .id(Id::new(ID_ENROLL))
                .on_input(Message::OnboardingEnrollInputChanged)
                .on_submit(Message::OnboardingEnrollSubmit)
                .secure(true)
                .style(theme::search_input_style)
                .padding(Padding::from([10, 14]))
                .size(15),
            Space::new().height(8),
        ]
        .spacing(0);

        let submit_enabled = !self.current_enrollment_input.is_empty() && done < target;
        let submit_btn = if submit_enabled {
            button(text("Submit").size(14))
                .on_press(Message::OnboardingEnrollSubmit)
                .style(theme::approve_button_style)
                .padding(Padding::from([10, 24]))
        } else {
            button(text("Submit").size(14))
                .style(theme::skill_button_style)
                .padding(Padding::from([10, 24]))
        };
        col = col.push(submit_btn);

        if let Some(ref err) = self.enrollment_error {
            col = col.push(Space::new().height(8));
            col = col.push(text(err.as_str()).size(12).color(theme::reject_red()));
        }

        if done >= target {
            col = col.push(Space::new().height(12));
            col = col.push(
                text("Enrollment complete! Press \"Get Started\" to finish.")
                    .size(13)
                    .color(theme::approve_green()),
            );
        }

        if done < target {
            col = col.push(Space::new().height(16));
            col = col.push(
                button(
                    text("Skip, set up later")
                        .size(12)
                        .color(theme::text_dim()),
                )
                .on_press(Message::OnboardingSkipAuth)
                .style(|_theme: &iced::Theme, _status: button::Status| button::Style {
                    background: None,
                    text_color: theme::text_dim(),
                    ..Default::default()
                })
                .padding(Padding::from([4, 0])),
            );
        }

        col.into()
    }

    // ── Helpers ──────────────────────────────────────────────────────

    fn view_strength_bar(&self, password: &str) -> Element<'_, Message> {
        let score = password_strength_score(password);
        let (label, color) = match score {
            0..=1 => ("Weak", theme::reject_red()),
            2..=3 => ("Fair", theme::external_orange()),
            _ => ("Strong", theme::approve_green()),
        };

        row![
            text(label).size(12).color(color),
            Space::new().width(8),
            text(format!("{}/5", score))
                .size(12)
                .color(theme::text_dim()),
        ]
        .spacing(0)
        .into()
    }
}

/// Simple password strength score (0-5).
fn password_strength_score(password: &str) -> u8 {
    let mut score = 0u8;
    if password.len() >= 12 {
        score += 1;
    }
    if password.chars().any(|c| c.is_uppercase()) {
        score += 1;
    }
    if password.chars().any(|c| c.is_lowercase()) {
        score += 1;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        score += 1;
    }
    if password.chars().any(|c| !c.is_alphanumeric() && !c.is_whitespace()) {
        score += 1;
    }
    score
}
