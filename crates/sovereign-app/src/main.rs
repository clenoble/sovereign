use std::sync::mpsc;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::lifecycle;
use sovereign_db::schema::{thing_to_raw, Document, RelationType, Thread};
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
        db.create_document(doc).await?;
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

            // Launch GTK4 UI
            sovereign_ui::app::build_app(
                &config.ui,
                documents,
                threads,
                query_callback,
                Some(orch_rx),
                ui_voice_rx,
                None, // skill_rx — canvas creates its own internally
                Some(save_cb),
                Some(close_cb),
            );
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
    }

    Ok(())
}
