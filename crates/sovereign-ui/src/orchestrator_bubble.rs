use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, Button, Label, Orientation, Overlay};

use sovereign_core::content::{ContentFields, ContentImage};
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_core::security::{ActionDecision, BubbleVisualState};
use sovereign_skills::registry::SkillRegistry;
use sovereign_skills::traits::{SkillDocument, SkillOutput};

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

/// CSS class names for each bubble visual state.
const BUBBLE_STATE_CLASSES: &[&str] = &[
    "bubble-idle",
    "bubble-processing-owned",
    "bubble-processing-external",
    "bubble-proposing",
    "bubble-executing",
    "bubble-suggesting",
];

/// Swap bubble CSS classes to reflect the current visual state.
pub fn set_bubble_state(bubble: &Label, state: BubbleVisualState) {
    for cls in BUBBLE_STATE_CLASSES {
        bubble.remove_css_class(cls);
    }
    let cls = match state {
        BubbleVisualState::Idle => "bubble-idle",
        BubbleVisualState::ProcessingOwned => "bubble-processing-owned",
        BubbleVisualState::ProcessingExternal => "bubble-processing-external",
        BubbleVisualState::Proposing => "bubble-proposing",
        BubbleVisualState::Executing => "bubble-executing",
        BubbleVisualState::Suggesting => "bubble-suggesting",
    };
    bubble.add_css_class(cls);
}

/// Handle returned from `add_orchestrator_bubble` for event-driven updates.
#[derive(Clone)]
pub struct BubbleHandle {
    pub bubble: Label,
    pub confirmation_panel: GtkBox,
    pub confirmation_label: Label,
    pub rejection_toast: Label,
    pub status_label: Label,
    pub suggestion_tooltip: Label,
    /// Tracks the action name of the currently displayed suggestion (for feedback).
    current_suggestion_action: Rc<RefCell<Option<String>>>,
}

impl BubbleHandle {
    /// Show the confirmation sub-panel with a description of the proposed action.
    pub fn show_confirmation(&self, description: &str) {
        self.confirmation_label.set_text(description);
        self.confirmation_panel.set_visible(true);
        self.rejection_toast.set_visible(false);
    }

    /// Hide the confirmation sub-panel.
    pub fn hide_confirmation(&self) {
        self.confirmation_panel.set_visible(false);
    }

    /// Show a brief rejection toast message.
    pub fn show_rejection(&self, reason: &str) {
        self.rejection_toast.set_text(reason);
        self.rejection_toast.set_visible(true);
        self.confirmation_panel.set_visible(false);
    }

    pub fn set_state(&self, state: BubbleVisualState) {
        set_bubble_state(&self.bubble, state);
    }

    /// Show a skill result in the status label.
    pub fn show_skill_result(&self, text: &str) {
        self.status_label.set_text(text);
    }

    /// Show a proactive suggestion tooltip near the bubble.
    /// Stores the action name so dismiss/accept feedback can reference it.
    pub fn show_suggestion(&self, text: &str, action: &str) {
        self.suggestion_tooltip.set_text(text);
        self.suggestion_tooltip.set_visible(true);
        *self.current_suggestion_action.borrow_mut() = Some(action.to_string());
        set_bubble_state(&self.bubble, BubbleVisualState::Suggesting);
    }

    /// Dismiss the suggestion tooltip and return to idle.
    pub fn dismiss_suggestion(&self) {
        self.suggestion_tooltip.set_visible(false);
        *self.current_suggestion_action.borrow_mut() = None;
        set_bubble_state(&self.bubble, BubbleVisualState::Idle);
    }
}

