use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Label, Orientation};

struct TaskbarItem {
    title: &'static str,
    is_owned: bool,
}

const MOCK_ITEMS: &[TaskbarItem] = &[
    TaskbarItem { title: "Research Notes", is_owned: true },
    TaskbarItem { title: "Meeting Summary", is_owned: true },
    TaskbarItem { title: "API Reference", is_owned: false },
    TaskbarItem { title: "Design Spec", is_owned: true },
];

pub fn build_taskbar(search_toggle: impl Fn() + 'static) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    bar.add_css_class("taskbar");

    for item in MOCK_ITEMS {
        let label = Label::new(Some(item.title));
        label.add_css_class("taskbar-item");
        if item.is_owned {
            label.add_css_class("owned-badge");
        } else {
            label.add_css_class("external-badge");
        }
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

    bar
}
