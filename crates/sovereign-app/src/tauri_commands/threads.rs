use super::*;

// ---------------------------------------------------------------------------
// Thread CRUD (Phase 3)
// ---------------------------------------------------------------------------

/// Create a new thread.
#[tauri::command]
pub async fn create_thread(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Result<ThreadDto, String> {
    state.require_unlocked(&webview).await?;
    // Through the one thread-write path, stamped (THREAD-sync S1).
    let docs = state.docs().await?;
    let id = docs
        .thread_create(&name, &description)
        .await
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let created = state.db.get_thread(&id).await.str_err()?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<ThreadDto, String> {
    state.require_unlocked(&webview).await?;
    // Through the one thread-write path, stamped (THREAD-sync S1).
    let docs = state.docs().await?;
    docs.thread_update(&id, name.as_deref(), description.as_deref())
        .await
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let updated = state.db.get_thread(&id).await.str_err()?;
    let tid = updated.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ThreadDto {
        id: tid,
        name: updated.name,
        description: updated.description,
        created_at: updated.created_at.to_rfc3339(),
    })
}

/// Delete a thread (C-DOC adoption step 5, S5-10; Céline, 2026-10-17):
/// every document still in it goes to the trash, then the thread is deleted
/// for good — all or nothing. The UI asks first, naming the thread and its
/// number of documents.
#[tauri::command]
pub async fn delete_thread(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    crate::doc_trash::delete_thread_to_trash(&docs, state.db.as_ref(), &id).await.map(|_| ())
}

/// Move a document to a different thread.
#[tauri::command]
pub async fn move_document_to_thread(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
    thread_id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state
        .db
        .move_document_to_thread(&doc_id, &thread_id)
        .await
        .str_err()?;
    Ok(())
}

