#[cfg(feature = "encryption")]
mod account_key_migration;
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
#[cfg(feature = "comms")]
mod pii_share_hook;
#[cfg(all(feature = "comms", feature = "encryption"))]
mod pii_sweep;
mod tauri_state;

#[cfg(feature = "web-browse")]
mod web;
mod browser;
mod browser_pii;
mod cookie_api;

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

/// Launch the Tauri web UI.
///
/// Backend init runs INSIDE Tauri's setup() callback so that on mobile
/// (iOS/Android) we can read `app.path().app_data_dir()` and set
/// `SOVEREIGN_DATA_DIR` before any sovereign code resolves a path.
/// On desktop the env var is left unset and `sovereign_dir()` falls back
/// to `~/.sovereign`.
fn run_tauri(config: &AppConfig, rt: &tokio::runtime::Runtime) -> Result<()> {
    let config_for_setup = config.clone();
    let rt_handle = rt.handle().clone();

    // Build Tauri app
    let mut builder = tauri::Builder::default();

    // Haptics plugin — iOS UIImpactFeedbackGenerator / Android VibratorService.
    // No-op on desktop but safe to register; gated so the dep isn't pulled in
    // on desktop builds.
    #[cfg(feature = "haptics")]
    {
        builder = builder.plugin(tauri_plugin_haptics::init());
    }

    builder
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
            // PII resolution
            tauri_commands::pii::resolve_pii_tokens,
            tauri_commands::pii::list_pii_entities,
            tauri_commands::pii::get_pii_entity,
            tauri_commands::pii::list_pii_records,
            tauri_commands::pii::confirm_pii_record,
            tauri_commands::pii::dismiss_pii_record,
            tauri_commands::pii::redact_pii_record,
            tauri_commands::pii::reveal_pii_record,
            tauri_commands::pii::create_vault_entry,
            tauri_commands::pii::list_share_records_for_entity,
            tauri_commands::pii::extract_form_fields,
            tauri_commands::pii::__browser_form_extracted,
            tauri_commands::pii::autofill_pii_record,
            tauri_commands::pii::generate_password,
            tauri_commands::pii::list_cookies_for_entity,
            tauri_commands::pii::delete_cookie,
            tauri_commands::pii::clear_entity_cookies,
            tauri_commands::pii::commit_signup_capture,
            // Mobile: voice transcription + share-sheet receiver
            tauri_commands::mobile::voice_transcribe_buffer,
            tauri_commands::mobile::receive_shared_content,
        ])
        .setup(move |app| -> std::result::Result<(), Box<dyn std::error::Error>> {
            use tauri::Manager;
            let config = config_for_setup;

            // Mobile: pin SOVEREIGN_DATA_DIR to the app sandbox before any
            // sovereign code resolves a path. Desktop leaves it unset and
            // sovereign_dir() falls back to ~/.sovereign.
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                let app_data = app.path().app_data_dir().map_err(
                    |e| -> Box<dyn std::error::Error> {
                        format!("app_data_dir lookup failed: {e}").into()
                    },
                )?;
                let _ = std::fs::create_dir_all(&app_data);
                std::env::set_var("SOVEREIGN_DATA_DIR", &app_data);
                tracing::info!("Mobile data dir: {}", app_data.display());
            }

            // Profile dir (correct on both platforms after the env-var step).
            let profile_dir = sovereign_core::sovereign_dir();

            // Run heavy backend init under the host tokio runtime.
            let init_result: anyhow::Result<BackendInit> =
                rt_handle.block_on(async { init_backend(&config, profile_dir).await });
            let backend = init_result.map_err(|e| -> Box<dyn std::error::Error> {
                format!("Backend init failed: {e:#}").into()
            })?;

            // Voice pipeline (gated at compile time + runtime)
            #[cfg(feature = "voice-stt")]
            if backend.config.voice.enabled {
                let (vtx, _vrx) = mpsc::channel();
                let voice_query_cb: Box<dyn Fn(String) + Send + 'static> =
                    if let Some(ref orch) = backend.orchestrator {
                        setup::orch_callback(orch, "Voice query error", |o, t| {
                            Box::pin(o.handle_query(t))
                        })
                    } else {
                        Box::new(|text: String| {
                            tracing::warn!("Voice query ignored (no orchestrator): {text}");
                        })
                    };

                match sovereign_ai::voice::VoicePipeline::spawn(
                    backend.config.voice.clone(),
                    vtx,
                    voice_query_cb,
                ) {
                    Ok(_handle) => tracing::info!("Voice pipeline started"),
                    Err(e) => tracing::warn!("Voice pipeline unavailable: {e}"),
                }
            } else {
                tracing::info!("Voice pipeline disabled in config");
            }
            #[cfg(not(feature = "voice-stt"))]
            tracing::info!("Voice pipeline omitted (voice-stt feature disabled)");

            // Register state with Tauri. The device_key is loaded post-login
            // by install_session() in tauri_commands::auth.rs; not at startup.
            // The theme is read from the persisted UserProfile so it survives
            // restarts (toggle_theme writes back through to the profile).
            let theme_initial = sovereign_core::profile::UserProfile::load(&backend.profile_dir)
                .map(|p| p.theme)
                .unwrap_or_else(|_| "dark".to_string());
            app.manage(tauri_state::AppState {
                db: backend.db.clone(),
                orchestrator: backend.orchestrator.clone(),
                config: backend.config.clone(),
                skill_registry: backend.skill_registry,
                skill_db: backend.skill_db,
                skill_llm: backend.skill_llm,
                decision_tx: backend.decision_tx,
                feedback_tx: backend.feedback_tx,
                orch_tx: backend.orch_tx,
                theme: std::sync::Mutex::new(theme_initial),
                autocommit: backend.autocommit.clone(),
                model_assignments: std::sync::Mutex::new(backend.model_assignments),
                profile_dir: backend.profile_dir,
                #[cfg(feature = "encryption")]
                account_key: tokio::sync::RwLock::new(None),
                #[cfg(feature = "encryption")]
                p2p_identity_key: tokio::sync::RwLock::new(None),
                #[cfg(feature = "voice-stt")]
                stt_engine: backend.stt_engine,
            });

            // Auto-open DevTools (desktop debug only)
            #[cfg(all(debug_assertions, not(any(target_os = "ios", target_os = "android"))))]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // Event forwarder
            tauri_events::spawn_event_forwarder(app.handle().clone(), backend.orch_rx);

            // Periodic auto-commit (30s)
            let autocommit = backend.autocommit;
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    autocommit.lock().await.check_and_commit().await;
                }
            });

            // Hourly purge of soft-deleted items
            let purge_db = backend.db.clone();
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

            // Memory consolidation idle-watcher
            if let Some(orch) = backend.orchestrator.clone() {
                tauri::async_runtime::spawn(async move {
                    use std::time::{Duration, Instant};
                    let check_interval = Duration::from_secs(30);
                    let post_run_cooldown = Duration::from_secs(300);
                    let mut last_run = Instant::now() - post_run_cooldown;

                    loop {
                        tokio::time::sleep(check_interval).await;
                        if !orch.is_model_idle() {
                            continue;
                        }
                        if last_run.elapsed() < post_run_cooldown {
                            continue;
                        }
                        match orch.consolidate_memory().await {
                            Ok(()) => tracing::debug!("Memory consolidation cycle completed"),
                            Err(e) => tracing::warn!("Memory consolidation failed: {e}"),
                        }
                        last_run = Instant::now();
                    }
                });
            }

            // PII sweep idle-watcher (4e4): deferred to v0.0.5 — see
            // comment earlier in run_tauri() for the rationale.

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running Sovereign GE (Tauri)");

    Ok(())
}

