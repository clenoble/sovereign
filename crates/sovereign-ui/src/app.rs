use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iced::widget::{column, container, shader, stack, text_editor};
use iced::{keyboard, Element, Length, Subscription, Task, Theme};

use sovereign_canvas::controller::CanvasCommand;
use sovereign_canvas::renderer::CanvasProgram;
use sovereign_canvas::state::CanvasState;
use sovereign_core::config::UiConfig;
use sovereign_core::content::ContentFields;
use sovereign_core::interfaces::{FeedbackEvent, OrchestratorEvent, SkillEvent};
use sovereign_core::security::ActionDecision;
use sovereign_db::schema::{Document, Thread};
use sovereign_skills::registry::SkillRegistry;
use sovereign_skills::traits::SkillOutput;

use crate::chat::ChatState;
use crate::orchestrator_bubble::{build_skill_doc, format_structured_data, BubbleState};
use crate::panels::document_panel::{FloatingPanel, DEAD_ZONE, DRAG_BAR_HEIGHT};
use crate::search::SearchState;
use crate::taskbar::TaskbarState;
use crate::theme;

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

/// All messages the application handles.
#[derive(Debug, Clone)]
pub enum Message {
    // Periodic polling
    Tick,
    // Search
    SearchToggled,
    SearchQueryChanged(String),
    SearchSubmitted,
    // Bubble
    BubbleClicked,
    SkillExecuted(String, String), // skill_name, action_id
    ApproveAction,
    RejectAction,
    DismissSuggestion,
    // Document panels
    OpenDocument(String),
    CloseDocument(usize),
    SaveDocument(usize),
    DocTitleChanged { panel_idx: usize, title: String },
    DocBodyAction { panel_idx: usize, action: text_editor::Action },
    // Taskbar
    TaskbarDocClicked(String),
    TaskbarDocPinToggled(String),
    MicToggled,
    // Chat
    ChatToggled,
    ChatInputChanged(String),
    ChatSubmitted,
    // Panel drag
    PanelDragStart(usize),
    PanelDragMove { panel_idx: usize, local: iced::Point },
    PanelDragEnd(usize),
    // File dialogs
    FileDialogResult { skill_name: String, action_id: String, path: Option<std::path::PathBuf> },
    SaveDialogResult { data: Vec<u8>, path: Option<std::path::PathBuf> },
    // Keyboard
    KeyEvent(keyboard::Event),
    // No-op
    Ignore,
}

/// The central application state.
pub struct SovereignApp {
    // Canvas
    canvas_program: CanvasProgram,
    canvas_state: Arc<Mutex<CanvasState>>,
    canvas_cmd_rx: Option<mpsc::Receiver<CanvasCommand>>,
    // UI components
    search: SearchState,
    chat: ChatState,
    bubble: BubbleState,
    taskbar: TaskbarState,
    doc_panels: Vec<FloatingPanel>,
    doc_map: HashMap<String, Document>,
    // Channels
    orch_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
    voice_rx: Option<mpsc::Receiver<VoiceEvent>>,
    skill_rx: Option<mpsc::Receiver<SkillEvent>>,
    // Callbacks
    query_callback: Option<Box<dyn Fn(String) + Send>>,
    chat_callback: Option<Box<dyn Fn(String) + Send>>,
    save_callback: Option<Box<dyn Fn(String, String, String) + Send>>,
    close_callback: Option<Box<dyn Fn(String) + Send>>,
    decision_tx: Option<mpsc::Sender<ActionDecision>>,
    feedback_tx: Option<mpsc::Sender<FeedbackEvent>>,
    skill_registry: SkillRegistry,
}

