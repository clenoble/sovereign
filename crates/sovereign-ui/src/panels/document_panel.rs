use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    ApplicationWindow, Box as GtkBox, Button, ContentFit, Entry, FlowBox, Label, Orientation,
    Picture, ScrolledWindow, TextView, Window, WrapMode,
};

use sovereign_core::content::{ContentFields, ContentImage};

use crate::orchestrator_bubble::ActiveDocument;

/// Rebuild the image gallery container from the given image list.
fn rebuild_gallery(container: &GtkBox, images: &[ContentImage]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    if images.is_empty() {
        return;
    }

    let gallery_label = Label::new(Some("Images:"));
    gallery_label.set_halign(gtk4::Align::Start);
    gallery_label.set_margin_start(12);
    gallery_label.set_margin_top(4);
    container.append(&gallery_label);

    let flow = FlowBox::new();
    flow.set_max_children_per_line(4);
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.add_css_class("image-gallery");
    flow.set_margin_start(12);
    flow.set_margin_end(12);
    flow.set_margin_bottom(8);

    for img in images {
        let item = GtkBox::new(Orientation::Vertical, 4);
        item.set_margin_start(4);
        item.set_margin_end(4);
        item.set_margin_top(4);
        item.set_margin_bottom(4);

        let picture = Picture::for_filename(&img.path);
        picture.set_content_fit(ContentFit::Contain);
        picture.set_size_request(150, 150);
        item.append(&picture);

        let filename = std::path::Path::new(&img.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| img.path.clone());
        let caption = if !img.caption.is_empty() {
            img.caption.clone()
        } else {
            filename
        };
        let label = Label::new(Some(&caption));
        label.set_max_width_chars(20);
        label.set_tooltip_text(Some(&img.path));
        item.append(&label);

        flow.insert(&item, -1);
    }
    container.append(&flow);
}

/// A floating document editing panel.
pub struct DocumentPanel;

impl DocumentPanel {
    /// Open a new document panel window.
    ///
    /// - `doc_id`, `title`, `content`: the document data
    /// - `active_doc`: shared state for the orchestrator bubble
    /// - `save_cb`: called with (doc_id, title, content_json) on save
    pub fn open(
        doc_id: &str,
        title: &str,
        content: &ContentFields,
        active_doc: Rc<RefCell<Option<ActiveDocument>>>,
        save_cb: Rc<dyn Fn(String, String, String)>,
        main_window: Option<&ApplicationWindow>,
    ) {
        let window = Window::builder()
            .title(&format!("Document — {}", title))
            .default_width(700)
            .default_height(500)
            .modal(false)
            .build();
        if let Some(main_win) = main_window {
            window.set_transient_for(Some(main_win));
        }
        window.add_css_class("document-panel");

        let vbox = GtkBox::new(Orientation::Vertical, 0);

        // Header: title entry + save button
        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_margin_start(12);
        header.set_margin_end(12);
        header.set_margin_top(8);
        header.set_margin_bottom(4);

        let title_entry = Entry::builder()
            .text(title)
            .hexpand(true)
            .placeholder_text("Document title")
            .build();
        title_entry.add_css_class("search-entry");
        header.append(&title_entry);

        let save_btn = Button::with_label("Save");
        save_btn.add_css_class("skill-button");
        header.append(&save_btn);

        vbox.append(&header);

        // Body: scrollable text editor
        let scrolled = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_height(200)
            .build();

        let text_view = TextView::builder()
            .wrap_mode(WrapMode::Word)
            .monospace(true)
            .left_margin(12)
            .right_margin(12)
            .top_margin(8)
            .bottom_margin(8)
            .build();
        text_view.add_css_class("markdown-editor");

        let buffer = text_view.buffer();
        buffer.set_text(&content.body);

        scrolled.set_child(Some(&text_view));
        vbox.append(&scrolled);

        // Image gallery — dynamic container rebuilt when images change
        let gallery_box = GtkBox::new(Orientation::Vertical, 0);
        rebuild_gallery(&gallery_box, &content.images);
        vbox.append(&gallery_box);

        window.set_child(Some(&vbox));

        // Wire up active document for orchestrator bubble
        let doc_id_owned = doc_id.to_string();
        let title_owned = title.to_string();
        let content_owned = content.clone();

        // Create a Rc to the text_view buffer for live body access
        let buffer_rc = Rc::new(buffer);

        {
            let get_body = {
                let buffer_rc = buffer_rc.clone();
                Rc::new(move || -> String {
                    let buf = &*buffer_rc;
                    let (start, end) = (buf.start_iter(), buf.end_iter());
                    buf.text(&start, &end, true).to_string()
                })
            };

            let on_images_changed = {
                let gallery_box = gallery_box.clone();
                Rc::new(move |images: &[ContentImage]| {
                    rebuild_gallery(&gallery_box, images);
                })
            };

            *active_doc.borrow_mut() = Some(ActiveDocument {
                doc_id: doc_id_owned.clone(),
                title: title_owned.clone(),
                content: content_owned,
                get_current_body: get_body,
                on_images_changed,
                panel_window: window.clone(),
            });
        }

        // Save button handler
        {
            let doc_id = doc_id_owned.clone();
            let title_entry = title_entry.clone();
            let buffer_rc = buffer_rc.clone();
            let save_cb = save_cb.clone();
            let active_doc = active_doc.clone();
            save_btn.connect_clicked(move |_| {
                let current_title = title_entry.text().to_string();
                let buf = &*buffer_rc;
                let (start, end) = (buf.start_iter(), buf.end_iter());
                let body = buf.text(&start, &end, true).to_string();

                // Preserve images from active_doc
                let images = active_doc
                    .borrow()
                    .as_ref()
                    .map(|ad| ad.content.images.clone())
                    .unwrap_or_default();

                let cf = ContentFields { body, images };
                save_cb(doc_id.clone(), current_title, cf.serialize());
                tracing::info!("Document saved: {}", doc_id);
            });
        }

        // On close: clear active document
        {
            let active_doc = active_doc.clone();
            window.connect_close_request(move |_| {
                *active_doc.borrow_mut() = None;
                gtk4::glib::Propagation::Proceed
            });
        }

        window.present();
    }
}
