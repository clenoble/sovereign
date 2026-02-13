use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// A single message in the chat history.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

/// State for the chat panel.
pub struct ChatState {
    pub visible: bool,
    pub input: String,
    pub messages: Vec<ChatMessage>,
    pub generating: bool,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            messages: Vec::new(),
            generating: false,
        }
    }

    pub fn push_user_message(&mut self, text: String) {
        self.messages.push(ChatMessage {
            role: ChatRole::User,
            text,
        });
        self.generating = true;
    }

    pub fn push_assistant_message(&mut self, text: String) {
        self.messages.push(ChatMessage {
            role: ChatRole::Assistant,
            text,
        });
        self.generating = false;
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(8).padding(12).width(380);

        // Header
        col = col.push(
            row![
                text("Chat").size(15).color(theme::TEXT_PRIMARY),
                Space::new().width(Length::Fill),
                button(text("X").size(13))
                    .on_press(Message::ChatToggled)
                    .style(theme::skill_button_style)
                    .padding(Padding::from([4, 8])),
            ],
        );

        // Message history
        let mut messages_col = column![].spacing(6);
        for msg in &self.messages {
            let (color, prefix) = match msg.role {
                ChatRole::User => (theme::OWNED_BLUE, "You"),
                ChatRole::Assistant => (theme::BORDER_ACCENT, "AI"),
            };
            messages_col = messages_col.push(
                column![
                    text(prefix).size(11).color(theme::TEXT_DIM),
                    text(msg.text.as_str())
                        .size(13)
                        .color(color)
                        .wrapping(text::Wrapping::Word),
                ]
                .spacing(2),
            );
        }

        if self.generating {
            messages_col = messages_col.push(
                text("Thinking...")
                    .size(12)
                    .color(theme::BUBBLE_SUGGESTING),
            );
        }

        col = col.push(scrollable(messages_col).height(Length::Fixed(400.0)));

        // Input
        let input = text_input("Type a message...", &self.input)
            .on_input(Message::ChatInputChanged)
            .on_submit(Message::ChatSubmitted)
            .style(theme::search_input_style)
            .padding(Padding::from([8, 12]))
            .size(14);

        col = col.push(input);

        container(col).style(theme::chat_panel_style).into()
    }
}
