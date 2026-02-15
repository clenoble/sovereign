use anyhow::Result;
use sovereign_core::content::ContentFields;
use sovereign_db::schema::{Document, RelationType, Thread};
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

/// Populate the database with sample data when it's empty.
/// Provides a visual baseline for testing the canvas.
pub async fn seed_if_empty(db: &SurrealGraphDB) -> Result<()> {
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
        thread_ids.push(
            created.id_string().ok_or_else(|| anyhow::anyhow!("Thread missing ID after creation"))?,
        );
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
        let content = ContentFields {
            body: body.to_string(),
            images: vec![],
        };
        doc.content = content.serialize();
        doc.created_at = timestamps[ts_idx % timestamps.len()];
        doc.modified_at = doc.created_at;
        ts_idx += 1;
        let created = db.create_document(doc).await?;
        created_doc_ids.push(
            created.id_string().ok_or_else(|| anyhow::anyhow!("Document missing ID after creation"))?,
        );
    }

    for (title, body, thread_idx) in &external_docs {
        let mut doc = Document::new(
            title.to_string(),
            thread_ids[*thread_idx].clone(),
            false,
        );
        let content = ContentFields {
            body: body.to_string(),
            images: vec![],
        };
        doc.content = content.serialize();
        doc.created_at = timestamps[ts_idx % timestamps.len()];
        doc.modified_at = doc.created_at;
        ts_idx += 1;
        let created = db.create_document(doc).await?;
        created_doc_ids.push(
            created.id_string().ok_or_else(|| anyhow::anyhow!("Document missing ID after creation"))?,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::surreal::StorageMode;

    async fn test_db() -> SurrealGraphDB {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        db
    }

    #[tokio::test]
    async fn seed_populates_empty_db() {
        let db = test_db().await;
        seed_if_empty(&db).await.unwrap();

        let threads = db.list_threads().await.unwrap();
        assert_eq!(threads.len(), 4, "Should create 4 threads");

        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 14, "Should create 14 documents (8 owned + 6 external)");

        let owned = docs.iter().filter(|d| d.is_owned).count();
        let external = docs.iter().filter(|d| !d.is_owned).count();
        assert_eq!(owned, 8);
        assert_eq!(external, 6);
    }

    #[tokio::test]
    async fn seed_is_idempotent() {
        let db = test_db().await;
        seed_if_empty(&db).await.unwrap();
        seed_if_empty(&db).await.unwrap(); // Should be a no-op

        let threads = db.list_threads().await.unwrap();
        assert_eq!(threads.len(), 4, "Should still be 4 threads after double seed");
    }
}
