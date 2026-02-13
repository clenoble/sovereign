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

/// State for the taskbar.
pub struct TaskbarState {
    pub items: Vec<TaskbarItem>,
    pub pinned_ids: HashSet<String>,
    pub listening: bool,
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
            listening: false,
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
                theme::OWNED_BLUE
            } else {
                theme::EXTERNAL_ORANGE
            };
            let doc_id = item.doc_id.clone();
            let pin_id = item.doc_id.clone();
            let is_pinned = self.pinned_ids.contains(&item.doc_id);

            let label = row![
                text(item.title.as_str()).size(13).color(color),
                if is_pinned {
                    text(" *").size(11).color(theme::TEXT_DIM)
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
                text(if is_pinned { "unpin" } else { "pin" }).size(10).color(theme::TEXT_DIM),
            )
            .on_press(Message::TaskbarDocPinToggled(pin_id))
            .style(theme::taskbar_button_style)
            .padding(Padding::from([2, 6]));

            items_row = items_row.push(pin_btn);
        }

        // Spacer
        items_row = items_row.push(Space::new().width(Length::Fill));

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
