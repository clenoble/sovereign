use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, CssProvider, Label, Orientation, Overlay,
};

use sovereign_core::config::UiConfig;

use crate::search::build_search_overlay;
use crate::taskbar::build_taskbar;
use crate::theme::DARK_THEME_CSS;

pub fn build_app(config: &UiConfig) {
    let app = Application::builder()
        .application_id("org.sovereign.os")
        .build();

    let width = config.default_width;
    let height = config.default_height;

    app.connect_activate(move |app| {
        // Load dark theme CSS
        let provider = CssProvider::new();
        provider.load_from_data(DARK_THEME_CSS);
        gtk4::style_context_add_provider_for_display(
            &Display::default().expect("Could not get default display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let window = ApplicationWindow::builder()
            .application(app)
            .title("Sovereign OS")
            .default_width(width)
            .default_height(height)
            .build();

        // Main vertical layout
        let vbox = GtkBox::new(Orientation::Vertical, 0);

        // Canvas area with overlay for search
        let overlay = Overlay::new();
        let canvas_placeholder = Label::new(Some("Canvas — Phase 2 will render here"));
        canvas_placeholder.add_css_class("canvas-placeholder");
        canvas_placeholder.set_vexpand(true);
        canvas_placeholder.set_hexpand(true);
        overlay.set_child(Some(&canvas_placeholder));

        // Search overlay (hidden by default)
        let search_box = build_search_overlay();
        overlay.add_overlay(&search_box);

        vbox.append(&overlay);

        // Taskbar at bottom
        let search_box_toggle = search_box.clone();
        let taskbar = build_taskbar(move || {
            let visible = search_box_toggle.is_visible();
            search_box_toggle.set_visible(!visible);
        });
        vbox.append(&taskbar);

        // Keyboard shortcut: Ctrl+F for search, Escape to close
        let search_box_key = search_box.clone();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
            let ctrl = modifier.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            if ctrl && keyval == gtk4::gdk::Key::f {
                search_box_key.set_visible(true);
                // Focus the entry inside the search box
                if let Some(entry) = search_box_key.first_child() {
                    entry.grab_focus();
                }
                return gtk4::glib::Propagation::Stop;
            }
            if keyval == gtk4::gdk::Key::Escape {
                search_box_key.set_visible(false);
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        window.add_controller(key_controller);

        window.set_child(Some(&vbox));
        window.present();
    });

    app.run_with_args::<String>(&[]);
}
