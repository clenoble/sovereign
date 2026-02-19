use anyhow::Result;
use sovereign_core::content::ContentFields;
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Document, Message, MessageDirection,
    ReadStatus, RelationType, Thread,
};
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
            ..Default::default()
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
            ..Default::default()
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

    // ── Contacts ───────────────────────────────────────────────────────────
    let contact_defs: Vec<(&str, bool, Vec<(ChannelType, &str, bool)>)> = vec![
        ("You", true, vec![
            (ChannelType::Email, "me@sovereign.local", true),
            (ChannelType::Signal, "+1-555-0100", false),
        ]),
        ("Alice Chen", false, vec![
            (ChannelType::Email, "alice.chen@example.com", true),
            (ChannelType::Signal, "+1-555-0101", false),
        ]),
        ("Bob Martinez", false, vec![
            (ChannelType::Email, "bob.m@example.com", true),
            (ChannelType::WhatsApp, "+1-555-0102", false),
        ]),
        ("Carol Nguyen", false, vec![
            (ChannelType::Email, "carol.n@example.com", true),
            (ChannelType::Sms, "+1-555-0103", false),
        ]),
        ("David Park", false, vec![
            (ChannelType::Signal, "+1-555-0104", true),
        ]),
    ];

    let mut contact_ids = Vec::new();
    for (name, is_owned, addresses) in &contact_defs {
        let mut contact = Contact::new(name.to_string(), *is_owned);
        contact.addresses = addresses
            .iter()
            .map(|(ch, addr, primary)| ChannelAddress {
                channel: ch.clone(),
                address: addr.to_string(),
                display_name: None,
                is_primary: *primary,
            })
            .collect();
        let created = db.create_contact(contact).await?;
        contact_ids.push(
            created
                .id_string()
                .ok_or_else(|| anyhow::anyhow!("Contact missing ID"))?,
        );
    }

    // ── Conversations ────────────────────────────────────────────────────
    // 0=You, 1=Alice, 2=Bob, 3=Carol, 4=David
    let conv_defs: Vec<(&str, ChannelType, Vec<usize>, Option<usize>)> = vec![
        ("Architecture discussion", ChannelType::Email, vec![0, 1], Some(1)),   // linked to Development thread
        ("Design feedback", ChannelType::Signal, vec![0, 1, 4], Some(2)),       // linked to Design thread
        ("Budget approval", ChannelType::WhatsApp, vec![0, 2], None),
        ("Quick check-in", ChannelType::Sms, vec![0, 3], None),
    ];

    let mut conv_ids = Vec::new();
    for (title, channel, participant_idxs, linked_thread_idx) in &conv_defs {
        let participants: Vec<String> = participant_idxs
            .iter()
            .map(|&i| contact_ids[i].clone())
            .collect();
        let mut conv = Conversation::new(title.to_string(), channel.clone(), participants);
        if let Some(ti) = linked_thread_idx {
            conv.linked_thread_id = Some(thread_ids[*ti].clone());
        }
        let created = db.create_conversation(conv).await?;
        conv_ids.push(
            created
                .id_string()
                .ok_or_else(|| anyhow::anyhow!("Conversation missing ID"))?,
        );
    }

    // ── Messages ─────────────────────────────────────────────────────────
    // (conv_idx, from_idx, to_idxs, body, direction, is_read, minutes_offset)
    let msg_defs: Vec<(usize, usize, Vec<usize>, &str, MessageDirection, bool, i64)> = vec![
        // Architecture discussion (email, conv 0)
        (0, 1, vec![0], "Hey, I reviewed the architecture doc. The component separation looks solid. One question — should we keep the DB abstraction as a trait or move to concrete types?", MessageDirection::Inbound, true, 0),
        (0, 0, vec![1], "Good catch. Let's keep the trait — it lets us swap SurrealDB for SQLite later if needed, and the mock is useful for tests.", MessageDirection::Outbound, true, 15),
        (0, 1, vec![0], "Makes sense. I'll update the API spec to reference the trait methods. Should be done by Friday.", MessageDirection::Inbound, true, 30),
        (0, 0, vec![1], "Perfect. I'll set up the integration tests in the meantime.", MessageDirection::Outbound, true, 45),
        (0, 1, vec![0], "One more thing — can we add a batch insert method? Seeding 14 docs one by one is slow.", MessageDirection::Inbound, false, 120),
        // Design feedback (Signal, conv 1)
        (1, 4, vec![0, 1], "Just tested the dark theme on my display. The contrast ratios look good — WCAG AA compliant.", MessageDirection::Inbound, true, 0),
        (1, 0, vec![1, 4], "Great to hear! Alice, what do you think about adding a light theme option?", MessageDirection::Outbound, true, 10),
        (1, 1, vec![0, 4], "I'd prioritize it after the canvas is stable. Users definitely expect a light mode though.", MessageDirection::Inbound, true, 25),
        (1, 4, vec![0, 1], "Agreed. I can mock up light theme colors this week if you want.", MessageDirection::Inbound, false, 40),
        // Budget approval (WhatsApp, conv 2)
        (2, 2, vec![0], "Hi! I looked at the budget overview. The $500 for infrastructure seems low — are we accounting for CI/CD costs?", MessageDirection::Inbound, true, 0),
        (2, 0, vec![2], "Good point. We're self-hosting CI on the NAS for now, but I'll add a line item for cloud CI as a contingency.", MessageDirection::Outbound, true, 20),
        (2, 2, vec![0], "Sounds good. I'll approve the revised budget once you update the doc.", MessageDirection::Inbound, false, 35),
        // Quick check-in (SMS, conv 3)
        (3, 3, vec![0], "Hey, are we still meeting Thursday?", MessageDirection::Inbound, true, 0),
        (3, 0, vec![3], "Yes! 2pm at the usual spot.", MessageDirection::Outbound, true, 5),
        (3, 3, vec![0], "See you there", MessageDirection::Inbound, true, 8),
    ];

    let msg_base = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
    for (conv_idx, from_idx, to_idxs, body, direction, is_read, minutes) in &msg_defs {
        let to_ids: Vec<String> = to_idxs.iter().map(|&i| contact_ids[i].clone()).collect();
        let channel = conv_defs[*conv_idx].1.clone();
        let mut msg = Message::new(
            conv_ids[*conv_idx].clone(),
            channel,
            direction.clone(),
            contact_ids[*from_idx].clone(),
            to_ids,
            body.to_string(),
        );
        msg.sent_at = msg_base + chrono::Duration::minutes(*minutes);
        msg.created_at = msg.sent_at;
        if *is_read {
            msg.read_status = ReadStatus::Read;
        }
        db.create_message(msg).await?;
    }

    // Update unread counts on conversations
    let unread_counts = [1u32, 1, 1, 0]; // conv 0-3
    for (i, &count) in unread_counts.iter().enumerate() {
        if count > 0 {
            db.update_conversation_unread(&conv_ids[i], count).await?;
        }
    }

    tracing::info!(
        "Seeded {} documents, {} threads, {} contacts, {} conversations, {} messages",
        owned_docs.len() + external_docs.len(),
        thread_ids.len(),
        contact_ids.len(),
        conv_ids.len(),
        msg_defs.len(),
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

        // Contacts
        let contacts = db.list_contacts().await.unwrap();
        assert_eq!(contacts.len(), 5, "Should create 5 contacts");
        let owned_contacts = contacts.iter().filter(|c| c.is_owned).count();
        assert_eq!(owned_contacts, 1, "1 owned contact (You)");

        // Conversations
        let convs = db.list_conversations(None).await.unwrap();
        assert_eq!(convs.len(), 4, "Should create 4 conversations");
        let linked = convs.iter().filter(|c| c.linked_thread_id.is_some()).count();
        assert_eq!(linked, 2, "2 conversations linked to threads");
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
