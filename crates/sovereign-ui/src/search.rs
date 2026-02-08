use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Entry, Label, Orientation, ScrolledWindow};

/// Handle to the search overlay for updating results and voice status.
#[derive(Clone)]
pub struct SearchHandle {
    entry: Entry,
    results_box: GtkBox,
    _hint_label: Label,
    voice_label: Label,
}

impl SearchHandle {
    /// Set the search entry text (used by voice transcription).
    pub fn set_text(&self, text: &str) {
        self.entry.set_text(text);
        self.entry.set_position(-1); // cursor to end
    }

    /// Show search results as a list of document IDs/titles.
    pub fn show_results(&self, doc_ids: &[String]) {
        // Clear previous results
        while let Some(child) = self.results_box.first_child() {
            self.results_box.remove(&child);
        }

        if doc_ids.is_empty() {
            let label = Label::new(Some("No documents found"));
            label.add_css_class("search-result-empty");
            self.results_box.append(&label);
        } else {
            for id in doc_ids {
                let label = Label::new(Some(id));
                label.add_css_class("search-result-item");
                label.set_halign(gtk4::Align::Start);
                self.results_box.append(&label);
            }
        }

        self.results_box.set_visible(true);
    }

    /// Update voice status indicator.
    pub fn show_listening(&self) {
        self.voice_label.set_text("Listening...");
        self.voice_label.set_visible(true);
    }

    /// Update voice status to transcribing.
    pub fn show_transcribing(&self) {
        self.voice_label.set_text("Transcribing...");
        self.voice_label.set_visible(true);
    }

    /// Clear voice status.
    pub fn hide_voice_status(&self) {
        self.voice_label.set_visible(false);
    }
}

/// Build the search overlay widget and return the overlay box + a SearchHandle.
pub fn build_search_overlay(
    query_callback: Option<Rc<dyn Fn(String)>>,
) -> (GtkBox, SearchHandle) {
    let overlay_box = GtkBox::new(Orientation::Vertical, 8);
    overlay_box.add_css_class("search-overlay");
    overlay_box.set_halign(gtk4::Align::Center);
    overlay_box.set_valign(gtk4::Align::Start);
    overlay_box.set_visible(false);

    let entry = Entry::new();
    entry.set_placeholder_text(Some("Search documents..."));
    entry.add_css_class("search-entry");
    overlay_box.append(&entry);

    // Voice status label (hidden by default)
    let voice_label = Label::new(None);
    voice_label.add_css_class("search-hint");
    voice_label.set_visible(false);
    overlay_box.append(&voice_label);

    // Results box
    let results_box = GtkBox::new(Orientation::Vertical, 4);
    results_box.add_css_class("search-results");
    results_box.set_visible(false);

    let scrolled = ScrolledWindow::new();
    scrolled.set_child(Some(&results_box));
    scrolled.set_max_content_height(300);
    scrolled.set_propagate_natural_height(true);
    overlay_box.append(&scrolled);

    // Hint label
    let hint_text = if query_callback.is_some() {
        "Press Enter to search"
    } else {
        "AI unavailable — search not connected"
    };
    let hint = Label::new(Some(hint_text));
    hint.add_css_class("search-hint");
    overlay_box.append(&hint);

    // Wire up Enter key to submit query
    if let Some(cb) = query_callback {
        entry.connect_activate(move |e| {
            let text = e.text().to_string();
            if !text.is_empty() {
                cb(text);
            }
        });
    }

    let handle = SearchHandle {
        entry,
        results_box,
        _hint_label: hint,
        voice_label,
    };

    (overlay_box, handle)
}