/// Bundle of values produced by backend init that the Tauri setup() callback
/// needs to register state and spawn background tasks.
struct BackendInit {
    config: AppConfig,
    profile_dir: std::path::PathBuf,
    db: Arc<sovereign_db::surreal::SurrealGraphDB>,
    orchestrator: Option<Arc<sovereign_ai::Orchestrator>>,
    skill_registry: Arc<sovereign_skills::SkillRegistry>,
    skill_db: Arc<dyn sovereign_skills::SkillDbAccess>,
    skill_llm: Option<Arc<dyn sovereign_skills::SkillLlmAccess>>,
    decision_tx: tokio::sync::mpsc::Sender<ActionDecision>,
    feedback_tx: tokio::sync::mpsc::Sender<FeedbackEvent>,
    orch_tx: mpsc::Sender<OrchestratorEvent>,
    orch_rx: mpsc::Receiver<OrchestratorEvent>,
    autocommit: Arc<tokio::sync::Mutex<sovereign_ai::AutoCommitEngine>>,
    model_assignments: tauri_state::ModelAssignments,
    #[cfg(feature = "voice-stt")]
    stt_engine: Option<Arc<tokio::sync::Mutex<sovereign_ai::voice::stt::SttEngine>>>,
}

/// Backend init: crypto, DB, seeding, skills, orchestrator, channels.
/// Called once from the Tauri setup() callback.
async fn init_backend(
    config: &AppConfig,
    profile_dir: std::path::PathBuf,
) -> anyhow::Result<BackendInit> {
    // Crypto subsystem: prepare the auth dir / salt / device-id but do
    // NOT prompt for a password here. The actual DeviceKey is loaded
    // post-authentication via install_session() in tauri_commands::auth
    // and stored in AppState's RwLock-backed device_key field.
    #[cfg(feature = "encryption")]
    if config.crypto.enabled {
        if let Err(e) = setup::prepare_auth() {
            tracing::warn!("prepare_auth failed: {e}");
        }
    }

    let db = create_db(config).await?;
    seed::seed_if_empty(&db).await?;

    // PII seed runs in complete_onboarding (auth.rs) once the device_key
    // is installed. Skipped at startup because the device_key isn't
    // available until login completes.

    let orchestrator_profile_dir = profile_dir.join("orchestrator");
    if let Err(e) = seed::seed_profile_and_history(&orchestrator_profile_dir) {
        tracing::warn!("Profile/history seed failed: {e}");
    }

    let db_arc: Arc<sovereign_db::surreal::SurrealGraphDB> = Arc::new(db);

    // Skill registry
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
    registry.register(Box::new(sovereign_skills::skills::outline_extractor::OutlineExtractorSkill));
    registry.register(Box::new(sovereign_skills::skills::link_checker::LinkCheckerSkill));
    registry.register(Box::new(sovereign_skills::skills::pii_detector::PiiDetectorSkill));
    registry.register(Box::new(sovereign_skills::skills::readability_score::ReadabilityScoreSkill));
    registry.register(Box::new(sovereign_skills::skills::html_export::HtmlExportSkill));
    registry.register(Box::new(sovereign_skills::skills::plaintext_export::PlaintextExportSkill));
    registry.register(Box::new(sovereign_skills::skills::table_of_contents::TableOfContentsSkill));
    registry.register(Box::new(sovereign_skills::skills::json_yaml_formatter::JsonYamlFormatterSkill));
    registry.register(Box::new(sovereign_skills::skills::csv_to_md::CsvToMdSkill));
    registry.register(Box::new(sovereign_skills::skills::redactor::RedactorSkill));
    registry.register(Box::new(sovereign_skills::skills::backlink_map::BacklinkMapSkill));
    registry.register(Box::new(sovereign_skills::skills::orphan_finder::OrphanFinderSkill));
    registry.register(Box::new(sovereign_skills::skills::daily_journal::DailyJournalSkill));
    registry.register(Box::new(sovereign_skills::skills::thread_summary::ThreadSummarySkill));
    tracing::info!("Registered {} core skills", registry.all_skills().len());

    // Channels
    let (orch_tx, orch_rx) = mpsc::channel::<OrchestratorEvent>();
    let (decision_tx, decision_rx) = tokio::sync::mpsc::channel::<ActionDecision>(32);
    let (feedback_tx, feedback_rx) = tokio::sync::mpsc::channel::<FeedbackEvent>(32);

    // Orchestrator
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

            // P2P startup is deferred to post-login wiring (v0.0.5);
            // requires the device key to derive identity, which is
            // not available until the user authenticates.
            #[cfg(feature = "p2p")]
            if config.p2p.enabled {
                tracing::warn!(
                    "P2P startup deferred until login wiring is complete (v0.0.5)"
                );
            }

            // Session-log encryption + PII tokenization for the
            // orchestrator are installed after login by
            // install_session() in tauri_commands::auth.rs.

            tracing::info!("AI orchestrator initialized (Tauri mode)");
            Some(Arc::new(o))
        }
        Err(e) => {
            tracing::warn!("AI orchestrator unavailable: {e}");
            None
        }
    };

    let autocommit = Arc::new(tokio::sync::Mutex::new(
        sovereign_ai::AutoCommitEngine::new(db_arc.clone()),
    ));

    let model_assignments = tauri_state::ModelAssignments {
        router: config.ai.router_model.clone(),
        reasoning: config.ai.reasoning_model.clone(),
    };

    let skill_llm: Option<Arc<dyn sovereign_skills::SkillLlmAccess>> =
        orchestrator.as_ref().map(|o| llm_bridge::wrap_orchestrator(o.clone()));

    // Mobile STT engine: shared Whisper instance for voice_transcribe_buffer
    // command. On desktop the cpal pipeline owns the SttEngine; here we
    // initialise one independently so Web Audio API audio can be transcribed.
    #[cfg(feature = "voice-stt")]
    let stt_engine = {
        use sovereign_ai::voice::stt::SttEngine;
        if config.voice.enabled {
            match SttEngine::new(&config.voice.whisper_model) {
                Ok(engine) => {
                    tracing::info!("STT engine ready for mobile transcription");
                    Some(Arc::new(tokio::sync::Mutex::new(engine)))
                }
                Err(e) => {
                    tracing::warn!("STT engine init failed, mobile voice unavailable: {e}");
                    None
                }
            }
        } else {
            None
        }
    };

    Ok(BackendInit {
        config: config.clone(),
        profile_dir,
        db: db_arc,
        orchestrator,
        skill_registry: Arc::new(registry),
        skill_db,
        skill_llm,
        decision_tx,
        feedback_tx,
        orch_tx,
        orch_rx,
        autocommit,
        model_assignments,
        #[cfg(feature = "voice-stt")]
        stt_engine,
    })
}
