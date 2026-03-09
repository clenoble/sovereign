use super::*;

// ---------------------------------------------------------------------------
// Contacts & Messaging (Phase 3)
// ---------------------------------------------------------------------------

/// List all non-owned contacts with unread counts.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<ContactSummaryDto>, String> {
    let contacts = state.db.list_contacts().await.str_err()?;
    let agg = aggregate_conversations(state.db.as_ref()).await?;
    let unread_by_contact = agg.unread_by_contact;
    let channels_by_contact = agg.channels_by_contact;

    Ok(contacts
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
        .collect())
}

/// Get full contact detail including conversations.
#[tauri::command]
pub async fn get_contact_detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<ContactDetailDto, String> {
    let contact = state.db.get_contact(&id).await.str_err()?;
    let all_convs = state.db.list_conversations(None).await.str_err()?;

    let contact_convs: Vec<ConversationDto> = all_convs
        .into_iter()
        .filter(|c| c.participant_contact_ids.contains(&id))
        .map(|c| {
            let cid = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            ConversationDto {
                id: cid,
                title: c.title,
                channel: c.channel.to_string(),
                participant_ids: c.participant_contact_ids,
                unread_count: c.unread_count,
                last_message_at: c.last_message_at.map(|t| t.to_rfc3339()),
            }
        })
        .collect();

    let cid = contact.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ContactDetailDto {
        id: cid,
        name: contact.name,
        avatar: contact.avatar,
        notes: contact.notes,
        addresses: contact
            .addresses
            .into_iter()
            .map(|a| ChannelAddressDto {
                channel: a.channel.to_string(),
                address: a.address,
                display_name: a.display_name,
                is_primary: a.is_primary,
            })
            .collect(),
        conversations: contact_convs,
    })
}

/// List conversations, optionally filtered by contact participant.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
    contact_id: Option<String>,
) -> Result<Vec<ConversationDto>, String> {
    let convs = state.db.list_conversations(None).await.str_err()?;

    Ok(convs
        .into_iter()
        .filter(|c| {
            contact_id
                .as_ref()
                .map(|cid| c.participant_contact_ids.contains(cid))
                .unwrap_or(true)
        })
        .map(|c| {
            let id = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            ConversationDto {
                id,
                title: c.title,
                channel: c.channel.to_string(),
                participant_ids: c.participant_contact_ids,
                unread_count: c.unread_count,
                last_message_at: c.last_message_at.map(|t| t.to_rfc3339()),
            }
        })
        .collect())
}

/// List messages in a conversation with cursor-based pagination.
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
    before: Option<String>,
    limit: u32,
) -> Result<Vec<MessageDto>, String> {
    let before_dt = before
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let msgs = state
        .db
        .list_messages(&conversation_id, before_dt, limit)
        .await
        .str_err()?;

    Ok(msgs
        .into_iter()
        .map(|m| {
            let id = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            MessageDto {
                id,
                conversation_id: m.conversation_id,
                direction: format!("{:?}", m.direction),
                from_contact_id: m.from_contact_id,
                subject: m.subject,
                body: m.body,
                sent_at: m.sent_at.to_rfc3339(),
                read_status: format!("{:?}", m.read_status),
            }
        })
        .collect())
}

/// Mark a message as read.
#[tauri::command]
pub async fn mark_message_read(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .update_message_read_status(&id, ReadStatus::Read)
        .await
        .str_err()?;
    Ok(())
}

/// Create a relationship between two documents.
#[tauri::command]
pub async fn create_relationship(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    relation_type: String,
    strength: f32,
) -> Result<(), String> {
    let rel_type = match relation_type.to_lowercase().as_str() {
        "references" => RelationType::References,
        "derivedfrom" => RelationType::DerivedFrom,
        "continues" => RelationType::Continues,
        "contradicts" => RelationType::Contradicts,
        "supports" => RelationType::Supports,
        "branchesfrom" => RelationType::BranchesFrom,
        "contactof" => RelationType::ContactOf,
        "attachedto" => RelationType::AttachedTo,
        _ => return Err(format!("Unknown relation type: {relation_type}")),
    };
    state
        .db
        .create_relationship(&from_id, &to_id, rel_type, strength)
        .await
        .str_err()?;
    Ok(())
}

