mod cli;
mod commands;
mod duress;
mod seed;
mod setup;

#[cfg(feature = "tauri-ui")]
mod tauri_commands;
#[cfg(feature = "tauri-ui")]
mod tauri_events;
#[cfg(feature = "tauri-ui")]
mod tauri_state;

#[cfg(feature = "web-browse")]
mod web;
#[cfg(feature = "tauri-ui")]
mod browser;

use std::sync::mpsc;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::{FeedbackEvent, OrchestratorEvent};
use sovereign_core::security::ActionDecision;
use sovereign_core::lifecycle;
use sovereign_db::GraphDB;

#[cfg(feature = "comms")]
use sovereign_comms::CommsSync;

use cli::{Cli, Commands};
use setup::create_db;

fn main() -> Result<()> {
    lifecycle::init_tracing();

    let cli = Cli::parse();
    let config = AppConfig::load_or_default(cli.config.as_deref());

    // Create a manual tokio runtime — Iced 0.14 (with `tokio` feature) creates
    // its own runtime, so we must NOT have an active runtime on the main thread
    // when calling run_app().
    let rt = tokio::runtime::Runtime::new()?;

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            #[cfg(feature = "tauri-ui")]
            {
                run_tauri(&config, &rt)?;
            }
            #[cfg(not(feature = "tauri-ui"))]
            {
                run_gui(&config, &rt)?;
            }
        }

        Commands::CreateDoc { title, thread_id, is_owned } => {
            rt.block_on(commands::create_doc(&config, title, thread_id, is_owned))?;
        }
        Commands::GetDoc { id } => {
            rt.block_on(commands::get_doc(&config, id))?;
        }
        Commands::ListDocs { thread_id } => {
            rt.block_on(commands::list_docs(&config, thread_id))?;
        }
        Commands::UpdateDoc { id, title, content } => {
            rt.block_on(commands::update_doc(&config, id, title, content))?;
        }
        Commands::DeleteDoc { id } => {
            rt.block_on(commands::delete_doc(&config, id))?;
        }
        Commands::CreateThread { name, description } => {
            rt.block_on(commands::create_thread(&config, name, description))?;
        }
        Commands::ListThreads => {
            rt.block_on(commands::list_threads(&config))?;
        }
        Commands::AddRelationship { from, to, relation_type, strength } => {
            rt.block_on(commands::add_relationship(&config, from, to, relation_type, strength))?;
        }
        Commands::ListRelationships { doc_id } => {
            rt.block_on(commands::list_relationships(&config, doc_id))?;
        }
        Commands::Commit { doc_id, message } => {
            rt.block_on(commands::commit_doc(&config, doc_id, message))?;
        }
        Commands::ListCommits { doc_id } => {
            rt.block_on(commands::list_commits(&config, doc_id))?;
        }

        #[cfg(feature = "encryption")]
        Commands::EncryptData => {
            let (_, key_db, kek) = setup::init_crypto()?;
            rt.block_on(commands::encrypt_data(&config, key_db, kek))?;
        }

        #[cfg(feature = "p2p")]
        Commands::PairDevice { peer_id } => {
            println!("Pairing with peer {peer_id}...");
            println!("(P2P pairing requires a running `sovereign run` instance)");
            println!("Use the orchestrator command: 'pair device {peer_id}'");
        }
        #[cfg(feature = "p2p")]
        Commands::ListDevices => {
            let dir = setup::crypto_dir().join("paired_devices.json");
            if dir.exists() {
                let content = std::fs::read_to_string(&dir)?;
                println!("{content}");
            } else {
                println!("No paired devices.");
            }
        }
        #[cfg(feature = "p2p")]
        Commands::EnrollGuardian { name, peer_id } => {
            println!("Enrolling guardian '{name}' (peer: {peer_id})...");
            println!("(Guardian enrollment requires a running `sovereign run` instance)");
            println!("Use the orchestrator command: 'enroll guardian {name}'");
        }

        #[cfg(feature = "encryption")]
        Commands::ListGuardians => {
            let dir = setup::crypto_dir().join("guardians.json");
            if dir.exists() {
                let content = std::fs::read_to_string(&dir)?;
                println!("{content}");
            } else {
                println!("No guardians enrolled.");
            }
        }
        #[cfg(feature = "encryption")]
        Commands::InitiateRecovery => {
            println!("Key recovery requires at least 3 of 5 guardian shards.");
            println!("(Recovery flow requires a running `sovereign run` instance with P2P)");
            println!("Use the orchestrator command: 'initiate recovery'");
        }

        Commands::ListContacts => {
            rt.block_on(commands::list_contacts(&config))?;
        }
        Commands::ListConversations { channel } => {
            rt.block_on(commands::list_conversations(&config, channel))?;
        }
    }

    Ok(())
}

