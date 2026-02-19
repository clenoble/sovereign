use iced::widget::{button, column, container, row, text};
use iced::{Element, Padding};

use sovereign_core::content::{ContentFields, ContentImage};
use sovereign_core::security::BubbleVisualState;
use sovereign_skills::registry::SkillRegistry;
use sovereign_skills::traits::SkillDocument;

use crate::app::Message;
use crate::theme;

/// State for the orchestrator bubble.
pub struct BubbleState {
    pub position: iced::Point,
    pub visual_state: BubbleVisualState,
    pub skills_panel_visible: bool,
    pub confirmation: Option<String>,
    pub suggestion: Option<(String, String)>, // (text, action)
    pub skill_result: Option<String>,
    pub rejection_toast: Option<String>,
    pub dragging: bool,
    pub drag_start: Option<iced::Point>,
}

impl BubbleState {
    pub fn new() -> Self {
        Self {
            position: iced::Point::new(20.0, 20.0),
            visual_state: BubbleVisualState::Idle,
            skills_panel_visible: false,
            confirmation: None,
            suggestion: None,
            skill_result: None,
            rejection_toast: None,
            dragging: false,
            drag_start: None,
        }
    }

    pub fn bubble_color(&self) -> iced::Color {
        match self.visual_state {
            BubbleVisualState::Idle => theme::bubble_idle(),
            BubbleVisualState::ProcessingOwned => theme::bubble_processing_owned(),
            BubbleVisualState::ProcessingExternal => theme::bubble_processing_ext(),
            BubbleVisualState::Proposing => theme::bubble_processing_ext(),
            BubbleVisualState::Executing => theme::bubble_executing(),
            BubbleVisualState::Suggesting => theme::bubble_suggesting(),
        }
    }

    pub fn set_state(&mut self, state: BubbleVisualState) {
        self.visual_state = state;
    }

    pub fn show_confirmation(&mut self, description: &str) {
        self.confirmation = Some(description.to_string());
        self.rejection_toast = None;
    }

    pub fn show_rejection(&mut self, reason: &str) {
        self.rejection_toast = Some(reason.to_string());
        self.confirmation = None;
    }

    pub fn show_suggestion(&mut self, txt: &str, action: &str) {
        self.suggestion = Some((txt.to_string(), action.to_string()));
        self.visual_state = BubbleVisualState::Suggesting;
    }

    pub fn dismiss_suggestion(&mut self) {
        self.suggestion = None;
        self.visual_state = BubbleVisualState::Idle;
    }

    pub fn show_skill_result(&mut self, txt: &str) {
        self.skill_result = Some(txt.to_string());
    }

    pub fn view<'a>(&'a self, registry: &'a SkillRegistry, has_active_doc: bool) -> Element<'a, Message> {
        let mut layers = column![].spacing(8);

        // Bubble circle
        let bubble = container(
            text("AI").size(16),
        )
        .padding(Padding::from([14, 18]))
        .style(theme::bubble_style(self.bubble_color()));

        layers = layers.push(
            iced::widget::mouse_area(bubble)
                .on_press(Message::BubbleClicked)
        );

        // Skills panel
        if self.skills_panel_visible {
            let mut skills_col = column![].spacing(4).padding(8);

            for skill in registry.all_skills() {
                let skill_name = skill.name().to_string();
                for (action_id, action_label) in skill.actions() {
                    let enabled = has_active_doc
                        || action_id == "search"
                        || action_id == "import";
                    let btn = button(text(action_label.clone()).size(13))
                        .on_press_maybe(
                            enabled.then(|| Message::SkillExecuted(skill_name.clone(), action_id.clone())),
                        )
                        .style(theme::skill_button_style)
                        .padding(Padding::from([8, 16]));
                    skills_col = skills_col.push(btn);
                }
            }

            // Status label
            if let Some(ref result) = self.skill_result {
                skills_col = skills_col.push(
                    text(result.as_str())
                        .size(12)
                        .color(theme::text_label())
                        .wrapping(text::Wrapping::Word),
                );
            } else if !has_active_doc {
                skills_col = skills_col.push(
                    text("Open a document to use skills")
                        .size(12)
                        .color(theme::text_dim()),
                );
            }

            layers = layers.push(
                container(skills_col).style(theme::skill_panel_style),
            );
        }

        // Confirmation panel
        if let Some(ref desc) = self.confirmation {
            let confirm = container(
                column![
                    text(desc.as_str())
                        .size(13)
                        .color(theme::text_primary())
                        .wrapping(text::Wrapping::Word),
                    row![
                        button(text("Approve").size(13))
                            .on_press(Message::ApproveAction)
                            .style(theme::approve_button_style)
                            .padding(Padding::from([6, 16])),
                        button(text("Reject").size(13))
                            .on_press(Message::RejectAction)
                            .style(theme::reject_button_style)
                            .padding(Padding::from([6, 16])),
                    ]
                    .spacing(8)
                ]
                .spacing(8)
                .padding(12),
            )
            .style(theme::confirmation_panel_style);

            layers = layers.push(confirm);
        }

        // Rejection toast
        if let Some(ref reason) = self.rejection_toast {
            layers = layers.push(
                text(reason.as_str())
                    .size(12)
                    .color(theme::external_orange()),
            );
        }

        // Suggestion tooltip
        if let Some((ref txt, _)) = self.suggestion {
            let tooltip = container(
                column![
                    text(txt.as_str())
                        .size(12)
                        .color(theme::text_label())
                        .wrapping(text::Wrapping::Word),
                    button(text("Dismiss").size(11))
                        .on_press(Message::DismissSuggestion)
                        .style(theme::skill_button_style)
                        .padding(Padding::from([4, 8])),
                ]
                .spacing(4)
                .padding(Padding::from([8, 12])),
            )
            .style(theme::suggestion_tooltip_style);

            layers = layers.push(tooltip);
        }

        container(layers)
            .padding(
                Padding::ZERO
                    .top(self.position.y)
                    .left(self.position.x),
            )
            .into()
    }
}

/// Build a SkillDocument from the active document state.
pub fn build_skill_doc(
    doc_id: &str,
    title: &str,
    body: &str,
    images: &[ContentImage],
) -> SkillDocument {
    SkillDocument {
        id: doc_id.to_string(),
        title: title.to_string(),
        content: ContentFields {
            body: body.to_string(),
            images: images.to_vec(),
            ..Default::default()
        },
    }
}

/// Format structured data for display.
pub fn format_structured_data(kind: &str, json: &str) -> String {
    match kind {
        "word_count" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!(
                    "Words: {} | Chars: {} | Lines: {} | ~{} min read",
                    v["words"], v["characters"], v["lines"], v["reading_time_min"]
                )
            } else {
                json.to_string()
            }
        }
        "search_results" => {
            if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(json) {
                if v.is_empty() {
                    "No results found".into()
                } else {
                    let titles: Vec<String> = v
                        .iter()
                        .take(5)
                        .filter_map(|item| item["title"].as_str().map(String::from))
                        .collect();
                    format!("{} results: {}", v.len(), titles.join(", "))
                }
            } else {
                json.to_string()
            }
        }
        "find_replace" => {
            if serde_json::from_str::<serde_json::Value>(json).is_ok() {
                "No matches found (0 replacements)".into()
            } else {
                json.to_string()
            }
        }
        "duplicate_result" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!("Created: {}", v["title"].as_str().unwrap_or("copy"))
            } else {
                json.to_string()
            }
        }
        "import_result" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!("Imported: {}", v["title"].as_str().unwrap_or("document"))
            } else {
                json.to_string()
            }
        }
        _ => json.to_string(),
    }
}
