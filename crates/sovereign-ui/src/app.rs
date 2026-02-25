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
use sovereign_db::schema::{Commit, Contact, Conversation, Document, Message as DbMessage, RelatedTo, Thread};
use sovereign_skills::registry::SkillRegistry;
use sovereign_skills::traits::SkillOutput;

use crate::chat::ChatState;
use crate::onboarding::OnboardingState;
use crate::orchestrator_bubble::{build_skill_doc, format_structured_data, BubbleState};
use crate::panels::camera_panel::{CameraPanel, SharedFrame};
use crate::panels::contact_panel::ContactPanel;
use crate::panels::document_panel::{FloatingPanel, FormatKind, DEAD_ZONE, DRAG_BAR_HEIGHT};
use crate::panels::inbox_panel::InboxPanel;
use crate::panels::model_panel::{ModelEntry, ModelPanel, ModelRole};
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
    SearchResultNavigate(String),
    SearchResultOpen(String),
    // Bubble
    BubbleClicked,
    SkillExecuted(String, String), // skill_name, action_id
    // Skills
    TaskbarSkillsToggled,
    DocSkillsOverflowToggled(usize),
    DocSkillExecuted { panel_idx: usize, skill_name: String, action_id: String },
    ApproveAction,
    RejectAction,
    DismissSuggestion,
    // Document panels
    OpenDocument(String),
    CloseDocument(usize),
    SaveDocument(usize),
    DocTitleChanged { panel_idx: usize, title: String },
    DocBodyAction { panel_idx: usize, action: text_editor::Action },
    ToggleHistory(usize),
    SelectCommit { panel_idx: usize, commit_idx: usize },
    // Contact panels
    OpenContact(String),
    CloseContactPanel(usize),
    SelectConversation { panel_idx: usize, conv_idx: usize },
    ContactPanelDragStart(usize),
    ContactPanelDragMove { panel_idx: usize, local: iced::Point },
    ContactPanelDragEnd(usize),
    // Inbox panel
    InboxToggled,
    InboxClose,
    InboxSelectConversation(usize),
    InboxBack,
    InboxReplyChanged(String),
    InboxReplySubmit,
    InboxDragStart,
    InboxDragMove(iced::Point),
    InboxDragEnd,
    // Taskbar
    TaskbarDocClicked(String),
    TaskbarDocPinToggled(String),
    TaskbarContactClicked(String),
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
    // Markdown formatting
    TogglePreview(usize),
    FormatAction { panel_idx: usize, kind: FormatKind },
    MarkdownLinkClicked(String),
    // Video
    VideoPlay { panel_idx: usize, video_idx: usize },
    // Model management
    ModelPanelToggled,
    ModelRefresh,
    ModelScanComplete(Vec<ModelEntry>),
    ModelAssignRole { model_idx: usize, role: ModelRole },
    ModelDelete(usize),
    ModelDeleteComplete,
    // Camera
    CameraToggled,
    // Theme
    ThemeToggled,
    // Login
    LoginPasswordChanged(String),
    LoginSubmit,
    // Onboarding
    OnboardingNext,
    OnboardingBack,
    OnboardingNicknameChanged(String),
    OnboardingBubbleSelected(sovereign_core::profile::BubbleStyle),
    OnboardingThemeToggled,
    OnboardingSeedToggled,
    OnboardingPrimaryChanged(String),
    OnboardingPrimaryConfirmChanged(String),
    OnboardingDuressChanged(String),
    OnboardingDuressConfirmChanged(String),
    OnboardingCanaryChanged(String),
    OnboardingCanaryConfirmChanged(String),
    OnboardingEnrollInputChanged(String),
    OnboardingEnrollSubmit,
    OnboardingSkipAuth,
    OnboardingFocusField(&'static str),
    OnboardingTryAdvance,
    OnboardingComplete,
    // Canary / lockdown
    CanaryTriggered,
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
    contact_panels: Vec<ContactPanel>,
    doc_map: HashMap<String, Document>,
    contact_map: HashMap<String, Contact>,
    conversations: Vec<Conversation>,
    messages: Vec<DbMessage>,
    commits_map: HashMap<String, Vec<Commit>>,
    // Channels
    orch_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
    voice_rx: Option<mpsc::Receiver<VoiceEvent>>,
    skill_rx: Option<mpsc::Receiver<SkillEvent>>,
    // Panels
    model_panel: ModelPanel,
    camera_panel: CameraPanel,
    inbox_panel: Option<InboxPanel>,
    // Auth
    login: Option<crate::login::LoginState>,
    canary_detector: Option<sovereign_crypto::canary::CanaryDetector>,
    lockdown_triggered: bool,
    // Onboarding
    onboarding: Option<OnboardingState>,
    // Callbacks
    query_callback: Option<Box<dyn Fn(String) + Send>>,
    chat_callback: Option<Box<dyn Fn(String) + Send>>,
    save_callback: Option<Box<dyn Fn(String, String, String) + Send>>,
    close_callback: Option<Box<dyn Fn(String) + Send>>,
    decision_tx: Option<tokio::sync::mpsc::Sender<ActionDecision>>,
    feedback_tx: Option<tokio::sync::mpsc::Sender<FeedbackEvent>>,
    send_message_tx: Option<tokio::sync::mpsc::Sender<crate::panels::inbox_panel::SendRequest>>,
    skill_registry: SkillRegistry,
    taskbar_skills_dropdown_open: bool,
}