/// Position the skills panel near the bubble, clamped to stay within the overlay.
fn position_panel(bubble: &Label, panel: &GtkBox, overlay: &Overlay) {
    let bx = bubble.margin_start();
    let by = bubble.margin_top();
    let bh = bubble.height();
    let ow = overlay.width();
    let oh = overlay.height();

    // Measure the panel's natural size (works even before first render)
    let (_, nat_w, _, _) = panel.measure(Orientation::Horizontal, -1);
    let (_, nat_h, _, _) = panel.measure(Orientation::Vertical, nat_w);
    let pw = if nat_w > 0 { nat_w } else { 160 };
    let ph = if nat_h > 0 { nat_h } else { 180 };
    let gap = 8;

    // Default: below the bubble, left-aligned
    let mut x = bx;
    let mut y = by + bh + gap;

    // Clamp horizontal: keep panel within overlay
    if ow > 0 && x + pw > ow {
        x = (ow - pw - gap).max(gap);
    }

    // If below goes off-screen, show above the bubble
    if oh > 0 && y + ph > oh {
        y = (by - ph - gap).max(gap);
    }

    panel.set_margin_start(x.max(0));
    panel.set_margin_top(y.max(0));
}

/// Build a SkillDocument from the active document state.
fn build_skill_doc(ad: &ActiveDocument) -> SkillDocument {
    let body = (ad.get_current_body)();
    SkillDocument {
        id: ad.doc_id.clone(),
        title: ad.title.clone(),
        content: ContentFields {
            body,
            images: ad.content.images.clone(),
        },
    }
}

