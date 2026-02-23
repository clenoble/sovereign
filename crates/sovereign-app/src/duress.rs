use anyhow::Result;
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Document, Message, MessageDirection,
    Thread,
};
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

/// Seed the duress persona database with plausible but innocuous data.
/// Called when the duress password is used and the duress DB is empty.
#[allow(dead_code)]
pub async fn seed_duress_db(db: &SurrealGraphDB) -> Result<()> {
    let threads = db.list_threads().await?;
    if !threads.is_empty() {
        return Ok(());
    }

    tracing::info!("Seeding duress persona database");

    // ── Threads ──────────────────────────────────────────────────

    let thread_defs = [
        ("Personal", "Personal notes and lists"),
        ("Work", "Work-related documents"),
        ("Recipes", "Cooking and meal planning"),
    ];

    let mut thread_ids = Vec::new();
    for (name, desc) in &thread_defs {
        let t = Thread::new(name.to_string(), desc.to_string());
        let created = db.create_thread(t).await?;
        thread_ids.push(
            created
                .id_string()
                .ok_or_else(|| anyhow::anyhow!("Thread missing ID"))?,
        );
    }

    // ── Documents ────────────────────────────────────────────────

    let docs = [
        (
            "Grocery List",
            "- Milk\n- Eggs\n- Bread\n- Butter\n- Tomatoes\n- Pasta\n- Olive oil\n- Cheese",
            0usize, // Personal
        ),
        (
            "Weekend Plans",
            "Saturday:\n- Morning run at 8am\n- Farmers market\n- Lunch with Sarah\n\nSunday:\n- Laundry\n- Read that book chapter\n- Meal prep for the week",
            0,
        ),
        (
            "Meeting Notes - Q1 Review",
            "Attendees: Team leads\n\n## Key Points\n- Revenue up 12% QoQ\n- New client onboarding on track\n- Need to hire 2 more engineers\n\n## Action Items\n- Draft hiring plan by Friday\n- Schedule follow-up with marketing",
            1, // Work
        ),
        (
            "Project Timeline",
            "Phase 1: Research (Jan-Feb)\nPhase 2: Design (Mar)\nPhase 3: Implementation (Apr-Jun)\nPhase 4: Testing (Jul)\nPhase 5: Launch (Aug)\n\nStatus: On track",
            1,
        ),
        (
            "Travel Expense Report",
            "Conference trip - Chicago\n\n| Item | Amount |\n|------|--------|\n| Flight | $342 |\n| Hotel (3 nights) | $567 |\n| Meals | $128 |\n| Taxi | $45 |\n| **Total** | **$1,082** |",
            1,
        ),
        (
            "Pasta Carbonara",
            "## Ingredients\n- 400g spaghetti\n- 200g guanciale\n- 4 egg yolks\n- 100g pecorino romano\n- Black pepper\n\n## Method\n1. Cook pasta in salted water\n2. Cut guanciale into strips, render in pan\n3. Mix yolks with grated pecorino\n4. Toss hot pasta with guanciale, remove from heat\n5. Add egg mixture, toss quickly\n6. Season with pepper, serve immediately",
            2, // Recipes
        ),
        (
            "Banana Bread",
            "## Ingredients\n- 3 ripe bananas\n- 1/3 cup melted butter\n- 3/4 cup sugar\n- 1 egg\n- 1 tsp vanilla\n- 1 tsp baking soda\n- 1.5 cups flour\n\n## Method\n1. Preheat oven to 350F\n2. Mash bananas, mix with butter\n3. Add sugar, egg, vanilla\n4. Fold in baking soda and flour\n5. Pour into greased loaf pan\n6. Bake 60-65 minutes",
            2,
        ),
        (
            "Books to Read",
            "- Atomic Habits by James Clear\n- The Design of Everyday Things by Don Norman\n- Thinking, Fast and Slow by Daniel Kahneman\n- Project Hail Mary by Andy Weir\n- The Pragmatic Programmer",
            0,
        ),
    ];

    for (title, body, thread_idx) in &docs {
        let mut doc = Document::new(title.to_string(), thread_ids[*thread_idx].clone(), true);
        doc.content = body.to_string();
        db.create_document(doc).await?;
    }

    // ── Contacts ─────────────────────────────────────────────────

    let mut contact1 = Contact::new("Alex Johnson".into(), false);
    contact1.addresses.push(ChannelAddress {
        channel: ChannelType::Email,
        address: "alex.j@example.com".into(),
        display_name: None,
        is_primary: true,
    });

    let mut contact2 = Contact::new("Sam Rivera".into(), false);
    contact2.addresses.push(ChannelAddress {
        channel: ChannelType::Email,
        address: "sam.rivera@example.com".into(),
        display_name: None,
        is_primary: true,
    });

    let c1 = db.create_contact(contact1).await?;
    let c2 = db.create_contact(contact2).await?;

    // ── Conversation ─────────────────────────────────────────────

    if let (Some(c1_id), Some(_c2_id)) = (c1.id_string(), c2.id_string()) {
        let conv = Conversation::new(
            "Lunch plans".into(),
            ChannelType::Email,
            vec![c1_id.clone()],
        );
        let created_conv = db.create_conversation(conv).await?;
        if let Some(conv_id) = created_conv.id_string() {
            let msgs = [
                ("Hey, are we still on for lunch Friday?", MessageDirection::Outbound),
                ("Yes! How about that Thai place on 5th?", MessageDirection::Inbound),
                ("Sounds great. 12:30 work for you?", MessageDirection::Outbound),
                ("Perfect, see you there!", MessageDirection::Inbound),
            ];
            for (body, direction) in &msgs {
                let msg = Message::new(
                    conv_id.clone(),
                    ChannelType::Email,
                    direction.clone(),
                    c1_id.clone(),
                    vec![],
                    body.to_string(),
                );
                db.create_message(msg).await?;
            }
        }
    }

    tracing::info!("Duress persona database seeded");
    Ok(())
}
