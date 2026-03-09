use super::*;

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// List all documents, optionally filtered by thread.
#[tauri::command]
pub async fn list_documents(
    state: State<'_, AppState>,
    thread_id: Option<String>,
) -> Result<Vec<DocSummary>, String> {
    let docs = state
        .db
        .list_documents(thread_id.as_deref())
        .await
        .str_err()?;

    Ok(docs
        .into_iter()
        .map(|d| DocSummary {
            id: d
                .id
                .as_ref()
                .map(sovereign_db::schema::thing_to_raw)
                .unwrap_or_default(),
            title: d.title,
            thread_id: d.thread_id,
            is_owned: d.is_owned,
            modified_at: d.modified_at.to_rfc3339(),
        })
        .collect())
}

/// List all threads.
#[tauri::command]
pub async fn list_threads(state: State<'_, AppState>) -> Result<Vec<ThreadSummary>, String> {
    let threads = state.db.list_threads().await.str_err()?;

    Ok(threads
        .into_iter()
        .map(|t| ThreadSummary {
            id: t
                .id
                .as_ref()
                .map(sovereign_db::schema::thing_to_raw)
                .unwrap_or_default(),
            name: t.name,
            description: t.description,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Toggle the UI theme and return the new theme name.
#[tauri::command]
pub async fn toggle_theme(state: State<'_, AppState>) -> Result<String, String> {
    let current = state.theme.lock().str_err()?;
    let next = if *current == "dark" { "light" } else { "dark" };
    drop(current);
    let mut theme = state.theme.lock().str_err()?;
    *theme = next.to_string();
    Ok(next.to_string())
}

/// Get the current theme.
#[tauri::command]
pub async fn get_theme(state: State<'_, AppState>) -> Result<String, String> {
    let theme = state.theme.lock().str_err()?;
    Ok(theme.clone())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_full_document(doc: Document) -> FullDocument {
    let id = doc
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default();
    let fields = ContentFields::parse(&doc.content);
    FullDocument {
        id,
        title: doc.title,
        body: fields.body,
        images: fields
            .images
            .into_iter()
            .map(|i| ContentImageDto {
                path: i.path,
                caption: i.caption,
            })
            .collect(),
        videos: fields
            .videos
            .into_iter()
            .map(|v| ContentVideoDto {
                path: v.path,
                caption: v.caption,
                duration_secs: v.duration_secs,
                thumbnail_path: v.thumbnail_path,
            })
            .collect(),
        thread_id: doc.thread_id,
        is_owned: doc.is_owned,
        created_at: doc.created_at.to_rfc3339(),
        modified_at: doc.modified_at.to_rfc3339(),
    }
}

// ---------------------------------------------------------------------------
// Document CRUD
// ---------------------------------------------------------------------------

/// Get a full document by ID (with parsed body/images/videos).
#[tauri::command]
pub async fn get_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<FullDocument, String> {
    let doc = state.db.get_document(&id).await.str_err()?;
    Ok(to_full_document(doc))
}

/// Save document content (title + body + images + videos).
#[tauri::command]
pub async fn save_document(
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
    images: Vec<ContentImageDto>,
    videos: Vec<ContentVideoDto>,
) -> Result<(), String> {
    let fields = ContentFields {
        body,
        images: images
            .into_iter()
            .map(|i| sovereign_core::content::ContentImage {
                path: i.path,
                caption: i.caption,
            })
            .collect(),
        videos: videos
            .into_iter()
            .map(|v| sovereign_core::content::ContentVideo {
                path: v.path,
                caption: v.caption,
                duration_secs: v.duration_secs,
                thumbnail_path: v.thumbnail_path,
            })
            .collect(),
    };
    let content_json = fields.serialize();
    state
        .db
        .update_document(&id, Some(&title), Some(&content_json))
        .await
        .str_err()?;
    state.autocommit.lock().await.record_edit(&id);
    Ok(())
}

/// Create a new document and return its ID.
#[tauri::command]
pub async fn create_document(
    state: State<'_, AppState>,
    title: String,
    thread_id: String,
) -> Result<String, String> {
    let doc = Document::new(title, thread_id, true);
    let created = state
        .db
        .create_document(doc)
        .await
        .str_err()?;
    Ok(created
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default())
}

/// Close a document (flush auto-commit).
#[tauri::command]
pub async fn close_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.autocommit.lock().await.commit_on_close(&id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Version history
// ---------------------------------------------------------------------------

/// List commits for a document.
#[tauri::command]
pub async fn list_commits(
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<Vec<CommitSummaryDto>, String> {
    let commits = state
        .db
        .list_document_commits(&doc_id)
        .await
        .str_err()?;

    Ok(commits
        .into_iter()
        .map(|c| {
            let preview = if c.snapshot.content.len() > 200 {
                format!("{}...", &c.snapshot.content[..200])
            } else {
                c.snapshot.content.clone()
            };
            CommitSummaryDto {
                id: c
                    .id
                    .as_ref()
                    .map(sovereign_db::schema::thing_to_raw)
                    .unwrap_or_default(),
                message: c.message,
                timestamp: c.timestamp.to_rfc3339(),
                snapshot_title: c.snapshot.title,
                snapshot_preview: preview,
            }
        })
        .collect())
}

/// Restore a document to a specific commit.
#[tauri::command]
pub async fn restore_commit(
    state: State<'_, AppState>,
    doc_id: String,
    commit_id: String,
) -> Result<FullDocument, String> {
    let doc = state
        .db
        .restore_document(&doc_id, &commit_id)
        .await
        .str_err()?;
    Ok(to_full_document(doc))
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

/// List skills applicable to a document (based on file extension in title).
#[tauri::command]
pub async fn list_skills_for_doc(
    state: State<'_, AppState>,
    doc_title: String,
) -> Result<Vec<SkillInfo>, String> {
    let ext = doc_title
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let skills = state.skill_registry.skills_for_file_type(&ext);
    Ok(skills
        .into_iter()
        .map(|(name, actions)| SkillInfo {
            skill_name: name.to_string(),
            actions: actions
                .into_iter()
                .map(|(id, label)| SkillActionInfo {
                    action_id: id,
                    label,
                })
                .collect(),
        })
        .collect())
}

/// Execute a skill action on a document.
#[tauri::command]
pub async fn execute_skill(
    state: State<'_, AppState>,
    skill_name: String,
    action: String,
    doc_id: String,
    params: String,
) -> Result<SkillResultDto, String> {
    let doc = state.db.get_document(&doc_id).await.str_err()?;
    let fields = ContentFields::parse(&doc.content);
    let skill_doc = SkillDocument {
        id: doc_id,
        title: doc.title,
        content: fields,
    };
    let ctx = SkillContext {
        granted: HashSet::new(),
        db: Some(state.skill_db.clone()),
    };
    let output = state
        .skill_registry
        .execute_skill(&skill_name, &action, &skill_doc, &params, &ctx)
        .str_err()?;

    match output {
        sovereign_skills::traits::SkillOutput::ContentUpdate(cf) => Ok(SkillResultDto {
            kind: "content_update".into(),
            body: Some(cf.body),
            images: Some(
                cf.images
                    .into_iter()
                    .map(|i| ContentImageDto {
                        path: i.path,
                        caption: i.caption,
                    })
                    .collect(),
            ),
            videos: Some(
                cf.videos
                    .into_iter()
                    .map(|v| ContentVideoDto {
                        path: v.path,
                        caption: v.caption,
                        duration_secs: v.duration_secs,
                        thumbnail_path: v.thumbnail_path,
                    })
                    .collect(),
            ),
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: None,
            structured_json: None,
        }),
        sovereign_skills::traits::SkillOutput::File { name, mime_type, data } => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            Ok(SkillResultDto {
                kind: "file".into(),
                body: None,
                images: None,
                videos: None,
                file_name: Some(name),
                file_mime: Some(mime_type),
                file_data_base64: Some(b64),
                structured_kind: None,
                structured_json: None,
            })
        }
        sovereign_skills::traits::SkillOutput::StructuredData { kind, json } => Ok(SkillResultDto {
            kind: "structured_data".into(),
            body: None,
            images: None,
            videos: None,
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: Some(kind),
            structured_json: Some(json),
        }),
        sovereign_skills::traits::SkillOutput::None => Ok(SkillResultDto {
            kind: "none".into(),
            body: None,
            images: None,
            videos: None,
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: None,
            structured_json: None,
        }),
    }
}

/// List all registered skills and their actions.
#[tauri::command]
pub async fn list_all_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let skills = state.skill_registry.all_skills();
    Ok(skills
        .iter()
        .map(|s| SkillInfo {
            skill_name: s.name().to_string(),
            actions: s
                .actions()
                .into_iter()
                .map(|(id, label)| SkillActionInfo {
                    action_id: id,
                    label,
                })
                .collect(),
        })
        .collect())
}


#[tauri::command]
pub async fn delete_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .soft_delete_document(&id)
        .await
        .str_err()
}


// ---------------------------------------------------------------------------
// Phase 5: File import
// ---------------------------------------------------------------------------

/// Import a file from the local filesystem as a new document.
#[tauri::command]
pub async fn import_file(
    state: State<'_, AppState>,
    file_path: String,
    thread_id: Option<String>,
) -> Result<CanvasDocDto, String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {file_path}"));
    }

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported File")
        .to_string();

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let tid = thread_id.unwrap_or_else(|| "thread:default".to_string());
    let doc = Document::new(title, tid.clone(), true);
    let created = state.db.create_document(doc).await.str_err()?;
    let id = created
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default();

    // Save the content
    state
        .db
        .update_document(&id, None, Some(&content))
        .await
        .str_err()?;

    Ok(CanvasDocDto {
        id,
        title: created.title,
        thread_id: tid,
        is_owned: true,
        spatial_x: created.spatial_x,
        spatial_y: created.spatial_y,
        created_at: created.created_at.to_rfc3339(),
        modified_at: created.modified_at.to_rfc3339(),
        reliability_classification: None,
        reliability_score: None,
        source_url: None,
    })
}

