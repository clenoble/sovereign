use anyhow::Result;
use clap::{Parser, Subcommand};
use sovereign_core::config::AppConfig;
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
                    tracing::info!("  - {} v{} ({})", manifest.name, manifest.version, manifest.description);
                }
            }

            // Launch GTK4 UI
            sovereign_ui::app::build_app(&config.ui);
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
