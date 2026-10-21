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
    // C-DOC adoption step 2: read through the document service, which also
    // gives the revision a save must be based on.
    let docs = state.docs().await?;
    crate::doc_saves::read_document(&docs, &id).await
}

/// A preview of stored content for a list (version history, search hits):
/// its text read through the v2 model (C-DOC adoption step 1 — never the
/// stored JSON), tokens kept as `[pii:<id>]`, cut at `max_chars` characters
/// (char-safe: a byte slice panics inside a multibyte character) with "..."
/// when cut.
pub(crate) fn text_preview(stored: &str, max_chars: usize) -> String {
    let text = sovereign_docformat::plain_text_of(stored);
    if text.chars().count() > max_chars {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    } else {
        text
    }
}

/// Save the document panel's state (desktop Tauri and the mobile reader):
/// through the document service (C-DOC adoption step 2) — written as v2, a
/// version of the prior state when another writer made it, personal data
/// ingested on every save, versions by amount of change (or `explicit`, Ctrl+S).
/// Refused, with nothing written, when the document changed since
/// `base_revision` was read (the error starts with
/// [`crate::doc_saves::CONFLICT_PREFIX`]), when images or videos changed, or
/// when a changed body would have to replace several text blocks. Returns the
/// document as stored (tokens in place of personal data, the new revision).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_document(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
    images: Vec<ContentImageDto>,
    videos: Vec<ContentVideoDto>,
    base_revision: String,
    explicit: Option<bool>,
) -> Result<FullDocument, String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    let save = crate::doc_saves::PanelSave {
        title,
        body,
        images,
        videos,
        base_revision,
        explicit: explicit.unwrap_or(false),
    };
    crate::doc_saves::save_from_panel(&docs, &state.edit_sessions, &id, save).await
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
    // C-DOC adoption step 3: through the document service — v2, a version
    // from creation, the user's own (owned, provenance written).
    let docs = state.docs().await?;
    let new = sovereign_ai::docs_host::new_user_document(title, thread_id, "");
    let created = sovereign_ai::docs_host::create(&docs, new)
        .await
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    Ok(created.id)
}

/// Close a document: its editing session ends, so its last saved state
/// becomes a version (C-DOC adoption step 2; the auto-commit engine is retired).
#[tauri::command]
pub async fn close_document(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    crate::doc_saves::close_panel(&docs, &state.edit_sessions, &id).await
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
            let preview = text_preview(&c.snapshot.content, 200);
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
    // C-DOC adoption step 2: the current state becomes a version first.
    let docs = state.docs().await?;
    crate::doc_saves::restore(&docs, &doc_id, &commit_id).await
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
    // C-DOC adoption step 2: the skill reads the document through the
    // service, and a content update is written back here, through the service
    // (`apply_skill_output`: the prior state versioned first, the result after)
    // — the frontend reloads the document instead of saving the output itself.
    let docs = state.docs().await?;
    let view = docs.read(&doc_id).await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let fields = sovereign_ai::docs_host::view_fields(&view);
    let skill_doc = SkillDocument {
        id: doc_id.clone(),
        title: view.title.clone(),
        content: fields.clone(),
    };
    // Auto-grant exactly the capabilities the skill declares.
    let granted: HashSet<_> = state
        .skill_registry
        .find_skill(&skill_name)
        .map(|s| s.required_capabilities().into_iter().collect())
        .unwrap_or_default();
    // C-DOC adoption step 3: what the skill creates (daily journal, file
    // import, duplicate, a WASM plugin's documents) goes through the document
    // service; its reads go to the store access skills always had.
    let skill_db: std::sync::Arc<dyn sovereign_skills::SkillDbAccess> = std::sync::Arc::new(
        sovereign_ai::docs_host::ServiceSkillDb::new(state.skill_db.clone(), docs.clone(), &skill_name),
    );
    let ctx = SkillContext {
        granted,
        db: Some(skill_db),
        llm: state.skill_llm.clone(),
    };
    let output = state
        .skill_registry
        .execute_skill(&skill_name, &action, &skill_doc, &params, &ctx)
        .str_err()?;

    match output {
        sovereign_skills::traits::SkillOutput::ContentUpdate(cf) => {
            crate::doc_saves::apply_skill(&docs, &doc_id, view.revision.clone(), &skill_name, &fields, &cf).await?;
            // What is stored now (personal data tokenized by the ingest).
            let stored = crate::doc_saves::read_document(&docs, &doc_id).await?;
            Ok(SkillResultDto {
                kind: "content_update".into(),
                body: Some(stored.body),
                images: Some(stored.images),
                videos: Some(stored.videos),
                file_name: None,
                file_mime: None,
                file_data_base64: None,
                structured_kind: None,
                structured_json: None,
            })
        }
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


/// Moves a document to the trash through the document service (C-DOC
/// adoption step 5, S5-1): recoverable for 30 days, then purged by the host's
/// purge tick. Nothing is hard-deleted here.
#[tauri::command]
pub async fn delete_document(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    docs.trash(&id).await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))
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
    // C-DOC adoption step 3: through the document service — a version from
    // creation, personal-data ingest by the service's hook (the one-shot
    // ingest this command ran before is gone), and the import's trust: a file
    // is not the user's own words because she picked it (spec ch. 04,
    // *Ownership is not decided at landing*) — **undetermined**, fenced until
    // she adopts it; provenance import with the file's path.
    let docs = state.docs().await?;
    let new = sovereign_ai::docs_host::new_document(
        title,
        tid.clone(),
        content,
        sovereign_docs::Trust::Undetermined,
        sovereign_docs::Provenance::Import { source_path: path.display().to_string() },
        sovereign_docs::Origin::Import,
    );
    let id = sovereign_ai::docs_host::create(&docs, new)
        .await
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?
        .id;
    let created = state.db.get_document(&id).await.str_err()?;

    Ok(CanvasDocDto {
        id,
        title: created.title,
        thread_id: tid,
        is_owned: created.is_owned,
        spatial_x: created.spatial_x,
        spatial_y: created.spatial_y,
        created_at: created.created_at.to_rfc3339(),
        modified_at: created.modified_at.to_rfc3339(),
        reliability_classification: None,
        reliability_score: None,
        source_url: None,
    })
}

