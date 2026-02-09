use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, GestureClick, Label, Orientation, Widget};

use sovereign_core::interfaces::SkillEvent;

/// Handle for dynamically adding items to the taskbar after construction.
pub struct TaskbarHandle {
    items_box: GtkBox,
    known_ids: Rc<RefCell<Vec<String>>>,
    skill_tx: mpsc::Sender<SkillEvent>,
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

        let label = make_item(doc_id, title, is_owned, &self.skill_tx);
        // Insert before the spacer (second-to-last) and search button (last)
        // items_box children: [doc items...] [spacer] [search_btn]
        // We want to insert right before the spacer.
        let n = self.items_box.observe_children().n_items();
        // Insert before the last 2 children (spacer + search btn)
        let position = if n >= 2 { n - 2 } else { 0 };
        let sibling = nth_child(&self.items_box, position);
        self.items_box.insert_child_after(&label, sibling.as_ref());
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
) -> Label {
    let label = Label::new(Some(title));
    label.add_css_class("taskbar-item");
    if is_owned {
        label.add_css_class("owned-badge");
    } else {
        label.add_css_class("external-badge");
    }

    let tx = skill_tx.clone();
    let id = doc_id.to_string();
    let click = GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        let _ = tx.send(SkillEvent::OpenDocument {
            doc_id: id.clone(),
        });
    });
    label.add_controller(click);

    label
}

pub fn build_taskbar(
    search_toggle: impl Fn() + 'static,
    pinned_docs: Vec<(String, String, bool)>,
    skill_tx: mpsc::Sender<SkillEvent>,
) -> (GtkBox, TaskbarHandle) {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    bar.add_css_class("taskbar");

    let known_ids = Rc::new(RefCell::new(Vec::new()));

    for (doc_id, title, is_owned) in &pinned_docs {
        known_ids.borrow_mut().push(doc_id.clone());
        let label = make_item(doc_id, title, *is_owned, &skill_tx);
        bar.append(&label);
    }

    // Spacer
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    // Search button
    let search_btn = gtk4::Button::with_label("Search (Ctrl+F)");
    search_btn.add_css_class("search-btn");
    search_btn.connect_clicked(move |_| search_toggle());
    bar.append(&search_btn);

    let handle = TaskbarHandle {
        items_box: bar.clone(),
        known_ids,
        skill_tx,
    };

    (bar, handle)
}
