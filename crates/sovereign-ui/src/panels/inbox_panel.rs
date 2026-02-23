use std::collections::HashMap;

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input, Space};
use iced::{Element, Length, Padding};

use sovereign_db::schema::{ChannelType, Conversation, Message as DbMessage, MessageDirection, ReadStatus};

use crate::app::Message as AppMessage;
use crate::panels::document_panel::DEAD_ZONE;
use crate::theme;

/// A request to send a reply message, passed from UI to the async runtime.
#[derive(Debug, Clone)]
pub struct SendRequest {
    pub conversation_id: String,
    pub to_addresses: Vec<String>,
    pub subject: Option<String>,
    pub body: String,
    pub in_reply_to: Option<String>,
}

/// A floating panel showing the unified inbox: conversations list + message thread view.
pub struct InboxPanel {
    pub conversations: Vec<Conversation>,
    pub messages: Vec<DbMessage>,
    /// Messages pre-grouped by conversation_id for O(1) lookup.
    messages_by_conv: HashMap<String, Vec<usize>>,
    pub selected_conversation: Option<usize>,
    pub reply_input: String,
    pub position: iced::Point,
    pub size: iced::Size,
    pub visible: bool,
    // Drag state
    pub dragging: bool,
    pub last_local_cursor: iced::Point,
    pub drag_start_screen: Option<iced::Point>,
    pub drag_start_panel: Option<iced::Point>,
}

impl InboxPanel {
    pub fn new(conversations: Vec<Conversation>, messages: Vec<DbMessage>) -> Self {
        let messages_by_conv = group_messages(&messages);
        Self {
            conversations,
            messages,
            messages_by_conv,
            selected_conversation: None,
            reply_input: String::new(),
            position: iced::Point::new(200.0, 60.0),
            size: iced::Size::new(520.0, 520.0),
            visible: true,
            dragging: false,
            last_local_cursor: iced::Point::ORIGIN,
            drag_start_screen: None,
            drag_start_panel: None,
        }
    }

    /// Refresh data (called when new messages arrive).
    pub fn refresh(&mut self, conversations: Vec<Conversation>, messages: Vec<DbMessage>) {
        self.messages_by_conv = group_messages(&messages);
        self.conversations = conversations;
        self.messages = messages;
    }

    /// Total unread count across all conversations.
    pub fn total_unread(&self) -> u32 {
        self.conversations.iter().map(|c| c.unread_count).sum()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        // Toolbar: title + unread badge + close button
        let unread = self.total_unread();
        let title_text = if unread > 0 {
            format!("Inbox ({})", unread)
        } else {
            "Inbox".to_string()
        };

        let toolbar = row![
            text(title_text).size(15).color(theme::text_primary()),
            Space::new().width(Length::Fill),
            button(text("Close").size(13))
                .on_press(AppMessage::InboxClose)
                .style(theme::reject_button_style)
                .padding(Padding::from([8, 12])),
        ]
        .spacing(8)
        .padding(Padding::from([8, 12]));

        // Body: conversation list or message thread
        let body = if let Some(conv_idx) = self.selected_conversation {
            self.view_messages(conv_idx)
        } else {
            self.view_conversations()
        };

        let content = column![toolbar, body].spacing(4);

        let panel = mouse_area(
            container(
                container(content)
                    .width(self.size.width)
                    .height(self.size.height)
                    .style(theme::document_panel_style),
            )
            .padding(DEAD_ZONE),
        )
        .on_press(AppMessage::InboxDragStart)
        .on_release(AppMessage::InboxDragEnd)
        .on_move(|p| AppMessage::InboxDragMove(p))
        .on_scroll(|_| AppMessage::Ignore);

        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(
                Padding::ZERO
                    .top((self.position.y - DEAD_ZONE).max(0.0))
                    .left((self.position.x - DEAD_ZONE).max(0.0)),
            )
            .into()
    }

