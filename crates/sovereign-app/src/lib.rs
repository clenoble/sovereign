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
#[cfg(feature = "p2p")]
mod sync_startup;
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
// Embedded browser uses Tauri Webview multi-window APIs (set_position/set_size/
// show/hide) that only exist on desktop. Mobile (Android/iOS) has one WebView
// per Activity and no positioning surface.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod browser;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
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

/// Mobile entry point. Called by Android's JNI loader via the
/// `tauri::mobile_entry_point` macro on `cdylib` builds. Skips CLI
/// parsing (no argv on Android) and runs the Tauri app with default
/// config loaded from disk.
#[cfg(any(target_os = "android", target_os = "ios"))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    lifecycle::init_tracing();
    let config = AppConfig::load_or_default(None);
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("tokio runtime init failed: {e}");
            return;
        }
    };
    if let Err(e) = run_tauri(&config, &rt) {
        tracing::error!("Tauri app exited with error: {e}");
    }
}

/// Desktop CLI entrypoint. The thin `src/main.rs` calls this; on
/// `Commands::Run` (the default) it brings up the Tauri webview, on
/// any subcommand it dispatches to the corresponding handler.
pub fn run_cli() -> Result<()> {
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

    // jiminy-bridge URL for the goodnight (sleep) animation fired on app exit.
    #[cfg(feature = "jiminy")]
    let jiminy_sleep_url = std::env::var("JIMINY_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9100".into());

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
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::open_browser,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::close_browser,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::navigate_browser,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::browser_back,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::browser_forward,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::browser_refresh,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::browser::set_browser_bounds,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
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
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::pii::extract_form_fields,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            tauri_commands::pii::__browser_form_extracted,
            tauri_commands::pii::autofill_pii_record,
            tauri_commands::pii::generate_password,
            tauri_commands::pii::list_cookies_for_entity,
            tauri_commands::pii::delete_cookie,
            tauri_commands::pii::clear_entity_cookies,
            tauri_commands::pii::commit_signup_capture,
            // Pairing (v0.0.5) — only registered when encryption is on.
            // Tauri's generate_handler doesn't support inline cfg, so the
            // module is always present; functions stub out PeerId derivation
            // when the p2p feature is off.
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::generate_pair_qr,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::consume_pair_qr_preview,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::complete_onboarding_paired,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::list_paired_devices,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::forget_paired_device,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::get_local_peer_id,
            #[cfg(feature = "encryption")]
            tauri_commands::pairing::trigger_sync_now,
            // Mobile: voice transcription + share-sheet receiver + connectivity
            tauri_commands::mobile::voice_transcribe_buffer,
            tauri_commands::mobile::receive_shared_content,
            tauri_commands::mobile::set_connectivity_state,
            tauri_commands::mobile::get_connectivity_state,
            // Voice: push-to-talk control surface
            tauri_commands::voice::start_listening,
            tauri_commands::voice::stop_listening,
        ])
        .setup(move |app| -> std::result::Result<(), Box<dyn std::error::Error>> {
            use tauri::Manager;
            #[allow(unused_mut)]
            let mut config = config_for_setup;

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

                // AppConfig::load_or_default ran before this hook so it
                // resolved `ai.model_dir` against `project_root()`, which
                // on Android falls back to CWD ("/") → "/models" (EROFS).
                // Re-point it at the app sandbox now that we know it.
                let model_dir = app_data.join("models");
                let _ = std::fs::create_dir_all(&model_dir);
                config.ai.model_dir = model_dir.to_string_lossy().into_owned();
                tracing::info!("Mobile model_dir: {}", config.ai.model_dir);

                // database.path stays as "data/sovereign.db" (relative);
                // create_db now resolves it against sovereign_dir() (which
                // honors SOVEREIGN_DATA_DIR), so on Android the DB lands
                // inside the app sandbox under .../data/sovereign.db.
                // Persistence is via SurrealKV (kv-mem replaced in v0.0.6).
            }

            // Profile dir (correct on both platforms after the env-var step).
            let profile_dir = sovereign_core::sovereign_dir();

            // Run heavy backend init under the host tokio runtime.
            let init_result: anyhow::Result<BackendInit> =
                rt_handle.block_on(async { init_backend(&config, profile_dir).await });
            let backend = init_result.map_err(|e| -> Box<dyn std::error::Error> {
                format!("Backend init failed: {e:#}").into()
            })?;

            // Voice pipeline (gated at compile time + runtime). Unlike the
            // upstream desktop path this KEEPS the receiver so the voice-event
            // forwarder (below) can drain it and surface listening/speaking/
            // idle state to the Svelte Taskbar mic button. Returns Some(vrx)
            // only when the pipeline actually spawned.
            #[cfg(feature = "voice-stt")]
            let voice_rx = if backend.config.voice.enabled {
                let (vtx, vrx) = mpsc::channel();
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
                    Ok(_handle) => {
                        tracing::info!("Voice pipeline started");
                        std::sync::Mutex::new(Some(vrx))
                    }
                    Err(e) => {
                        tracing::warn!("Voice pipeline unavailable: {e}");
                        std::sync::Mutex::new(None)
                    }
                }
            } else {
                tracing::info!("Voice pipeline disabled in config");
                std::sync::Mutex::new(None)
            };
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
                #[cfg(feature = "encryption")]
                pending_pairing: tokio::sync::RwLock::new(None),
                #[cfg(feature = "p2p")]
                p2p_command_tx: tokio::sync::RwLock::new(None),
                #[cfg(feature = "p2p")]
                pairing_manager: tokio::sync::RwLock::new(None),
                #[cfg(feature = "p2p")]
                connectivity: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
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

            // Event forwarder. On `jiminy` builds the orchestrator stream is
            // fanned out to BOTH the Tauri forwarder AND the JiminyBridge
            // (robot head/antenna/emotions + ChatResponse -> sidecar /speak),
            // so `orch_rx` is rebound to the UI receiver below.
            let orch_rx = backend.orch_rx;

            // Jiminy embodiment (BODY): fan-out every orchestrator event to BOTH
            // the Tauri event forwarder AND the JiminyBridge. Rebind orch_rx to
            // the UI receiver so spawn_event_forwarder consumes the fanned-out
            // stream.
            #[cfg(feature = "jiminy")]
            let orch_rx = {
                let (ui_tx, ui_rx) = mpsc::channel::<OrchestratorEvent>();
                let (jiminy_tx, jiminy_rx) = mpsc::channel::<OrchestratorEvent>();
                std::thread::Builder::new()
                    .name("jiminy-fanout".into())
                    .spawn(move || {
                        while let Ok(event) = orch_rx.recv() {
                            let _ = ui_tx.send(event.clone());
                            let _ = jiminy_tx.send(event);
                        }
                    })
                    .expect("Failed to spawn jiminy-fanout thread");
                let jiminy_url = std::env::var("JIMINY_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:9100".into());
                let _jiminy_handle =
                    sovereign_ai::jiminy::JiminyBridge::new(&jiminy_url).spawn(jiminy_rx);
                tracing::info!("Jiminy bridge started (sidecar at {jiminy_url})");
                ui_rx
            };

            // Jiminy camera poller (keep-compiling only — no Tauri consumer yet).
            #[cfg(feature = "jiminy")]
            {
                let jiminy_url = std::env::var("JIMINY_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:9100".into());
                let frame = sovereign_ai::jiminy_camera::shared_frame();
                let _camera_handle = sovereign_ai::jiminy_camera::spawn_poller(
                    &jiminy_url,
                    frame.clone(),
                    70,
                    640,
                );
                let _ = frame;
                tracing::info!("Jiminy camera poller started");
            }

            // Jiminy vision: poll the vision sidecar for gestures + scene; react
            // to shush by POSTing /stop to the jiminy-bridge (speech barge-in).
            // `vision` is the same store the orchestrator reads for scene context.
            #[cfg(feature = "vision")]
            {
                let vision_url = std::env::var("JIMINY_VISION_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:9101".into());
                let bridge_url = std::env::var("JIMINY_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:9100".into());

                // Gesture-driven voice input: the 'talking_hand' mime opens the
                // robot mic by POSTing /listen to the bridge (out-of-process STT —
                // in-process whisper clashes with the LLM's bundled ggml). The
                // recognized text is fed to the orchestrator, which replies (and,
                // on jiminy builds, speaks it through Jiminy).
                let (listen_tx, mut listen_rx) = tokio::sync::mpsc::channel::<()>(2);
                if let Some(orch) = backend.orchestrator.clone() {
                    let listen_url = format!("{}/listen", bridge_url.trim_end_matches('/'));
                    let app_handle = app.handle().clone();
                    let query_cb = setup::orch_callback(&orch, "Gesture-listen error", |o, t| {
                        Box::pin(o.handle_query(t))
                    });
                    tauri::async_runtime::spawn(async move {
                        use tauri::Emitter;
                        // Surface listening / heard text to the Svelte mic button +
                        // chat window (mirrors the voice-pipeline forwarder).
                        let voice_evt = |kind: &str, text: Option<String>| {
                            let _ = app_handle.emit(
                                "voice-event",
                                tauri_events::VoiceEventPayload { kind: kind.into(), text },
                            );
                        };
                        let client = reqwest::Client::builder()
                            .timeout(std::time::Duration::from_secs(30))
                            .build()
                            .unwrap_or_default();
                        while listen_rx.recv().await.is_some() {
                            tracing::info!("Gesture-listen: recording a turn…");
                            voice_evt("listening", None);
                            match client.post(&listen_url).send().await {
                                Ok(resp) => match resp.json::<serde_json::Value>().await {
                                    Ok(v) => {
                                        let text = v
                                            .get("text")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("")
                                            .trim()
                                            .to_string();
                                        if text.is_empty() {
                                            tracing::info!("Gesture-listen: nothing heard");
                                            voice_evt("idle", None);
                                        } else {
                                            tracing::info!("Gesture-listen heard: {text}");
                                            voice_evt("transcription", Some(text.clone()));
                                            query_cb(text);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Gesture-listen parse failed: {e}");
                                        voice_evt("idle", None);
                                    }
                                },
                                Err(e) => {
                                    tracing::warn!("Gesture-listen /listen failed: {e}");
                                    voice_evt("idle", None);
                                }
                            }
                        }
                    });
                }

                let on_gesture = move |g: String| {
                    if sovereign_ai::jiminy_vision::gesture_starts_listening(&g) {
                        // Signal the listen task; drop if one is already queued.
                        let _ = listen_tx.try_send(());
                    }
                };

                let _vision_handle = sovereign_ai::jiminy_vision::spawn_poller(
                    &vision_url,
                    backend.vision,
                    Some(bridge_url.clone()),
                    on_gesture,
                    1.5,
                );
                tracing::info!(
                    "Jiminy vision poller started ({vision_url}; reactions -> {bridge_url})"
                );
            }

            tauri_events::spawn_event_forwarder(app.handle().clone(), orch_rx);

            // Voice-event forwarder: drains the voice pipeline rx and emits
            // "voice-event" to the Svelte frontend (listening / speaking / idle).
            #[cfg(feature = "voice-stt")]
            if let Some(vrx) = voice_rx.lock().unwrap().take() {
                tauri_events::spawn_voice_forwarder(app.handle().clone(), vrx);
            }

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

            // Phase 3c: periodic 5-minute sync poll. Belt-and-suspenders
            // alongside the mDNS-discovery auto-trigger and the
            // foreground `trigger_sync_now` Tauri command. No-op until
            // the post-login P2P startup runs.
            //
            // Phase 4.5: exponential backoff. After 3 consecutive
            // "fired = 0" cycles (no peers reachable / connectivity
            // gate denied / no paired peers) the cadence stretches
            // 5min → 15min → 1h → 1h ... Once a cycle fires anything,
            // it snaps back to 5min. Keeps idle Android devices out of
            // a tight 5-min wakeup loop on cellular.
            #[cfg(feature = "p2p")]
            {
                let app_handle_for_poll = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Manager as _;
                    use std::time::Duration;
                    const BASE: Duration = Duration::from_secs(300); // 5min
                    const STEP_2: Duration = Duration::from_secs(900); // 15min
                    const STEP_3: Duration = Duration::from_secs(3600); // 1h

                    let mut consecutive_idle: u32 = 0;
                    // Skip the immediate tick — a freshly logged-in user
                    // already gets a sync via the mDNS auto-trigger.
                    tokio::time::sleep(BASE).await;
                    loop {
                        let fired = if let Some(state) =
                            app_handle_for_poll.try_state::<tauri_state::AppState>()
                        {
                            crate::sync_startup::trigger_sync_for_all_paired(&state).await
                        } else {
                            0
                        };

                        if fired > 0 {
                            tracing::debug!(
                                "periodic sync poll: fired StartSync for {fired} paired peers"
                            );
                            consecutive_idle = 0;
                        } else {
                            consecutive_idle = consecutive_idle.saturating_add(1);
                        }

                        let next = match consecutive_idle {
                            0..=2 => BASE,
                            3..=4 => STEP_2,
                            _ => STEP_3,
                        };
                        if consecutive_idle >= 3 {
                            tracing::debug!(
                                "periodic sync poll: idle for {consecutive_idle} cycles, sleeping {next:?}"
                            );
                        }
                        tokio::time::sleep(next).await;
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building Sovereign GE (Tauri)")
        .run(move |_app, event| {
            // Goodnight: when the user quits, ask the bridge to play Jiminy's
            // sleep animation before the process winds down.
            #[cfg(feature = "jiminy")]
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                sleep_jiminy(&jiminy_sleep_url);
            }
            #[cfg(not(feature = "jiminy"))]
            let _ = event;
        });

    Ok(())
}

/// Best-effort goodnight: tell the jiminy-bridge to play Jiminy's sleep
/// animation as the app exits. Uses a raw blocking TCP request because the async
/// runtime is being torn down at this point; the bridge keeps running and plays
/// the animation out after we're gone.
#[cfg(feature = "jiminy")]
fn sleep_jiminy(bridge_url: &str) {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let addr = bridge_url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let sock: SocketAddr = match addr.parse() {
        Ok(s) => s,
        Err(_) => return,
    };
    if let Ok(mut stream) = TcpStream::connect_timeout(&sock, Duration::from_secs(2)) {
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
        let req = format!(
            "POST /sleep HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        if stream.write_all(req.as_bytes()).is_ok() {
            let _ = stream.flush();
            let mut buf = [0u8; 128];
            let _ = stream.read(&mut buf); // wait for the bridge to ack before exiting
            tracing::info!("Jiminy: goodnight (sleep requested)");
        }
    }
}

/// Bundle of values produced by backend init that the Tauri setup() callback
/// needs to register state and spawn background tasks.
struct BackendInit {
    config: AppConfig,
    profile_dir: std::path::PathBuf,
    db: Arc<sovereign_db::layered::LayeredGraphDB>,
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
    #[cfg(feature = "vision")]
    vision: sovereign_ai::jiminy_vision::SharedVision,
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

    // Wrap the raw SurrealGraphDB in a LayeredGraphDB. Boot uses the raw
    // inner; install_session() in tauri_commands/auth.rs swaps in an
    // EncryptedGraphDB after login (KEK is not unlocked until then). All
    // consumers (skills, orchestrator, tauri_commands::*) hold the same
    // LayeredGraphDB Arc; trait calls flow through the current inner.
    let raw_db_arc: Arc<dyn sovereign_db::GraphDB> = Arc::new(db);
    let db_arc: Arc<sovereign_db::layered::LayeredGraphDB> =
        Arc::new(sovereign_db::layered::LayeredGraphDB::new(raw_db_arc));

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

    // Shared vision state: written by the vision poller (in .setup() below),
    // read by the orchestrator's chat context — one store shared by both.
    #[cfg(feature = "vision")]
    let vision = sovereign_ai::jiminy_vision::shared_vision();

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
            #[cfg(feature = "vision")]
            o.set_vision(vision.clone());

            // Session-log encryption + PII tokenization for the
            // orchestrator are installed after login by
            // install_session() in tauri_commands::auth.rs. The P2P
            // node startup also runs there once the per-device identity
            // key is loaded (Phase 3c).

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
        #[cfg(feature = "vision")]
        vision,
    })
}