#[cfg(test)]
mod adoption_step1_tests {
    //! C-DOC adoption step 1: the document readers of the Tauri commands read
    //! a v2 document, a v1 envelope and raw text through the v2 model (since
    //! step 2, through the document service). The writes: `crate::doc_saves`.
    use super::*;
    use sovereign_db::schema::Document;

    const V2: &str = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000000e01","text":"Héllo [pii:pii_record:e1]"},{"type":"image","id":"00000000000000000000000000000e02","ref":{"path":"/i.png"},"alt":"a","caption":"cap"},{"type":"text","id":"00000000000000000000000000000e03","text":"world"}]}"#;
    const V1: &str = r#"{"body":"Héllo [pii:pii_record:e1]","images":[{"path":"/i.png","caption":"cap"}],"videos":[{"path":"/v.mp4","caption":"","duration_secs":2.0,"thumbnail_path":"/t.png"}]}"#;
    const RAW: &str = "Héllo [pii:pii_record:e1]";

    /// The DTO of a document stored in `content`, read through the service.
    async fn dto(content: &str) -> FullDocument {
        use sovereign_ai::docs_host::{build_document_service, testing::encrypted_memory_store};
        let store = encrypted_memory_store("dto").await;
        let mut d = Document::new("T".into(), "thread:1".into(), true);
        d.content = content.into();
        let id = store.create_document(d).await.unwrap().id_string().unwrap();
        let docs = build_document_service(store, None, "test".into());
        crate::doc_saves::read_document(&docs, &id).await.unwrap()
    }

    #[tokio::test]
    async fn full_document_reads_v2_v1_and_raw() {
        let f = dto(V2).await;
        assert_eq!(f.body, "Héllo [pii:pii_record:e1]\n\nworld");
        assert_eq!(f.images.len(), 1);
        assert_eq!(f.images[0].path, "/i.png");
        assert_eq!(f.images[0].caption, "cap");
        assert!(!f.revision.is_empty(), "a save needs the revision");

        let f = dto(V1).await;
        assert_eq!(f.body, "Héllo [pii:pii_record:e1]");
        assert_eq!(f.images.len(), 1);
        assert_eq!(f.videos.len(), 1);
        assert_eq!(f.videos[0].duration_secs, Some(2.0));
        assert_eq!(f.videos[0].thumbnail_path.as_deref(), Some("/t.png"));

        let f = dto(RAW).await;
        assert_eq!(f.body, RAW);
        assert!(f.images.is_empty() && f.videos.is_empty());
    }

    #[test]
    fn previews_are_text_with_tokens_kept() {
        for (stored, want) in [
            (V2, "Héllo [pii:pii_record:e1]\n\nworld"),
            (V1, "Héllo [pii:pii_record:e1]"),
            (RAW, RAW),
        ] {
            let p = text_preview(stored, 200);
            assert_eq!(p, want);
            assert!(p.contains("[pii:pii_record:e1]"), "tokens stay tokens");
            assert!(!p.contains("\"body\"") && !p.contains("sovereign.doc"));
        }
        // Char-safe cut on the text, not on the stored JSON.
        assert_eq!(text_preview(V2, 3), "Hél...");
        assert_eq!(text_preview(&"é".repeat(130), 120), format!("{}...", "é".repeat(120)));
    }
}

