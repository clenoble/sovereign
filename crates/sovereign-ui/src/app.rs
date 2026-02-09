use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use gtk4::gdk::Display;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, CssProvider, Orientation, Overlay};

use sovereign_core::config::UiConfig;
use sovereign_core::content::ContentFields;
use sovereign_core::interfaces::{OrchestratorEvent, SkillEvent};
use sovereign_db::schema::{Document, Thread};

use crate::orchestrator_bubble::{add_orchestrator_bubble, ActiveDocument};
use crate::panels::document_panel::DocumentPanel;
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
    skill_rx: Option<mpsc::Receiver<SkillEvent>>,
    save_callback: Option<Box<dyn Fn(String, String, String) + Send + 'static>>,
    close_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
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
    let skill_rx_cell = RefCell::new(skill_rx);
    let save_cb_cell = RefCell::new(save_callback);
    let close_cb_cell = RefCell::new(close_callback);

    // Build a local doc HashMap for lookups
    let doc_map: HashMap<String, Document> = documents
        .iter()
        .filter_map(|d| d.id_string().map(|id| (id, d.clone())))
        .collect();
    let doc_map = Rc::new(RefCell::new(doc_map));

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

        // Canvas area with overlay for search + bubble
        let overlay = Overlay::new();

        // Create skill_tx channel for canvas double-click events
        let (skill_tx, skill_rx_canvas) = mpsc::channel::<SkillEvent>();

        let (gl_area, controller) =
            sovereign_canvas::build_canvas(documents.clone(), threads.clone(), Some(skill_tx));
        overlay.set_child(Some(&gl_area));

        // Take query callback (only on first activation)
        let query_cb = query_cb_cell.borrow_mut().take();
        let query_rc: Option<Rc<dyn Fn(String)>> =
            query_cb.map(|cb| Rc::new(move |text: String| cb(text)) as Rc<dyn Fn(String)>);

        let (search_box, search_handle) = build_search_overlay(query_rc);
        overlay.add_overlay(&search_box);

        // Shared active document state
        let active_doc: Rc<RefCell<Option<ActiveDocument>>> = Rc::new(RefCell::new(None));

        // Save callback wrapped in Rc for GTK use
        let save_cb_taken = save_cb_cell.borrow_mut().take();
        let save_rc: Rc<dyn Fn(String, String, String)> = match save_cb_taken {
            Some(cb) => {
                let doc_map = doc_map.clone();
                Rc::new(move |doc_id: String, title: String, content: String| {
                    // Update local map
                    if let Some(doc) = doc_map.borrow_mut().get_mut(&doc_id) {
                        doc.title = title.clone();
                        doc.content = content.clone();
                    }
                    cb(doc_id, title, content);
                })
            }
            None => Rc::new(|_, _, _| {}),
        };

        // Close callback wrapped in Rc
        let close_cb_taken = close_cb_cell.borrow_mut().take();
        let close_rc: Rc<dyn Fn(String)> = match close_cb_taken {
            Some(cb) => Rc::new(move |doc_id: String| cb(doc_id)),
            None => Rc::new(|_| {}),
        };

        // Orchestrator bubble — added directly onto overlay (no Fixed container)
        add_orchestrator_bubble(
            &overlay,
            active_doc.clone(),
            save_rc.clone(),
        );

        vbox.append(&overlay);

        // Taskbar — pick the first owned document as the pinned item
        let pinned_docs: Vec<(String, String, bool)> = documents
            .iter()
            .filter(|d| d.is_owned)
            .take(1)
            .filter_map(|d| d.id_string().map(|id| (id, d.title.clone(), d.is_owned)))
            .collect();

        let (taskbar_skill_tx, taskbar_skill_rx) = mpsc::channel::<SkillEvent>();

        let search_box_toggle = search_box.clone();
        let (taskbar, taskbar_handle) = build_taskbar(
            move || {
                let visible = search_box_toggle.is_visible();
                search_box_toggle.set_visible(!visible);
            },
            pinned_docs,
            taskbar_skill_tx,
        );
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
        let ext_skill_rx = skill_rx_cell.borrow_mut().take();
        let sh = search_handle.clone();

        // Merge the external skill_rx with the canvas-internal one
        let doc_map_poll = doc_map.clone();
        let active_doc_poll = active_doc.clone();
        let save_rc_poll = save_rc.clone();
        let close_rc_poll = close_rc.clone();

        gl_area.add_tick_callback(move |_area, _| {
            // Poll orchestrator events
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
                        OrchestratorEvent::ThreadCreated { ref thread_id, ref name } => {
                            tracing::info!("UI: Thread created: {} ({})", name, thread_id);
                        }
                        OrchestratorEvent::ThreadRenamed { ref thread_id, ref name } => {
                            tracing::info!("UI: Thread renamed: {} → {}", thread_id, name);
                        }
                        OrchestratorEvent::ThreadDeleted { ref thread_id } => {
                            tracing::info!("UI: Thread deleted: {}", thread_id);
                        }
                        OrchestratorEvent::DocumentMoved { ref doc_id, ref new_thread_id } => {
                            tracing::info!("UI: Document {} moved to {}", doc_id, new_thread_id);
                        }
                        OrchestratorEvent::VersionHistory { ref doc_id, ref commits } => {
                            tracing::info!("UI: Version history for {}: {} commits", doc_id, commits.len());
                            for c in commits {
                                tracing::info!("  {} — {} ({})", c.id, c.message, c.timestamp);
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Poll voice events
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

            // Poll skill events (from canvas double-click + taskbar clicks)
            for rx in [
                Some(&skill_rx_canvas),
                ext_skill_rx.as_ref(),
                Some(&taskbar_skill_rx),
            ] {
                if let Some(rx) = rx {
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            SkillEvent::OpenDocument { ref doc_id } => {
                                if let Some(doc) = doc_map_poll.borrow().get(doc_id) {
                                    let content = ContentFields::parse(&doc.content);
                                    DocumentPanel::open(
                                        doc_id,
                                        &doc.title,
                                        &content,
                                        active_doc_poll.clone(),
                                        save_rc_poll.clone(),
                                    );
                                    taskbar_handle.add_document(
                                        doc_id,
                                        &doc.title,
                                        doc.is_owned,
                                    );
                                }
                            }
                            SkillEvent::DocumentClosed { ref doc_id } => {
                                tracing::info!("Document closed: {}", doc_id);
                                close_rc_poll(doc_id.clone());
                            }
                        }
                    }
                }
            }

            gtk4::glib::ControlFlow::Continue
        });

        window.set_child(Some(&vbox));
        window.present();
    });

    app.run_with_args::<String>(&[]);
}