impl SovereignApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _config: &UiConfig,
        documents: Vec<Document>,
        threads: Vec<Thread>,
        query_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        chat_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        orchestrator_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
        voice_rx: Option<mpsc::Receiver<VoiceEvent>>,
        skill_rx: Option<mpsc::Receiver<SkillEvent>>,
        save_callback: Option<Box<dyn Fn(String, String, String) + Send + 'static>>,
        close_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        decision_tx: Option<mpsc::Sender<ActionDecision>>,
        skill_registry: Option<SkillRegistry>,
        feedback_tx: Option<mpsc::Sender<FeedbackEvent>>,
    ) -> (Self, Task<Message>) {
        let doc_map: HashMap<String, Document> = documents
            .iter()
            .filter_map(|d| d.id_string().map(|id| (id, d.clone())))
            .collect();

        let pinned_docs: Vec<(String, String, bool)> = documents
            .iter()
            .filter(|d| d.is_owned)
            .take(1)
            .filter_map(|d| d.id_string().map(|id| (id, d.title.clone(), d.is_owned)))
            .collect();

        let (canvas_program, canvas_cmd_rx, _controller) =
            sovereign_canvas::build_canvas(documents, threads);

        let canvas_state = canvas_program.state.clone();

        let app = Self {
            canvas_program,
            canvas_state,
            canvas_cmd_rx: Some(canvas_cmd_rx),
            search: SearchState::new(),
            chat: ChatState::new(),
            bubble: BubbleState::new(),
            taskbar: TaskbarState::new(pinned_docs),
            doc_panels: Vec::new(),
            doc_map,
            orch_rx: orchestrator_rx,
            voice_rx,
            skill_rx,
            query_callback,
            chat_callback,
            save_callback,
            close_callback,
            decision_tx,
            feedback_tx,
            skill_registry: skill_registry.unwrap_or_default(),
        };

        (app, Task::none())
    }

    pub fn title(&self) -> String {
        "Sovereign OS".to_string()
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.poll_channels();
                Task::none()
            }

            // ── Search ───────────────────────────────────────────────────
            Message::SearchToggled => {
                self.search.visible = !self.search.visible;
                Task::none()
            }
            Message::SearchQueryChanged(query) => {
                self.search.query = query;
                Task::none()
            }
            Message::SearchSubmitted => {
                let query = self.search.query.clone();
                if !query.is_empty() {
                    if let Some(ref cb) = self.query_callback {
                        cb(query);
                    }
                }
                Task::none()
            }

            // ── Bubble ───────────────────────────────────────────────────
            Message::BubbleClicked => {
                self.bubble.skills_panel_visible = !self.bubble.skills_panel_visible;
                Task::none()
            }
            Message::SkillExecuted(skill_name, action_id) => {
                self.execute_skill(&skill_name, &action_id)
            }
            Message::ApproveAction => {
                if let Some(ref tx) = self.decision_tx {
                    let _ = tx.send(ActionDecision::Approve);
                }
                self.bubble.confirmation = None;
                Task::none()
            }
            Message::RejectAction => {
                if let Some(ref tx) = self.decision_tx {
                    let _ = tx.send(ActionDecision::Reject("User rejected".into()));
                }
                self.bubble.confirmation = None;
                Task::none()
            }
            Message::DismissSuggestion => {
                if let Some(ref tx) = self.feedback_tx {
                    if let Some((_, ref action)) = self.bubble.suggestion {
                        let _ = tx.send(FeedbackEvent::SuggestionDismissed {
                            action: action.clone(),
                        });
                    }
                }
                self.bubble.dismiss_suggestion();
                Task::none()
            }

            // ── Document panels ──────────────────────────────────────────
            Message::OpenDocument(doc_id) => {
                // Don't open duplicates
                if self.doc_panels.iter().any(|p| p.doc_id == doc_id) {
                    return Task::none();
                }
                if let Some(doc) = self.doc_map.get(&doc_id) {
                    let content = ContentFields::parse(&doc.content);
                    let panel = FloatingPanel::new(
                        doc_id.clone(),
                        doc.title.clone(),
                        content.body,
                        content.images,
                    );
                    self.doc_panels.push(panel);
                    self.taskbar.add_document(&doc_id, &doc.title, doc.is_owned);
                }
                Task::none()
            }
            Message::CloseDocument(idx) => {
                if idx < self.doc_panels.len() {
                    let doc_id = self.doc_panels[idx].doc_id.clone();
                    self.doc_panels.remove(idx);
                    if let Some(ref cb) = self.close_callback {
                        cb(doc_id);
                    }
                }
                Task::none()
            }
            Message::SaveDocument(idx) => {
                if let Some(panel) = self.doc_panels.get(idx) {
                    let doc_id = panel.doc_id.clone();
                    let title = panel.title.clone();
                    let body = panel.get_body_text();
                    let images = panel.images.clone();
                    let cf = ContentFields { body, images };
                    if let Some(ref cb) = self.save_callback {
                        cb(doc_id.clone(), title, cf.serialize());
                    }
                    // Update local doc map
                    if let Some(doc) = self.doc_map.get_mut(&doc_id) {
                        doc.content = cf.serialize();
                    }
                }
                Task::none()
            }
            Message::DocTitleChanged { panel_idx, title } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    panel.title = title;
                }
                Task::none()
            }
            Message::DocBodyAction { panel_idx, action } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    panel.body.perform(action);
                }
                Task::none()
            }

            // ── Panel drag ──────────────────────────────────────────────
            Message::PanelDragStart(idx) => {
                if let Some(panel) = self.doc_panels.get_mut(idx) {
                    // Local coords include the DEAD_ZONE padding: panel content
                    // starts at (DEAD_ZONE, DEAD_ZONE). Only start drag if
                    // cursor is in the toolbar row (the drag handle).
                    let local_y = panel.last_local_cursor.y;
                    if local_y >= DEAD_ZONE && local_y < DEAD_ZONE + DRAG_BAR_HEIGHT {
                        let screen = iced::Point::new(
                            (panel.position.x - DEAD_ZONE) + panel.last_local_cursor.x,
                            (panel.position.y - DEAD_ZONE) + panel.last_local_cursor.y,
                        );
                        panel.drag_start_screen = Some(screen);
                        panel.drag_start_panel = Some(panel.position);
                    }
                }
                Task::none()
            }
            Message::PanelDragMove { panel_idx, local } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    panel.last_local_cursor = local;

                    if let (Some(start_screen), Some(start_panel)) =
                        (panel.drag_start_screen, panel.drag_start_panel)
                    {
                        // Convert local coords to screen coords, accounting
                        // for the dead-zone offset in the outer container.
                        let screen = iced::Point::new(
                            (panel.position.x - DEAD_ZONE) + local.x,
                            (panel.position.y - DEAD_ZONE) + local.y,
                        );
                        let dx = screen.x - start_screen.x;
                        let dy = screen.y - start_screen.y;

                        // 3px threshold to start drag
                        if !panel.dragging && (dx.abs() > 3.0 || dy.abs() > 3.0) {
                            panel.dragging = true;
                        }

                        if panel.dragging {
                            panel.position = iced::Point::new(
                                (start_panel.x + dx).max(0.0),
                                (start_panel.y + dy).max(0.0),
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::PanelDragEnd(idx) => {
                if let Some(panel) = self.doc_panels.get_mut(idx) {
                    panel.dragging = false;
                    panel.drag_start_screen = None;
                    panel.drag_start_panel = None;
                }
                Task::none()
            }

            // ── Taskbar ──────────────────────────────────────────────────
            Message::TaskbarDocClicked(doc_id) => {
                self.update(Message::OpenDocument(doc_id))
            }
            Message::TaskbarDocPinToggled(doc_id) => {
                self.taskbar.toggle_pin(&doc_id);
                Task::none()
            }
            Message::MicToggled => {
                self.taskbar.listening = !self.taskbar.listening;
                Task::none()
            }

            // ── Chat ──────────────────────────────────────────────────────
            Message::ChatToggled => {
                self.chat.visible = !self.chat.visible;
                Task::none()
            }
            Message::ChatInputChanged(input) => {
                self.chat.input = input;
                Task::none()
            }
            Message::ChatSubmitted => {
                let input = self.chat.input.clone();
                if !input.is_empty() {
                    self.chat.push_user_message(input.clone());
                    self.chat.input.clear();
                    if let Some(ref cb) = self.chat_callback {
                        cb(input);
                    }
                }
                Task::none()
            }

            // ── File dialogs ─────────────────────────────────────────────
            Message::FileDialogResult { skill_name, action_id, path } => {
                if let Some(path) = path {
                    self.execute_skill_with_path(&skill_name, &action_id, &path);
                }
                Task::none()
            }
            Message::SaveDialogResult { data, path } => {
                if let Some(path) = path {
                    if let Err(e) = std::fs::write(&path, &data) {
                        tracing::error!("Failed to write file: {e}");
                    } else {
                        tracing::info!("Exported to {}", path.display());
                    }
                }
                Task::none()
            }

            // ── Keyboard ─────────────────────────────────────────────────
            Message::KeyEvent(event) => {
                if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
                    let ctrl = modifiers.command();
                    match key {
                        keyboard::Key::Character(ref c) if ctrl && c.as_str() == "f" => {
                            self.search.visible = !self.search.visible;
                        }
                        keyboard::Key::Character(ref c) if ctrl && c.as_str() == "g" => {
                            if let Some(ref cb) = self.query_callback {
                                cb("jump_to_date".to_string());
                            }
                        }
                        keyboard::Key::Named(keyboard::key::Named::Space) if ctrl => {
                            self.chat.visible = !self.chat.visible;
                        }
                        keyboard::Key::Named(keyboard::key::Named::Escape) => {
                            self.search.visible = false;
                            self.chat.visible = false;
                        }
                        _ => {}
                    }
                }
                Task::none()
            }

            Message::Ignore => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // Canvas: shader handles mouse events directly via Program::update()
        let canvas = shader(&self.canvas_program)
            .width(Length::Fill)
            .height(Length::Fill);

        let mut layers: Vec<Element<Message>> = vec![canvas.into()];

        // Floating document panels
        for (i, panel) in self.doc_panels.iter().enumerate() {
            if panel.visible {
                layers.push(panel.view(i));
            }
        }

        // Orchestrator bubble
        let has_active_doc = !self.doc_panels.is_empty();
        layers.push(self.bubble.view(&self.skill_registry, has_active_doc));

        // Chat panel (next to bubble)
        if self.chat.visible {
            let chat_positioned = container(self.chat.view())
                .padding(
                    iced::Padding::ZERO
                        .top(20.0)
                        .left(80.0),
                );
            layers.push(chat_positioned.into());
        }

        // Search overlay (conditional)
        if self.search.visible {
            layers.push(self.search.view());
        }

        // Compose: stack of layers + taskbar at bottom
        let content = column![
            stack(layers).width(Length::Fill).height(Length::Fill),
            self.taskbar.view(),
        ];

        container(content)
            .style(theme::dark_background)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // Poll channels at ~60fps
            iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick),
            // Keyboard events
            keyboard::listen().map(Message::KeyEvent),
        ])
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn poll_channels(&mut self) {
        // Check for double-click → open document from the canvas shader
        {
            let mut st = self.canvas_state.lock().unwrap();
            if let Some(doc_id) = st.pending_open.take() {
                drop(st);
                // Inline the OpenDocument logic (poll_channels can't return Task)
                if !self.doc_panels.iter().any(|p| p.doc_id == doc_id) {
                    if let Some(doc) = self.doc_map.get(&doc_id) {
                        let content = ContentFields::parse(&doc.content);
                        let panel = FloatingPanel::new(
                            doc_id.clone(),
                            doc.title.clone(),
                            content.body,
                            content.images,
                        );
                        self.doc_panels.push(panel);
                        self.taskbar.add_document(&doc_id, &doc.title, doc.is_owned);
                    }
                }
            }
        }

        // Drain orchestrator events first, then process (avoids borrow conflict)
        let orch_events: Vec<_> = self
            .orch_rx
            .as_ref()
            .map(|rx| {
                let mut events = Vec::new();
                while let Ok(e) = rx.try_recv() {
                    events.push(e);
                }
                events
            })
            .unwrap_or_default();

        for event in orch_events {
            self.handle_orchestrator_event(event);
        }

        // Poll voice events (handled inline, no self method call needed)
        if let Some(ref rx) = self.voice_rx {
            let voice_events: Vec<_> = {
                let mut events = Vec::new();
                while let Ok(e) = rx.try_recv() {
                    events.push(e);
                }
                events
            };
            for event in voice_events {
                match event {
                    VoiceEvent::TranscriptionReady(ref text) => {
                        if self.chat.visible {
                            // Route voice to chat when chat is open
                            self.chat.push_user_message(text.clone());
                            if let Some(ref cb) = self.chat_callback {
                                cb(text.clone());
                            }
                        } else {
                            self.search.query = text.clone();
                        }
                        self.search.voice_status = None;
                    }
                    VoiceEvent::WakeWordDetected | VoiceEvent::ListeningStarted => {
                        self.search.voice_status = Some("Listening...".to_string());
                    }
                    VoiceEvent::ListeningStopped => {
                        self.search.voice_status = Some("Transcribing...".to_string());
                    }
                    VoiceEvent::TtsDone => {
                        self.search.voice_status = None;
                    }
                    _ => {}
                }
            }
        }

        // Drain skill events first, then process
        let skill_events: Vec<_> = self
            .skill_rx
            .as_ref()
            .map(|rx| {
                let mut events = Vec::new();
                while let Ok(e) = rx.try_recv() {
                    events.push(e);
                }
                events
            })
            .unwrap_or_default();

        for event in skill_events {
            self.handle_skill_event(event);
        }

        // Poll canvas commands
        let cmds: Vec<CanvasCommand> = self
            .canvas_cmd_rx
            .as_ref()
            .map(|rx| {
                let mut cmds = Vec::new();
                while let Ok(cmd) = rx.try_recv() {
                    cmds.push(cmd);
                }
                cmds
            })
            .unwrap_or_default();

        for cmd in cmds {
            sovereign_canvas::apply_command(&self.canvas_state, cmd);
        }
    }

    fn handle_orchestrator_event(&mut self, event: OrchestratorEvent) {
        match event {
            OrchestratorEvent::SearchResults { ref doc_ids, .. } => {
                self.search.results = doc_ids.clone();
                let mut st = self.canvas_state.lock().unwrap();
                for id in doc_ids {
                    if !st.highlighted.contains(id) {
                        st.highlighted.push(id.clone());
                    }
                }
                st.mark_dirty();
            }
            OrchestratorEvent::DocumentOpened { ref doc_id } => {
                sovereign_canvas::apply_command(
                    &self.canvas_state,
                    CanvasCommand::NavigateTo(doc_id.clone()),
                );
            }
            OrchestratorEvent::BubbleState(state) => {
                self.bubble.set_state(state);
            }
            OrchestratorEvent::ActionProposed { ref proposal } => {
                self.bubble.show_confirmation(&proposal.description);
            }
            OrchestratorEvent::ActionRejected { ref reason, .. } => {
                self.bubble.show_rejection(reason);
            }
            OrchestratorEvent::Suggestion { ref text, ref action } => {
                self.bubble.show_suggestion(text, action);
            }
            OrchestratorEvent::DocumentCreated {
                ref doc_id,
                ref title,
                ref thread_id,
            } => {
                tracing::info!("UI: Document created: {} ({}) in {}", title, doc_id, thread_id);
                let new_doc = sovereign_db::schema::Document::new(
                    title.clone(),
                    thread_id.clone(),
                    true,
                );
                self.doc_map.insert(doc_id.clone(), new_doc);
            }
            OrchestratorEvent::SkillResult { ref kind, ref data, .. } => {
                let display = match kind.as_str() {
                    "summary" => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                            format!("Summary: {}", v["summary"].as_str().unwrap_or(data))
                        } else {
                            data.clone()
                        }
                    }
                    _ => data.clone(),
                };
                self.bubble.show_skill_result(&display);
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
            }
            OrchestratorEvent::ChatResponse { ref text } => {
                self.chat.push_assistant_message(text.clone());
                // TODO: Trigger TTS when cross-platform audio playback is available
            }
            _ => {}
        }
    }

    fn handle_skill_event(&mut self, event: SkillEvent) {
        match event {
            SkillEvent::OpenDocument { ref doc_id } => {
                if let Some(doc) = self.doc_map.get(doc_id) {
                    let content = ContentFields::parse(&doc.content);
                    if !self.doc_panels.iter().any(|p| p.doc_id == *doc_id) {
                        let panel = FloatingPanel::new(
                            doc_id.clone(),
                            doc.title.clone(),
                            content.body,
                            content.images,
                        );
                        self.doc_panels.push(panel);
                        self.taskbar.add_document(doc_id, &doc.title, doc.is_owned);
                    }
                }
            }
            SkillEvent::DocumentClosed { ref doc_id } => {
                tracing::info!("Document closed: {}", doc_id);
                if let Some(ref cb) = self.close_callback {
                    cb(doc_id.clone());
                }
            }
        }
    }

    fn execute_skill(&mut self, skill_name: &str, action_id: &str) -> Task<Message> {
        // Special cases: file dialogs
        if (skill_name == "image" && action_id == "add")
            || (skill_name == "file-import" && action_id == "import")
        {
            self.bubble.skills_panel_visible = false;
            let skill_name_owned = skill_name.to_string();
            let action_id_owned = action_id.to_string();
            let is_image = skill_name_owned == "image";
            tracing::info!("File dialog requested for {}", skill_name_owned);
            return Task::perform(
                async move {
                    let mut dialog = rfd::AsyncFileDialog::new()
                        .set_title(if is_image { "Select Image" } else { "Import File" });
                    if is_image {
                        dialog = dialog.add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]);
                    }
                    dialog.pick_file().await.map(|f| f.path().to_path_buf())
                },
                move |path| Message::FileDialogResult {
                    skill_name: skill_name_owned,
                    action_id: action_id_owned,
                    path,
                },
            );
        }

        // Special case: PDF export
        if skill_name == "pdf-export" && action_id == "export" {
            self.bubble.skills_panel_visible = false;
            if let Some((doc_data, _)) = self.get_active_doc_data() {
                if let Some(skill) = self.skill_registry.find_skill("pdf-export") {
                    match skill.execute("export", &doc_data, "") {
                        Ok(SkillOutput::File { name, data: _, .. }) => {
                            tracing::info!("PDF generated: {name}");
                        }
                        Ok(_) => {}
                        Err(e) => tracing::error!("PDF export failed: {e}"),
                    }
                }
            }
            return Task::none();
        }

        // Special case: search/find_replace hint
        if action_id == "find_replace" || action_id == "search" {
            self.bubble.show_skill_result(
                &format!("Use search bar for {}", action_id.replace('_', " ")),
            );
            return Task::none();
        }

        // Default: immediate execution
        if let Some((skill_doc, panel_idx)) = self.get_active_doc_data() {
            if let Some(skill) = self.skill_registry.find_skill(skill_name) {
                match skill.execute(action_id, &skill_doc, "") {
                    Ok(SkillOutput::ContentUpdate(cf)) => {
                        let doc_id = skill_doc.id.clone();
                        let title = skill_doc.title.clone();
                        if let Some(ref cb) = self.save_callback {
                            cb(doc_id, title, cf.serialize());
                        }
                        if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                            if panel.images != cf.images {
                                panel.images = cf.images;
                            }
                        }
                        self.bubble.show_skill_result("Done");
                    }
                    Ok(SkillOutput::StructuredData { kind, json }) => {
                        let display = format_structured_data(&kind, &json);
                        self.bubble.show_skill_result(&display);
                    }
                    Ok(SkillOutput::File { .. }) => {
                        self.bubble.show_skill_result("File generated");
                    }
                    Ok(SkillOutput::None) => {
                        self.bubble.show_skill_result("Done");
                    }
                    Err(e) => {
                        self.bubble.show_skill_result(&format!("Error: {e}"));
                    }
                }
            }
        } else {
            self.bubble.show_skill_result("Open a document first");
        }
        Task::none()
    }

    fn execute_skill_with_path(
        &mut self,
        skill_name: &str,
        action_id: &str,
        path: &std::path::Path,
    ) {
        let path_str = path.to_string_lossy().to_string();
        if let Some((skill_doc, panel_idx)) = self.get_active_doc_data() {
            if let Some(skill) = self.skill_registry.find_skill(skill_name) {
                match skill.execute(action_id, &skill_doc, &path_str) {
                    Ok(SkillOutput::ContentUpdate(cf)) => {
                        if let Some(ref cb) = self.save_callback {
                            cb(skill_doc.id.clone(), skill_doc.title.clone(), cf.serialize());
                        }
                        if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                            panel.images = cf.images;
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

    /// Get the first open document panel's data as a SkillDocument + panel index.
    fn get_active_doc_data(&self) -> Option<(sovereign_skills::traits::SkillDocument, usize)> {
        self.doc_panels.first().map(|panel| {
            let doc = build_skill_doc(
                &panel.doc_id,
                &panel.title,
                &panel.get_body_text(),
                &panel.images,
            );
            (doc, 0)
        })
    }
}

/// Launch the Iced application.
pub fn run_app(app: SovereignApp) -> iced::Result {
    let boot_state = Mutex::new(Some(app));
    iced::application(
        move || {
            let app = boot_state.lock().unwrap().take().expect("boot called twice");
            (app, Task::none())
        },
        SovereignApp::update,
        SovereignApp::view,
    )
    .title(SovereignApp::title)
    .subscription(SovereignApp::subscription)
    .theme(SovereignApp::theme)
    .window_size(iced::Size::new(1280.0, 800.0))
    .antialiasing(true)
    .run()
}
