use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use sovereign_core::interfaces::{CanvasController, Viewport};
use crate::state::CanvasFilter;

/// Commands sent from CanvasController to the GTK main loop.
pub enum CanvasCommand {
    NavigateTo(String),
    Highlight(String, bool),
    ZoomToThread(String),
    GoHome,
    /// Jump the camera to the closest timeline marker matching the date string.
    JumpToDate(String),
    /// Apply a filter to control which cards are shown.
    SetFilter(CanvasFilter),
    /// Toggle the minimap overlay.
    ToggleMinimap,
    /// Start the adoption (external→owned) animation for a card.
    AnimateAdoption(String),
}

/// Thread-safe CanvasController implementation.
///
/// Uses a `mpsc::Sender` (Send) to dispatch commands that are polled
/// from the GTK tick callback, and `Arc<Mutex<Viewport>>` for viewport state.
pub struct SovereignCanvasController {
    sender: mpsc::Sender<CanvasCommand>,
    viewport: Arc<Mutex<Viewport>>,
}

impl SovereignCanvasController {
    pub fn new(sender: mpsc::Sender<CanvasCommand>, viewport: Arc<Mutex<Viewport>>) -> Self {
        Self { sender, viewport }
    }
}

impl CanvasController for SovereignCanvasController {
    fn navigate_to_document(&self, doc_id: &str) {
        let _ = self.sender.send(CanvasCommand::NavigateTo(doc_id.to_string()));
    }

    fn highlight_card(&self, doc_id: &str, highlight: bool) {
        let _ = self
            .sender
            .send(CanvasCommand::Highlight(doc_id.to_string(), highlight));
    }

    fn zoom_to_thread(&self, thread_id: &str) {
        let _ = self
            .sender
            .send(CanvasCommand::ZoomToThread(thread_id.to_string()));
    }

    fn get_viewport(&self) -> Viewport {
        self.viewport
            .lock()
            .map(|v| v.clone())
            .unwrap_or(Viewport {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
                width: 1280.0,
                height: 720.0,
            })
    }
}
