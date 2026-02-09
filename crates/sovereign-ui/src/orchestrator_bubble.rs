use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Orientation, Overlay, Popover, PositionType};

use sovereign_core::content::{ContentFields, ContentImage};
use sovereign_skills::skills::image::ImageSkill;
use sovereign_skills::skills::pdf_export::PdfExportSkill;
use sovereign_skills::traits::{CoreSkill, SkillDocument, SkillOutput};

/// State for the currently open document, shared between panel and bubble.
#[derive(Clone)]
pub struct ActiveDocument {
    pub doc_id: String,
    pub title: String,
    pub content: ContentFields,
    pub get_current_body: Rc<dyn Fn() -> String>,
    /// Called when images are added/removed so the panel gallery can refresh.
    pub on_images_changed: Rc<dyn Fn(&[ContentImage])>,
    /// The document panel window, used as parent for file dialogs.
    pub panel_window: gtk4::Window,
}

/// Add the orchestrator bubble directly onto an Overlay.
///
/// The bubble is positioned via alignment + margins (no Fixed container)
/// so that events outside the bubble pass through to the canvas.
pub fn add_orchestrator_bubble(
    overlay: &Overlay,
    active_doc: Rc<RefCell<Option<ActiveDocument>>>,
    save_cb: Rc<dyn Fn(String, String, String)>,
) {
    let bubble = Label::new(Some("AI"));
    bubble.add_css_class("orchestrator-bubble");
    bubble.set_can_target(true);
    bubble.set_focusable(true);

    // Position top-left via alignment + margins
    bubble.set_halign(gtk4::Align::Start);
    bubble.set_valign(gtk4::Align::Start);
    bubble.set_margin_start(20);
    bubble.set_margin_top(20);

    overlay.add_overlay(&bubble);

    // Popover opens below the bubble
    let popover = Popover::new();
    popover.set_parent(&bubble);
    popover.set_position(PositionType::Bottom);
    popover.add_css_class("skill-popover");
    popover.set_autohide(false);

    let pop_box = GtkBox::new(Orientation::Vertical, 4);

    let save_btn = Button::with_label("Save");
    save_btn.add_css_class("skill-button");
    pop_box.append(&save_btn);

    let add_image_btn = Button::with_label("Add Image");
    add_image_btn.add_css_class("skill-button");
    pop_box.append(&add_image_btn);

    let export_pdf_btn = Button::with_label("Export PDF");
    export_pdf_btn.add_css_class("skill-button");
    pop_box.append(&export_pdf_btn);

    let status_label = Label::new(None);
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_margin_top(4);
    pop_box.append(&status_label);

    popover.set_child(Some(&pop_box));

    // Track drag vs click
    let dragged = Rc::new(RefCell::new(false));

    // Drag gesture — move bubble by updating margins
    let drag = gtk4::GestureDrag::new();
    drag.set_button(1);
    {
        let bubble_ref = bubble.clone();
        let dragged = dragged.clone();
        let start_margins = Rc::new(RefCell::new((0i32, 0i32)));

        let sm = start_margins.clone();
        let br = bubble_ref.clone();
        let dragged2 = dragged.clone();
        drag.connect_drag_begin(move |_, _, _| {
            *sm.borrow_mut() = (br.margin_start(), br.margin_top());
            *dragged2.borrow_mut() = false;
        });

        let sm = start_margins;
        let dragged3 = dragged.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            if dx.abs() > 5.0 || dy.abs() > 5.0 {
                *dragged3.borrow_mut() = true;
            }
            let (sx, sy) = *sm.borrow();
            bubble_ref.set_margin_start((sx + dx as i32).max(0));
            bubble_ref.set_margin_top((sy + dy as i32).max(0));
        });
    }
    bubble.add_controller(drag);

    // Click gesture — open popover (only if not a drag)
    let click = gtk4::GestureClick::new();
    click.set_button(1);
    {
        let popover = popover.clone();
        let active_doc = active_doc.clone();
        let save_btn = save_btn.clone();
        let add_image_btn = add_image_btn.clone();
        let export_pdf_btn = export_pdf_btn.clone();
        let status_label = status_label.clone();
        let dragged = dragged.clone();
        click.connect_released(move |_, _, _, _| {
            if *dragged.borrow() {
                return;
            }
            // Toggle popover
            if popover.is_visible() {
                popover.popdown();
                return;
            }
            let has_doc = active_doc.borrow().is_some();
            save_btn.set_sensitive(has_doc);
            add_image_btn.set_sensitive(has_doc);
            export_pdf_btn.set_sensitive(has_doc);
            if !has_doc {
                status_label.set_text("Open a document to use skills");
            } else {
                status_label.set_text("");
            }
            popover.popup();
        });
    }
    bubble.add_controller(click);

    // Save action
    {
        let active_doc = active_doc.clone();
        let save_cb = save_cb.clone();
        let popover = popover.clone();
        save_btn.connect_clicked(move |_| {
            if let Some(ref doc) = *active_doc.borrow() {
                let body = (doc.get_current_body)();
                let cf = ContentFields {
                    body,
                    images: doc.content.images.clone(),
                };
                save_cb(doc.doc_id.clone(), doc.title.clone(), cf.serialize());
                tracing::info!("Saved via orchestrator bubble: {}", doc.doc_id);
            }
            popover.popdown();
        });
    }

    // Add Image action
    {
        let active_doc = active_doc.clone();
        let save_cb = save_cb.clone();
        let popover = popover.clone();
        add_image_btn.connect_clicked(move |_| {
            popover.popdown();

            // Use the document panel as dialog parent so it appears on top
            let panel = active_doc.borrow().as_ref().map(|ad| ad.panel_window.clone());
            let active_doc = active_doc.clone();
            let save_cb = save_cb.clone();

            let dialog = gtk4::FileDialog::builder()
                .title("Select Image")
                .build();

            let filter = gtk4::FileFilter::new();
            filter.add_mime_type("image/*");
            filter.set_name(Some("Images"));
            let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));

            dialog.open(panel.as_ref(), gtk4::gio::Cancellable::NONE, move |result: Result<gtk4::gio::File, gtk4::glib::Error>| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        let path_str = path.to_string_lossy().to_string();

                        // Extract data (immutable borrow, then drop before borrow_mut)
                        let doc_data = {
                            let guard = active_doc.borrow();
                            guard.as_ref().map(|ad| {
                                let body = (ad.get_current_body)();
                                (ad.doc_id.clone(), ad.title.clone(), body, ad.content.images.clone())
                            })
                        };

                        if let Some((doc_id, title, body, images)) = doc_data {
                            let skill_doc = SkillDocument {
                                id: doc_id.clone(),
                                title: title.clone(),
                                content: ContentFields { body, images },
                            };
                            let skill = ImageSkill;
                            match skill.execute("add", &skill_doc, &path_str) {
                                Ok(SkillOutput::ContentUpdate(cf)) => {
                                    save_cb(doc_id, title, cf.serialize());
                                    // Update active_doc and refresh panel gallery
                                    if let Some(ref mut ad) = *active_doc.borrow_mut() {
                                        ad.content.images = cf.images.clone();
                                        (ad.on_images_changed)(&cf.images);
                                    }
                                    tracing::info!("Image added: {}", path_str);
                                }
                                Ok(_) => {}
                                Err(e) => tracing::error!("Add image failed: {e}"),
                            }
                        }
                    }
                }
            });
        });
    }

    // Export PDF action
    {
        let active_doc = active_doc.clone();
        let popover = popover.clone();
        export_pdf_btn.connect_clicked(move |_| {
            popover.popdown();

            // Extract data and panel window (immutable borrow, then drop)
            let doc_data = {
                let guard = active_doc.borrow();
                guard.as_ref().map(|ad| {
                    let body = (ad.get_current_body)();
                    let skill_doc = SkillDocument {
                        id: ad.doc_id.clone(),
                        title: ad.title.clone(),
                        content: ContentFields {
                            body,
                            images: ad.content.images.clone(),
                        },
                    };
                    (skill_doc, ad.panel_window.clone())
                })
            };

            if let Some((skill_doc, panel)) = doc_data {
                let skill = PdfExportSkill;
                match skill.execute("export", &skill_doc, "") {
                    Ok(SkillOutput::File { name, data, .. }) => {
                        let dialog = gtk4::FileDialog::builder()
                            .title("Save PDF")
                            .initial_name(&name)
                            .build();

                        let data = data.clone();
                        dialog.save(
                            Some(&panel),
                            gtk4::gio::Cancellable::NONE,
                            move |result: Result<gtk4::gio::File, gtk4::glib::Error>| {
                                if let Ok(file) = result {
                                    if let Some(path) = file.path() {
                                        match std::fs::write(path.as_path(), &data) {
                                            Ok(()) => tracing::info!(
                                                "PDF exported to {}",
                                                path.display()
                                            ),
                                            Err(e) => tracing::error!(
                                                "Failed to write PDF: {e}"
                                            ),
                                        }
                                    }
                                }
                            },
                        );
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!("PDF export failed: {e}"),
                }
            }
        });
    }
}
