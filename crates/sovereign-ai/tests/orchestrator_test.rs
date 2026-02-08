use std::sync::mpsc;
use std::sync::Arc;

use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use sovereign_db::schema::{Document, DocumentType, Thread};

/// Helper: create an in-memory SurrealDB with some test documents.
async fn setup_db_with_docs() -> Arc<SurrealGraphDB> {
    let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
    db.connect().await.unwrap();
    db.init_schema().await.unwrap();

    let thread = Thread::new("Work".to_string(), "Work thread".to_string());
    let thread = db.create_thread(thread).await.unwrap();
    let tid = thread.id_string().unwrap();

    let doc1 = Document::new(
        "Meeting Notes Q1".to_string(),
        DocumentType::Markdown,
        tid.clone(),
        true,
    );
    let doc2 = Document::new(
        "Budget Report 2026".to_string(),
        DocumentType::Spreadsheet,
        tid.clone(),
        true,
    );
    let doc3 = Document::new(
        "Project Roadmap".to_string(),
        DocumentType::Markdown,
        tid,
        true,
    );

    db.create_document(doc1).await.unwrap();
    db.create_document(doc2).await.unwrap();
    db.create_document(doc3).await.unwrap();

    Arc::new(db)
}

#[tokio::test]
async fn orchestrator_fails_without_model_files() {
    let db = setup_db_with_docs().await;
    let (tx, _rx) = mpsc::channel::<OrchestratorEvent>();

    // Default AiConfig has empty model paths — should fail to load
    let config = sovereign_core::config::AiConfig::default();
    let result = sovereign_ai::Orchestrator::new(config, db, tx).await;

    assert!(result.is_err(), "Orchestrator should fail when model files are missing");
}

#[tokio::test]
async fn voice_pipeline_fails_without_model_files() {
    let config = sovereign_core::config::VoiceConfig::default();
    let (tx, _rx) = mpsc::channel();

    let result = sovereign_ai::voice::VoicePipeline::spawn(
        config,
        tx,
        Box::new(|_| {}),
    );

    assert!(result.is_err(), "Voice pipeline should fail when wake word model is missing");
}
