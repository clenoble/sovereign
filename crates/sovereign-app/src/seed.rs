use std::io::Write;
use std::path::Path;

use anyhow::Result;
use sovereign_core::content::ContentFields;
use sovereign_core::profile::{BubbleStyle, SuggestionFeedback, UserProfile};
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Document, Message, MessageDirection,
    ReadStatus, RelationType, Thread,
};
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

#[cfg(feature = "encryption")]
use std::sync::Arc;
#[cfg(feature = "encryption")]
use sovereign_crypto::device_key::DeviceKey;

/// Populate the database with sample data when it's empty.
/// Provides a visual baseline for testing the canvas.
pub async fn seed_if_empty(db: &SurrealGraphDB) -> Result<()> {
    use chrono::{Duration, Utc};

    let threads = db.list_threads().await?;
    let contacts = db.list_contacts().await?;

    if !threads.is_empty() && !contacts.is_empty() {
        return Ok(());
    }

    let needs_base = threads.is_empty();
    let needs_comms = contacts.is_empty();

    tracing::info!("Seeding sample data (base={}, comms={})", needs_base, needs_comms);

    let thread_defs = [
        ("Research", "Research and exploration"),
        ("Development", "Engineering and code"),
        ("Design", "UX and visual design"),
        ("Admin", "Administrative and planning"),
    ];

    let mut thread_ids = Vec::new();
    if needs_base {
        for (name, desc) in &thread_defs {
            let t = Thread::new(name.to_string(), desc.to_string());
            let created = db.create_thread(t).await?;
            thread_ids.push(
                created.id_string().ok_or_else(|| anyhow::anyhow!("Thread missing ID after creation"))?,
            );
        }
    } else {
        // Threads already exist — collect their IDs in definition order
        for (name, _) in &thread_defs {
            let found = threads.iter().find(|t| t.name == *name);
            if let Some(t) = found {
                thread_ids.push(
                    t.id_string().ok_or_else(|| anyhow::anyhow!("Existing thread missing ID"))?,
                );
            }
        }
        if thread_ids.len() < thread_defs.len() {
            tracing::warn!("Could not find all expected threads; skipping comms seeding");
            return Ok(());
        }
    }

    if needs_base {
    // Relative timestamps: offsets from "now" so seed data is always in the past
    let now = Utc::now();
    let timestamps = [
        now - Duration::days(90) + Duration::hours(10),
        now - Duration::days(77) + Duration::hours(14) + Duration::minutes(30),
        now - Duration::days(62) + Duration::hours(9) + Duration::minutes(15),
        now - Duration::days(50) + Duration::hours(11),
        now - Duration::days(36) + Duration::hours(16) + Duration::minutes(45),
        now - Duration::days(29) + Duration::hours(8),
        now - Duration::days(19) + Duration::hours(13) + Duration::minutes(20),
        now - Duration::days(9) + Duration::hours(10) + Duration::minutes(30),
        now - Duration::days(7) + Duration::hours(9),
        now - Duration::days(6) + Duration::hours(15),
        now - Duration::days(5) + Duration::hours(11) + Duration::minutes(30),
        now - Duration::days(4) + Duration::hours(14),
        now - Duration::days(2) + Duration::hours(10),
        now - Duration::days(1) + Duration::hours(16),
    ];
    let owned_docs: Vec<(&str, &str, usize)> = vec![
        ("Research Notes", "# Research Notes\n\nExploring Rust + GTK4 for desktop OS development.\n\n## Key Findings\n- GTK4 bindings are solid\n- Skia provides GPU rendering", 0),
        ("Project Plan", "# Project Plan\n\n## Phase 1: Foundation\n- Data layer\n- UI shell\n\n## Phase 2: Canvas\n- Spatial layout\n- GPU rendering", 1),
        ("Architecture Diagram", "# Architecture\n\nComponent overview for Sovereign GE.", 1),
        ("API Specification", "# API Spec\n\n## Endpoints\n- Document CRUD\n- Thread management\n- Relationship graph", 1),
        ("Budget Overview", "# Budget 2026\n\n| Item | Cost |\n|------|------|\n| Infrastructure | $500 |\n| Tools | $200 |", 3),
        ("Meeting Notes Q1", "# Meeting Notes — Q1 2026\n\n## Jan 15\n- Discussed architecture\n- Agreed on Rust + GTK4 stack", 3),
        ("Design Document", "# Design System\n\n## Colors\n- Background: #0e0e10\n- Accent: #5a9fd4\n\n## Typography\n- System font, 13-16px", 2),
        ("Test Results", "# Test Results\n\n- sovereign-db: 12 pass\n- sovereign-app: 12 pass\n- sovereign-ai: 8 pass", 1),
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
    if created_doc_ids.len() > 11 {
        db.create_relationship(
            &created_doc_ids[0], &created_doc_ids[11],
            RelationType::References, 0.8,
        ).await?;
    }
    if created_doc_ids.len() > 3 {
        db.create_relationship(
            &created_doc_ids[2], &created_doc_ids[3],
            RelationType::References, 0.9,
        ).await?;
    }
    if created_doc_ids.len() > 6 {
        db.create_relationship(
            &created_doc_ids[6], &created_doc_ids[2],
            RelationType::References, 0.7,
        ).await?;
    }
    if created_doc_ids.len() > 2 {
        db.create_relationship(
            &created_doc_ids[2], &created_doc_ids[1],
            RelationType::BranchesFrom, 0.85,
        ).await?;
    }
    if created_doc_ids.len() > 10 {
        db.create_relationship(
            &created_doc_ids[7], &created_doc_ids[10],
            RelationType::References, 0.6,
        ).await?;
    }

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
    } // needs_base

    // ── Contacts ───────────────────────────────────────────────────────────
    if needs_comms {
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

    let msg_base = Utc::now() - Duration::days(2) + Duration::hours(9);
    for (idx, (conv_idx, from_idx, to_idxs, body, direction, is_read, minutes)) in
        msg_defs.iter().enumerate()
    {
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
        // Stagger conversations by ~73 minutes so their first messages don't
        // collide at msg_base (which would visually stack message circles on
        // the canvas timeline when zoomed out at the hour level). Per-message
        // seconds jitter spreads them further at the minute level.
        let conv_offset_minutes = (*conv_idx as i64) * 73;
        let jitter_seconds = ((idx as i64) * 17) % 59;
        msg.sent_at = msg_base
            + Duration::minutes(conv_offset_minutes + *minutes)
            + Duration::seconds(jitter_seconds);
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
        "Seeded {} contacts, {} conversations, {} messages",
        contact_ids.len(),
        conv_ids.len(),
        msg_defs.len(),
    );
    } // needs_comms

    Ok(())
}

/// Seed the user profile and session log with realistic multi-day history.
/// Called once during first launch so the orchestrator has context for testing.
pub fn seed_profile_and_history(profile_dir: &Path) -> Result<()> {
    // Seed UserProfile if it doesn't exist
    let profile_path = profile_dir.join("user_profile.json");
    if !profile_path.exists() {
        std::fs::create_dir_all(profile_dir)?;

        let mut profile = UserProfile::default_new();
        profile.display_name = Some("Alex".into());
        profile.nickname = Some("Ike".into());
        profile.bubble_style = BubbleStyle::Wave;
        profile.interaction_patterns.command_verbosity = "detailed".into();
        profile
            .skill_preferences
            .insert("text".into(), "markdown-editor".into());
        profile
            .skill_preferences
            .insert("export".into(), "pdf-export".into());

        // Simulate suggestion feedback history
        let mut adopt_fb = SuggestionFeedback::new();
        adopt_fb.accepted = 4;
        adopt_fb.dismissed = 1;
        adopt_fb.shown = 5;
        profile
            .suggestion_feedback
            .insert("adopt".into(), adopt_fb);

        let mut thread_fb = SuggestionFeedback::new();
        thread_fb.accepted = 2;
        thread_fb.shown = 2;
        profile
            .suggestion_feedback
            .insert("create_thread".into(), thread_fb);

        profile.save(profile_dir)?;
        tracing::info!(
            "Seeded user profile: display_name=Alex, nickname=Ike, designation={}, bubble=Wave",
            profile.designation,
        );
    }

    // Seed session log if it doesn't exist
    let log_path = profile_dir.join("session_log.jsonl");
    if !log_path.exists() {
        let mut file = std::fs::File::create(&log_path)?;

        // 3 days of realistic interaction history (relative to now)
        // Consistent with the DB seed data (documents, contacts, conversations)
        use chrono::{Duration, Utc};
        let now = Utc::now();
        let d1 = (now - Duration::days(5)).format("%Y-%m-%d");
        let d2 = (now - Duration::days(4)).format("%Y-%m-%d");
        let d3 = (now - Duration::days(3)).format("%Y-%m-%d");

        let entries: Vec<String> = vec![
            // Day 1: initial exploration
            format!(r#"{{"ts":"{d1}T09:15:00Z","type":"user_input","mode":"text","content":"search meeting notes","intent":"search"}}"#),
            format!(r#"{{"ts":"{d1}T09:15:01Z","type":"orchestrator_action","action":"search","details":"found 1 match: Meeting Notes Q1"}}"#),
            format!(r#"{{"ts":"{d1}T09:20:00Z","type":"user_input","mode":"text","content":"open project plan","intent":"open"}}"#),
            format!(r#"{{"ts":"{d1}T09:20:01Z","type":"orchestrator_action","action":"open","details":"opened Project Plan"}}"#),
            format!(r#"{{"ts":"{d1}T14:30:00Z","type":"user_input","mode":"chat","content":"what threads do I have?","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d1}T14:30:03Z","type":"chat_response","content":"You have 4 threads: Research, Development, Design, and Admin. Development has the most documents with 7, followed by Research with 3."}}"#),
            format!(r#"{{"ts":"{d1}T14:31:00Z","type":"user_input","mode":"chat","content":"how many documents are in Development?","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d1}T14:31:02Z","type":"chat_response","content":"The Development thread has 7 documents: Project Plan, Architecture Diagram, API Specification, and Test Results (all owned), plus 3 external references — a Stack Overflow Q&A on GTK4 bindings, GitHub Issue #42, and an API Response Log."}}"#),
            // Day 2: deeper work
            format!(r#"{{"ts":"{d2}T10:00:00Z","type":"user_input","mode":"text","content":"find architecture","intent":"search"}}"#),
            format!(r#"{{"ts":"{d2}T10:00:01Z","type":"orchestrator_action","action":"search","details":"found 1 match: Architecture Diagram"}}"#),
            format!(r#"{{"ts":"{d2}T11:15:00Z","type":"user_input","mode":"chat","content":"summarize the design document","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d2}T11:15:04Z","type":"chat_response","content":"From your Design Document (owned): it defines the visual system for Sovereign GE — dark background (#0e0e10), accent blue (#5a9fd4), with system font at 13-16px. It references the Architecture Diagram for component structure."}}"#),
            format!(r#"{{"ts":"{d2}T14:00:00Z","type":"user_input","mode":"chat","content":"who are my contacts?","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d2}T14:00:03Z","type":"chat_response","content":"You have 4 contacts: Alice Chen (email, Signal), Bob Martinez (email, WhatsApp), Carol Nguyen (email, SMS), and David Park (Signal)."}}"#),
            format!(r#"{{"ts":"{d2}T15:00:00Z","type":"user_input","mode":"text","content":"create thread Prototyping","intent":"create_thread"}}"#),
            format!(r#"{{"ts":"{d2}T15:00:01Z","type":"orchestrator_action","action":"create_thread","details":"created thread Prototyping"}}"#),
            // Day 3: conversations and follow-ups
            format!(r#"{{"ts":"{d3}T09:00:00Z","type":"user_input","mode":"chat","content":"what did Alice say about the architecture?","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d3}T09:00:05Z","type":"chat_response","content":"In your email conversation 'Architecture discussion' with Alice Chen: she reviewed the architecture doc and asked about keeping the DB abstraction as a trait. You agreed to keep it for future flexibility. She said she'd update the API spec by Friday. Her latest message (unread) asks about adding a batch insert method for seeding."}}"#),
            format!(r#"{{"ts":"{d3}T10:30:00Z","type":"user_input","mode":"text","content":"open budget overview","intent":"open"}}"#),
            format!(r#"{{"ts":"{d3}T10:30:01Z","type":"orchestrator_action","action":"open","details":"opened Budget Overview"}}"#),
            format!(r#"{{"ts":"{d3}T14:00:00Z","type":"user_input","mode":"chat","content":"what are my unread messages?","intent":"chat"}}"#),
            format!(r#"{{"ts":"{d3}T14:00:04Z","type":"chat_response","content":"You have 3 unread messages across 3 conversations:\n1. Alice Chen in 'Architecture discussion' (email) — asks about adding a batch insert method\n2. David Park in 'Design feedback' (Signal) — offers to mock up light theme colors this week\n3. Bob Martinez in 'Budget approval' (WhatsApp) — waiting for the revised budget doc"}}"#),
            format!(r#"{{"ts":"{d3}T16:00:00Z","type":"user_input","mode":"text","content":"search test results","intent":"search"}}"#),
            format!(r#"{{"ts":"{d3}T16:00:01Z","type":"orchestrator_action","action":"search","details":"found 1 match: Test Results"}}"#),
        ];

        for entry in &entries {
            writeln!(file, "{entry}")?;
        }

        tracing::info!("Seeded session log with {} entries", entries.len());
    }

    Ok(())
}

/// Populate the database with sample PII data when no entities exist yet.
///
/// Provides a visual baseline for testing the PII management dashboard:
/// 5 entities across all `EntityKind` variants, ~10 PiiRecords spanning
/// discovered findings (`stored_secret = false`, mixed review states) and
/// vault entries (`stored_secret = true`, encrypted under the DeviceKey),
/// plus 3 ShareRecords on the disclosure ledger.
///
/// Skipped if any entity already exists, so it's idempotent across restarts.
/// Requires the `encryption` feature because vault entries need a real
/// DeviceKey for the encrypt → reveal round-trip to work in the dashboard.
#[cfg(feature = "encryption")]
pub async fn seed_pii_if_empty(db: &SurrealGraphDB, device_key: &Arc<DeviceKey>) -> Result<()> {
    use chrono::{Duration, Utc};
    use sovereign_crypto::vault::EncryptedBlob;
    use sovereign_db::schema::{
        Entity, EntityKind, PiiKind, PiiRecord, ReviewState, ShareChannel, ShareRecord, SourceRef,
    };

    let entities = db.list_entities().await?;
    if !entities.is_empty() {
        return Ok(());
    }

    tracing::info!("Seeding sample PII data (entities, records, share ledger)");

    // ── Entities ─────────────────────────────────────────────────────────
    // (name, kind, domains, is_owned, notes)
    let entity_defs: Vec<(&str, EntityKind, Vec<&str>, bool, &str)> = vec![
        ("You",                EntityKind::SelfEntity, vec![],                                  true,  "Self entity — own PII anchor"),
        ("Acme Bank",          EntityKind::Org,        vec!["acmebank.example", "acmebank.ch"], false, "Primary checking + savings"),
        ("GitHub",             EntityKind::Service,    vec!["github.com"],                      false, "Code hosting + CI"),
        ("Dr. Sarah Kim",      EntityKind::Person,     vec![],                                  false, "Family doctor"),
        ("Sunlife Insurance",  EntityKind::Org,        vec!["sunlife.example"],                 false, "Health + life policies"),
    ];

    let now = Utc::now();
    let mut entity_ids = Vec::new();
    for (name, kind, domains, is_owned, notes) in &entity_defs {
        let mut ent = Entity::new(name.to_string(), kind.clone());
        ent.domains = domains.iter().map(|s| s.to_string()).collect();
        ent.is_owned = *is_owned;
        ent.notes = notes.to_string();
        let created = db.create_entity(ent).await?;
        entity_ids.push(
            created.id_string().ok_or_else(|| anyhow::anyhow!("Entity missing ID"))?,
        );
    }
    // Index: 0=Self, 1=Acme Bank, 2=GitHub, 3=Doctor, 4=Sunlife

    // ── Discovered PII (stored_secret = false) ────────────────────────────
    // (kind, label, value, entity_idx, confidence, review_state, days_ago)
    let discovered: Vec<(PiiKind, &str, &str, usize, f32, ReviewState, i64)> = vec![
        (PiiKind::Email,       "personal email",       "alex@personal.example",       0, 0.95, ReviewState::Confirmed,  30),
        (PiiKind::Phone,       "mobile",               "+1-555-0100",                 0, 0.92, ReviewState::Confirmed,  28),
        (PiiKind::Dob,         "date of birth",        "1990-04-12",                  0, 0.88, ReviewState::Unreviewed, 14),
        (PiiKind::Ssn,         "SSN (US)",             "123-45-6789",                 0, 0.97, ReviewState::Unreviewed, 10),
        (PiiKind::Iban,        "Acme checking IBAN",   "CH93 0076 2011 6238 5295 7",  1, 0.99, ReviewState::Confirmed,   7),
        (PiiKind::Email,       "Alice's address",      "alice.chen@example.com",      1, 0.65, ReviewState::Dismissed,   6),
        (PiiKind::Address,     "home address",         "742 Evergreen Terrace",       0, 0.80, ReviewState::Unreviewed,  3),
    ];

    let mut discovered_ids = Vec::new();
    for (kind, label, value, ent_idx, confidence, state, days_ago) in &discovered {
        let blob = EncryptedBlob::encrypt_str(value, device_key)
            .map_err(|e| anyhow::anyhow!("seed PII encrypt failed: {e}"))?;
        let discovered_at = now - Duration::days(*days_ago);
        let record = PiiRecord {
            id: None,
            kind: kind.clone(),
            value_encrypted: blob.ciphertext_b64,
            value_nonce: blob.nonce_b64,
            label: Some(label.to_string()),
            entity_id: Some(entity_ids[*ent_idx].clone()),
            stored_secret: false,
            confidence: *confidence,
            sources: vec![],
            discovered_at,
            last_revealed_at: None,
            use_count: 0,
            review_state: state.clone(),
            deleted_at: None,
        };
        let created = db.create_pii_record(record).await?;
        discovered_ids.push(
            created.id_string().ok_or_else(|| anyhow::anyhow!("PiiRecord missing ID"))?,
        );
    }

    // ── Vault entries (stored_secret = true, always Confirmed) ────────────
    // (kind, label, value, entity_idx, days_ago)
    let vault: Vec<(PiiKind, &str, &str, usize, i64)> = vec![
        (PiiKind::Password,     "main password",        "Tr0ub4dor&3-correct-horse",      2, 21),
        (PiiKind::ApiToken,     "personal access token","ghp_seedFakeToken1234567890ABCD", 2, 18),
        (PiiKind::BankAccount,  "checking #",           "1234-5678-9012",                 1, 12),
        (PiiKind::DocumentId,   "passport",             "X1234567",                       0,  9),
        (PiiKind::Note,         "office wifi",          "WaitingRoom-Wifi-2026",          3,  4),
    ];

    for (kind, label, value, ent_idx, days_ago) in &vault {
        let blob = EncryptedBlob::encrypt_str(value, device_key)
            .map_err(|e| anyhow::anyhow!("seed vault encrypt failed: {e}"))?;
        let discovered_at = now - Duration::days(*days_ago);
        let record = PiiRecord {
            id: None,
            kind: kind.clone(),
            value_encrypted: blob.ciphertext_b64,
            value_nonce: blob.nonce_b64,
            label: Some(label.to_string()),
            entity_id: Some(entity_ids[*ent_idx].clone()),
            stored_secret: true,
            confidence: 1.0,
            sources: vec![],
            discovered_at,
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        };
        db.create_pii_record(record).await?;
    }

    // ── Share ledger ─────────────────────────────────────────────────────
    // Disclosure events that show up under each recipient's "Shared" tab.
    // (pii_record_idx into discovered_ids, recipient entity_idx, channel, days_ago, via_url)
    let shares: Vec<(usize, usize, ShareChannel, i64, Option<&str>)> = vec![
        (0, 1, ShareChannel::Web,   29, Some("https://acmebank.example/signup")),    // email → Acme Bank
        (1, 2, ShareChannel::Web,   25, Some("https://github.com/join")),            // phone → GitHub
        (2, 4, ShareChannel::Web,   12, Some("https://sunlife.example/onboarding")), // DOB → Sunlife
    ];

    for (pii_idx, ent_idx, channel, days_ago, via_url) in &shares {
        let shared_at = now - Duration::days(*days_ago);
        let record = ShareRecord {
            id: None,
            pii_record_id: discovered_ids[*pii_idx].clone(),
            to_entity_id: entity_ids[*ent_idx].clone(),
            via_message_id: None,
            via_url: via_url.map(|s| s.to_string()),
            shared_at,
            channel: channel.clone(),
        };
        db.create_share_record(record).await?;
    }

    // ── PII-bearing documents ────────────────────────────────────────────
    // For the dashboard's mask/reveal flow on document content: the canonical
    // body (`Document.content`) carries `[pii:<record_id>]` tokens, while the
    // original PII-inline text is encrypted into `body_raw_encrypted`. Each
    // referenced record gets a SourceRef pointing back at the token's byte
    // span in the canonical body.
    use sovereign_db::schema::SourceKind;
    use std::collections::HashMap;

    let threads = db.list_threads().await?;
    let host_thread_id = threads
        .iter()
        .find(|t| t.name == "Admin")
        .or_else(|| threads.first())
        .and_then(|t| t.id_string());

    let mut pii_docs_count = 0usize;
    if let Some(thread_id) = host_thread_id {
        // (title, [(text-prefix, record_idx into `discovered`)])
        // Each segment writes prefix + the record's plaintext value (raw)
        // or prefix + `[pii:<id>]` token (canonical).
        let pii_docs: Vec<(&str, Vec<(&str, usize)>)> = vec![
            (
                "Personal contact card",
                vec![
                    ("Email: ", 0),         // alex@personal.example
                    ("\nMobile: ", 1),      // +1-555-0100
                    ("\nIBAN (Acme): ", 4), // CH93 0076 2011 6238 5295 7
                ],
            ),
            (
                "Insurance application draft",
                vec![
                    ("Date of birth: ", 2), // 1990-04-12
                    ("\nSSN: ", 3),         // 123-45-6789
                    ("\nHome: ", 6),        // 742 Evergreen Terrace
                ],
            ),
        ];

        // Records can be referenced from multiple documents — accumulate
        // sources per record then write them all in one update at the end.
        let mut record_sources: HashMap<usize, Vec<SourceRef>> = HashMap::new();

        for (title, segments) in &pii_docs {
            let mut raw = String::new();
            let mut canonical = String::new();
            let mut spans: Vec<(usize, usize, usize)> = Vec::new();

            for (prefix, rec_idx) in segments {
                raw.push_str(prefix);
                canonical.push_str(prefix);

                let value = discovered[*rec_idx].2;
                raw.push_str(value);

                let token = format!("[pii:{}]", discovered_ids[*rec_idx]);
                let span_start = canonical.len();
                canonical.push_str(&token);
                let span_end = canonical.len();
                spans.push((*rec_idx, span_start, span_end));
            }

            let mut doc = Document::new(title.to_string(), thread_id.clone(), true);
            let content = ContentFields {
                body: canonical,
                ..Default::default()
            };
            doc.content = content.serialize();
            let created = db.create_document(doc).await?;
            let doc_id = created
                .id_string()
                .ok_or_else(|| anyhow::anyhow!("PII doc missing ID"))?;

            // Encrypt the raw (PII-inline) body so reveal can decrypt it later.
            let raw_blob = EncryptedBlob::encrypt_str(&raw, device_key)
                .map_err(|e| anyhow::anyhow!("seed raw-body encrypt failed: {e}"))?;
            db.update_document_pii_fields(
                &doc_id,
                Some(&raw_blob.ciphertext_b64),
                Some(&raw_blob.nonce_b64),
                Some(now),
            )
            .await?;

            for (rec_idx, span_start, span_end) in spans {
                record_sources.entry(rec_idx).or_default().push(SourceRef {
                    source_kind: SourceKind::Document,
                    source_id: doc_id.clone(),
                    span_start,
                    span_end,
                });
            }
            pii_docs_count += 1;
        }

        // Replace each referenced record's sources with the accumulated list.
        for (rec_idx, sources) in record_sources {
            db.update_pii_record_sources(&discovered_ids[rec_idx], sources)
                .await?;
        }
    } else {
        tracing::warn!("No threads available for PII document seed; skipping");
    }

    tracing::info!(
        "Seeded {} PII entities, {} discovered records, {} vault entries, {} share records, {} PII-bearing docs",
        entity_defs.len(),
        discovered.len(),
        vault.len(),
        shares.len(),
        pii_docs_count,
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

    #[cfg(feature = "encryption")]
    #[tokio::test]
    async fn seed_pii_populates_empty_db() {
        use sovereign_core::content::ContentFields;
        use sovereign_crypto::device_key::DeviceKey;
        use sovereign_crypto::master_key::MasterKey;
        use sovereign_crypto::vault::EncryptedBlob;
        use sovereign_db::schema::ReviewState;

        let db = test_db().await;
        // Threads are needed for the PII-bearing doc seed to land.
        seed_if_empty(&db).await.unwrap();
        let mk = MasterKey::generate();
        let dk = Arc::new(DeviceKey::derive(&mk, "seed-test").unwrap());

        seed_pii_if_empty(&db, &dk).await.unwrap();

        let entities = db.list_entities().await.unwrap();
        assert_eq!(entities.len(), 5, "Should create 5 PII entities");
        assert_eq!(
            entities.iter().filter(|e| e.is_owned).count(),
            1,
            "1 owned entity (You)"
        );

        let records = db.list_pii_records(None, None, None).await.unwrap();
        assert_eq!(records.len(), 12, "7 discovered + 5 vault entries");

        let vault = db.list_pii_records(None, None, Some(true)).await.unwrap();
        assert_eq!(vault.len(), 5);
        let discovered = db.list_pii_records(None, None, Some(false)).await.unwrap();
        assert_eq!(discovered.len(), 7);

        // Each review state has at least one record so the dashboard renders all tabs.
        for state in [ReviewState::Unreviewed, ReviewState::Confirmed, ReviewState::Dismissed] {
            let n = db
                .list_pii_records(None, Some(state.clone()), None)
                .await
                .unwrap()
                .len();
            assert!(n > 0, "Should have at least one {state:?} record");
        }

        // Reveal round-trip: encrypted values must decrypt with the same DeviceKey.
        let one = &records[0];
        let blob = EncryptedBlob {
            ciphertext_b64: one.value_encrypted.clone(),
            nonce_b64: one.value_nonce.clone(),
        };
        let plaintext = blob.decrypt(&dk).expect("decrypt with seed key");
        assert!(!plaintext.is_empty(), "Decrypted value should be non-empty");

        // ── PII-bearing documents ───────────────────────────────────────
        let docs = db.list_documents(None).await.unwrap();
        let pii_docs: Vec<_> = docs
            .iter()
            .filter(|d| d.body_raw_encrypted.is_some())
            .collect();
        assert_eq!(pii_docs.len(), 2, "Should create 2 PII-bearing docs");

        for doc in &pii_docs {
            // Canonical body must contain at least one [pii:...] token.
            let cf = ContentFields::parse(&doc.content);
            assert!(
                cf.body.contains("[pii:"),
                "Canonical body should carry [pii:<id>] tokens, got: {}",
                cf.body
            );
            // pii_scanned_at marks the doc as scanned.
            assert!(doc.pii_scanned_at.is_some());

            // body_raw_encrypted decrypts back to the inline (PII-visible) text.
            let raw_blob = EncryptedBlob {
                ciphertext_b64: doc.body_raw_encrypted.clone().unwrap(),
                nonce_b64: doc.body_raw_nonce.clone().unwrap(),
            };
            let raw = String::from_utf8(raw_blob.decrypt(&dk).unwrap()).unwrap();
            assert!(!raw.contains("[pii:"), "Raw body should NOT contain tokens");
            assert!(raw.len() > cf.body.len() / 2, "Raw body should be substantial");
        }

        // At least one referenced record gained non-empty sources.
        let with_sources = discovered.iter().filter(|r| !r.sources.is_empty()).count();
        assert!(with_sources >= 6, "≥6 records should have sources, got {with_sources}");
    }

    #[cfg(feature = "encryption")]
    #[tokio::test]
    async fn seed_pii_is_idempotent() {
        use sovereign_crypto::device_key::DeviceKey;
        use sovereign_crypto::master_key::MasterKey;

        let db = test_db().await;
        seed_if_empty(&db).await.unwrap();
        let mk = MasterKey::generate();
        let dk = Arc::new(DeviceKey::derive(&mk, "seed-test").unwrap());

        seed_pii_if_empty(&db, &dk).await.unwrap();
        seed_pii_if_empty(&db, &dk).await.unwrap(); // no-op

        let entities = db.list_entities().await.unwrap();
        assert_eq!(entities.len(), 5, "Still 5 entities after double seed");
    }
}