impl SovereignApp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _config: &UiConfig,
        documents: Vec<Document>,
        threads: Vec<Thread>,
        relationships: Vec<RelatedTo>,
        commits_map: HashMap<String, Vec<Commit>>,
        contacts: Vec<Contact>,
        conversations: Vec<Conversation>,
        messages: Vec<DbMessage>,
        query_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        chat_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        orchestrator_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
        voice_rx: Option<mpsc::Receiver<VoiceEvent>>,
        skill_rx: Option<mpsc::Receiver<SkillEvent>>,
        save_callback: Option<Box<dyn Fn(String, String, String) + Send + 'static>>,
        close_callback: Option<Box<dyn Fn(String) + Send + 'static>>,
        decision_tx: Option<tokio::sync::mpsc::Sender<ActionDecision>>,
        skill_registry: Option<SkillRegistry>,
        feedback_tx: Option<tokio::sync::mpsc::Sender<FeedbackEvent>>,
        send_message_tx: Option<tokio::sync::mpsc::Sender<crate::panels::inbox_panel::SendRequest>>,
        first_launch: bool,
        model_dir: String,
        router_model: String,
        reasoning_model: String,
        camera_frame: Option<SharedFrame>,
        bubble_style: Option<sovereign_core::profile::BubbleStyle>,
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

        let contact_map: HashMap<String, Contact> = contacts
            .iter()
            .filter_map(|c| c.id_string().map(|id| (id, c.clone())))
            .collect();

        // Pin up to 3 non-owned contacts for the taskbar
        let pinned_contacts: Vec<(String, String)> = contacts
            .iter()
            .filter(|c| !c.is_owned)
            .take(3)
            .filter_map(|c| c.id_string().map(|id| (id, c.name.clone())))
            .collect();

        let (canvas_program, canvas_cmd_rx, _controller) =
            sovereign_canvas::build_canvas(documents, threads, relationships);

        let canvas_state = canvas_program.state.clone();

        let mut taskbar = TaskbarState::new(pinned_docs);
        taskbar.set_pinned_contacts(pinned_contacts);

        let app = Self {
            canvas_program,
            canvas_state,
            canvas_cmd_rx: Some(canvas_cmd_rx),
            search: SearchState::new(),
            chat: ChatState::new(),
            bubble: {
                let mut b = BubbleState::new();
                if let Some(style) = bubble_style {
                    b.bubble_style = style;
                }
                b
            },
            taskbar,
            doc_panels: Vec::new(),
            contact_panels: Vec::new(),
            doc_map,
            contact_map,
            conversations,
            messages,
            commits_map,
            orch_rx: orchestrator_rx,
            voice_rx,
            skill_rx,
            query_callback,
            chat_callback,
            save_callback,
            close_callback,
            model_panel: ModelPanel::new(model_dir, router_model, reasoning_model),
            camera_panel: CameraPanel::new(
                camera_frame.unwrap_or_else(|| Arc::new(Mutex::new(None))),
            ),
            inbox_panel: None,
            login: None,
            canary_detector: None,
            lockdown_triggered: false,
            decision_tx,
            onboarding: if first_launch {
                Some(OnboardingState::new())
            } else {
                None
            },
            feedback_tx,
            send_message_tx,
            skill_registry: skill_registry.unwrap_or_default(),
            taskbar_skills_dropdown_open: false,
        };

        (app, Task::none())
    }

    pub fn title(&self) -> String {
        "Sovereign OS".to_string()
    }

    pub fn theme(&self) -> Theme {
        theme::iced_theme()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.bubble.elapsed += 1.0 / 60.0;
                if let Some(ref mut ob) = self.onboarding {
                    ob.elapsed += 1.0 / 60.0;
                }
                self.poll_channels();
                Task::none()
            }

            // ── Search ───────────────────────────────────────────────────
            Message::SearchToggled => {
                self.search.visible = !self.search.visible;
                Task::none()
            }
            Message::SearchQueryChanged(query) => {
                if self.feed_canary(&query) {
                    return self.update(Message::CanaryTriggered);
                }
                // Instant client-side filtering by title and content
                let q = query.to_lowercase();
                if q.is_empty() {
                    self.search.results.clear();
                } else {
                    self.search.results = self
                        .doc_map
                        .values()
                        .filter(|d| {
                            d.title.to_lowercase().contains(&q)
                                || d.content.to_lowercase().contains(&q)
                        })
                        .filter_map(|d| {
                            d.id_string().map(|id| crate::search::SearchResult {
                                id,
                                title: d.title.clone(),
                            })
                        })
                        .collect();
                }
                self.search.query = query;
                Task::none()
            }
            Message::SearchSubmitted => {
                let query = self.search.query.trim().to_string();
                if !query.is_empty() {
                    if let Some(ref cb) = self.query_callback {
                        cb(query);
                    }
                }
                Task::none()
            }
            Message::SearchResultNavigate(doc_id) => {
                sovereign_canvas::apply_command(
                    &self.canvas_state,
                    CanvasCommand::NavigateTo(doc_id),
                );
                self.search.visible = false;
                Task::none()
            }
            Message::SearchResultOpen(doc_id) => {
                self.open_document(&doc_id);
                self.search.visible = false;
                Task::none()
            }

            // ── Bubble ───────────────────────────────────────────────────
            Message::BubbleClicked => {
                self.chat.visible = !self.chat.visible;
                Task::none()
            }
            Message::SkillExecuted(skill_name, action_id) => {
                self.execute_skill(&skill_name, &action_id)
            }
            Message::TaskbarSkillsToggled => {
                self.taskbar_skills_dropdown_open = !self.taskbar_skills_dropdown_open;
                Task::none()
            }
            Message::DocSkillsOverflowToggled(idx) => {
                if let Some(panel) = self.doc_panels.get_mut(idx) {
                    panel.skills_overflow_open = !panel.skills_overflow_open;
                }
                Task::none()
            }
            Message::DocSkillExecuted { panel_idx, skill_name, action_id } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    panel.skills_overflow_open = false;
                }
                self.execute_skill(&skill_name, &action_id)
            }
            Message::ApproveAction => {
                if let Some(ref tx) = self.decision_tx {
                    if tx.try_send(ActionDecision::Approve).is_err() {
                        tracing::warn!("Decision channel closed — approval not delivered");
                    }
                }
                self.bubble.confirmation = None;
                Task::none()
            }
            Message::RejectAction => {
                if let Some(ref tx) = self.decision_tx {
                    if tx.try_send(ActionDecision::Reject("User rejected".into())).is_err() {
                        tracing::warn!("Decision channel closed — rejection not delivered");
                    }
                }
                self.bubble.confirmation = None;
                Task::none()
            }
            Message::DismissSuggestion => {
                if let Some(ref tx) = self.feedback_tx {
                    if let Some((_, ref action)) = self.bubble.suggestion {
                        if tx.try_send(FeedbackEvent::SuggestionDismissed {
                            action: action.clone(),
                        }).is_err() {
                            tracing::warn!("Feedback channel closed — dismissal not delivered");
                        }
                    }
                }
                self.bubble.dismiss_suggestion();
                Task::none()
            }

            // ── Document panels ──────────────────────────────────────────
            msg @ (Message::OpenDocument(..)
                | Message::CloseDocument(..)
                | Message::SaveDocument(..)
                | Message::DocTitleChanged { .. }
                | Message::DocBodyAction { .. }
                | Message::ToggleHistory(..)
                | Message::SelectCommit { .. }) => self.handle_document_panel(msg),

            // ── Contact panels ──────────────────────────────────────────
            msg @ (Message::OpenContact(..)
                | Message::CloseContactPanel(..)
                | Message::SelectConversation { .. }
                | Message::ContactPanelDragStart(..)
                | Message::ContactPanelDragMove { .. }
                | Message::ContactPanelDragEnd(..)) => self.handle_contact_panel(msg),

            // ── Inbox panel ─────────────────────────────────────────────
            msg @ (Message::InboxToggled
                | Message::InboxClose
                | Message::InboxSelectConversation(..)
                | Message::InboxBack
                | Message::InboxReplyChanged(..)
                | Message::InboxReplySubmit
                | Message::InboxDragStart
                | Message::InboxDragMove(..)
                | Message::InboxDragEnd) => self.handle_inbox(msg),

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
            Message::TaskbarContactClicked(contact_id) => {
                self.open_contact(&contact_id);
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
                if self.feed_canary(&input) {
                    return self.update(Message::CanaryTriggered);
                }
                self.chat.input = input;
                Task::none()
            }
            Message::ChatSubmitted => {
                let input = self.chat.input.trim().to_string();
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
                    Task::perform(
                        async move {
                            if let Err(e) = std::fs::write(&path, &data) {
                                tracing::error!("Failed to write file: {e}");
                            } else {
                                tracing::info!("Exported to {}", path.display());
                            }
                        },
                        |_| Message::Ignore,
                    )
                } else {
                    Task::none()
                }
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
                            if self.onboarding.is_some() {
                                return self.update(Message::OnboardingBack);
                            }
                            self.search.visible = false;
                            self.chat.visible = false;
                            self.taskbar_skills_dropdown_open = false;
                            for panel in &mut self.doc_panels {
                                panel.skills_overflow_open = false;
                            }
                        }
                        _ => {}
                    }
                }
                Task::none()
            }

            // ── Markdown formatting ─────────────────────────────────────────
            Message::TogglePreview(idx) => {
                if let Some(panel) = self.doc_panels.get_mut(idx) {
                    panel.preview_mode = !panel.preview_mode;
                    if panel.preview_mode {
                        panel.refresh_preview();
                    }
                }
                Task::none()
            }
            Message::FormatAction { panel_idx, kind } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    panel.apply_format(kind);
                }
                Task::none()
            }
            Message::MarkdownLinkClicked(url) => {
                if let Err(e) = open::that(&url) {
                    tracing::warn!("Failed to open URL {url}: {e}");
                }
                Task::none()
            }

            // ── Video ─────────────────────────────────────────────────────────
            Message::VideoPlay { panel_idx, video_idx } => {
                if let Some(panel) = self.doc_panels.get(panel_idx) {
                    if let Some(video) = panel.videos.get(video_idx) {
                        if let Err(e) = open::that(&video.path) {
                            tracing::error!("Failed to open video {}: {e}", video.path);
                        }
                    }
                }
                Task::none()
            }

            // ── Model management ──────────────────────────────────────────
            Message::CameraToggled => {
                self.camera_panel.visible = !self.camera_panel.visible;
                Task::none()
            }

            Message::ModelPanelToggled => {
                self.model_panel.visible = !self.model_panel.visible;
                if self.model_panel.visible && self.model_panel.models.is_empty() {
                    let dir = self.model_panel.model_dir.clone();
                    return Task::perform(
                        async move { ModelPanel::scan_models(dir) },
                        Message::ModelScanComplete,
                    );
                }
                Task::none()
            }
            Message::ModelRefresh => {
                let dir = self.model_panel.model_dir.clone();
                Task::perform(
                    async move { ModelPanel::scan_models(dir) },
                    Message::ModelScanComplete,
                )
            }
            Message::ModelScanComplete(models) => {
                self.model_panel.apply_scan(models);
                Task::none()
            }
            Message::ModelAssignRole { model_idx, role } => {
                self.model_panel.assign_role(model_idx, role);
                Task::none()
            }
            Message::ModelDelete(idx) => {
                if let Some(entry) = self.model_panel.models.get(idx) {
                    let dir = self.model_panel.model_dir.clone();
                    let filename = entry.filename.clone();
                    return Task::perform(
                        async move {
                            ModelPanel::delete_model_file(dir.clone(), filename);
                            ModelPanel::scan_models(dir)
                        },
                        Message::ModelScanComplete,
                    );
                }
                Task::none()
            }
            Message::ModelDeleteComplete => {
                Task::none()
            }

            Message::ThemeToggled => {
                theme::toggle_theme();
                Task::none()
            }

            // ── Onboarding ────────────────────────────────────────────────
            msg @ (Message::OnboardingNext
                | Message::OnboardingBack
                | Message::OnboardingNicknameChanged(..)
                | Message::OnboardingBubbleSelected(..)
                | Message::OnboardingThemeToggled
                | Message::OnboardingSeedToggled
                | Message::OnboardingPrimaryChanged(..)
                | Message::OnboardingPrimaryConfirmChanged(..)
                | Message::OnboardingDuressChanged(..)
                | Message::OnboardingDuressConfirmChanged(..)
                | Message::OnboardingCanaryChanged(..)
                | Message::OnboardingCanaryConfirmChanged(..)
                | Message::OnboardingEnrollInputChanged(..)
                | Message::OnboardingEnrollSubmit
                | Message::OnboardingFocusField(..)
                | Message::OnboardingTryAdvance
                | Message::OnboardingSkipAuth
                | Message::OnboardingComplete) => self.handle_onboarding(msg),

            // ── Login ─────────────────────────────────────────────────────
            Message::LoginPasswordChanged(val) => {
                if let Some(ref mut login) = self.login {
                    login.password_input = val;
                }
                Task::none()
            }
            Message::LoginSubmit => {
                // Auth logic will be wired in main.rs bootstrap
                // For now, just clear the login screen
                if let Some(ref mut login) = self.login {
                    login.error_message =
                        Some("Auth not yet wired — restart app".into());
                }
                Task::none()
            }

            // ── Canary / Lockdown ─────────────────────────────────────────
            Message::CanaryTriggered => {
                tracing::warn!("Canary phrase detected — initiating lockdown");
                self.lockdown_triggered = true;
                // Zeroize canary detector
                self.canary_detector = None;
                // Clear sensitive UI state
                self.chat = ChatState::new();
                self.search = SearchState::new();
                self.doc_panels.clear();
                self.contact_panels.clear();
                Task::none()
            }

            Message::Ignore => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        // Login screen (highest priority — blocks everything)
        if let Some(ref login) = self.login {
            return login.view();
        }
        // Lockdown screen (canary triggered)
        if self.lockdown_triggered {
            return self.view_lockdown();
        }
        // Onboarding overlay (full-screen, blocks everything else)
        if let Some(ref onboarding) = self.onboarding {
            return onboarding.view();
        }

        // Canvas: shader handles mouse events directly via Program::update()
        let canvas = shader(&self.canvas_program)
            .width(Length::Fill)
            .height(Length::Fill);

        let mut layers: Vec<Element<Message>> = vec![canvas.into()];

        // Floating document panels
        for (i, panel) in self.doc_panels.iter().enumerate() {
            if panel.visible {
                layers.push(panel.view(i, &self.skill_registry));
            }
        }

        // Floating contact panels
        for (i, panel) in self.contact_panels.iter().enumerate() {
            if panel.visible {
                layers.push(panel.view(i));
            }
        }

        // Inbox panel
        if let Some(ref inbox) = self.inbox_panel {
            if inbox.visible {
                layers.push(inbox.view());
            }
        }

        // Orchestrator bubble
        layers.push(self.bubble.view());

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

        // Model management panel (right side)
        if self.model_panel.visible {
            let model_positioned = container(self.model_panel.view())
                .width(Length::Fill)
                .padding(
                    iced::Padding::ZERO
                        .top(20.0)
                        .right(20.0),
                )
                .align_right(Length::Fill);
            layers.push(model_positioned.into());
        }

        // Camera panel (bottom-right)
        if self.camera_panel.visible {
            layers.push(self.camera_panel.view());
        }

        // Taskbar skills dropdown
        if self.taskbar_skills_dropdown_open {
            layers.push(self.view_taskbar_skills_dropdown());
        }

        // Search overlay (conditional)
        if self.search.visible {
            layers.push(self.search.view());
        }

        // Action-gate confirmation overlay (above everything)
        if let Some(confirm_overlay) = self.bubble.view_confirmation() {
            layers.push(confirm_overlay);
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

    fn view_lockdown(&self) -> Element<'_, Message> {
        use iced::widget::{column, container, text, Space};
        let content = column![
            text("Session expired")
                .size(24)
                .color(theme::text_primary()),
            Space::new().height(12),
            text("Please restart the application.")
                .size(14)
                .color(theme::text_dim()),
        ]
        .spacing(0)
        .padding(40)
        .width(420);

        container(container(content).style(theme::skill_panel_style))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(theme::dark_background)
            .into()
    }

    /// Feed text into the canary detector. Returns true if canary triggered.
    fn feed_canary(&mut self, text: &str) -> bool {
        if let Some(ref mut detector) = self.canary_detector {
            if detector.feed_str(text) {
                return true;
            }
        }
        false
    }

    // ── Handler methods (extracted from update()) ─────────────────────────

    fn handle_document_panel(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenDocument(doc_id) => {
                self.open_document(&doc_id);
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
                    let videos = panel.videos.clone();
                    let cf = ContentFields { body, images, videos };
                    let serialized = cf.serialize();
                    if let Some(ref cb) = self.save_callback {
                        cb(doc_id.clone(), title, serialized.clone());
                    }
                    if let Some(doc) = self.doc_map.get_mut(&doc_id) {
                        doc.content = serialized;
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
            Message::ToggleHistory(idx) => {
                if let Some(panel) = self.doc_panels.get_mut(idx) {
                    panel.show_history = !panel.show_history;
                    panel.selected_commit = None;
                }
                Task::none()
            }
            Message::SelectCommit { panel_idx, commit_idx } => {
                if let Some(panel) = self.doc_panels.get_mut(panel_idx) {
                    if panel.selected_commit == Some(commit_idx) {
                        panel.selected_commit = None;
                    } else {
                        panel.selected_commit = Some(commit_idx);
                    }
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn handle_contact_panel(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenContact(contact_id) => {
                self.open_contact(&contact_id);
                Task::none()
            }
            Message::CloseContactPanel(idx) => {
                if idx < self.contact_panels.len() {
                    self.contact_panels.remove(idx);
                }
                Task::none()
            }
            Message::SelectConversation { panel_idx, conv_idx } => {
                if let Some(panel) = self.contact_panels.get_mut(panel_idx) {
                    if conv_idx == usize::MAX {
                        panel.selected_conversation = None;
                    } else {
                        panel.selected_conversation = Some(conv_idx);
                    }
                }
                Task::none()
            }
            Message::ContactPanelDragStart(idx) => {
                if let Some(panel) = self.contact_panels.get_mut(idx) {
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
            Message::ContactPanelDragMove { panel_idx, local } => {
                if let Some(panel) = self.contact_panels.get_mut(panel_idx) {
                    panel.last_local_cursor = local;
                    if let (Some(start_screen), Some(start_panel)) =
                        (panel.drag_start_screen, panel.drag_start_panel)
                    {
                        let screen = iced::Point::new(
                            (panel.position.x - DEAD_ZONE) + local.x,
                            (panel.position.y - DEAD_ZONE) + local.y,
                        );
                        let dx = screen.x - start_screen.x;
                        let dy = screen.y - start_screen.y;
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
            Message::ContactPanelDragEnd(idx) => {
                if let Some(panel) = self.contact_panels.get_mut(idx) {
                    panel.dragging = false;
                    panel.drag_start_screen = None;
                    panel.drag_start_panel = None;
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn handle_inbox(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InboxToggled => {
                if self.inbox_panel.is_some() {
                    self.inbox_panel = None;
                } else {
                    let panel = InboxPanel::new(
                        self.conversations.clone(),
                        self.messages.clone(),
                    );
                    self.taskbar.inbox_unread = panel.total_unread();
                    self.inbox_panel = Some(panel);
                }
                Task::none()
            }
            Message::InboxClose => {
                self.inbox_panel = None;
                Task::none()
            }
            Message::InboxSelectConversation(idx) => {
                if let Some(ref mut panel) = self.inbox_panel {
                    panel.selected_conversation = Some(idx);
                }
                Task::none()
            }
            Message::InboxBack => {
                if let Some(ref mut panel) = self.inbox_panel {
                    panel.selected_conversation = None;
                }
                Task::none()
            }
            Message::InboxReplyChanged(text) => {
                if let Some(ref mut panel) = self.inbox_panel {
                    panel.reply_input = text;
                }
                Task::none()
            }
            Message::InboxReplySubmit => {
                if let Some(ref mut panel) = self.inbox_panel {
                    let body = panel.reply_input.clone();
                    if body.is_empty() {
                        return Task::none();
                    }

                    if let Some(conv_idx) = panel.selected_conversation {
                        if let Some(conv) = panel.conversations.get(conv_idx) {
                            let conv_id = conv.id_string().unwrap_or_default();
                            let to_addresses = conv.participant_contact_ids.clone();
                            let subject = Some(format!("Re: {}", conv.title));
                            let in_reply_to = panel.messages.iter()
                                .filter(|m| m.conversation_id == conv_id)
                                .last()
                                .and_then(|m| m.external_id.clone());

                            let request = crate::panels::inbox_panel::SendRequest {
                                conversation_id: conv_id,
                                to_addresses,
                                subject,
                                body,
                                in_reply_to,
                            };

                            if let Some(ref tx) = self.send_message_tx {
                                let _ = tx.try_send(request);
                            }
                        }
                    }

                    panel.reply_input.clear();
                }
                Task::none()
            }
            Message::InboxDragStart => {
                if let Some(ref mut panel) = self.inbox_panel {
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
            Message::InboxDragMove(local) => {
                if let Some(ref mut panel) = self.inbox_panel {
                    panel.last_local_cursor = local;
                    if let (Some(start_screen), Some(start_panel)) =
                        (panel.drag_start_screen, panel.drag_start_panel)
                    {
                        let screen = iced::Point::new(
                            (panel.position.x - DEAD_ZONE) + local.x,
                            (panel.position.y - DEAD_ZONE) + local.y,
                        );
                        let dx = screen.x - start_screen.x;
                        let dy = screen.y - start_screen.y;
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
            Message::InboxDragEnd => {
                if let Some(ref mut panel) = self.inbox_panel {
                    panel.dragging = false;
                    panel.drag_start_screen = None;
                    panel.drag_start_panel = None;
                }
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn handle_onboarding(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OnboardingNext => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.next_step();
                    return crate::onboarding::focus_first_input(ob.step);
                }
                Task::none()
            }
            Message::OnboardingBack => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.prev_step();
                    return crate::onboarding::focus_first_input(ob.step);
                }
                Task::none()
            }
            Message::OnboardingNicknameChanged(name) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.nickname = name;
                }
                Task::none()
            }
            Message::OnboardingBubbleSelected(style) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.bubble_style = style;
                }
                Task::none()
            }
            Message::OnboardingThemeToggled => {
                theme::toggle_theme();
                Task::none()
            }
            Message::OnboardingSeedToggled => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.seed_sample_data = !ob.seed_sample_data;
                }
                Task::none()
            }
            Message::OnboardingPrimaryChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.primary_password = val;
                    ob.validate_primary();
                }
                Task::none()
            }
            Message::OnboardingPrimaryConfirmChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.primary_confirm = val;
                    ob.validate_primary();
                }
                Task::none()
            }
            Message::OnboardingDuressChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.duress_password = val;
                    ob.validate_duress();
                }
                Task::none()
            }
            Message::OnboardingDuressConfirmChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.duress_confirm = val;
                    ob.validate_duress();
                }
                Task::none()
            }
            Message::OnboardingCanaryChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.canary_phrase = val;
                }
                Task::none()
            }
            Message::OnboardingCanaryConfirmChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.canary_confirm = val;
                }
                Task::none()
            }
            Message::OnboardingEnrollInputChanged(val) => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.current_enrollment_input = val;
                }
                Task::none()
            }
            Message::OnboardingEnrollSubmit => {
                if let Some(ref mut ob) = self.onboarding {
                    if ob.current_enrollment_input == ob.primary_password {
                        let keystrokes =
                            std::mem::take(&mut ob.current_enrollment_keystrokes);
                        ob.enrollment_samples.push(keystrokes);
                        ob.current_enrollment_input.clear();
                        ob.enrollment_error = None;
                    } else {
                        ob.enrollment_error =
                            Some("Password doesn't match — try again.".into());
                        ob.current_enrollment_input.clear();
                        ob.current_enrollment_keystrokes.clear();
                    }
                }
                Task::none()
            }
            Message::OnboardingFocusField(id) => {
                iced::widget::operation::focus(iced::widget::Id::new(id))
            }
            Message::OnboardingTryAdvance => {
                if let Some(ref mut ob) = self.onboarding {
                    if ob.can_advance() {
                        ob.next_step();
                        if ob.step == crate::onboarding::OnboardingStep::Complete {
                            return self.handle_onboarding(Message::OnboardingComplete);
                        }
                        return crate::onboarding::focus_first_input(ob.step);
                    }
                }
                Task::none()
            }
            Message::OnboardingSkipAuth => {
                if let Some(ref mut ob) = self.onboarding {
                    ob.next_step();
                    if ob.step == crate::onboarding::OnboardingStep::Complete {
                        return self.handle_onboarding(Message::OnboardingComplete);
                    }
                    return crate::onboarding::focus_first_input(ob.step);
                }
                Task::none()
            }
            Message::OnboardingComplete => {
                if let Some(ref ob) = self.onboarding {
                    self.bubble.bubble_style = ob.bubble_style;
                    let dir = sovereign_data_dir();
                    if let Ok(mut profile) = sovereign_core::profile::UserProfile::load(&dir) {
                        profile.designation = ob.designation.clone();
                        if !ob.nickname.is_empty() {
                            profile.nickname = Some(ob.nickname.clone());
                        }
                        profile.bubble_style = ob.bubble_style;
                        let _ = profile.save(&dir);
                    }
                }
                self.onboarding = None;
                let dir = sovereign_data_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join("onboarding_done"), "1");
                Task::none()
            }
            _ => Task::none(),
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn poll_channels(&mut self) {
        // Check for double-click → open document from the canvas shader
        {
            let mut st = self.canvas_state.lock().unwrap();
            if let Some(doc_id) = st.pending_open.take() {
                drop(st);
                self.open_document(&doc_id);
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

        // Poll camera frame
        self.camera_panel.poll_frame();
    }

    fn handle_orchestrator_event(&mut self, event: OrchestratorEvent) {
        match event {
            OrchestratorEvent::SearchResults { ref doc_ids, .. } => {
                self.search.results = doc_ids
                    .iter()
                    .map(|id| {
                        let title = self
                            .doc_map
                            .get(id)
                            .map(|d| d.title.clone())
                            .unwrap_or_else(|| id.clone());
                        crate::search::SearchResult {
                            id: id.clone(),
                            title,
                        }
                    })
                    .collect();
                let mut st = self.canvas_state.lock().unwrap();
                for id in doc_ids {
                    st.highlighted.insert(id.clone());
                }
                st.mark_dirty();
            }
            OrchestratorEvent::DocumentOpened { ref doc_id } => {
                sovereign_canvas::apply_command(
                    &self.canvas_state,
                    CanvasCommand::NavigateTo(doc_id.clone()),
                );
                self.open_document(doc_id);
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

                // Add a card to the canvas layout so it appears immediately
                {
                    let mut st = self.canvas_state.lock().unwrap();
                    // Find the lane for this thread and place after the last card in it
                    let lane_y = st
                        .layout
                        .lanes
                        .iter()
                        .find(|l| l.thread_id == *thread_id)
                        .map(|l| l.y + sovereign_canvas::layout::LANE_PADDING_TOP)
                        .unwrap_or(0.0);
                    // Place at the global "Now" edge (rightmost card across ALL
                    // threads), so the new card appears at the current moment on
                    // the timeline — not squeezed among older cards in the lane.
                    let global_now_x = st
                        .layout
                        .cards
                        .iter()
                        .map(|c| c.x + c.w + sovereign_canvas::layout::CARD_SPACING_H)
                        .fold(sovereign_canvas::layout::LANE_HEADER_WIDTH, f32::max);
                    st.layout.cards.push(sovereign_canvas::layout::CardLayout {
                        doc_id: doc_id.clone(),
                        title: title.clone(),
                        is_owned: true,
                        thread_id: thread_id.clone(),
                        created_at_ts: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0),
                        x: global_now_x,
                        y: lane_y,
                        w: sovereign_canvas::layout::CARD_WIDTH,
                        h: sovereign_canvas::layout::CARD_HEIGHT,
                    });
                    st.mark_dirty();
                }

                // Navigate to the new document and open it
                sovereign_canvas::apply_command(
                    &self.canvas_state,
                    CanvasCommand::NavigateTo(doc_id.clone()),
                );
                self.open_document(doc_id);
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
            // P2P sync events
            OrchestratorEvent::SyncStatus { ref status, .. } => {
                self.bubble.show_skill_result(&format!("Sync: {status}"));
            }
            OrchestratorEvent::SyncConflict { ref doc_id, ref description } => {
                self.bubble.show_skill_result(&format!("Conflict on {doc_id}: {description}"));
                let mut st = self.canvas_state.lock().unwrap();
                st.highlighted.insert(doc_id.clone());
                st.mark_dirty();
            }
            OrchestratorEvent::DeviceDiscovered { ref device_name, .. } => {
                self.bubble.show_skill_result(&format!("Device found: {device_name}"));
            }
            OrchestratorEvent::DevicePaired { ref device_id } => {
                self.bubble.show_skill_result(&format!("Paired with {device_id}"));
            }
            // Comms events — refresh inbox
            OrchestratorEvent::NewMessagesReceived { ref channel, count, .. } => {
                self.bubble.show_skill_result(&format!("{count} new {channel} messages"));
                if let Some(ref mut inbox) = self.inbox_panel {
                    inbox.refresh(self.conversations.clone(), self.messages.clone());
                    self.taskbar.inbox_unread = inbox.total_unread();
                }
            }
            OrchestratorEvent::CommsSyncComplete { ref channel, new_messages } => {
                if new_messages > 0 {
                    self.bubble.show_skill_result(
                        &format!("{channel} sync: {new_messages} new messages"),
                    );
                }
            }
            OrchestratorEvent::CommsSyncError { ref channel, ref error } => {
                self.bubble.show_skill_result(&format!("{channel} sync error: {error}"));
            }
            OrchestratorEvent::ContactCreated { ref name, .. } => {
                self.bubble.show_skill_result(&format!("New contact: {name}"));
            }
            _ => {
                tracing::debug!("Unhandled orchestrator event: {:?}", event);
            }
        }
    }

    /// Open a document in a floating panel (no-op if already open).
    fn open_document(&mut self, doc_id: &str) {
        if self.doc_panels.iter().any(|p| p.doc_id == doc_id) {
            return;
        }
        if let Some(doc) = self.doc_map.get(doc_id) {
            let content = ContentFields::parse(&doc.content);
            let commits = self.commits_map.get(doc_id).cloned().unwrap_or_default();
            let panel = FloatingPanel::new(
                doc_id.to_string(),
                doc.title.clone(),
                content.body,
                content.images,
                content.videos,
                commits,
            );
            self.doc_panels.push(panel);
            self.taskbar.add_document(doc_id, &doc.title, doc.is_owned);
        }
    }

    /// Open a contact in a floating panel (no-op if already open).
    fn open_contact(&mut self, contact_id: &str) {
        if self.contact_panels.iter().any(|p| p.contact_id == contact_id) {
            return;
        }
        if let Some(contact) = self.contact_map.get(contact_id) {
            // Find conversations this contact participates in
            let convs: Vec<Conversation> = self
                .conversations
                .iter()
                .filter(|c| c.participant_contact_ids.contains(&contact_id.to_string()))
                .cloned()
                .collect();

            // Collect messages for those conversations
            let conv_ids: Vec<String> = convs
                .iter()
                .filter_map(|c| c.id_string())
                .collect();
            let msgs: Vec<DbMessage> = self
                .messages
                .iter()
                .filter(|m| conv_ids.contains(&m.conversation_id))
                .cloned()
                .collect();

            let panel = ContactPanel::new(
                contact.clone(),
                contact_id.to_string(),
                convs,
                msgs,
            );
            self.contact_panels.push(panel);
        }
    }

    fn handle_skill_event(&mut self, event: SkillEvent) {
        match event {
            SkillEvent::OpenDocument { ref doc_id } => {
                self.open_document(doc_id);
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
                tracing::debug!("Executing skill '{skill_name}' action '{action_id}'");
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
            } else {
                tracing::warn!("Skill '{skill_name}' not found in registry");
                self.bubble.show_skill_result(&format!("Skill '{skill_name}' not available"));
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

    fn view_taskbar_skills_dropdown(&self) -> Element<'_, Message> {
        use iced::widget::{button, column, container, scrollable, text};
        use iced::Padding;

        let has_active_doc = !self.doc_panels.is_empty();
        let mut col = column![].spacing(4).padding(8);

        for skill in self.skill_registry.all_skills() {
            let skill_name = skill.name().to_string();
            for (action_id, action_label) in skill.actions() {
                let enabled = has_active_doc
                    || action_id == "search"
                    || action_id == "import";
                let btn = button(text(action_label).size(13))
                    .on_press_maybe(
                        enabled.then(|| Message::SkillExecuted(skill_name.clone(), action_id.clone())),
                    )
                    .style(theme::skill_button_style)
                    .padding(Padding::from([6, 14]));
                col = col.push(btn);
            }
        }

        container(
            container(scrollable(col).height(Length::Shrink))
                .max_height(400.0)
                .style(theme::skill_panel_style),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_right(Length::Fill)
        .align_bottom(Length::Fill)
        .padding(Padding::ZERO.right(200.0).bottom(48.0))
        .into()
    }
}

/// Path to the Sovereign data directory (~/.sovereign or %USERPROFILE%/.sovereign).
pub fn sovereign_data_dir() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join(".sovereign")
}

/// Returns true if this is the first launch (onboarding not yet completed).
pub fn is_first_launch() -> bool {
    !sovereign_data_dir().join("onboarding_done").exists()
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
    .window(iced::window::Settings {
        size: iced::Size::new(1280.0, 800.0),
        icon: iced::window::icon::from_file_data(
            include_bytes!("../assets/icon.png"),
            None,
        )
        .ok(),
        ..Default::default()
    })
    .antialiasing(true)
    .run()
}
