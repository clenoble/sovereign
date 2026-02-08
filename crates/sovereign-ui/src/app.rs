use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, CssProvider, Orientation, Overlay};

use sovereign_core::config::UiConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_db::schema::{Document, Thread};

use crate::search::build_search_overlay;
use crate::taskbar::build_taskbar;
use crate::theme::DARK_THEME_CSS;

/// Voice event type mirrored here so sovereign-ui doesn't depend on sovereign-ai.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    WakeWordDetected,
    ListeningStarted,
    TranscriptionReady(String),
    ListeningStopped,
    TtsSpeaking(String),
    TtsDone,
}

pub fn build_app(
    config: &UiConfig,
    documents: Vec<Document>,
    threads: Vec<Thread>,
    query_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
    orchestrator_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
    voice_rx: Option<mpsc::Receiver<VoiceEvent>>,
) {
    let app = Application::builder()
        .application_id("org.sovereign.os")
        .build();

    let width = config.default_width;
    let height = config.default_height;

    // Wrap move-once values in RefCell so the Fn closure can take them
    let query_cb_cell = RefCell::new(query_callback);
    let orch_rx_cell = RefCell::new(orchestrator_rx);
    let voice_rx_cell = RefCell::new(voice_rx);

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

        let (gl_area, controller) =
            sovereign_canvas::build_canvas(documents.clone(), threads.clone());
        overlay.set_child(Some(&gl_area));

        // Take query callback (only on first activation)
        let query_cb = query_cb_cell.borrow_mut().take();
        let query_rc: Option<Rc<dyn Fn(String)>> =
            query_cb.map(|cb| Rc::new(move |text: String| cb(text)) as Rc<dyn Fn(String)>);

        let (search_box, search_handle) = build_search_overlay(query_rc);
        overlay.add_overlay(&search_box);

        vbox.append(&overlay);

        // Taskbar
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

        // Event polling via tick callback
        let orch_rx = orch_rx_cell.borrow_mut().take();
        let v_rx = voice_rx_cell.borrow_mut().take();
        let sh = search_handle.clone();

        if orch_rx.is_some() || v_rx.is_some() {
            gl_area.add_tick_callback(move |_area, _| {
                if let Some(ref rx) = orch_rx {
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            OrchestratorEvent::SearchResults { ref doc_ids, .. } => {
                                sh.show_results(doc_ids);
                                for id in doc_ids {
                                    controller.highlight_card(id, true);
                                }
                            }
                            OrchestratorEvent::DocumentOpened { ref doc_id } => {
                                controller.navigate_to_document(doc_id);
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(ref rx) = v_rx {
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            VoiceEvent::TranscriptionReady(ref text) => {
                                sh.set_text(text);
                                sh.hide_voice_status();
                            }
                            VoiceEvent::WakeWordDetected | VoiceEvent::ListeningStarted => {
                                sh.show_listening();
                            }
                            VoiceEvent::ListeningStopped => {
                                sh.show_transcribing();
                            }
                            VoiceEvent::TtsDone => {
                                sh.hide_voice_status();
                            }
                            _ => {}
                        }
                    }
                }
                gtk4::glib::ControlFlow::Continue
            });
        }

        window.set_child(Some(&vbox));
        window.present();
    });

    app.run_with_args::<String>(&[]);
}
