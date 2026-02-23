use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// A search result with ID and display title.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
}

/// State for the search overlay.
pub struct SearchState {
    pub visible: bool,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub voice_status: Option<String>,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            results: Vec::new(),
            voice_status: None,
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let input = text_input("Search documents...", &self.query)
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SearchSubmitted)
            .style(theme::search_input_style)
            .padding(Padding::from([10, 16]))
            .size(16);

        let mut col = column![input].spacing(8).padding(16).width(450);

        // Voice status
        if let Some(ref status) = self.voice_status {
            col = col.push(
                text(status.as_str())
                    .size(12)
                    .color(theme::border_accent()),
            );
        }

        // Results (capped at 50)
        if !self.results.is_empty() {
            let mut results_col = column![].spacing(2);
            let display_count = self.results.len().min(50);
            for result in self.results.iter().take(display_count) {
                let title_btn = button(
                    text(result.title.as_str())
                        .size(13)
                        .color(theme::text_label()),
                )
                .on_press(Message::SearchResultNavigate(result.id.clone()))
                .style(theme::skill_button_style)
                .padding(Padding::from([4, 8]));

                let open_btn = button(
                    text("Open")
                        .size(11)
                        .color(theme::border_accent()),
                )
                .on_press(Message::SearchResultOpen(result.id.clone()))
                .style(|_theme: &iced::Theme, status: button::Status| {
                    let bg = match status {
                        button::Status::Hovered => theme::bg_button_hover(),
                        _ => iced::Color::TRANSPARENT,
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: theme::border_accent(),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .padding(Padding::from([2, 6]));

                results_col = results_col.push(
                    row![title_btn, open_btn]
                        .spacing(4)
                        .align_y(iced::Alignment::Center),
                );
            }
            if self.results.len() > 50 {
                results_col = results_col.push(
                    text(format!("...and {} more", self.results.len() - 50))
                        .size(12)
                        .color(theme::text_dim()),
                );
            }
            col = col.push(scrollable(results_col).height(Length::Shrink));
        } else if !self.query.is_empty() {
            col = col.push(
                text("No documents found")
                    .size(13)
                    .color(theme::text_dim()),
            );
        }

        // Hint
        let hint = if self.query.is_empty() {
            "Type to search documents"
        } else {
            "Press Enter to ask the AI assistant"
        };
        col = col.push(text(hint).size(12).color(theme::text_dim()));

        // Wrap in a styled panel with max height so it doesn't cover the AI bubble
        let panel = container(col)
            .style(theme::skill_panel_style)
            .max_height(400);

        container(panel)
            .center_x(Length::Fill)
            .padding(Padding::ZERO.top(60))
            .into()
    }
}
