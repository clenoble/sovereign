use super::*;

// ---------------------------------------------------------------------------
// Canvas (Phase 3)
// ---------------------------------------------------------------------------

/// Bulk-load all data needed for the spatial canvas.
#[tauri::command]
pub async fn canvas_load(state: State<'_, AppState>) -> Result<CanvasData, String> {
    tracing::info!("canvas_load: called from frontend");
    let docs = state.db.list_documents(None).await.str_err()?;
    tracing::info!("canvas_load: got {} documents from DB", docs.len());
    let threads = state.db.list_threads().await.str_err()?;
    let rels = state.db.list_all_relationships().await.str_err()?;
    let contacts = state.db.list_contacts().await.str_err()?;

    // Compute unread counts per contact from conversations
    let agg = aggregate_conversations(state.db.as_ref()).await?;
    let unread_by_contact = agg.unread_by_contact;
    let channels_by_contact = agg.channels_by_contact;

    // Batch-load all milestones (single query instead of N per-thread queries)
    let all_milestones = state.db.list_all_milestones().await.str_err()?;

    // Messages are loaded separately via canvas_load_messages (viewport-scoped)

    let result = Ok(CanvasData {
        documents: docs
            .into_iter()
            .map(|d| {
                let id = d.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                CanvasDocDto {
                    id,
                    title: d.title,
                    thread_id: d.thread_id,
                    is_owned: d.is_owned,
                    spatial_x: d.spatial_x,
                    spatial_y: d.spatial_y,
                    created_at: d.created_at.to_rfc3339(),
                    modified_at: d.modified_at.to_rfc3339(),
                    reliability_classification: d.reliability_classification,
                    reliability_score: d.reliability_score,
                    source_url: d.source_url,
                }
            })
            .collect(),
        threads: threads
            .into_iter()
            .map(|t| {
                let id = t.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                ThreadDto {
                    id,
                    name: t.name,
                    description: t.description,
                    created_at: t.created_at.to_rfc3339(),
                }
            })
            .collect(),
        relationships: rels
            .into_iter()
            .map(|r| {
                let id = r.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let from = r.out.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let to = r.in_.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                RelationshipDto {
                    id,
                    from_doc_id: from,
                    to_doc_id: to,
                    relation_type: format!("{:?}", r.relation_type),
                    strength: r.strength,
                }
            })
            .collect(),
        contacts: contacts
            .into_iter()
            .filter(|c| !c.is_owned)
            .map(|c| {
                let id = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let unread = unread_by_contact.get(&id).copied().unwrap_or(0);
                let channels: Vec<String> = channels_by_contact
                    .get(&id)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default();
                ContactSummaryDto {
                    id,
                    name: c.name,
                    avatar: c.avatar,
                    unread_count: unread,
                    channels,
                }
            })
            .collect(),
        milestones: all_milestones
            .into_iter()
            .map(|m| {
                let id = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                MilestoneDto {
                    id,
                    title: m.title,
                    timestamp: m.timestamp.to_rfc3339(),
                    thread_id: m.thread_id,
                    description: m.description,
                }
            })
            .collect(),
        messages: vec![],
    });
    tracing::info!("canvas_load: returning {} docs, {} threads, {} rels, {} contacts, {} milestones, {} messages",
        result.as_ref().map(|r| r.documents.len()).unwrap_or(0),
        result.as_ref().map(|r| r.threads.len()).unwrap_or(0),
        result.as_ref().map(|r| r.relationships.len()).unwrap_or(0),
        result.as_ref().map(|r| r.contacts.len()).unwrap_or(0),
        result.as_ref().map(|r| r.milestones.len()).unwrap_or(0),
        result.as_ref().map(|r| r.messages.len()).unwrap_or(0),
    );
    result
}

/// Update a document's spatial canvas position.
#[tauri::command]
pub async fn update_document_position(
    state: State<'_, AppState>,
    id: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    state
        .db
        .update_document_position(&id, x, y)
        .await
        .str_err()
}

/// Load messages for a specific time range (viewport-scoped).
#[tauri::command]
pub async fn canvas_load_messages(
    state: State<'_, AppState>,
    t_min: String,
    t_max: String,
    limit: Option<u32>,
) -> Result<Vec<CanvasMessageDto>, String> {
    let after = chrono::DateTime::parse_from_rfc3339(&t_min)
        .map_err(|e| format!("Invalid t_min: {e}"))?
        .with_timezone(&Utc);
    let before = chrono::DateTime::parse_from_rfc3339(&t_max)
        .map_err(|e| format!("Invalid t_max: {e}"))?
        .with_timezone(&Utc);

    let msgs = state.db
        .list_messages_in_time_range(after, before, limit.unwrap_or(200))
        .await
        .str_err()?;

    // Build conversation lookup for thread_id and contact resolution
    let conversations = state.db.list_conversations(None).await.str_err()?;
    let contacts = state.db.list_contacts().await.str_err()?;

    let owned_contact_ids: HashSet<String> = contacts
        .iter()
        .filter(|c| c.is_owned)
        .filter_map(|c| c.id.as_ref().map(sovereign_db::schema::thing_to_raw))
        .collect();

    // Map conversation_id → (thread_id, contact_id)
    let conv_map: std::collections::HashMap<String, (String, String)> = conversations
        .iter()
        .filter_map(|conv| {
            let conv_id = conv.id.as_ref().map(sovereign_db::schema::thing_to_raw)?;
            let thread_id = conv.linked_thread_id.as_ref()?;
            let contact_id = conv.participant_contact_ids
                .iter()
                .find(|pid| !owned_contact_ids.contains(*pid))
                .or_else(|| conv.participant_contact_ids.first())
                .cloned()
                .unwrap_or_default();
            Some((conv_id, (thread_id.clone(), contact_id)))
        })
        .collect();

    let result: Vec<CanvasMessageDto> = msgs
        .into_iter()
        .filter_map(|m| {
            let (thread_id, contact_id) = conv_map.get(&m.conversation_id)?;
            let mid = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            let subject = m.subject.unwrap_or_else(|| {
                let body = m.body.chars().take(30).collect::<String>();
                if m.body.len() > 30 { format!("{}...", body) } else { body }
            });
            Some(CanvasMessageDto {
                id: mid,
                conversation_id: m.conversation_id,
                thread_id: thread_id.clone(),
                contact_id: contact_id.clone(),
                subject,
                is_outbound: matches!(m.direction, MessageDirection::Outbound),
                sent_at: m.sent_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(result)
}