/// Add the orchestrator bubble directly onto an Overlay.
///
/// The bubble is positioned via alignment + margins (no Fixed container)
/// so that events outside the bubble pass through to the canvas.
/// The skills panel is an in-overlay widget (not a Popover) so it always
/// stays within the canvas bounds regardless of bubble position.
pub fn add_orchestrator_bubble(
    overlay: &Overlay,
    active_doc: Rc<RefCell<Option<ActiveDocument>>>,
    save_cb: Rc<dyn Fn(String, String, String)>,
    decision_tx: Option<mpsc::Sender<ActionDecision>>,
    registry: Rc<SkillRegistry>,
    feedback_tx: Option<mpsc::Sender<FeedbackEvent>>,
) -> BubbleHandle {
    let bubble = Label::new(Some("AI"));
    bubble.add_css_class("orchestrator-bubble");
    bubble.add_css_class("bubble-idle");
    bubble.set_can_target(true);
    bubble.set_focusable(true);

    // Position top-left via alignment + margins
    bubble.set_halign(gtk4::Align::Start);
    bubble.set_valign(gtk4::Align::Start);
    bubble.set_margin_start(20);
    bubble.set_margin_top(20);

    overlay.add_overlay(&bubble);

    // Confirmation sub-panel (below skills panel)
    let confirm_panel = GtkBox::new(Orientation::Vertical, 4);
    confirm_panel.add_css_class("confirmation-panel");
    confirm_panel.set_halign(gtk4::Align::Start);
    confirm_panel.set_valign(gtk4::Align::Start);
    confirm_panel.set_margin_start(20);
    confirm_panel.set_margin_top(90);
    confirm_panel.set_visible(false);

    let confirm_label = Label::new(None);
    confirm_label.add_css_class("confirmation-label");
    confirm_label.set_halign(gtk4::Align::Start);
    confirm_label.set_wrap(true);
    confirm_label.set_max_width_chars(40);
    confirm_panel.append(&confirm_label);

    let button_row = GtkBox::new(Orientation::Horizontal, 4);
    let approve_btn = Button::with_label("Approve");
    approve_btn.add_css_class("approve-button");
    let reject_btn = Button::with_label("Reject");
    reject_btn.add_css_class("reject-button");
    button_row.append(&approve_btn);
    button_row.append(&reject_btn);
    confirm_panel.append(&button_row);

    overlay.add_overlay(&confirm_panel);

    // Rejection toast
    let rejection_toast = Label::new(None);
    rejection_toast.add_css_class("rejection-toast");
    rejection_toast.set_halign(gtk4::Align::Start);
    rejection_toast.set_valign(gtk4::Align::Start);
    rejection_toast.set_margin_start(20);
    rejection_toast.set_margin_top(90);
    rejection_toast.set_visible(false);
    overlay.add_overlay(&rejection_toast);

    // Wire approve/reject buttons to decision channel
    {
        let tx = decision_tx.clone();
        let cp = confirm_panel.clone();
        approve_btn.connect_clicked(move |_| {
            if let Some(ref tx) = tx {
                let _ = tx.send(ActionDecision::Approve);
            }
            cp.set_visible(false);
        });
    }
    {
        let tx = decision_tx;
        let cp = confirm_panel.clone();
        reject_btn.connect_clicked(move |_| {
            if let Some(ref tx) = tx {
                let _ = tx.send(ActionDecision::Reject("User rejected".into()));
            }
            cp.set_visible(false);
        });
    }

    // Skills panel — dynamically generated from registry
    let skills_panel = GtkBox::new(Orientation::Vertical, 4);
    skills_panel.add_css_class("skill-panel");
    skills_panel.set_halign(gtk4::Align::Start);
    skills_panel.set_valign(gtk4::Align::Start);
    skills_panel.set_visible(false);

    let status_label = Label::new(None);
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_margin_top(4);
    status_label.set_wrap(true);
    status_label.set_max_width_chars(30);

    // Collect all buttons so we can enable/disable them
    let all_buttons: Rc<RefCell<Vec<Button>>> = Rc::new(RefCell::new(Vec::new()));

    // Generate buttons from registry
    for skill in registry.all_skills() {
        let skill_name = skill.name().to_string();
        for (action_id, action_label) in skill.actions() {
            let btn = Button::with_label(&action_label);
            btn.add_css_class("skill-button");
            skills_panel.append(&btn);
            all_buttons.borrow_mut().push(btn.clone());

            let skill_name = skill_name.clone();
            let action_id = action_id.clone();
            let active_doc = active_doc.clone();
            let save_cb = save_cb.clone();
            let skills_panel_ref = skills_panel.clone();
            let status_label_ref = status_label.clone();
            let registry = registry.clone();

            btn.connect_clicked(move |_| {
                // Special case: "add" (image) and "import" need file dialog
                if (skill_name == "image" && action_id == "add")
                    || (skill_name == "file-import" && action_id == "import")
                {
                    skills_panel_ref.set_visible(false);
                    handle_file_dialog_action(
                        &skill_name,
                        &action_id,
                        &active_doc,
                        &save_cb,
                        &registry,
                    );
                    return;
                }

                // Special case: "export" (PDF) needs save dialog
                if skill_name == "pdf-export" && action_id == "export" {
                    skills_panel_ref.set_visible(false);
                    handle_export_action(&active_doc, &registry);
                    return;
                }

                // Special case: "find_replace" and "search" need text entry
                // For now, show a hint; in future, open a dialog
                if action_id == "find_replace" || action_id == "search" {
                    status_label_ref.set_text(&format!("Use search bar for {}", action_id.replace('_', " ")));
                    return;
                }

                // Default: immediate execution
                let doc_data = {
                    let guard = active_doc.borrow();
                    guard.as_ref().map(|ad| build_skill_doc(ad))
                };

                if let Some(skill_doc) = doc_data {
                    if let Some(skill) = registry.find_skill(&skill_name) {
                        match skill.execute(&action_id, &skill_doc, "") {
                            Ok(SkillOutput::ContentUpdate(cf)) => {
                                let doc_id = skill_doc.id.clone();
                                let title = skill_doc.title.clone();
                                save_cb(doc_id, title, cf.serialize());
                                // Update images if they changed
                                if let Some(ref mut ad) = *active_doc.borrow_mut() {
                                    if ad.content.images != cf.images {
                                        ad.content.images = cf.images.clone();
                                        (ad.on_images_changed)(&cf.images);
                                    }
                                }
                                status_label_ref.set_text("Done");
                            }
                            Ok(SkillOutput::StructuredData { kind, json }) => {
                                let display = format_structured_data(&kind, &json);
                                status_label_ref.set_text(&display);
                            }
                            Ok(SkillOutput::File { .. }) => {
                                status_label_ref.set_text("File generated");
                            }
                            Ok(SkillOutput::None) => {
                                status_label_ref.set_text("Done");
                            }
                            Err(e) => {
                                status_label_ref.set_text(&format!("Error: {e}"));
                            }
                        }
                    }
                } else {
                    status_label_ref.set_text("Open a document first");
                }
            });
        }
    }

    skills_panel.append(&status_label);
    overlay.add_overlay(&skills_panel);

    // Suggestion tooltip — dismissible label for proactive AI suggestions
    let suggestion_tooltip = Label::new(None);
    suggestion_tooltip.add_css_class("suggestion-tooltip");
    suggestion_tooltip.set_halign(gtk4::Align::Start);
    suggestion_tooltip.set_valign(gtk4::Align::Start);
    suggestion_tooltip.set_margin_start(80);
    suggestion_tooltip.set_margin_top(28);
    suggestion_tooltip.set_wrap(true);
    suggestion_tooltip.set_max_width_chars(40);
    suggestion_tooltip.set_visible(false);
    overlay.add_overlay(&suggestion_tooltip);

    // Shared state for the current suggestion's action name
    let current_suggestion_action: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    // Click on suggestion tooltip dismisses it + sends feedback
    {
        let tooltip_ref = suggestion_tooltip.clone();
        let bubble_ref = bubble.clone();
        let action_ref = current_suggestion_action.clone();
        let ftx = feedback_tx;
        let dismiss_click = gtk4::GestureClick::new();
        dismiss_click.set_button(1);
        dismiss_click.connect_released(move |_, _, _, _| {
            // Send dismiss feedback if we know the action
            if let Some(ref tx) = ftx {
                if let Some(action) = action_ref.borrow().clone() {
                    let _ = tx.send(FeedbackEvent::SuggestionDismissed { action });
                }
            }
            *action_ref.borrow_mut() = None;
            tooltip_ref.set_visible(false);
            set_bubble_state(&bubble_ref, BubbleVisualState::Idle);
        });
        suggestion_tooltip.add_controller(dismiss_click);
    }

    let handle = BubbleHandle {
        bubble: bubble.clone(),
        confirmation_panel: confirm_panel,
        confirmation_label: confirm_label,
        rejection_toast,
        status_label: status_label.clone(),
        suggestion_tooltip,
        current_suggestion_action,
    };

    // Track drag vs click
    let dragged = Rc::new(RefCell::new(false));

    // Drag gesture — move bubble by updating margins
    let drag = gtk4::GestureDrag::new();
    drag.set_button(1);
    {
        let bubble_ref = bubble.clone();
        let dragged = dragged.clone();
        let skills_panel = skills_panel.clone();
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
                skills_panel.set_visible(false);
            }
            let (sx, sy) = *sm.borrow();
            bubble_ref.set_margin_start((sx + dx as i32).max(0));
            bubble_ref.set_margin_top((sy + dy as i32).max(0));
        });
    }
    bubble.add_controller(drag);

    // Click gesture — toggle skills panel
    let click = gtk4::GestureClick::new();
    click.set_button(1);
    {
        let skills_panel = skills_panel.clone();
        let bubble = bubble.clone();
        let overlay = overlay.clone();
        let active_doc = active_doc.clone();
        let all_buttons = all_buttons.clone();
        let status_label = status_label.clone();
        let dragged = dragged.clone();
        click.connect_released(move |_, _, _, _| {
            if *dragged.borrow() {
                return;
            }
            // Toggle
            if skills_panel.is_visible() {
                skills_panel.set_visible(false);
                return;
            }
            let has_doc = active_doc.borrow().is_some();
            for btn in all_buttons.borrow().iter() {
                btn.set_sensitive(has_doc);
            }
            if !has_doc {
                status_label.set_text("Open a document to use skills");
            } else {
                status_label.set_text("");
            }
            position_panel(&bubble, &skills_panel, &overlay);
            skills_panel.set_visible(true);
        });
    }
    bubble.add_controller(click);

    handle
}

