use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Element, Length, Padding};

use sovereign_db::schema::{ChannelType, Contact, Conversation, Message};

use crate::app::Message as AppMessage;
use crate::panels::document_panel::DEAD_ZONE;
use crate::theme;

/// A floating panel showing contact details and conversation history.
pub struct ContactPanel {
    pub contact: Contact,
    pub contact_id: String,
    pub conversations: Vec<Conversation>,
    pub messages: Vec<Message>,
    pub selected_conversation: Option<usize>,
    pub position: iced::Point,
    pub size: iced::Size,
    pub visible: bool,
    // Drag state
    pub dragging: bool,
    pub last_local_cursor: iced::Point,
    pub drag_start_screen: Option<iced::Point>,
    pub drag_start_panel: Option<iced::Point>,
}

impl ContactPanel {
    pub fn new(
        contact: Contact,
        contact_id: String,
        conversations: Vec<Conversation>,
        messages: Vec<Message>,
    ) -> Self {
        Self {
            contact,
            contact_id,
            conversations,
            messages,
            selected_conversation: None,
            position: iced::Point::new(250.0, 80.0),
            size: iced::Size::new(500.0, 480.0),
            visible: true,
            dragging: false,
            last_local_cursor: iced::Point::ORIGIN,
            drag_start_screen: None,
            drag_start_panel: None,
        }
    }

    pub fn view(&self, index: usize) -> Element<'_, AppMessage> {
        // Toolbar: contact name + close button
        let initial = self.contact.name.chars().next().unwrap_or('?');
        let toolbar = row![
            container(
                text(initial.to_string()).size(18).color(theme::TEXT_PRIMARY),
            )
            .width(32)
            .height(32)
            .center_x(32)
            .center_y(32)
            .style(theme::skill_panel_style),
            text(&self.contact.name).size(15).color(theme::TEXT_PRIMARY),
            Space::new().width(Length::Fill),
            button(text("Close").size(13))
                .on_press(AppMessage::CloseContactPanel(index))
                .style(theme::reject_button_style)
                .padding(Padding::from([8, 12])),
        ]
        .spacing(8)
        .padding(Padding::from([8, 12]));

        // Addresses
        let mut addr_col = column![].spacing(2).padding(Padding::from([0, 12]));
        for addr in &self.contact.addresses {
            let ch_label = channel_label(&addr.channel);
            let line = text(format!("{}: {}", ch_label, addr.address))
                .size(12)
                .color(theme::TEXT_DIM);
            addr_col = addr_col.push(line);
        }
        if !self.contact.notes.is_empty() {
            addr_col = addr_col.push(
                text(format!("Notes: {}", self.contact.notes))
                    .size(12)
                    .color(theme::TEXT_DIM),
            );
        }

        // Conversation list or message view
        let body = if let Some(conv_idx) = self.selected_conversation {
            self.view_messages(index, conv_idx)
        } else {
            self.view_conversations(index)
        };

        let content = column![toolbar, addr_col, body].spacing(4);

        let panel = mouse_area(
            container(
                container(content)
                    .width(self.size.width)
                    .height(self.size.height)
                    .style(theme::document_panel_style),
            )
            .padding(DEAD_ZONE),
        )
        .on_press(AppMessage::ContactPanelDragStart(index))
        .on_release(AppMessage::ContactPanelDragEnd(index))
        .on_move(move |p| AppMessage::ContactPanelDragMove {
            panel_idx: index,
            local: p,
        })
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

    fn view_conversations(&self, index: usize) -> Element<'_, AppMessage> {
        if self.conversations.is_empty() {
            return container(
                text("No conversations").size(13).color(theme::TEXT_DIM),
            )
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let mut list = column![
            text("Conversations").size(13).color(theme::TEXT_PRIMARY),
        ]
        .spacing(4)
        .padding(Padding::from([8, 12]));

        for (i, conv) in self.conversations.iter().enumerate() {
            let ch_label = channel_label(&conv.channel);
            let unread = if conv.unread_count > 0 {
                format!(" ({})", conv.unread_count)
            } else {
                String::new()
            };
            let linked = if conv.linked_thread_id.is_some() {
                " [linked]"
            } else {
                ""
            };
            let label = text(format!("{} {} {}{}", ch_label, conv.title, unread, linked))
                .size(12)
                .color(theme::TEXT_PRIMARY);

            let btn = button(label)
                .on_press(AppMessage::SelectConversation {
                    panel_idx: index,
                    conv_idx: i,
                })
                .style(theme::skill_button_style)
                .padding(Padding::from([6, 10]))
                .width(Length::Fill);

            list = list.push(btn);
        }

        scrollable(list)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn view_messages(&self, index: usize, conv_idx: usize) -> Element<'_, AppMessage> {
        let conv = &self.conversations[conv_idx];
        let conv_id = conv.id_string().unwrap_or_default();

        // Filter messages for this conversation
        let conv_messages: Vec<&Message> = self
            .messages
            .iter()
            .filter(|m| m.conversation_id == conv_id)
            .collect();

        let back_btn = button(text("< Back").size(12))
            .on_press(AppMessage::SelectConversation {
                panel_idx: index,
                conv_idx: usize::MAX, // sentinel for "back"
            })
            .style(theme::skill_button_style)
            .padding(Padding::from([4, 8]));

        let header = row![
            back_btn,
            Space::new().width(8),
            text(format!("{} — {}", channel_label(&conv.channel), conv.title))
                .size(13)
                .color(theme::TEXT_PRIMARY),
        ]
        .spacing(0)
        .padding(Padding::from([4, 12]));

        let mut msg_list = column![].spacing(4).padding(Padding::from([4, 12]));

        if conv_messages.is_empty() {
            msg_list = msg_list.push(
                text("No messages yet").size(12).color(theme::TEXT_DIM),
            );
        } else {
            for msg in &conv_messages {
                let ts = msg.sent_at.format("%m/%d %H:%M").to_string();
                let dir_indicator = match msg.direction {
                    sovereign_db::schema::MessageDirection::Outbound => ">",
                    sovereign_db::schema::MessageDirection::Inbound => "<",
                };
                let read_mark = match msg.read_status {
                    sovereign_db::schema::ReadStatus::Unread => " [new]",
                    _ => "",
                };
                let preview: String = msg.body.chars().take(120).collect();
                let line = text(format!("{} {} {}{}", ts, dir_indicator, preview, read_mark))
                    .size(11)
                    .color(theme::TEXT_PRIMARY);
                msg_list = msg_list.push(line);
            }
        }

        column![header, scrollable(msg_list).height(Length::Fill)]
            .spacing(2)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
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
