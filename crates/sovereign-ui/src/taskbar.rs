use std::collections::HashSet;

use iced::widget::{button, container, row, text, Space};
use iced::{Element, Length, Padding};

use crate::app::Message;
use crate::theme;

/// An item in the taskbar.
#[derive(Clone)]
pub struct TaskbarItem {
    pub doc_id: String,
    pub title: String,
    pub is_owned: bool,
}

/// A contact item in the taskbar.
#[derive(Clone)]
pub struct TaskbarContactItem {
    pub contact_id: String,
    pub name: String,
    /// Pre-computed display label (e.g. "A Alice") to avoid per-frame allocation.
    pub display_label: String,
}

/// State for the taskbar.
pub struct TaskbarState {
    pub items: Vec<TaskbarItem>,
    pub pinned_ids: HashSet<String>,
    pub contacts: Vec<TaskbarContactItem>,
    pub pinned_contact_ids: HashSet<String>,
    pub listening: bool,
    pub inbox_unread: u32,
}

impl TaskbarState {
    pub fn new(pinned_docs: Vec<(String, String, bool)>) -> Self {
        let mut pinned_ids = HashSet::new();
        let items: Vec<TaskbarItem> = pinned_docs
            .into_iter()
            .map(|(id, title, is_owned)| {
                pinned_ids.insert(id.clone());
                TaskbarItem {
                    doc_id: id,
                    title,
                    is_owned,
                }
            })
            .collect();

        Self {
            items,
            pinned_ids,
            contacts: Vec::new(),
            pinned_contact_ids: HashSet::new(),
            listening: false,
            inbox_unread: 0,
        }
    }

    pub fn set_pinned_contacts(&mut self, contacts: Vec<(String, String)>) {
        for (id, name) in contacts {
            self.pinned_contact_ids.insert(id.clone());
            let initial = name.chars().next().unwrap_or('?');
            let display_label = format!("{initial} {name}");
            self.contacts.push(TaskbarContactItem {
                contact_id: id,
                name,
                display_label,
            });
        }
    }

    pub fn toggle_contact_pin(&mut self, contact_id: &str) {
        if self.pinned_contact_ids.contains(contact_id) {
            self.pinned_contact_ids.remove(contact_id);
            self.contacts.retain(|c| c.contact_id != contact_id);
        } else {
            self.pinned_contact_ids.insert(contact_id.to_string());
        }
    }

    pub fn add_document(&mut self, doc_id: &str, title: &str, is_owned: bool) {
        if self.items.iter().any(|i| i.doc_id == doc_id) {
            return;
        }
        self.items.push(TaskbarItem {
            doc_id: doc_id.to_string(),
            title: title.to_string(),
            is_owned,
        });
    }

    pub fn toggle_pin(&mut self, doc_id: &str) {
        if self.pinned_ids.contains(doc_id) {
            self.pinned_ids.remove(doc_id);
        } else {
            self.pinned_ids.insert(doc_id.to_string());
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut items_row = row![].spacing(4);

        for item in &self.items {
            let color = if item.is_owned {
                theme::owned_blue()
            } else {
                theme::external_orange()
            };
            let doc_id = item.doc_id.clone();
            let pin_id = item.doc_id.clone();
            let is_pinned = self.pinned_ids.contains(&item.doc_id);

            let label = row![
                text(item.title.as_str()).size(13).color(color),
                if is_pinned {
                    text(" *").size(11).color(theme::text_dim())
                } else {
                    text("").size(11)
                },
            ];

            let btn = button(label)
                .on_press(Message::TaskbarDocClicked(doc_id))
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12]));

            items_row = items_row.push(btn);

            // Pin/unpin toggle button
            let pin_btn = button(
                text(if is_pinned { "unpin" } else { "pin" }).size(10).color(theme::text_dim()),
            )
            .on_press(Message::TaskbarDocPinToggled(pin_id))
            .style(theme::taskbar_button_style)
            .padding(Padding::from([2, 6]));

            items_row = items_row.push(pin_btn);
        }

        // Pinned contacts
        if !self.contacts.is_empty() {
            items_row = items_row.push(
                text("|").size(13).color(theme::border_dim()),
            );
            for contact in &self.contacts {
                let cid = contact.contact_id.clone();
                let label = text(contact.display_label.as_str())
                    .size(13)
                    .color(theme::approve_green());
                let btn = button(label)
                    .on_press(Message::TaskbarContactClicked(cid))
                    .style(theme::taskbar_button_style)
                    .padding(Padding::from([4, 10]));
                items_row = items_row.push(btn);
            }
        }

        // Spacer
        items_row = items_row.push(Space::new().width(Length::Fill));

        // Inbox button (with unread badge)
        let inbox_label = if self.inbox_unread > 0 {
            format!("Inbox ({})", self.inbox_unread)
        } else {
            "Inbox".to_string()
        };
        items_row = items_row.push(
            button(text(inbox_label).size(13))
                .on_press(Message::InboxToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        // Theme toggle
        let theme_label = match theme::current_mode() {
            theme::ThemeMode::Dark => "Light",
            theme::ThemeMode::Light => "Dark",
        };
        items_row = items_row.push(
            button(text(theme_label).size(13))
                .on_press(Message::ThemeToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        // Models button
        items_row = items_row.push(
            button(text("Models").size(13))
                .on_press(Message::ModelPanelToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        // Chat button
        items_row = items_row.push(
            button(text("Chat").size(13))
                .on_press(Message::ChatToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        // Mic button
        let mic_label = if self.listening { "Listening..." } else { "Mic" };
        items_row = items_row.push(
            button(text(mic_label).size(13))
                .on_press(Message::MicToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        // Search button
        items_row = items_row.push(
            button(text("Search (Ctrl+F)").size(13))
                .on_press(Message::SearchToggled)
                .style(theme::taskbar_button_style)
                .padding(Padding::from([4, 12])),
        );

        container(items_row.padding(Padding::from([6, 12])))
            .style(theme::taskbar_style)
            .width(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_unpin_tracking() {
        let mut state = TaskbarState::new(vec![]);
        state.add_document("d1", "Test", true);

        state.toggle_pin("d1");
        assert!(state.pinned_ids.contains("d1"));

        state.toggle_pin("d1");
        assert!(!state.pinned_ids.contains("d1"));
    }
}
