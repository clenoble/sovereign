pub mod camera;
pub mod colors;
pub mod controller;
pub mod layout;
pub mod lod;
pub mod minimap;
pub mod renderer;
pub mod state;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use sovereign_core::interfaces::{CanvasController, Viewport};
use sovereign_db::schema::{Document, RelatedTo, Thread};

use camera::home_position;
use controller::{CanvasCommand, SovereignCanvasController};
use layout::compute_layout_with_edges;
use renderer::CanvasProgram;
use state::CanvasState;

/// Build the spatial canvas and its controller.
///
/// Returns:
/// - `CanvasProgram` — the Iced shader program (clone for view())
/// - `mpsc::Receiver<CanvasCommand>` — polled by the app's update() to apply commands
/// - `Box<dyn CanvasController>` — thread-safe controller for orchestrator/AI
pub fn build_canvas(
    documents: Vec<Document>,
    threads: Vec<Thread>,
    relationships: Vec<RelatedTo>,
) -> (CanvasProgram, mpsc::Receiver<CanvasCommand>, Box<dyn CanvasController>) {
    let canvas_layout = compute_layout_with_edges(&documents, &threads, &relationships);
    let (home_x, home_y) = home_position(&canvas_layout);

    let viewport = Arc::new(Mutex::new(Viewport {
        x: home_x,
        y: home_y,
        zoom: 1.0,
        width: 1280.0,
        height: 720.0,
    }));

    let mut initial_state = CanvasState::new(canvas_layout);
    initial_state.camera.pan_x = home_x;
    initial_state.camera.pan_y = home_y;
    let state = Arc::new(Mutex::new(initial_state));

    let (sender, receiver) = mpsc::channel::<CanvasCommand>();

    let program = CanvasProgram {
        state: state.clone(),
    };

    let controller = SovereignCanvasController::new(sender, viewport);

    (program, receiver, Box::new(controller))
}

/// Apply a canvas command to the state. Called from the app's update() loop.
pub fn apply_command(state: &Arc<Mutex<CanvasState>>, cmd: CanvasCommand) {
    let mut st = state.lock().unwrap();
    match cmd {
        CanvasCommand::NavigateTo(doc_id) => {
            if let Some(idx) = st.layout.cards.iter().position(|c| c.doc_id == doc_id) {
                let cx = st.layout.cards[idx].x;
                let cy = st.layout.cards[idx].y;
                st.camera.pan_x = cx as f64 - 400.0;
                st.camera.pan_y = cy as f64 - 200.0;
                st.selected = Some(idx);
            }
        }
        CanvasCommand::Highlight(doc_id, on) => {
            if on {
                st.highlighted.insert(doc_id);
            } else {
                st.highlighted.remove(&doc_id);
            }
        }
        CanvasCommand::ZoomToThread(thread_id) => {
            if let Some(lane) = st.layout.lanes.iter().find(|l| l.thread_id == thread_id) {
                let lane_y = lane.y;
                st.camera.pan_x = -100.0;
                st.camera.pan_y = lane_y as f64 - 50.0;
                st.camera.zoom = 1.0;
            }
        }
        CanvasCommand::GoHome => {
            let (hx, hy) = home_position(&st.layout);
            st.camera.pan_x = hx;
            st.camera.pan_y = hy;
            st.camera.zoom = 1.0;
        }
        CanvasCommand::JumpToDate(ref date_str) => {
            let needle = date_str.to_lowercase();
            if let Some(marker) = st
                .layout
                .timeline_markers
                .iter()
                .find(|m| m.label.to_lowercase().contains(&needle))
            {
                st.camera.pan_x = marker.x as f64 - 200.0;
                st.camera.zoom = 1.0;
            }
        }
        CanvasCommand::SetFilter(filter) => {
            st.filter = filter;
        }
        CanvasCommand::ToggleMinimap => {
            st.minimap_visible = !st.minimap_visible;
        }
        CanvasCommand::AnimateAdoption(doc_id) => {
            st.adoption_animations
                .insert(doc_id, crate::state::AdoptionAnim::new());
        }
    }
    st.mark_dirty();
}
