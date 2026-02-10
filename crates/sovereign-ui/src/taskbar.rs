use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, GestureClick, Label, Orientation, Widget};

use sovereign_core::interfaces::{SkillEvent, VoiceCommand};

/// Handle for dynamically adding items to the taskbar after construction.
pub struct TaskbarHandle {
    items_box: GtkBox,
    known_ids: Rc<RefCell<Vec<String>>>,
    pinned_ids: Rc<RefCell<Vec<String>>>,
    skill_tx: mpsc::Sender<SkillEvent>,
    last_thread_label: Label,
}

impl TaskbarHandle {
    /// Add a document to the taskbar. No-op if the doc_id is already shown.
    pub fn add_document(&self, doc_id: &str, title: &str, is_owned: bool) {
        {
            let ids = self.known_ids.borrow();
            if ids.iter().any(|id| id == doc_id) {
                return;
            }
        }
        self.known_ids.borrow_mut().push(doc_id.to_string());

        let pinned = self.pinned_ids.clone();
        let label = make_item(doc_id, title, is_owned, &self.skill_tx, pinned);
        // Insert before the spacer (second-to-last) and search button (last)
        let n = self.items_box.observe_children().n_items();
        let position = if n >= 2 { n - 2 } else { 0 };
        let sibling = nth_child(&self.items_box, position);
        self.items_box.insert_child_after(&label, sibling.as_ref());
    }

    /// Update the "last thread" indicator in the taskbar.
    pub fn set_last_thread(&self, thread_name: &str) {
        self.last_thread_label
            .set_label(&format!("Thread: {}", thread_name));
        self.last_thread_label.set_visible(true);
    }

    /// Pin a document so it persists in the taskbar.
    pub fn pin(&self, doc_id: &str) {
        let mut pinned = self.pinned_ids.borrow_mut();
        if !pinned.contains(&doc_id.to_string()) {
            pinned.push(doc_id.to_string());
        }
    }

    /// Unpin a document from the taskbar.
    pub fn unpin(&self, doc_id: &str) {
        self.pinned_ids
            .borrow_mut()
            .retain(|id| id != doc_id);
    }

    /// Check if a document is pinned.
    pub fn is_pinned(&self, doc_id: &str) -> bool {
        self.pinned_ids.borrow().contains(&doc_id.to_string())
    }
}

/// Get the nth child widget (0-indexed). Returns None for position 0 (prepend).
fn nth_child(container: &GtkBox, position: u32) -> Option<Widget> {
    if position == 0 {
        return None;
    }
    let mut child = container.first_child();
    for _ in 1..position {
        child = child.and_then(|c| c.next_sibling());
    }
    child
}

fn make_item(
    doc_id: &str,
    title: &str,
    is_owned: bool,
    skill_tx: &mpsc::Sender<SkillEvent>,
    pinned_ids: Rc<RefCell<Vec<String>>>,
) -> Label {
    let label = Label::new(Some(title));
    label.add_css_class("taskbar-item");
    if is_owned {
        label.add_css_class("owned-badge");
    } else {
        label.add_css_class("external-badge");
    }

    // Left click: open document
    let tx = skill_tx.clone();
    let id = doc_id.to_string();
    let click = GestureClick::new();
    click.set_button(1);
    click.connect_released(move |_, _, _, _| {
        let _ = tx.send(SkillEvent::OpenDocument {
            doc_id: id.clone(),
        });
    });
    label.add_controller(click);

    // Right click: toggle pin
    let id_right = doc_id.to_string();
    let right_click = GestureClick::new();
    right_click.set_button(3);
    right_click.connect_released(move |_, _, _, _| {
        let mut pinned = pinned_ids.borrow_mut();
        if pinned.contains(&id_right) {
            pinned.retain(|id| id != &id_right);
            tracing::info!("Unpinned: {}", id_right);
        } else {
            pinned.push(id_right.clone());
            tracing::info!("Pinned: {}", id_right);
        }
    });
    label.add_controller(right_click);

    label
}

pub fn build_taskbar(
    search_toggle: impl Fn() + 'static,
    pinned_docs: Vec<(String, String, bool)>,
    skill_tx: mpsc::Sender<SkillEvent>,
    voice_tx: Option<mpsc::Sender<VoiceCommand>>,
) -> (GtkBox, TaskbarHandle) {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    bar.add_css_class("taskbar");

    let known_ids = Rc::new(RefCell::new(Vec::new()));
    let pinned_ids = Rc::new(RefCell::new(Vec::new()));

    // Last-thread label (hidden initially)
    let last_thread_label = Label::new(None);
    last_thread_label.add_css_class("taskbar-thread");
    last_thread_label.set_visible(false);
    bar.append(&last_thread_label);

    for (doc_id, title, is_owned) in &pinned_docs {
        known_ids.borrow_mut().push(doc_id.clone());
        pinned_ids.borrow_mut().push(doc_id.clone());
        let label = make_item(doc_id, title, *is_owned, &skill_tx, pinned_ids.clone());
        bar.append(&label);
    }

    // Spacer
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    // Mic button (push-to-talk)
    if let Some(vtx) = voice_tx {
        let mic_btn = gtk4::Button::with_label("Mic");
        mic_btn.add_css_class("mic-btn");
        let listening = Rc::new(RefCell::new(false));
        let listening_c = listening.clone();
        mic_btn.connect_clicked(move |btn| {
            let mut is_listening = listening_c.borrow_mut();
            if *is_listening {
                let _ = vtx.send(VoiceCommand::StopListening);
                btn.set_label("Mic");
                btn.remove_css_class("mic-active");
            } else {
                let _ = vtx.send(VoiceCommand::StartListening);
                btn.set_label("Listening...");
                btn.add_css_class("mic-active");
            }
            *is_listening = !*is_listening;
        });
        bar.append(&mic_btn);
    }

    // Search button
    let search_btn = gtk4::Button::with_label("Search (Ctrl+F)");
    search_btn.add_css_class("search-btn");
    search_btn.connect_clicked(move |_| search_toggle());
    bar.append(&search_btn);

    let handle = TaskbarHandle {
        items_box: bar.clone(),
        known_ids,
        pinned_ids,
        skill_tx,
        last_thread_label,
    };

    (bar, handle)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pin_unpin_tracking() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let pinned = Rc::new(RefCell::new(Vec::<String>::new()));

        // Pin
        pinned.borrow_mut().push("d1".to_string());
        assert!(pinned.borrow().contains(&"d1".to_string()));

        // Unpin
        pinned.borrow_mut().retain(|id| id != "d1");
        assert!(!pinned.borrow().contains(&"d1".to_string()));
    }
}
