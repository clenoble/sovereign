pub mod camera;
pub mod colors;
pub mod controller;
pub mod gl_loader;
pub mod layout;
pub mod renderer;
pub mod state;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use gtk4::prelude::*;
use sovereign_core::interfaces::{CanvasController, Viewport};
use sovereign_db::schema::{Document, Thread};

use controller::{CanvasCommand, SovereignCanvasController};
use layout::compute_layout;
use renderer::create_gl_area;
use state::CanvasState;

/// Build the spatial canvas widget and its controller.
///
/// Call this inside `connect_activate` on the GTK main thread.
/// Returns the GLArea widget to embed in the UI and a boxed CanvasController
/// that can be sent to other threads (AI orchestrator, skills).
pub fn build_canvas(
    documents: Vec<Document>,
    threads: Vec<Thread>,
) -> (gtk4::GLArea, Box<dyn CanvasController>) {
    let canvas_layout = compute_layout(&documents, &threads);

    let viewport = Arc::new(Mutex::new(Viewport {
        x: -200.0,
        y: -100.0,
        zoom: 1.0,
        width: 1280.0,
        height: 720.0,
    }));

    let state = Rc::new(RefCell::new(CanvasState::new(canvas_layout)));

    let (sender, receiver) = mpsc::channel::<CanvasCommand>();
    let receiver = Rc::new(RefCell::new(receiver));

    let gl_area = create_gl_area(state.clone(), viewport.clone());

    // Poll commands from the tick callback (already runs every frame)
    {
        let state = state.clone();
        let gl_area_cmd = gl_area.clone();
        let receiver = receiver.clone();
        gl_area.add_tick_callback(move |_, _| {
            let rx = receiver.borrow();
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    CanvasCommand::NavigateTo(doc_id) => {
                        let mut st = state.borrow_mut();
                        if let Some(idx) = st.layout.cards.iter().position(|c| c.doc_id == doc_id)
                        {
                            let cx = st.layout.cards[idx].x;
                            let cy = st.layout.cards[idx].y;
                            st.camera.pan_x = cx as f64 - 400.0;
                            st.camera.pan_y = cy as f64 - 200.0;
                            st.selected = Some(idx);
                        }
                    }
                    CanvasCommand::Highlight(doc_id, on) => {
                        let mut st = state.borrow_mut();
                        if on {
                            if !st.highlighted.contains(&doc_id) {
                                st.highlighted.push(doc_id);
                            }
                        } else {
                            st.highlighted.retain(|id| id != &doc_id);
                        }
                    }
                    CanvasCommand::ZoomToThread(thread_id) => {
                        let mut st = state.borrow_mut();
                        if let Some(lane) =
                            st.layout.lanes.iter().find(|l| l.thread_id == thread_id)
                        {
                            let lane_y = lane.y;
                            st.camera.pan_x = -100.0;
                            st.camera.pan_y = lane_y as f64 - 50.0;
                            st.camera.zoom = 1.0;
                        }
                    }
                }
                gl_area_cmd.queue_draw();
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    let controller = SovereignCanvasController::new(sender, viewport);

    (gl_area, Box::new(controller))
}
