use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Entry, Label, Orientation};

pub fn build_search_overlay() -> GtkBox {
    let overlay_box = GtkBox::new(Orientation::Vertical, 8);
    overlay_box.add_css_class("search-overlay");
    overlay_box.set_halign(gtk4::Align::Center);
    overlay_box.set_valign(gtk4::Align::Start);
    overlay_box.set_visible(false);

    let entry = Entry::new();
    entry.set_placeholder_text(Some("Search documents..."));
    entry.add_css_class("search-entry");
    overlay_box.append(&entry);

    let hint = Label::new(Some("Not yet connected — search will be available in Phase 4"));
    hint.add_css_class("search-hint");
    overlay_box.append(&hint);

    overlay_box
}
