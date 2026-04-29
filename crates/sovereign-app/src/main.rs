mod cli;
mod commands;
mod llm_bridge;
#[cfg(feature = "duress")]
mod duress;
mod err;
mod seed;
mod setup;

mod tauri_commands;
mod tauri_events;
#[cfg(feature = "encryption")]
mod pii_ingest;
#[cfg(all(feature = "comms", feature = "encryption"))]
mod pii_contact_hook;
#[cfg(all(feature = "comms", feature = "encryption"))]
mod pii_message_hook;
#[cfg(all(feature = "comms", feature = "encryption"))]
mod pii_sweep;
mod tauri_state;

#[cfg(feature = "web-browse")]
mod web;
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

    let rt = tokio::runtime::Runtime::new()?;

    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            run_tauri(&config, &rt)?;
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
fn run_tauri(config: &AppConfig, rt: &tokio::runtime::Runtime) -> Result<()> {
    // Compute profile directory (~/.sovereign)
    let profile_dir = sovereign_core::home_dir().join(".sovereign");

    // Initialize crypto subsystem if enabled. The DeviceKey is wrapped
    // in Arc so it can be shared with the Tauri AppState for the PII
    // ingest path (see pii_ingest.rs); without DeviceKey the pipeline
    // runs in pass-through mode (no records written).
    #[cfg(feature = "encryption")]
    let _crypto_state = if config.crypto.enabled {
        match setup::init_crypto() {
            Ok((device_key, key_db, kek)) => {
                Some((Arc::new(device_key), key_db, kek))
            }
            Err(e) => {
                tracing::warn!("Crypto init failed (continuing without encryption): {e}");
                None
            }
        }
    } else {
        None
    };

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
            // Wave A: read_document only
            registry.register(Box::new(sovereign_skills::skills::outline_extractor::OutlineExtractorSkill));
            registry.register(Box::new(sovereign_skills::skills::link_checker::LinkCheckerSkill));
            registry.register(Box::new(sovereign_skills::skills::pii_detector::PiiDetectorSkill));
            registry.register(Box::new(sovereign_skills::skills::readability_score::ReadabilityScoreSkill));
            registry.register(Box::new(sovereign_skills::skills::html_export::HtmlExportSkill));
            registry.register(Box::new(sovereign_skills::skills::plaintext_export::PlaintextExportSkill));
            // Wave B: read_document + write_document
            registry.register(Box::new(sovereign_skills::skills::table_of_contents::TableOfContentsSkill));
            registry.register(Box::new(sovereign_skills::skills::json_yaml_formatter::JsonYamlFormatterSkill));
            registry.register(Box::new(sovereign_skills::skills::csv_to_md::CsvToMdSkill));
            registry.register(Box::new(sovereign_skills::skills::redactor::RedactorSkill));
            // Wave D: read_all / write_all (cross-document)
            registry.register(Box::new(sovereign_skills::skills::backlink_map::BacklinkMapSkill));
            registry.register(Box::new(sovereign_skills::skills::orphan_finder::OrphanFinderSkill));
            registry.register(Box::new(sovereign_skills::skills::daily_journal::DailyJournalSkill));
            // Wave E: LLM-using
            registry.register(Box::new(sovereign_skills::skills::thread_summary::ThreadSummarySkill));
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

                    // Wire device key into the orchestrator so session log
                    // entries (user input + chat responses) get PII-tokenized
                    // at write time.
                    #[cfg(feature = "encryption")]
                    if let Some((ref device_key, _, _)) = _crypto_state {
                        o.set_pii_device_key(device_key.clone());
                    }

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

    // Clones for the PII sweep idle-watcher (4e4). The sweep cycle
    // takes db + device_key directly so it can run independently of
    // the orchestrator's model — useful because the sweep is regex-only
    // and shouldn't wait for an idle LLM.
    #[cfg(all(feature = "comms", feature = "encryption"))]
    let pii_sweep_db: std::sync::Arc<dyn sovereign_db::GraphDB> = db.clone();
    #[cfg(all(feature = "comms", feature = "encryption"))]
    let pii_sweep_device_key: Option<Arc<sovereign_crypto::device_key::DeviceKey>> =
        _crypto_state.as_ref().map(|(dk, _, _)| dk.clone());

    // Bridge orchestrator into SkillLlmAccess for skills that need inference.
    let skill_llm: Option<Arc<dyn sovereign_skills::SkillLlmAccess>> =
        orchestrator.as_ref().map(|o| llm_bridge::wrap_orchestrator(o.clone()));

    // Initialize voice pipeline (optional). Voice events are dropped here —
    // the wake-word + transcribe + voice_query_cb path still routes user
    // speech into the orchestrator; UI feedback for the listening state
    // can be added later by forwarding the rx onto a Tauri event.
    if config.voice.enabled {
        let (vtx, _vrx) = mpsc::channel();
        let voice_query_cb: Box<dyn Fn(String) + Send + 'static> =
            if let Some(ref orch) = orchestrator {
                setup::orch_callback(orch, "Voice query error", |o, t| {
                    Box::pin(o.handle_query(t))
                })
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
            Ok(_handle) => tracing::info!("Voice pipeline started"),
            Err(e) => tracing::warn!("Voice pipeline unavailable: {e}"),
        }
    } else {
        tracing::info!("Voice pipeline disabled in config");
    }

    // Build Tauri app
    tauri::Builder::default()
        .manage(tauri_state::AppState {
            db,
            orchestrator,
            config: config.clone(),
            skill_registry,
            skill_db,
            skill_llm,
            decision_tx,
            feedback_tx,
            orch_tx,
            theme: std::sync::Mutex::new("dark".to_string()),
            autocommit: autocommit.clone(),
            model_assignments: std::sync::Mutex::new(model_assignments),
            profile_dir,
            #[cfg(feature = "encryption")]
            device_key: _crypto_state
                .as_ref()
                .map(|(dk, _, _)| dk.clone()),
        })
        .invoke_handler(tauri::generate_handler![
            // AI: status, chat, search, action gate, models, trust
            tauri_commands::ai::greet,
            tauri_commands::ai::get_status,
            tauri_commands::ai::chat_message,
            tauri_commands::ai::search_documents,
            tauri_commands::ai::search_query,
            tauri_commands::ai::approve_action,
            tauri_commands::ai::reject_action,
            tauri_commands::ai::accept_suggestion,
            tauri_commands::ai::dismiss_suggestion,
            tauri_commands::ai::scan_models,
            tauri_commands::ai::assign_model_role,
            tauri_commands::ai::delete_model,
            tauri_commands::ai::get_trust_entries,
            tauri_commands::ai::reset_trust_action,
            tauri_commands::ai::reset_trust_all,
            // Documents: list, CRUD, commits, skills, import
            tauri_commands::documents::list_documents,
            tauri_commands::documents::list_threads,
            tauri_commands::documents::toggle_theme,
            tauri_commands::documents::get_theme,
            tauri_commands::documents::get_document,
            tauri_commands::documents::save_document,
            tauri_commands::documents::create_document,
            tauri_commands::documents::close_document,
            tauri_commands::documents::delete_document,
            tauri_commands::documents::list_commits,
            tauri_commands::documents::restore_commit,
            tauri_commands::documents::list_skills_for_doc,
            tauri_commands::documents::execute_skill,
            tauri_commands::documents::list_all_skills,
            tauri_commands::documents::import_file,
            // Canvas
            tauri_commands::canvas::canvas_load,
            tauri_commands::canvas::update_document_position,
            tauri_commands::canvas::canvas_load_messages,
            // Threads
            tauri_commands::threads::create_thread,
            tauri_commands::threads::update_thread,
            tauri_commands::threads::delete_thread,
            tauri_commands::threads::move_document_to_thread,
            // Contacts & messaging
            tauri_commands::contacts::list_contacts,
            tauri_commands::contacts::get_contact_detail,
            tauri_commands::contacts::list_conversations,
            tauri_commands::contacts::list_messages,
            tauri_commands::contacts::mark_message_read,
            tauri_commands::contacts::create_relationship,
            // Auth, onboarding, profile, config
            tauri_commands::auth::check_auth_state,
            tauri_commands::auth::validate_password,
            tauri_commands::auth::validate_password_policy,
            tauri_commands::auth::complete_onboarding,
            tauri_commands::auth::get_profile,
            tauri_commands::auth::save_profile,
            tauri_commands::auth::get_config,
            // Browser, web, comms
            tauri_commands::browser::get_comms_config,
            tauri_commands::browser::save_comms_config,
            tauri_commands::browser::open_browser,
            tauri_commands::browser::close_browser,
            tauri_commands::browser::navigate_browser,
            tauri_commands::browser::browser_back,
            tauri_commands::browser::browser_forward,
            tauri_commands::browser::browser_refresh,
            tauri_commands::browser::set_browser_bounds,
            tauri_commands::browser::set_browser_visible,
            tauri_commands::browser::fetch_web_page,
            tauri_commands::browser::save_web_page,
            tauri_commands::browser::assess_reliability,
            tauri_commands::browser::reassess_reliability,
            // Memory consolidation — AI-suggested links
            tauri_commands::suggestions::list_pending_suggestions,
            tauri_commands::suggestions::accept_link_suggestion,
            tauri_commands::suggestions::dismiss_link_suggestion,
            tauri_commands::suggestions::trigger_consolidation,
            // PII resolution (5c). Stubs out to a clear error string
            // when the `encryption` feature isn't enabled.
            tauri_commands::pii::resolve_pii_tokens,
            // PII dashboard (6) — read paths and review/redact write paths.
            tauri_commands::pii::list_pii_entities,
            tauri_commands::pii::get_pii_entity,
            tauri_commands::pii::list_pii_records,
            tauri_commands::pii::confirm_pii_record,
            tauri_commands::pii::dismiss_pii_record,
            tauri_commands::pii::redact_pii_record,
            // PII dashboard (6c) — per-record reveal + vault add.
            tauri_commands::pii::reveal_pii_record,
            tauri_commands::pii::create_vault_entry,
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

            // PII sweep idle-watcher — rescans documents, messages, and
            // contacts that lack a `pii_scanned_at` marker. Runs every
            // 5 minutes; each cycle scans up to BATCH_SIZE items per
            // kind. Doesn't depend on model idle since it's regex-only.
            #[cfg(all(feature = "comms", feature = "encryption"))]
            if let Some(device_key) = pii_sweep_device_key {
                let db = pii_sweep_db.clone();
                tauri::async_runtime::spawn(async move {
                    use std::time::Duration;
                    let interval = Duration::from_secs(300);
                    // First run after a short delay, so app startup
                    // tasks (seeding, schema init) have settled.
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    loop {
                        let _stats =
                            pii_sweep::run_sweep_cycle(db.clone(), device_key.clone()).await;
                        tokio::time::sleep(interval).await;
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running Sovereign GE (Tauri)");

    Ok(())
}
