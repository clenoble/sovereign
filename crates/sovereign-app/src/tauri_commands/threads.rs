use super::*;

// ---------------------------------------------------------------------------
// Thread CRUD (Phase 3)
// ---------------------------------------------------------------------------

/// Create a new thread.
#[tauri::command]
pub async fn create_thread(
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Result<ThreadDto, String> {
    let thread = Thread::new(name, description);
    let created = state.db.create_thread(thread).await.str_err()?;
    let id = created.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ThreadDto {
        id,
        name: created.name,
        description: created.description,
        created_at: created.created_at.to_rfc3339(),
    })
}

/// Update a thread's name and/or description.
#[tauri::command]
pub async fn update_thread(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<ThreadDto, String> {
    let updated = state
        .db
        .update_thread(&id, name.as_deref(), description.as_deref())
        .await
        .str_err()?;
    let tid = updated.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ThreadDto {
        id: tid,
        name: updated.name,
        description: updated.description,
        created_at: updated.created_at.to_rfc3339(),
    })
}

/// Soft-delete a thread.
#[tauri::command]
pub async fn delete_thread(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.db.soft_delete_thread(&id).await.str_err()
}

/// Move a document to a different thread.
#[tauri::command]
pub async fn move_document_to_thread(
    state: State<'_, AppState>,
    doc_id: String,
    thread_id: String,
) -> Result<(), String> {
    state
        .db
        .move_document_to_thread(&doc_id, &thread_id)
        .await
        .str_err()?;
    Ok(())
}

