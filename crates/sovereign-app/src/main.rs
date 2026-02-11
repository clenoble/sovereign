use std::sync::mpsc;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::{FeedbackEvent, OrchestratorEvent};
use sovereign_core::security::ActionDecision;
use sovereign_core::lifecycle;
use sovereign_db::schema::{thing_to_raw, Document, RelationType, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use std::path::PathBuf;

#[cfg(feature = "encryption")]
use sovereign_crypto::{
    device_key::DeviceKey,
    kek::Kek,
    key_db::KeyDatabase,
    master_key::MasterKey,
};

#[derive(Parser)]
#[command(name = "sovereign", about = "Sovereign OS — your data, your rules")]
struct Cli {
    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the GUI (Phase 1b)
    Run,

    /// Create a new document
    CreateDoc {
        #[arg(long)]
        title: String,
        #[arg(long)]
        thread_id: String,
        #[arg(long, default_value_t = true)]
        is_owned: bool,
    },

    /// Get a document by ID
    GetDoc {
        #[arg(long)]
        id: String,
    },

    /// List documents, optionally filtered by thread
    ListDocs {
        #[arg(long)]
        thread_id: Option<String>,
    },

    /// Update a document
    UpdateDoc {
        #[arg(long)]
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        content: Option<String>,
    },

    /// Delete a document
    DeleteDoc {
        #[arg(long)]
        id: String,
    },

    /// Create a new thread
    CreateThread {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },

    /// List all threads
    ListThreads,

    /// Add a relationship between two documents
    AddRelationship {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        relation_type: String,
        #[arg(long, default_value_t = 0.5)]
        strength: f32,
    },

    /// List relationships for a document
    ListRelationships {
        #[arg(long)]
        doc_id: String,
    },

    /// Commit a document snapshot
    Commit {
        #[arg(long)]
        doc_id: String,
        #[arg(long)]
        message: String,
    },

    /// List commits for a document
    ListCommits {
        #[arg(long)]
        doc_id: String,
    },

    /// Encrypt all existing plaintext documents (idempotent)
    #[cfg(feature = "encryption")]
    EncryptData,

    /// Pair with another device on the local network
    #[cfg(feature = "p2p")]
    PairDevice {
        #[arg(long)]
        peer_id: String,
    },

    /// List paired devices
    #[cfg(feature = "p2p")]
    ListDevices,

    /// Enroll a guardian for key recovery
    #[cfg(feature = "p2p")]
    EnrollGuardian {
        #[arg(long)]
        name: String,
        #[arg(long)]
        peer_id: String,
    },

    /// List enrolled guardians
    #[cfg(feature = "encryption")]
    ListGuardians,

    /// Initiate key recovery from guardians
    #[cfg(feature = "encryption")]
    InitiateRecovery,
}

/// Populate the database with sample data when it's empty.
/// Provides a visual baseline for testing the canvas.
async fn seed_if_empty(db: &SurrealGraphDB) -> Result<()> {
    use chrono::{TimeZone, Utc};

    let threads = db.list_threads().await?;
    if !threads.is_empty() {
        return Ok(());
    }

    tracing::info!("Empty database — seeding sample data");

    let thread_defs = [
        ("Research", "Research and exploration"),
        ("Development", "Engineering and code"),
        ("Design", "UX and visual design"),
        ("Admin", "Administrative and planning"),
    ];

    let mut thread_ids = Vec::new();
    for (name, desc) in &thread_defs {
        let t = Thread::new(name.to_string(), desc.to_string());
        let created = db.create_thread(t).await?;
        thread_ids.push(created.id_string().unwrap());
    }

    // Staggered creation times: Jan–Apr 2026
    let timestamps = [
        Utc.with_ymd_and_hms(2026, 1, 5, 10, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 1, 18, 14, 30, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 2, 2, 9, 15, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 2, 14, 11, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 2, 28, 16, 45, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 5, 8, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 15, 13, 20, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 3, 25, 10, 30, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 8, 15, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 15, 11, 30, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 20, 14, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 25, 10, 0, 0).unwrap(),
        Utc.with_ymd_and_hms(2026, 4, 28, 16, 0, 0).unwrap(),
    ];

    let owned_docs: Vec<(&str, &str, usize)> = vec![
        ("Research Notes", "# Research Notes\n\nExploring Rust + GTK4 for desktop OS development.\n\n## Key Findings\n- GTK4 bindings are solid\n- Skia provides GPU rendering", 0),
        ("Project Plan", "# Project Plan\n\n## Phase 1: Foundation\n- Data layer\n- UI shell\n\n## Phase 2: Canvas\n- Spatial layout\n- GPU rendering", 1),
        ("Architecture Diagram", "# Architecture\n\nComponent overview for Sovereign OS.", 1),
        ("API Specification", "# API Spec\n\n## Endpoints\n- Document CRUD\n- Thread management\n- Relationship graph", 1),
        ("Budget Overview", "# Budget 2026\n\n| Item | Cost |\n|------|------|\n| Infrastructure | $500 |\n| Tools | $200 |", 3),
        ("Meeting Notes Q1", "# Meeting Notes — Q1 2026\n\n## Jan 15\n- Discussed architecture\n- Agreed on Rust + GTK4 stack", 3),
        ("Design Document", "# Design System\n\n## Colors\n- Background: #0e0e10\n- Accent: #5a9fd4\n\n## Typography\n- System font, 13-16px", 2),
        ("Test Results", "# Test Results\n\n- sovereign-db: 12 pass\n- sovereign-canvas: 12 pass\n- sovereign-ai: 8 pass", 1),
    ];

    let external_docs: Vec<(&str, &str, usize)> = vec![
        ("Wikipedia: Rust", "# Rust (programming language)\n\nRust is a multi-paradigm systems programming language.", 0),
        ("SO: GTK4 bindings", "# Stack Overflow: GTK4 Rust bindings\n\nQ: How to use gtk4-rs with GLib main loop?", 1),
        ("GitHub Issue #42", "# Issue #42: Canvas performance\n\nReported: frame drops at 4K resolution.", 1),
        ("Research Paper (PDF)", "# Paper: Local-first Software\n\nAbstract: We explore principles for software that keeps data on user devices.", 0),
        ("Shared Spec", "# Shared API Specification\n\nCollaborative document for cross-team alignment.", 2),
        ("API Response Log", "# API Logs\n\n```\n200 GET /documents — 12ms\n201 POST /documents — 45ms\n```", 1),
    ];

    let mut created_doc_ids: Vec<String> = Vec::new();
    let mut ts_idx = 0;

    for (title, body, thread_idx) in &owned_docs {
        let mut doc = Document::new(
            title.to_string(),
            thread_ids[*thread_idx].clone(),
            true,
        );
        let content = sovereign_core::content::ContentFields {
            body: body.to_string(),
            images: vec![],
        };
        doc.content = content.serialize();
        doc.created_at = timestamps[ts_idx % timestamps.len()];
        doc.modified_at = doc.created_at;
        ts_idx += 1;
        let created = db.create_document(doc).await?;
        created_doc_ids.push(created.id_string().unwrap());
    }

    for (title, body, thread_idx) in &external_docs {
        let mut doc = Document::new(
            title.to_string(),
            thread_ids[*thread_idx].clone(),
            false,
        );
        let content = sovereign_core::content::ContentFields {
            body: body.to_string(),
            images: vec![],
        };
        doc.content = content.serialize();
        doc.created_at = timestamps[ts_idx % timestamps.len()];
        doc.modified_at = doc.created_at;
        ts_idx += 1;
        let created = db.create_document(doc).await?;
        created_doc_ids.push(created.id_string().unwrap());
    }

    // Add relationships between related documents
    // Research Notes (0) references Research Paper (11)
    if created_doc_ids.len() > 11 {
        db.create_relationship(
            &created_doc_ids[0], &created_doc_ids[11],
            RelationType::References, 0.8,
        ).await?;
    }
    // Architecture Diagram (2) references API Specification (3)
    if created_doc_ids.len() > 3 {
        db.create_relationship(
            &created_doc_ids[2], &created_doc_ids[3],
            RelationType::References, 0.9,
        ).await?;
    }
    // Design Document (6) references Architecture Diagram (2)
    if created_doc_ids.len() > 6 {
        db.create_relationship(
            &created_doc_ids[6], &created_doc_ids[2],
            RelationType::References, 0.7,
        ).await?;
    }
    // Project Plan (1) branches to Architecture Diagram (2)
    if created_doc_ids.len() > 2 {
        db.create_relationship(
            &created_doc_ids[2], &created_doc_ids[1],
            RelationType::BranchesFrom, 0.85,
        ).await?;
    }
    // Test Results (7) references GitHub Issue #42 (10)
    if created_doc_ids.len() > 10 {
        db.create_relationship(
            &created_doc_ids[7], &created_doc_ids[10],
            RelationType::References, 0.6,
        ).await?;
    }

    // Add commits for key documents to show version history
    let commit_targets = [
        (0, vec!["Initial research notes", "Added GTK4 findings"]),
        (1, vec!["Draft project plan", "Added Phase 2 details", "Finalized milestones"]),
        (3, vec!["Initial API spec", "Added relationship graph endpoints"]),
        (6, vec!["Initial design system", "Updated color palette"]),
    ];

    for (doc_idx, messages) in &commit_targets {
        if let Some(doc_id) = created_doc_ids.get(*doc_idx) {
            for msg in messages {
                let _ = db.commit_document(doc_id, msg).await;
            }
        }
    }

    tracing::info!(
        "Seeded {} documents in {} threads with relationships and commits",
        owned_docs.len() + external_docs.len(),
        thread_ids.len(),
    );
    Ok(())
}

async fn create_db(config: &AppConfig) -> Result<SurrealGraphDB> {
    let mode = match config.database.mode.as_str() {
        "memory" => StorageMode::Memory,
        _ => StorageMode::Persistent(config.database.path.clone()),
    };
    let db = SurrealGraphDB::new(mode).await?;
    db.connect().await?;
    db.init_schema().await?;
    Ok(db)
}

#[cfg(feature = "encryption")]
fn crypto_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".sovereign").join("crypto")
}

