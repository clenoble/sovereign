use super::*;

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// List all documents, optionally filtered by thread.
#[tauri::command]
pub async fn list_documents(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    thread_id: Option<String>,
) -> Result<Vec<DocSummary>, String> {
    state.require_unlocked(&webview).await?;
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
pub async fn list_threads(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<ThreadSummary>, String> {
    state.require_unlocked(&webview).await?;
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

/// Toggle the UI theme and persist to the user profile.
#[tauri::command]
pub async fn toggle_theme(state: State<'_, AppState>) -> Result<String, String> {
    let next = {
        let current = state.theme.lock().str_err()?;
        if *current == "dark" { "light" } else { "dark" }
    };
    {
        let mut theme = state.theme.lock().str_err()?;
        *theme = next.to_string();
    }
    // Persist to profile so the choice survives a restart.
    if let Ok(mut profile) = sovereign_core::profile::UserProfile::load(&state.profile_dir) {
        profile.theme = next.to_string();
        if let Err(e) = profile.save(&state.profile_dir) {
            tracing::warn!("Failed to persist theme to profile: {e}");
        }
    }
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<FullDocument, String> {
    state.require_unlocked(&webview).await?;
    let doc = state.db.get_document(&id).await.str_err()?;
    Ok(to_full_document(doc))
}

/// Save document content (title + body + images + videos).
#[tauri::command]
pub async fn save_document(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
    images: Vec<ContentImageDto>,
    videos: Vec<ContentVideoDto>,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    // PII-002: PII ingest on the body before persisting — runs regardless of
    // the `encryption` feature so PII is tokenized in non-encryption builds
    // too. The helper short-circuits gracefully when no account_key is
    // available or the document was already scanned — see
    // `pii_ingest::maybe_ingest_document_body` for the policy.
    let body = crate::pii_ingest::maybe_ingest_document_body(&state, &id, &body).await?;

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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    title: String,
    thread_id: String,
) -> Result<String, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state.autocommit.lock().await.commit_on_close(&id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Version history
// ---------------------------------------------------------------------------

/// List commits for a document.
#[tauri::command]
pub async fn list_commits(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<Vec<CommitSummaryDto>, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
    commit_id: String,
) -> Result<FullDocument, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_title: String,
) -> Result<Vec<SkillInfo>, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    skill_name: String,
    action: String,
    doc_id: String,
    params: String,
) -> Result<SkillResultDto, String> {
    state.require_unlocked(&webview).await?;
    let doc = state.db.get_document(&doc_id).await.str_err()?;
    let fields = ContentFields::parse(&doc.content);
    let skill_doc = SkillDocument {
        id: doc_id,
        title: doc.title,
        content: fields,
    };
    // Auto-grant exactly the capabilities the skill declares.
    let granted: HashSet<_> = state
        .skill_registry
        .find_skill(&skill_name)
        .map(|s| s.required_capabilities().into_iter().collect())
        .unwrap_or_default();
    let ctx = SkillContext {
        granted,
        db: Some(state.skill_db.clone()),
        llm: state.skill_llm.clone(),
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
pub async fn list_all_skills(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<SkillInfo>, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    file_path: String,
    thread_id: Option<String>,
) -> Result<CanvasDocDto, String> {
    state.require_unlocked(&webview).await?;

    // IPC-001: contain the import path. Canonicalize the requested path
    // (resolving symlinks + `..`) and reject anything that escapes the
    // user's home directory. `std::fs::canonicalize` errors if the path
    // doesn't exist, which also covers the previous existence check.
    let canonical = std::fs::canonicalize(&file_path)
        .map_err(|e| format!("File not found or inaccessible: {file_path}: {e}"))?;
    // Default-deny: only allow imports from the user's standard document
    // folders. Confining to $HOME is NOT enough — auth.store, salt
    // (~/.sovereign/crypto), ~/.ssh keys, and other dotfile secrets all
    // live under $HOME; Documents/Downloads/Desktop excludes every dotdir.
    let home = sovereign_core::home_dir();
    let allowed_roots: Vec<std::path::PathBuf> = ["Documents", "Downloads", "Desktop"]
        .iter()
        .filter_map(|d| std::fs::canonicalize(home.join(d)).ok())
        .collect();
    if !allowed_roots.iter().any(|root| canonical.starts_with(root)) {
        return Err(format!(
            "Import rejected: '{file_path}' is outside the allowed import folders (Documents, Downloads, Desktop)"
        ));
    }
    let path = canonical.as_path();

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

    // PII-002: PII ingest on the imported content — runs regardless of the
    // `encryption` feature. The body is rewritten with `[pii:<record_id>]`
    // tokens (and the original preserved encrypted) whenever an account_key is
    // available; otherwise it's a graceful pass-through.
    let content = crate::pii_ingest::maybe_ingest_document_body(&state, &id, &content).await?;

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