/// Handle file dialog actions (add image, import file).
fn handle_file_dialog_action(
    skill_name: &str,
    action_id: &str,
    active_doc: &Rc<RefCell<Option<ActiveDocument>>>,
    save_cb: &Rc<dyn Fn(String, String, String)>,
    registry: &Rc<SkillRegistry>,
) {
    let panel = active_doc.borrow().as_ref().map(|ad| ad.panel_window.clone());
    let active_doc = active_doc.clone();
    let save_cb = save_cb.clone();
    let skill_name = skill_name.to_string();
    let action_id = action_id.to_string();
    let registry = registry.clone();

    let dialog = gtk4::FileDialog::builder()
        .title(if skill_name == "image" { "Select Image" } else { "Import File" })
        .build();

    if skill_name == "image" {
        let filter = gtk4::FileFilter::new();
        filter.add_mime_type("image/*");
        filter.set_name(Some("Images"));
        let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
    }

    dialog.open(panel.as_ref(), gtk4::gio::Cancellable::NONE, move |result: Result<gtk4::gio::File, gtk4::glib::Error>| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                let path_str = path.to_string_lossy().to_string();

                let doc_data = {
                    let guard = active_doc.borrow();
                    guard.as_ref().map(|ad| build_skill_doc(ad))
                };

                if let Some(skill_doc) = doc_data {
                    if let Some(skill) = registry.find_skill(&skill_name) {
                        match skill.execute(&action_id, &skill_doc, &path_str) {
                            Ok(SkillOutput::ContentUpdate(cf)) => {
                                save_cb(skill_doc.id.clone(), skill_doc.title.clone(), cf.serialize());
                                if let Some(ref mut ad) = *active_doc.borrow_mut() {
                                    ad.content.images = cf.images.clone();
                                    (ad.on_images_changed)(&cf.images);
                                }
                                tracing::info!("{} completed: {}", skill_name, path_str);
                            }
                            Ok(SkillOutput::StructuredData { kind, json }) => {
                                tracing::info!("{}: {} — {}", skill_name, kind, json);
                            }
                            Ok(_) => {}
                            Err(e) => tracing::error!("{} failed: {e}", skill_name),
                        }
                    }
                }
            }
        }
    });
}