/// Launch the Tauri web UI: initialize backend subsystems, then start Tauri.
#[cfg(feature = "tauri-ui")]
fn run_tauri(config: &AppConfig, rt: &tokio::runtime::Runtime) -> Result<()> {
    // Compute profile directory (~/.sovereign)
    let profile_dir = sovereign_core::home_dir().join(".sovereign");

    // Run async backend setup inside the existing runtime
    let (db, orchestrator, skill_registry, skill_db, decision_tx, feedback_tx, orch_tx, orch_rx, autocommit, model_assignments) =
        rt.block_on(async {
            let db = create_db(config).await?;
            seed::seed_if_empty(&db).await?;

            // Seed user profile and session log history
            let profile_dir = sovereign_core::home_dir()
                .join(".sovereign")
                .join("orchestrator");
            if let Err(e) = seed::seed_profile_and_history(&profile_dir) {
                tracing::warn!("Profile/history seed failed: {e}");
            }

            let db_arc = Arc::new(db);

            // Register skills
            let mut registry = sovereign_skills::SkillRegistry::new();
            let skill_db: Arc<dyn sovereign_skills::SkillDbAccess> =
                sovereign_skills::wrap_db(db_arc.clone());
            registry.register(Box::new(sovereign_skills::skills::text_editor::TextEditorSkill));
            registry.register(Box::new(sovereign_skills::skills::image::ImageSkill));
            registry.register(Box::new(sovereign_skills::skills::pdf_export::PdfExportSkill));
            registry.register(Box::new(sovereign_skills::skills::word_count::WordCountSkill));
            registry.register(Box::new(sovereign_skills::skills::find_replace::FindReplaceSkill));
            registry.register(Box::new(sovereign_skills::skills::search::SearchSkill));
            registry.register(Box::new(sovereign_skills::skills::file_import::FileImportSkill));
            registry.register(Box::new(sovereign_skills::skills::duplicate_document::DuplicateDocumentSkill));
            registry.register(Box::new(sovereign_skills::skills::markdown_editor::MarkdownEditorSkill));
            registry.register(Box::new(sovereign_skills::skills::video::VideoSkill));
            tracing::info!("Registered {} core skills", registry.all_skills().len());

            // Create orchestrator channels
            let (orch_tx, orch_rx) = mpsc::channel::<OrchestratorEvent>();
            let (decision_tx, decision_rx) = tokio::sync::mpsc::channel::<ActionDecision>(32);
            let (feedback_tx, feedback_rx) = tokio::sync::mpsc::channel::<FeedbackEvent>(32);

            // Initialize orchestrator
            let db_dyn: Arc<dyn sovereign_db::GraphDB> = db_arc.clone();
            let orchestrator = match sovereign_ai::Orchestrator::new(
                config.ai.clone(),
                db_dyn,
                orch_tx.clone(),
            )
            .await
            {
                Ok(mut o) => {
                    o.set_decision_rx(decision_rx);
                    o.set_feedback_rx(feedback_rx);
                    tracing::info!("AI orchestrator initialized (Tauri mode)");
                    Some(Arc::new(o))
                }
                Err(e) => {
                    tracing::warn!("AI orchestrator unavailable: {e}");
                    None
                }
            };

            // Auto-commit engine
            let autocommit = Arc::new(tokio::sync::Mutex::new(
                sovereign_ai::AutoCommitEngine::new(db_arc.clone()),
            ));

            // Model assignments from config
            let model_assignments = tauri_state::ModelAssignments {
                router: config.ai.router_model.clone(),
                reasoning: config.ai.reasoning_model.clone(),
            };

            Ok::<_, anyhow::Error>((
                db_arc, orchestrator, Arc::new(registry), skill_db,
                decision_tx, feedback_tx, orch_tx, orch_rx,
                autocommit, model_assignments,
            ))
        })?;

    // Wrap orch_rx so it can be moved into the setup closure
    let orch_rx = std::sync::Mutex::new(Some(orch_rx));

    // Clone DB for purge background task
    let purge_db = db.clone();

    // Clone orchestrator for consolidation idle-watcher
    let consolidation_orch = orchestrator.clone();

    // Build Tauri app
    tauri::Builder::default()
        .manage(tauri_state::AppState {
            db,
            orchestrator,
            config: config.clone(),
            skill_registry,
            skill_db,
            decision_tx,
            feedback_tx,
            orch_tx,
            theme: std::sync::Mutex::new("dark".to_string()),
            autocommit: autocommit.clone(),
            model_assignments: std::sync::Mutex::new(model_assignments),
            profile_dir,
        })
        .invoke_handler(tauri::generate_handler![
            tauri_commands::greet,
            tauri_commands::get_status,
            tauri_commands::chat_message,
            tauri_commands::search_documents,
            tauri_commands::search_query,
            tauri_commands::approve_action,
            tauri_commands::reject_action,
            tauri_commands::accept_suggestion,
            tauri_commands::dismiss_suggestion,
            // Memory consolidation — AI-suggested links
            tauri_commands::list_pending_suggestions,
            tauri_commands::accept_link_suggestion,
            tauri_commands::dismiss_link_suggestion,
            tauri_commands::trigger_consolidation,
            tauri_commands::list_documents,
            tauri_commands::list_threads,
            tauri_commands::toggle_theme,
            tauri_commands::get_theme,
            // Phase 2: Document CRUD + version history + skills + models
            tauri_commands::get_document,
            tauri_commands::save_document,
            tauri_commands::create_document,
            tauri_commands::close_document,
            tauri_commands::list_commits,
            tauri_commands::restore_commit,
            tauri_commands::list_skills_for_doc,
            tauri_commands::execute_skill,
            tauri_commands::list_all_skills,
            tauri_commands::scan_models,
            tauri_commands::assign_model_role,
            tauri_commands::delete_model,
            // Phase 3: Canvas + threads + contacts + messaging
            tauri_commands::canvas_load,
            tauri_commands::update_document_position,
            tauri_commands::create_thread,
            tauri_commands::update_thread,
            tauri_commands::delete_thread,
            tauri_commands::move_document_to_thread,
            tauri_commands::list_contacts,
            tauri_commands::get_contact_detail,
            tauri_commands::list_conversations,
            tauri_commands::list_messages,
            tauri_commands::mark_message_read,
            tauri_commands::create_relationship,
            // Phase 4: Auth, onboarding, settings, document deletion
            tauri_commands::check_auth_state,
            tauri_commands::validate_password,
            tauri_commands::validate_password_policy,
            tauri_commands::complete_onboarding,
            tauri_commands::get_profile,
            tauri_commands::save_profile,
            tauri_commands::get_config,
            tauri_commands::delete_document,
            // Phase 5: Trust, import, comms
            tauri_commands::get_trust_entries,
            tauri_commands::reset_trust_action,
            tauri_commands::reset_trust_all,
            tauri_commands::import_file,
            tauri_commands::get_comms_config,
            tauri_commands::save_comms_config,
            // Embedded browser
            tauri_commands::open_browser,
            tauri_commands::close_browser,
            tauri_commands::navigate_browser,
            tauri_commands::browser_back,
            tauri_commands::browser_forward,
            tauri_commands::browser_refresh,
            tauri_commands::set_browser_bounds,
            tauri_commands::set_browser_visible,
            // Web browsing — fetch & reliability
            tauri_commands::fetch_web_page,
            tauri_commands::save_web_page,
            tauri_commands::assess_reliability,
            tauri_commands::reassess_reliability,
        ])
        .setup(move |app| {
            // Auto-open DevTools in debug builds
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // Start the event forwarder — drains orch_rx and emits Tauri events
            if let Some(rx) = orch_rx.lock().unwrap().take() {
                tauri_events::spawn_event_forwarder(app.handle().clone(), rx);
            }

            // Periodic auto-commit check (every 30s)
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    autocommit.lock().await.check_and_commit().await;
                }
            });

            // Hourly purge of soft-deleted items older than 30 days
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
                loop {
                    interval.tick().await;
                    let max_age = std::time::Duration::from_secs(30 * 24 * 3600);
                    match (*purge_db).purge_deleted(max_age).await {
                        Ok(n) if n > 0 => tracing::info!("Purged {n} soft-deleted items"),
                        Err(e) => tracing::warn!("Purge failed: {e}"),
                        _ => {}
                    }
                }
            });

            // Memory consolidation idle-watcher — runs only when:
            // 1. Orchestrator exists and model is idle (mutex free)
            // 2. User hasn't interacted for 60s (cooldown)
            // 3. At least 5 minutes between consolidation runs
            if let Some(orch) = consolidation_orch {
                tauri::async_runtime::spawn(async move {
                    use std::time::{Duration, Instant};
                    let check_interval = Duration::from_secs(30);
                    let post_run_cooldown = Duration::from_secs(300);
                    let mut last_run = Instant::now() - post_run_cooldown; // allow first run after idle_cooldown

                    loop {
                        tokio::time::sleep(check_interval).await;

                        // Skip if model is busy
                        if !orch.is_model_idle() {
                            continue;
                        }

                        // Skip if not enough time since last run
                        if last_run.elapsed() < post_run_cooldown {
                            continue;
                        }

                        // Run consolidation
                        match orch.consolidate_memory().await {
                            Ok(()) => {
                                tracing::debug!("Memory consolidation cycle completed");
                            }
                            Err(e) => {
                                tracing::warn!("Memory consolidation failed: {e}");
                            }
                        }
                        last_run = Instant::now();
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running Sovereign GE (Tauri)");

    Ok(())
}

/// Launch the GUI: initialize all subsystems and start the Iced application.
#[cfg(feature = "iced-ui")]
fn run_gui(config: &AppConfig, rt: &tokio::runtime::Runtime) -> Result<()> {
    // Scan skills directory for manifests
    let mut registry = sovereign_skills::SkillRegistry::new();
    let skills_dir = std::path::Path::new("skills");
    if skills_dir.exists() {
        registry.scan_directory(skills_dir)?;
        tracing::info!("Loaded {} skill manifests", registry.manifests().len());
        for manifest in registry.manifests() {
            tracing::info!(
                "  - {} v{} ({})",
                manifest.name,
                manifest.version,
                manifest.description
            );
        }
    }

    // Initialize crypto subsystem if enabled
    #[cfg(feature = "encryption")]
    let _crypto_state = if config.crypto.enabled {
        match setup::init_crypto() {
            Ok((device_key, key_db, kek)) => {
                Some((device_key, key_db, kek))
            }
            Err(e) => {
                tracing::warn!("Crypto init failed (continuing without encryption): {e}");
                None
            }
        }
    } else {
        None
    };

    // Run all async setup inside rt.block_on, then launch Iced outside
    // so the main thread has no active tokio context.
    let (app, _boot_task) = rt.block_on(async {
        // Load documents and threads from DB for the canvas
        let db = create_db(config).await?;
        seed::seed_if_empty(&db).await?;

        // Seed user profile and session log history for testing
        let profile_dir = sovereign_core::home_dir()
            .join(".sovereign")
            .join("orchestrator");
        if let Err(e) = seed::seed_profile_and_history(&profile_dir) {
            tracing::warn!("Profile/history seed failed: {e}");
        }

        // Parallelize the 6 independent DB queries.
        let (threads, documents, relationships, contacts, conversations, all_messages) = tokio::try_join!(
            db.list_threads(),
            db.list_documents(None),
            db.list_all_relationships(),
            db.list_contacts(),
            db.list_conversations(None),
            db.list_all_messages(),
        )?;

        // Lazy-load commits: pass empty map, load on demand when user opens version history.
        let commits_map = std::collections::HashMap::new();

        tracing::info!(
            "Loaded {} documents, {} threads, {} relationships, {} contacts, {} conversations, {} messages",
            documents.len(),
            threads.len(),
            relationships.len(),
            contacts.len(),
            conversations.len(),
            all_messages.len(),
        );

        // Register all core skills
        let db_arc_for_skills = Arc::new(db);
        let skill_db: std::sync::Arc<dyn sovereign_skills::SkillDbAccess> =
            sovereign_skills::wrap_db(db_arc_for_skills.clone());
        registry.register(Box::new(
            sovereign_skills::skills::text_editor::TextEditorSkill,
        ));
        registry.register(Box::new(sovereign_skills::skills::image::ImageSkill));
        registry.register(Box::new(
            sovereign_skills::skills::pdf_export::PdfExportSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::word_count::WordCountSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::find_replace::FindReplaceSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::search::SearchSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::file_import::FileImportSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::duplicate_document::DuplicateDocumentSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::markdown_editor::MarkdownEditorSkill,
        ));
        registry.register(Box::new(
            sovereign_skills::skills::video::VideoSkill,
        ));
        tracing::info!("Registered {} core skills", registry.all_skills().len());

        // Load WASM community skills (if feature enabled)
        #[cfg(feature = "wasm-plugins")]
        {
            match registry.load_wasm_skills(&skills_dir) {
                Ok(n) if n > 0 => tracing::info!("Loaded {n} WASM skills"),
                Ok(_) => {}
                Err(e) => tracing::warn!("WASM skill loading failed: {e}"),
            }
        }

        // Create event channels
        let (orch_tx, orch_rx) = mpsc::channel::<OrchestratorEvent>();
        let (decision_tx, decision_rx) = tokio::sync::mpsc::channel::<ActionDecision>(32);
        let (feedback_tx, feedback_rx) = tokio::sync::mpsc::channel::<FeedbackEvent>(32);

        // Try to initialize AI orchestrator
        let db_arc = db_arc_for_skills;
        let db_dyn: std::sync::Arc<dyn sovereign_db::GraphDB> = db_arc.clone();
        let orchestrator = match sovereign_ai::Orchestrator::new(
            config.ai.clone(),
            db_dyn,
            orch_tx.clone(),
        )
        .await
        {
            Ok(mut o) => {
                o.set_decision_rx(decision_rx);
                o.set_feedback_rx(feedback_rx);

                // Wire P2P channels if crypto + P2P are enabled
                #[cfg(feature = "p2p")]
                if config.p2p.enabled {
                    if let Some((ref device_key, _, _)) = _crypto_state {
                        match sovereign_p2p::identity::derive_keypair(device_key) {
                            Ok(keypair) => {
                                let p2p_config = sovereign_p2p::config::P2pConfig {
                                    enabled: true,
                                    listen_port: config.p2p.listen_port,
                                    rendezvous_server: config.p2p.rendezvous_server.clone(),
                                    device_name: config.p2p.device_name.clone(),
                                };
                                let (p2p_event_tx, p2p_event_rx) =
                                    tokio::sync::mpsc::channel(256);
                                let (p2p_cmd_tx, p2p_cmd_rx) =
                                    tokio::sync::mpsc::channel(64);

                                let sync_service = Arc::new(
                                    sovereign_p2p::SyncService::new(
                                        db_arc.clone(),
                                        p2p_config.device_name.clone(),
                                    ),
                                );
                                match sovereign_p2p::node::SovereignNode::new(
                                    &p2p_config, keypair, p2p_event_tx, p2p_cmd_rx, sync_service,
                                ) {
                                    Ok(node) => {
                                        tokio::spawn(node.run());
                                        o.set_p2p_channels(p2p_cmd_tx, p2p_event_rx);
                                        tracing::info!("P2P node spawned");
                                    }
                                    Err(e) => {
                                        tracing::warn!("P2P node failed to start: {e}");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("P2P identity derivation failed: {e}");
                            }
                        }
                    } else {
                        tracing::warn!(
                            "P2P enabled but crypto not initialized — skipping P2P"
                        );
                    }
                }

                // Enable session log encryption if crypto is initialized
                #[cfg(all(feature = "encryption", feature = "encrypted-log"))]
                if let Some((ref device_key, _, _)) = _crypto_state {
                    let session_key = setup::derive_session_log_key(device_key);
                    o.set_session_log_key(session_key);
                }

                tracing::info!("AI orchestrator initialized");
                Some(Arc::new(o))
            }
            Err(e) => {
                tracing::warn!("AI orchestrator unavailable: {e}");
                None
            }
        };

        // Channel for inbox reply → async send task
        let send_message_tx: Option<
            tokio::sync::mpsc::Sender<sovereign_ui::panels::inbox_panel::SendRequest>,
        > = None;
        // Wire comms sync if enabled
        #[cfg(feature = "comms")]
        if config.comms.enabled {
            let (comms_event_tx, mut comms_event_rx) =
                tokio::sync::mpsc::channel::<sovereign_comms::CommsEvent>(256);
            let mut comms_sync = CommsSync::new(
                comms_event_tx,
                config.comms.poll_interval_secs,
            );

            // Add email channel if configured
            #[cfg(feature = "comms-email")]
            {
                if let Ok(password) = std::env::var("SOVEREIGN_EMAIL_PASSWORD") {
                    let email_config = sovereign_comms::EmailAccountConfig {
                        imap_host: std::env::var("SOVEREIGN_IMAP_HOST")
                            .unwrap_or_default(),
                        imap_port: std::env::var("SOVEREIGN_IMAP_PORT")
                            .ok()
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(993),
                        smtp_host: std::env::var("SOVEREIGN_SMTP_HOST")
                            .unwrap_or_default(),
                        smtp_port: std::env::var("SOVEREIGN_SMTP_PORT")
                            .ok()
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(587),
                        username: std::env::var("SOVEREIGN_EMAIL_USER")
                            .unwrap_or_default(),
                        display_name: std::env::var("SOVEREIGN_EMAIL_NAME").ok(),
                    };

                    if !email_config.imap_host.is_empty() {
                        // Sync instance (moved into CommsSync)
                        let email_channel_sync =
                            sovereign_comms::channels::email::EmailChannel::new(
                                email_config.clone(),
                                db_arc.clone(),
                                password.clone(),
                            );
                        comms_sync.add_channel(Box::new(email_channel_sync));

                        // Send instance (for inbox reply)
                        let email_channel_send =
                            sovereign_comms::channels::email::EmailChannel::new(
                                email_config,
                                db_arc.clone(),
                                password,
                            );
                        let (stx, mut srx) = tokio::sync::mpsc::channel::<
                            sovereign_ui::panels::inbox_panel::SendRequest,
                        >(32);
                        send_message_tx = Some(stx);
                        tokio::spawn(async move {
                            use sovereign_comms::channel::{CommunicationChannel, OutgoingMessage};
                            while let Some(req) = srx.recv().await {
                                let msg = OutgoingMessage {
                                    to: req.to_addresses,
                                    subject: req.subject,
                                    body: req.body,
                                    body_html: None,
                                    in_reply_to: req.in_reply_to,
                                    conversation_id: Some(req.conversation_id),
                                };
                                match email_channel_send.send_message(&msg).await {
                                    Ok(id) => tracing::info!("Reply sent: {id}"),
                                    Err(e) => tracing::error!("Reply send failed: {e}"),
                                }
                            }
                        });

                        tracing::info!("Email channel registered");
                    }
                }
            }

            // Add Signal channel if configured
            #[cfg(feature = "comms-signal")]
            {
                let signal_phone = std::env::var("SOVEREIGN_SIGNAL_PHONE")
                    .unwrap_or_default();
                if !signal_phone.is_empty() {
                    let signal_config = sovereign_comms::SignalAccountConfig {
                        phone_number: signal_phone,
                        store_path: std::env::var("SOVEREIGN_SIGNAL_STORE")
                            .unwrap_or_else(|_| {
                                sovereign_core::home_dir()
                                    .join(".sovereign/signal")
                                    .to_string_lossy()
                                    .into_owned()
                            }),
                        device_name: std::env::var("SOVEREIGN_SIGNAL_NAME").ok(),
                    };
                    let signal_channel =
                        sovereign_comms::channels::signal::SignalChannel::new(
                            signal_config,
                            db_arc.clone(),
                        );
                    comms_sync.add_channel(Box::new(signal_channel));
                    tracing::info!("Signal channel registered");
                }
            }

            // Add WhatsApp channel if configured
            #[cfg(feature = "comms-whatsapp")]
            {
                if let Ok(token) = std::env::var("SOVEREIGN_WHATSAPP_TOKEN") {
                    let wa_phone_id = std::env::var("SOVEREIGN_WHATSAPP_PHONE_ID")
                        .unwrap_or_default();
                    let wa_biz_id = std::env::var("SOVEREIGN_WHATSAPP_BUSINESS_ID")
                        .unwrap_or_default();
                    if !wa_phone_id.is_empty() {
                        let wa_config = sovereign_comms::WhatsAppAccountConfig {
                            phone_number_id: wa_phone_id,
                            business_account_id: wa_biz_id,
                            api_url: std::env::var("SOVEREIGN_WHATSAPP_API_URL")
                                .unwrap_or_else(|_| {
                                    "https://graph.facebook.com".into()
                                }),
                            api_version: std::env::var("SOVEREIGN_WHATSAPP_API_VERSION")
                                .unwrap_or_else(|_| "v21.0".into()),
                            display_name: std::env::var("SOVEREIGN_WHATSAPP_NAME").ok(),
                        };
                        let wa_channel =
                            sovereign_comms::channels::whatsapp::WhatsAppChannel::new(
                                wa_config,
                                db_arc.clone(),
                                token,
                            );
                        comms_sync.add_channel(Box::new(wa_channel));
                        tracing::info!("WhatsApp channel registered");
                    }
                }
            }

            // Bridge CommsEvent → OrchestratorEvent
            let orch_tx_comms = orch_tx.clone();
            tokio::spawn(async move {
                while let Some(event) = comms_event_rx.recv().await {
                    let orch_event = match event {
                        sovereign_comms::CommsEvent::NewMessages {
                            channel,
                            count,
                            conversation_id,
                        } => OrchestratorEvent::NewMessagesReceived {
                            channel: channel.to_string(),
                            count,
                            conversation_id,
                        },
                        sovereign_comms::CommsEvent::SyncComplete {
                            channel,
                            result,
                        } => OrchestratorEvent::CommsSyncComplete {
                            channel: channel.to_string(),
                            new_messages: result.new_messages,
                        },
                        sovereign_comms::CommsEvent::SyncError {
                            channel,
                            error,
                        } => OrchestratorEvent::CommsSyncError {
                            channel: channel.to_string(),
                            error,
                        },
                        sovereign_comms::CommsEvent::ContactDiscovered {
                            contact_id,
                            name,
                        } => OrchestratorEvent::ContactCreated {
                            contact_id,
                            name,
                        },
                    };
                    if orch_tx_comms.send(orch_event).is_err() {
                        break;
                    }
                }
            });

            tokio::spawn(comms_sync.run());
            tracing::info!("Communications sync engine spawned");
        }

        let query_callback: Option<Box<dyn Fn(String) + Send + 'static>> =
            orchestrator.as_ref().map(|orch| {
                setup::orch_callback(orch, "Query error", |o, t| Box::pin(o.handle_query(t)))
            });

        let chat_callback: Option<Box<dyn Fn(String) + Send + 'static>> =
            orchestrator.as_ref().map(|orch| {
                setup::orch_callback(orch, "Chat error", |o, t| Box::pin(o.handle_chat(t)))
            });

        // Initialize voice pipeline (optional)
        let voice_rx = if config.voice.enabled {
            let (vtx, vrx) = mpsc::channel();

            let voice_query_cb: Box<dyn Fn(String) + Send + 'static> =
                if let Some(ref orch) = orchestrator {
                    setup::orch_callback(orch, "Voice query error", |o, t| Box::pin(o.handle_query(t)))
                } else {
                    Box::new(|text: String| {
                        tracing::warn!("Voice query ignored (no orchestrator): {text}");
                    })
                };

            match sovereign_ai::voice::VoicePipeline::spawn(
                config.voice.clone(),
                vtx,
                voice_query_cb,
            ) {
                Ok(_handle) => {
                    tracing::info!("Voice pipeline started");
                    Some(vrx)
                }
                Err(e) => {
                    tracing::warn!("Voice pipeline unavailable: {e}");
                    None
                }
            }
        } else {
            tracing::info!("Voice pipeline disabled in config");
            None
        };

        // Convert voice_rx to UI's VoiceEvent type
        let ui_voice_rx = voice_rx.map(|rx| {
            let (ui_tx, ui_rx) = mpsc::channel();
            std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    let ui_event = match event {
                        sovereign_ai::VoiceEvent::WakeWordDetected => {
                            sovereign_ui::app::VoiceEvent::WakeWordDetected
                        }
                        sovereign_ai::VoiceEvent::ListeningStarted => {
                            sovereign_ui::app::VoiceEvent::ListeningStarted
                        }
                        sovereign_ai::VoiceEvent::TranscriptionReady(t) => {
                            sovereign_ui::app::VoiceEvent::TranscriptionReady(t)
                        }
                        sovereign_ai::VoiceEvent::ListeningStopped => {
                            sovereign_ui::app::VoiceEvent::ListeningStopped
                        }
                        sovereign_ai::VoiceEvent::TtsSpeaking(t) => {
                            sovereign_ui::app::VoiceEvent::TtsSpeaking(t)
                        }
                        sovereign_ai::VoiceEvent::TtsDone => {
                            sovereign_ui::app::VoiceEvent::TtsDone
                        }
                    };
                    if ui_tx.send(ui_event).is_err() {
                        break;
                    }
                }
            });
            ui_rx
        });

        // Auto-commit engine
        let autocommit = Arc::new(tokio::sync::Mutex::new(
            sovereign_ai::AutoCommitEngine::new(db_arc.clone()),
        ));

        // Build save callback for document panel — records edits for auto-commit
        let save_cb: Box<dyn Fn(String, String, String) + Send + 'static> = {
            let db = db_arc.clone();
            let ac = autocommit.clone();
            Box::new(move |doc_id: String, title: String, content: String| {
                let db = db.clone();
                let ac = ac.clone();
                tokio::spawn(async move {
                    if let Err(e) = db
                        .update_document(&doc_id, Some(&title), Some(&content))
                        .await
                    {
                        tracing::error!("Failed to save document {doc_id}: {e}");
                    }
                    ac.lock().await.record_edit(&doc_id);
                });
            })
        };

        // Periodic auto-commit check (every 30s)
        {
            let ac = autocommit.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    ac.lock().await.check_and_commit().await;
                }
            });
        }

        // Close callback for auto-commit on document close
        let close_cb: Box<dyn Fn(String) + Send + 'static> = {
            let ac = autocommit.clone();
            Box::new(move |doc_id: String| {
                let ac = ac.clone();
                tokio::spawn(async move {
                    ac.lock().await.commit_on_close(&doc_id).await;
                });
            })
        };

        // Detect first launch for onboarding wizard
        let first_launch = sovereign_ui::app::is_first_launch();

        // Load bubble style from user profile
        let bubble_style = sovereign_core::profile::UserProfile::load(&profile_dir)
            .map(|p| p.bubble_style)
            .ok();

        // Build SovereignApp (but don't launch Iced yet)
        let (app, _boot_task) = sovereign_ui::app::SovereignApp::new(
            &config.ui,
            documents,
            threads,
            relationships,
            commits_map,
            contacts,
            conversations,
            all_messages,
            query_callback,
            chat_callback,
            Some(orch_rx),
            ui_voice_rx,
            None, // skill_rx — canvas creates its own internally
            Some(save_cb),
            Some(close_cb),
            Some(decision_tx),
            Some(registry),
            Some(skill_db),
            Some(feedback_tx),
            send_message_tx,
            first_launch,
            config.ai.model_dir.clone(),
            config.ai.router_model.clone(),
            config.ai.reasoning_model.clone(),
            None, // camera_frame — initialized later if camera is available
            bubble_style,
        );
        Ok::<_, anyhow::Error>((app, _boot_task))
    })?;

    // rt stays alive — spawned tasks (P2P, orchestrator, auto-commit, comms) keep running.
    // Iced creates its own tokio runtime on the main thread (no active context here).
    sovereign_ui::app::run_app(app)?;
    Ok(())
}
