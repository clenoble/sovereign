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
    // Persist to the device preferences so the choice survives a restart:
    // shared by the personas, read before login (C-AREA AR2).
    let defaults = crate::tauri_commands::auth::device_preferences(&state);
    if let Err(e) = sovereign_core::profile::DevicePreferences::update(&state.profile_dir, defaults, |p| {
        p.theme = next.to_string()
    }) {
        tracing::warn!("Failed to persist the theme: {e}");
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

/// A document's versions, through the document service (item C5 of v0.0.13:
/// [`commits_listing`]), each row named as History shows it (spec ch. 12,
/// HN1-HN5: [`commits_listing_named`]).
#[tauri::command]
pub async fn list_commits(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<NamedCommitListDto, String> {
    state.require_unlocked(&webview).await?;
    let (devices, own_id) = history_names_in(&state).await;
    commits_listing_named(state.docs().await, &doc_id, &devices, &own_id).await
}

/// The documents whose history this session could not read, for the phone's
/// sync alerts (item C5: [`histories_not_read_listing`]).
#[tauri::command]
pub async fn histories_not_read(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<HistoriesNotReadDto, String> {
    state.require_unlocked(&webview).await?;
    histories_not_read_listing(state.docs().await).await
}

/// At most `max_chars` characters of `text`, then « ... » when cut (as
/// [`text_preview`] does).
fn cut_preview(text: String, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    } else {
        text
    }
}

// Item C5 of v0.0.13 (spec ch. 03, *What cannot be read is never shown as
// empty*; decided 2026-11-18 by the Fable maille, by delegation of Céline).
// Signatures by its test author; the bodies, and the commands that call them,
// are the implementer's.

/// A document's versions as the phone's History receives them.
/// `list_commits` answers [`NamedCommitListDto`] from HN on; this shape stays as C5 built
/// it, held by its tests.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Serialize)]
pub struct CommitListDto {
    /// Newest first; empty when they could not be read. A version that could
    /// not be read stays in its place, with its time, said by its own
    /// `could_not_read`.
    pub commits: Vec<CommitSummaryDto>,
    /// `sovereign_docs::COULD_NOT_READ_HISTORY` when the versions could not be
    /// read; `None` when they were.
    pub could_not_read: Option<String>,
}

/// What `list_commits` answers once the caller is checked: `docs` is what
/// `AppState::docs` gives, the session's document service or why there is
/// none. No service is a refusal (`Err`), never « could not be read ». A
/// failed read, whole or of one version, counts the document for this session
/// until its next good read (`histories_not_read_listing`).
#[cfg_attr(not(test), allow(dead_code))]
pub async fn commits_listing(
    docs: Result<std::sync::Arc<sovereign_docs::DocumentService>, String>,
    doc_id: &str,
) -> Result<CommitListDto, String> {
    let (rows, could_not_read) = timed_listing(docs, doc_id).await?;
    Ok(CommitListDto { commits: rows.into_iter().map(|(c, _)| c).collect(), could_not_read })
}

/// [`commits_listing`]'s rows, each with its time as the service gives it (for
/// History's names, which read times, never the RFC 3339 text back), and the
/// listing's own `could_not_read`.
async fn timed_listing(
    docs: Result<std::sync::Arc<sovereign_docs::DocumentService>, String>,
    doc_id: &str,
) -> Result<(Vec<(CommitSummaryDto, chrono::DateTime<chrono::Utc>)>, Option<String>), String> {
    let docs = docs?;
    let listed = docs.history(doc_id).await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let Some(versions) = listed else {
        return Ok((Vec::new(), Some(sovereign_docs::COULD_NOT_READ_HISTORY.to_string())));
    };
    let commits = versions
        .into_iter()
        .map(|(v, view)| {
            // A version that reads gives its title and preview; one that does
            // not gives none, and its sentence (HR4), never its ciphertext.
            let (title, preview) = match (v.could_not_read, view) {
                (None, Some(view)) => {
                    let text = view.doc.map(|d| d.plain_text()).unwrap_or_default();
                    (view.title, cut_preview(text, 200))
                }
                _ => (String::new(), String::new()),
            };
            let row = CommitSummaryDto {
                id: v.commit_id,
                message: v.message,
                timestamp: v.timestamp.to_rfc3339(),
                snapshot_title: title,
                snapshot_preview: preview,
                could_not_read: v.could_not_read.map(str::to_string),
            };
            (row, v.timestamp)
        })
        .collect();
    Ok((commits, None))
}

// History's names on the phone (spec ch. 12, HN1-HN5; v0.0.13, the C5 line of
// the scope). Signatures by the test author of HN on the phone; the bodies, and
// the commands that call them, are the implementer's.

/// A History row as the phone receives it from v0.0.13 on (HN1-HN3): the version as C5 gives
/// it, unchanged (`message` the stored one, `timestamp` RFC 3339), and beside it the row's
/// words and its local time. One object on the wire: the frontend's `CommitSummary` with two
/// more fields. A wrapper, so that `CommitSummaryDto`, whose shape C5's tests hold, stays as it is.
#[derive(Debug, Serialize)]
pub struct NamedCommitDto {
    #[serde(flatten)]
    pub commit: CommitSummaryDto,
    /// The row's words, as `sovereign_docs::history_names::shown_message` makes them.
    pub shown_message: String,
    /// The row's own time, `history_names::local_time`: `YYYY-MM-DD HH:MM`, local time.
    pub local_time: String,
}

/// What `list_commits` answers from v0.0.13 on: [`CommitListDto`]'s two fields, its rows named.
#[derive(Debug, Serialize)]
pub struct NamedCommitListDto {
    pub commits: Vec<NamedCommitDto>,
    pub could_not_read: Option<String>,
}

/// HN1-HN3, HN5: [`commits_listing`] with each row's `shown_message` and
/// `local_time` made by `sovereign_docs::history_names`, from `devices` (peer id
/// → the name this device's paired list holds), `own_id` (this device's peer id,
/// « this device ») and the document's own versions. `list_commits` answers this,
/// with [`history_names_in`].
pub async fn commits_listing_named(
    docs: Result<std::sync::Arc<sovereign_docs::DocumentService>, String>,
    doc_id: &str,
    devices: &std::collections::HashMap<String, String>,
    own_id: &str,
) -> Result<NamedCommitListDto, String> {
    let (rows, could_not_read) = timed_listing(docs, doc_id).await?;
    // HN2: a version a row names is named by its time, read in this document's
    // own history, as the desktop does.
    let ids: Vec<(String, chrono::DateTime<chrono::Utc>)> = rows.iter().map(|(c, at)| (c.id.clone(), *at)).collect();
    let versions: Vec<(&str, chrono::DateTime<chrono::Utc>)> = ids.iter().map(|(id, at)| (id.as_str(), *at)).collect();
    let named = rows
        .into_iter()
        .map(|(c, at)| NamedCommitDto {
            shown_message: sovereign_docs::history_names::shown_message(&c.message, devices, own_id, &versions),
            local_time: sovereign_docs::history_names::local_time(at),
            commit: c,
        })
        .collect();
    Ok(NamedCommitListDto { commits: named, could_not_read })
}

/// HN1: what names a device in this session's History: the paired list (peer id
/// → name) and this device's peer id; empty, and `""`, when the session has no
/// node or no paired list.
pub async fn history_names_in(state: &AppState) -> (std::collections::HashMap<String, String>, String) {
    // The paired list, as the node installed it at login (none in a build
    // without p2p, before login, or in a duress session).
    #[allow(unused_mut)]
    let mut devices = std::collections::HashMap::new();
    #[cfg(feature = "p2p")]
    if let Some(manager) = state.pairing_manager.read().await.as_ref() {
        devices.extend(manager.list_devices().into_iter().map(|d| (d.peer_id.clone(), d.device_name.clone())));
    }
    // This device's own peer id, derived from the per-device key the login
    // installs: the id its node runs under and the document service stamps
    // with (the desktop's `own_peer_id` does the same).
    #[cfg(feature = "encryption")]
    let own_id = state
        .p2p_identity_key()
        .await
        .and_then(|key| sovereign_crypto::peer_identity::peer_id(&key).ok())
        .unwrap_or_default();
    #[cfg(not(feature = "encryption"))]
    let own_id = {
        let _ = state;
        String::new()
    };
    (devices, own_id)
}

/// What `restore_commit` answers (HN4): the document after the restore, and
/// `sovereign_docs::history_names::ALREADY_CURRENT` when the version chosen was
/// already the current one, nothing written; `None` when it was restored.
#[derive(Debug, Serialize)]
pub struct RestoredDto {
    pub document: FullDocument,
    pub already_current: Option<String>,
}

/// HN4: the restore behind `restore_commit`, once the caller is checked.
pub async fn restore_in(
    docs: Result<std::sync::Arc<sovereign_docs::DocumentService>, String>,
    doc_id: &str,
    commit_id: &str,
) -> Result<RestoredDto, String> {
    let docs = docs?;
    // C-DOC adoption step 2: the current state becomes a version first.
    let (document, changed) = crate::doc_saves::restore_said(&docs, doc_id, commit_id).await?;
    let already_current = (!changed).then(|| sovereign_docs::history_names::ALREADY_CURRENT.to_string());
    Ok(RestoredDto { document, already_current })
}

/// The documents whose history this session could not read, for the phone's
/// sync alerts: how many, and the sentence to say
/// (`sovereign_docs::histories_not_read_sentence`), `None` when there are none.
/// Kept in memory for the session, never stored (decided 2026-11-18).
///
/// Its own command, beside `list_sync_alerts`, not a field of `SyncAlertsDto`:
/// that object's shape is held by a protected test (`could_not_check_tests`,
/// « the lists travel with the frontend's field names »).
#[derive(Debug, Serialize)]
pub struct HistoriesNotReadDto {
    pub count: usize,
    pub sentence: Option<String>,
}

/// What the command behind `HistoriesNotReadDto` answers once the caller is
/// checked; no service is a refusal (`Err`).
pub async fn histories_not_read_listing(
    docs: Result<std::sync::Arc<sovereign_docs::DocumentService>, String>,
) -> Result<HistoriesNotReadDto, String> {
    let docs = docs?;
    let count = docs.histories_not_read();
    let sentence = (count > 0).then(|| sovereign_docs::histories_not_read_sentence(count));
    Ok(HistoriesNotReadDto { count, sentence })
}

/// Restore a document to a specific commit.
#[tauri::command]
pub async fn restore_commit(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
    commit_id: String,
) -> Result<RestoredDto, String> {
    state.require_unlocked(&webview).await?;
    restore_in(state.docs().await, &doc_id, &commit_id).await
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
    // The body, and FC1's question when a skill's find-or-create needs her
    // answer (item 9): `crate::thread_choice::execute_skill_on`.
    let docs = state.docs().await?;
    crate::thread_choice::execute_skill_on(
        docs,
        &state.skill_registry,
        state.skill_db.clone(),
        state.skill_llm.clone(),
        &skill_name,
        &action,
        &doc_id,
        &params,
    )
    .await
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
/// purge tick. Nothing is hard-deleted here. Answers what it did (v0.0.12
/// item 9, T3): `"trashed"`, or `"already_in_trash"` when the document was
/// there already (the phone then offers no Undo).
#[tauri::command]
pub async fn delete_document(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<crate::doc_trash::DeleteOutcome, String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    docs.trash(&id)
        .await
        .map(crate::doc_trash::DeleteOutcome::from)
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))
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

