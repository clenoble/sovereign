use std::collections::HashMap;

use iced::widget::{button, column, container, mouse_area, row, scrollable, text, Space};
use iced::{Element, Length, Padding};

use sovereign_db::schema::{ChannelType, Contact, Conversation};

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

/// A floating panel showing a contact-centric inbox: contacts list with unread badges.
/// Clicking a contact opens their ContactPanel with all communications.
pub struct InboxPanel {
    /// (contact_id, contact) pairs, sorted by unread count descending.
    pub contacts: Vec<(String, Contact)>,
    /// Per-contact unread count aggregated from conversations.
    pub unread_by_contact: HashMap<String, u32>,
    /// Per-contact channel badges.
    pub channels_by_contact: HashMap<String, Vec<ChannelType>>,
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
    pub fn new(contacts: Vec<(String, Contact)>, conversations: &[Conversation]) -> Self {
        let (unread_by_contact, channels_by_contact) =
            compute_contact_stats(&contacts, conversations);

        // Sort contacts: unread first, then alphabetical
        let mut sorted = contacts;
        sorted.sort_by(|a, b| {
            let ua = unread_by_contact.get(&a.0).copied().unwrap_or(0);
            let ub = unread_by_contact.get(&b.0).copied().unwrap_or(0);
            ub.cmp(&ua).then_with(|| a.1.name.cmp(&b.1.name))
        });

        Self {
            contacts: sorted,
            unread_by_contact,
            channels_by_contact,
            position: iced::Point::new(200.0, 60.0),
            size: iced::Size::new(400.0, 480.0),
            visible: true,
            dragging: false,
            last_local_cursor: iced::Point::ORIGIN,
            drag_start_screen: None,
            drag_start_panel: None,
        }
    }

    /// Total unread count across all contacts.
    pub fn total_unread(&self) -> u32 {
        self.unread_by_contact.values().sum()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        // Toolbar: title + unread badge + close button
        let unread = self.total_unread();
        let title_text = if unread > 0 {
            format!("Contacts ({})", unread)
        } else {
            "Contacts".to_string()
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

        let body = self.view_contacts();

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

    fn view_contacts(&self) -> Element<'_, AppMessage> {
        if self.contacts.is_empty() {
            return container(
                text("No contacts yet").size(13).color(theme::text_dim()),
            )
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        }

        let mut list = column![].spacing(4).padding(Padding::from([8, 12]));

        for (contact_id, contact) in &self.contacts {
            // Skip the "You" contact (owned)
            if contact.is_owned {
                continue;
            }

            let initial = contact.name.chars().next().unwrap_or('?');
            let unread = self.unread_by_contact.get(contact_id).copied().unwrap_or(0);
            let unread_badge = if unread > 0 {
                format!(" ({})", unread)
            } else {
                String::new()
            };

            // Channel badges
            let channels = self
                .channels_by_contact
                .get(contact_id)
                .cloned()
                .unwrap_or_default();
            let channel_labels: String = channels
                .iter()
                .map(channel_label)
                .collect::<Vec<_>>()
                .join(" · ");

            let name_row = row![
                container(
                    text(initial.to_string()).size(16).color(theme::text_primary()),
                )
                .width(28)
                .height(28)
                .center_x(28)
                .center_y(28)
                .style(theme::skill_panel_style),
                Space::new().width(8),
                text(&contact.name).size(13).color(theme::text_primary()),
                text(unread_badge).size(13).color(theme::approve_green()),
            ]
            .spacing(0);

            let mut item = column![name_row].spacing(2);
            if !channel_labels.is_empty() {
                item = item.push(
                    text(channel_labels)
                        .size(10)
                        .color(theme::text_dim()),
                );
            }

            let cid = contact_id.clone();
            let btn = button(item)
                .on_press(AppMessage::OpenContact(cid))
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
}

fn compute_contact_stats(
    contacts: &[(String, Contact)],
    conversations: &[Conversation],
) -> (HashMap<String, u32>, HashMap<String, Vec<ChannelType>>) {
    let mut unread: HashMap<String, u32> = HashMap::new();
    let mut channels: HashMap<String, Vec<ChannelType>> = HashMap::new();

    for conv in conversations {
        for pid in &conv.participant_contact_ids {
            *unread.entry(pid.clone()).or_default() += conv.unread_count;
            let ch = channels.entry(pid.clone()).or_default();
            if !ch.contains(&conv.channel) {
                ch.push(conv.channel.clone());
            }
        }
    }

    // Also include contacts with no conversations (they'll show 0 unread)
    for (cid, _) in contacts {
        unread.entry(cid.clone()).or_default();
    }

    (unread, channels)
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
    fn inbox_panel_total_unread_empty() {
        let panel = InboxPanel::new(vec![], &[]);
        assert_eq!(panel.total_unread(), 0);
    }

    #[test]
    fn compute_contact_stats_empty() {
        let (unread, channels) = compute_contact_stats(&[], &[]);
        assert!(unread.is_empty());
        assert!(channels.is_empty());
    }
}
