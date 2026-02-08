use std::sync::mpsc;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::lifecycle;
use sovereign_db::schema::{thing_to_raw, Document, DocumentType, RelationType, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use std::path::PathBuf;

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
        doc_type: String,
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

    /// Snapshot all documents into a commit
    Commit {
        #[arg(long)]
        message: String,
    },
}

/// Populate the database with sample data when it's empty.
/// Provides a visual baseline for testing the canvas.
async fn seed_if_empty(db: &SurrealGraphDB) -> Result<()> {
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

    let owned_docs = [
        ("Research Notes", DocumentType::Markdown, 0),
        ("Project Plan", DocumentType::Markdown, 1),
        ("Architecture Diagram", DocumentType::Image, 1),
        ("API Specification", DocumentType::Markdown, 1),
        ("Budget Overview", DocumentType::Spreadsheet, 3),
        ("Meeting Notes Q1", DocumentType::Markdown, 3),
        ("Design Document", DocumentType::Markdown, 2),
        ("Test Results", DocumentType::Data, 1),
    ];

    let external_docs = [
        ("Wikipedia: Rust", DocumentType::Web, 0),
        ("SO: GTK4 bindings", DocumentType::Web, 1),
        ("GitHub Issue #42", DocumentType::Web, 1),
        ("Research Paper (PDF)", DocumentType::Pdf, 0),
        ("Shared Spec", DocumentType::Markdown, 2),
        ("API Response Log", DocumentType::Data, 1),
    ];

    for (title, doc_type, thread_idx) in &owned_docs {
        let doc = Document::new(
            title.to_string(),
            doc_type.clone(),
            thread_ids[*thread_idx].clone(),
            true,
        );
        db.create_document(doc).await?;
    }

    for (title, doc_type, thread_idx) in &external_docs {
        let doc = Document::new(
            title.to_string(),
            doc_type.clone(),
            thread_ids[*thread_idx].clone(),
            false,
        );
        db.create_document(doc).await?;
    }

    // Add some relationships
    let docs = db.list_documents(None).await?;
    if docs.len() >= 4 {
        let id0 = docs[0].id_string().unwrap();
        let id3 = docs[3].id_string().unwrap();
        db.create_relationship(&id0, &id3, RelationType::References, 0.8)
            .await?;
    }

    tracing::info!("Seeded {} documents in {} threads", owned_docs.len() + external_docs.len(), thread_ids.len());
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

#[tokio::main]
async fn main() -> Result<()> {
    lifecycle::init_tracing();

    let cli = Cli::parse();
    let config = AppConfig::load_or_default(cli.config.as_deref());

    match cli.command {
        Commands::Run => {
            // Scan skills directory
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

            // Create event channels
            let (orch_tx, orch_rx) = mpsc::channel::<OrchestratorEvent>();

            // Try to initialize AI orchestrator
            let db_arc = Arc::new(db);
            let orchestrator = match sovereign_ai::Orchestrator::new(
                config.ai.clone(),
                db_arc.clone(),
                orch_tx,
            )
            .await
            {
                Ok(o) => {
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

            // Launch GTK4 UI
            sovereign_ui::app::build_app(
                &config.ui,
                documents,
                threads,
                query_callback,
                Some(orch_rx),
                ui_voice_rx,
            );
        }

        Commands::CreateDoc {
            title,
            doc_type,
            thread_id,
            is_owned,
        } => {
            let db = create_db(&config).await?;
            let dt: DocumentType = doc_type
                .parse()
                .map_err(|e: String| anyhow::anyhow!(e))?;
            let doc = Document::new(title, dt, thread_id, is_owned);
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
                println!("{id}\t{}\t{}", doc.title, doc.doc_type);
            }
            println!("({} documents)", docs.len());
        }

        Commands::UpdateDoc { id, title, content } => {
            let db = create_db(&config).await?;
            let updated = db
                .update_document(&id, title.as_deref(), content.as_deref(), None)
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

        Commands::Commit { message } => {
            let db = create_db(&config).await?;
            let commit = db.commit(&message).await?;
            let id = commit.id.map(|t| thing_to_raw(&t)).unwrap_or_default();
            println!("{id} ({} document snapshots)", commit.snapshots.len());
        }
    }

    Ok(())
}