/// Handle export actions (PDF export → save dialog).
fn handle_export_action(
    active_doc: &Rc<RefCell<Option<ActiveDocument>>>,
    registry: &Rc<SkillRegistry>,
) {
    let doc_data = {
        let guard = active_doc.borrow();
        guard.as_ref().map(|ad| (build_skill_doc(ad), ad.panel_window.clone()))
    };

    if let Some((skill_doc, panel)) = doc_data {
        if let Some(skill) = registry.find_skill("pdf-export") {
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
    }
}

/// Format structured data for display in the status label.
fn format_structured_data(kind: &str, json: &str) -> String {
    match kind {
        "word_count" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!(
                    "Words: {} | Chars: {} | Lines: {} | ~{} min read",
                    v["words"], v["characters"], v["lines"], v["reading_time_min"]
                )
            } else {
                json.to_string()
            }
        }
        "search_results" => {
            if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(json) {
                if v.is_empty() {
                    "No results found".into()
                } else {
                    let titles: Vec<String> = v.iter()
                        .take(5)
                        .filter_map(|item| item["title"].as_str().map(String::from))
                        .collect();
                    format!("{} results: {}", v.len(), titles.join(", "))
                }
            } else {
                json.to_string()
            }
        }
        "find_replace" => {
            if serde_json::from_str::<serde_json::Value>(json).is_ok() {
                "No matches found (0 replacements)".into()
            } else {
                json.to_string()
            }
        }
        "duplicate_result" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!("Created: {}", v["title"].as_str().unwrap_or("copy"))
            } else {
                json.to_string()
            }
        }
        "import_result" => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                format!("Imported: {}", v["title"].as_str().unwrap_or("document"))
            } else {
                json.to_string()
            }
        }
        _ => json.to_string(),
    }
}