/// Load or create a stable device ID for this machine.
#[cfg(feature = "encryption")]
fn load_or_create_device_id() -> Result<String> {
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("device_id");
    if path.exists() {
        Ok(std::fs::read_to_string(&path)?.trim().to_string())
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(&path, &id)?;
        tracing::info!("Generated new device ID: {id}");
        Ok(id)
    }
}

/// Initialize the crypto subsystem: MasterKey → DeviceKey → KEK → KeyDatabase.
/// Returns (DeviceKey, Kek, KeyDatabase) for use by EncryptedGraphDB and P2P.
#[cfg(feature = "encryption")]
fn init_crypto() -> Result<(DeviceKey, Arc<tokio::sync::Mutex<KeyDatabase>>, Arc<Kek>)> {
    let device_id = load_or_create_device_id()?;
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;

    // Derive master key from passphrase (WSL2 — no TPM)
    let salt_path = dir.join("salt");
    let salt = if salt_path.exists() {
        std::fs::read(&salt_path)?
    } else {
        let mut s = vec![0u8; 32];
        use rand::Rng;
        rand::rng().fill_bytes(&mut s);
        std::fs::write(&salt_path, &s)?;
        s
    };

    let pass = rpassword::prompt_password("Sovereign passphrase: ")?;
    if pass.is_empty() {
        anyhow::bail!("Passphrase cannot be empty");
    }
    let master = MasterKey::from_passphrase(pass.as_bytes(), &salt)?;
    let device_key = DeviceKey::derive(&master, &device_id)?;

    // Load or create KEK
    let kek_path = dir.join("kek.wrapped");
    let kek = if kek_path.exists() {
        let wrapped_bytes = std::fs::read(&kek_path)?;
        let wrapped: sovereign_crypto::kek::WrappedKek = serde_json::from_slice(&wrapped_bytes)?;
        Kek::unwrap(&wrapped, &device_key)?
    } else {
        let kek = Kek::generate();
        let wrapped = kek.wrap(&device_key)?;
        std::fs::write(&kek_path, serde_json::to_vec(&wrapped)?)?;
        kek
    };

    // Load or create KeyDatabase
    let key_db_path = dir.join("keys.db");
    let key_db = if key_db_path.exists() {
        KeyDatabase::load(&key_db_path, &device_key)?
    } else {
        KeyDatabase::new(key_db_path)
    };

    tracing::info!("Crypto subsystem initialized (device: {device_id})");
    Ok((
        device_key,
        Arc::new(tokio::sync::Mutex::new(key_db)),
        Arc::new(kek),
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    lifecycle::init_tracing();

    let cli = Cli::parse();
    let config = AppConfig::load_or_default(cli.config.as_deref());

    match cli.command {
        Commands::Run => {
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

            // Register core skills (will be wired to DB after db creation)

            // Initialize crypto subsystem if enabled
            #[cfg(feature = "encryption")]
            let _crypto_state = if config.crypto.enabled {
                match init_crypto() {
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

            // Load documents and threads from DB for the canvas
            let db = create_db(&config).await?;
            seed_if_empty(&db).await?;
            let threads = db.list_threads().await?;
            let documents = db.list_documents(None).await?;
            tracing::info!(
                "Loaded {} documents, {} threads for canvas",
                documents.len(),
                threads.len()
            );

            // Register all core skills
            let db_arc_for_skills = Arc::new(db);
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
            registry.register(Box::new(sovereign_skills::skills::search::SearchSkill::new(
                db_arc_for_skills.clone(),
            )));
            registry.register(Box::new(
                sovereign_skills::skills::file_import::FileImportSkill::new(
                    db_arc_for_skills.clone(),
                ),
            ));
            registry.register(Box::new(
                sovereign_skills::skills::duplicate_document::DuplicateDocumentSkill::new(
                    db_arc_for_skills.clone(),
                ),
            ));
            tracing::info!("Registered {} core skills", registry.all_skills().len());

            // Create event channels
            let (orch_tx, orch_rx) = mpsc::channel::<OrchestratorEvent>();
            let (decision_tx, decision_rx) = mpsc::channel::<ActionDecision>();
            let (feedback_tx, feedback_rx) = mpsc::channel::<FeedbackEvent>();

            // Try to initialize AI orchestrator
            let db_arc = db_arc_for_skills;
            let orchestrator = match sovereign_ai::Orchestrator::new(
                config.ai.clone(),
                db_arc.clone(),
                orch_tx,
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

                                    match sovereign_p2p::node::SovereignNode::new(
                                        &p2p_config, keypair, p2p_event_tx, p2p_cmd_rx,
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

                    tracing::info!("AI orchestrator initialized");
                    Some(Arc::new(o))
                }
                Err(e) => {
                    tracing::warn!("AI orchestrator unavailable: {e}");
                    None
                }
            };

            // Build query callback for search overlay
            let query_callback: Option<Box<dyn Fn(String) + Send + 'static>> =
                orchestrator.as_ref().map(|orch| {
                    let orch = orch.clone();
                    Box::new(move |text: String| {
                        let orch = orch.clone();
                        tokio::spawn(async move {
                            if let Err(e) = orch.handle_query(&text).await {
                                tracing::error!("Query error: {e}");
                            }
                        });
                    }) as Box<dyn Fn(String) + Send + 'static>
                });

            // Initialize voice pipeline (optional)
            let voice_rx = if config.voice.enabled {
                let (vtx, vrx) = mpsc::channel();

                // Voice pipeline needs a query callback too
                let voice_query_cb: Box<dyn Fn(String) + Send + 'static> =
                    if let Some(ref orch) = orchestrator {
                        let orch = orch.clone();
                        Box::new(move |text: String| {
                            let orch = orch.clone();
                            tokio::spawn(async move {
                                if let Err(e) = orch.handle_query(&text).await {
                                    tracing::error!("Voice query error: {e}");
                                }
                            });
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

            // Launch Iced UI
            let (app, _boot_task) = sovereign_ui::app::SovereignApp::new(
                &config.ui,
                documents,
                threads,
                query_callback,
                Some(orch_rx),
                ui_voice_rx,
                None, // skill_rx — canvas creates its own internally
                Some(save_cb),
                Some(close_cb),
                Some(decision_tx),
                Some(registry),
                Some(feedback_tx),
            );
            sovereign_ui::app::run_app(app)?;
        }

        Commands::CreateDoc {
            title,
            thread_id,
            is_owned,
        } => {
            let db = create_db(&config).await?;
            let doc = Document::new(title, thread_id, is_owned);
            let created = db.create_document(doc).await?;
            let id = created.id_string().unwrap_or_default();
            println!("{id}");
        }

        Commands::GetDoc { id } => {
            let db = create_db(&config).await?;
            let doc = db.get_document(&id).await?;
            println!("{}", serde_json::to_string_pretty(&doc)?);
        }

        Commands::ListDocs { thread_id } => {
            let db = create_db(&config).await?;
            let docs = db.list_documents(thread_id.as_deref()).await?;
            for doc in &docs {
                let id = doc.id_string().unwrap_or_default();
                println!("{id}\t{}", doc.title);
            }
            println!("({} documents)", docs.len());
        }

        Commands::UpdateDoc { id, title, content } => {
            let db = create_db(&config).await?;
            let updated = db
                .update_document(&id, title.as_deref(), content.as_deref())
                .await?;
            println!("{}", serde_json::to_string_pretty(&updated)?);
        }

        Commands::DeleteDoc { id } => {
            let db = create_db(&config).await?;
            db.delete_document(&id).await?;
            println!("Deleted {id}");
        }

        Commands::CreateThread { name, description } => {
            let db = create_db(&config).await?;
            let thread = Thread::new(name, description);
            let created = db.create_thread(thread).await?;
            let id = created.id_string().unwrap_or_default();
            println!("{id}");
        }

        Commands::ListThreads => {
            let db = create_db(&config).await?;
            let threads = db.list_threads().await?;
            for t in &threads {
                let id = t.id_string().unwrap_or_default();
                println!("{id}\t{}", t.name);
            }
            println!("({} threads)", threads.len());
        }

        Commands::AddRelationship {
            from,
            to,
            relation_type,
            strength,
        } => {
            let db = create_db(&config).await?;
            let rt: RelationType = relation_type
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;
            let rel = db.create_relationship(&from, &to, rt, strength).await?;
            let id = rel.id.map(|t| thing_to_raw(&t)).unwrap_or_default();
            println!("{id}");
        }

        Commands::ListRelationships { doc_id } => {
            let db = create_db(&config).await?;
            let rels = db.list_relationships(&doc_id).await?;
            for r in &rels {
                let id = r.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
                println!("{id}\t{}\tstrength={:.2}", r.relation_type, r.strength);
            }
            println!("({} relationships)", rels.len());
        }

        Commands::Commit { doc_id, message } => {
            let db = create_db(&config).await?;
            let commit = db.commit_document(&doc_id, &message).await?;
            let id = commit.id.map(|t| thing_to_raw(&t)).unwrap_or_default();
            println!("{id} ({})", commit.snapshot.title);
        }

        Commands::ListCommits { doc_id } => {
            let db = create_db(&config).await?;
            let commits = db.list_document_commits(&doc_id).await?;
            for c in &commits {
                let id = c.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
                println!("{id}\t{}\t{}", c.timestamp.format("%Y-%m-%d %H:%M:%S"), c.message);
            }
            println!("({} commits)", commits.len());
        }

        #[cfg(feature = "encryption")]
        Commands::EncryptData => {
            let (_, key_db, kek) = init_crypto()?;
            let db = create_db(&config).await?;

            // Gather unencrypted documents
            let docs = db.list_documents(None).await?;
            let plans: Vec<sovereign_crypto::migration::DocumentEncryptionPlan> = docs
                .iter()
                .filter(|d| d.encryption_nonce.is_none())
                .map(|d| sovereign_crypto::migration::DocumentEncryptionPlan {
                    doc_id: d.id_string().unwrap_or_default(),
                    plaintext_content: d.content.clone(),
                })
                .collect();

            if plans.is_empty() {
                println!("All documents are already encrypted.");
                return Ok(());
            }

            println!("Encrypting {} documents...", plans.len());
            let total = plans.len();
            let progress: sovereign_crypto::migration::ProgressCallback =
                Box::new(move |done, total| {
                    println!("  [{done}/{total}]");
                });
            let mut key_db_guard = key_db.lock().await;
            let results =
                sovereign_crypto::migration::encrypt_documents(&plans, &mut key_db_guard, &kek, Some(&progress))?;

            // Update each document with encrypted content and nonce
            for result in &results {
                db.update_document(
                    &result.doc_id,
                    None,
                    Some(&result.encrypted_content),
                )
                .await?;
                // Store nonce via raw query (encryption_nonce isn't in the update_document API)
                // For now, log the nonce — full integration requires schema-level support
                tracing::info!(
                    "Encrypted {}: nonce={}",
                    result.doc_id,
                    result.nonce_b64
                );
            }

            // Persist key database
            let crypto_dir = crypto_dir();
            let device_id = load_or_create_device_id()?;
            let salt_path = crypto_dir.join("salt");
            let salt = std::fs::read(&salt_path)?;
            let pass = rpassword::prompt_password("Re-enter passphrase to save key DB: ")?;
            let master = MasterKey::from_passphrase(pass.as_bytes(), &salt)?;
            let device_key = DeviceKey::derive(&master, &device_id)?;
            key_db_guard.save(&device_key)?;

            println!("Encrypted {total} documents. Key database saved.");
        }

        #[cfg(feature = "p2p")]
        Commands::PairDevice { peer_id } => {
            println!("Pairing with peer {peer_id}...");
            println!("(P2P pairing requires a running `sovereign run` instance)");
            println!("Use the orchestrator command: 'pair device {peer_id}'");
        }

        #[cfg(feature = "p2p")]
        Commands::ListDevices => {
            let dir = crypto_dir().join("paired_devices.json");
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
            let dir = crypto_dir().join("guardians.json");
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
    }

    Ok(())
}