    fn view_conversations(&self) -> Element<'_, AppMessage> {
        if self.conversations.is_empty() {
            return container(
                text("No conversations yet").size(13).color(theme::text_dim()),
            )
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let mut list = column![].spacing(4).padding(Padding::from([8, 12]));

        for (i, conv) in self.conversations.iter().enumerate() {
            let ch_label = channel_label(&conv.channel);
            let unread_badge = if conv.unread_count > 0 {
                format!(" ({})", conv.unread_count)
            } else {
                String::new()
            };

            // Preview: last message snippet
            let conv_id = conv.id_string().unwrap_or_default();
            let preview = self.last_message_preview(&conv_id);

            let title_row = row![
                text(ch_label).size(11).color(theme::text_dim()),
                Space::new().width(4),
                text(&conv.title).size(13).color(theme::text_primary()),
                text(unread_badge).size(13).color(theme::approve_green()),
            ]
            .spacing(0);

            let preview_text = text(preview)
                .size(11)
                .color(theme::text_dim());

            let item = column![title_row, preview_text].spacing(2);

            let btn = button(item)
                .on_press(AppMessage::InboxSelectConversation(i))
                .style(theme::skill_button_style)
                .padding(Padding::from([8, 10]))
                .width(Length::Fill);

            list = list.push(btn);
        }

        scrollable(list)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn view_messages(&self, conv_idx: usize) -> Element<'_, AppMessage> {
        let conv = &self.conversations[conv_idx];
        let conv_id = conv.id_string().unwrap_or_default();

        let empty = Vec::new();
        let msg_indices = self.messages_by_conv.get(&conv_id).unwrap_or(&empty);
        let conv_messages: Vec<&DbMessage> = msg_indices.iter().map(|&i| &self.messages[i]).collect();

        let back_btn = button(text("< Back").size(12))
            .on_press(AppMessage::InboxBack)
            .style(theme::skill_button_style)
            .padding(Padding::from([4, 8]));

        let header = row![
            back_btn,
            Space::new().width(8),
            text(format!("{} — {}", channel_label(&conv.channel), conv.title))
                .size(13)
                .color(theme::text_primary()),
        ]
        .spacing(0)
        .padding(Padding::from([4, 12]));

        let mut msg_list = column![].spacing(6).padding(Padding::from([4, 12]));

        if conv_messages.is_empty() {
            msg_list = msg_list.push(
                text("No messages yet").size(12).color(theme::text_dim()),
            );
        } else {
            for msg in &conv_messages {
                let ts = msg.sent_at.format("%m/%d %H:%M").to_string();
                let dir_indicator = match msg.direction {
                    MessageDirection::Outbound => "> You",
                    MessageDirection::Inbound => "<",
                };
                let read_mark = match msg.read_status {
                    ReadStatus::Unread => " [new]",
                    _ => "",
                };
                let preview: String = msg.body.chars().take(200).collect();

                let meta_row = row![
                    text(ts).size(10).color(theme::text_dim()),
                    Space::new().width(6),
                    text(dir_indicator).size(10).color(theme::text_dim()),
                    text(read_mark).size(10).color(theme::approve_green()),
                ]
                .spacing(0);

                let body_text = text(preview).size(12).color(theme::text_primary());

                msg_list = msg_list.push(column![meta_row, body_text].spacing(1));
            }
        }

        // Reply input at the bottom
        let reply_row = row![
            text_input("Reply...", &self.reply_input)
                .on_input(AppMessage::InboxReplyChanged)
                .on_submit(AppMessage::InboxReplySubmit)
                .size(13)
                .padding(Padding::from([6, 10]))
                .width(Length::Fill),
            button(text("Send").size(12))
                .on_press(AppMessage::InboxReplySubmit)
                .style(theme::approve_button_style)
                .padding(Padding::from([6, 12])),
        ]
        .spacing(6)
        .padding(Padding::from([8, 12]));

        column![
            header,
            scrollable(msg_list).height(Length::Fill),
            reply_row,
        ]
        .spacing(2)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn last_message_preview(&self, conv_id: &str) -> String {
        let empty = Vec::new();
        let indices = self.messages_by_conv.get(conv_id).unwrap_or(&empty);
        if let Some(&last_idx) = indices.last() {
            let msg = &self.messages[last_idx];
            let preview: String = msg.body.chars().take(60).collect();
            if msg.body.len() > 60 {
                format!("{}...", preview)
            } else {
                preview
            }
        } else {
            String::new()
        }
    }
}

fn group_messages(messages: &[DbMessage]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, msg) in messages.iter().enumerate() {
        map.entry(msg.conversation_id.clone()).or_default().push(i);
    }
    map
}

fn channel_label(ch: &ChannelType) -> &'static str {
    match ch {
        ChannelType::Email => "Email",
        ChannelType::Sms => "SMS",
        ChannelType::Signal => "Signal",
        ChannelType::WhatsApp => "WhatsApp",
        ChannelType::Matrix => "Matrix",
        ChannelType::Phone => "Phone",
        ChannelType::Custom(_) => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_messages_empty() {
        let map = group_messages(&[]);
        assert!(map.is_empty());
    }

    #[test]
    fn inbox_panel_total_unread() {
        let panel = InboxPanel::new(vec![], vec![]);
        assert_eq!(panel.total_unread(), 0);
    }
}
