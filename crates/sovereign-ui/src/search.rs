use iced::widget::{column, container, scrollable, text, text_input};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// State for the search overlay.
pub struct SearchState {
    pub visible: bool,
    pub query: String,
    pub results: Vec<String>,
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

        // Results
        if !self.results.is_empty() {
            let mut results_col = column![].spacing(4);
            for id in &self.results {
                results_col = results_col.push(
                    text(id.as_str())
                        .size(13)
                        .color(theme::text_label()),
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
        col = col.push(
            text("Press Enter to search")
                .size(12)
                .color(theme::text_dim()),
        );

        container(col)
            .style(theme::search_overlay_style)
            .center_x(Length::Fill)
            .padding(Padding::ZERO.top(80))
            .into()
    }
}
