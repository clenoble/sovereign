//! The document writes of the Tauri commands, through the document service
//! (C-DOC adoption step 2, `doc/plans/C-DOC-adoption.md`): the document
//! panel's save (desktop Tauri and the mobile reader share it), the end of its
//! editing session, version restores and a skill's write-back. Kept out of the
//! `#[tauri::command]` wrappers so they are tested against a real encrypted
//! store without a webview.
//!
//! What the panel sends is the v1 field set it was given (title, body, images,
//! videos) plus the revision it read. A save writes only what changed: a new
//! title, and the body as the document's text (`EditContent::PlainText`, which
//! keeps every non-text block); images and videos cannot be changed from the
//! panel. A document with several text blocks refuses a changed body (the
//! service's `AmbiguousText`) — the panel cannot say which block changed.
//! Nothing is ever overwritten silently: a save based on a revision that is
//! not the current one is refused with [`CONFLICT_PREFIX`], and the panel asks
//! the user (keep their text, or load the stored version).

use std::collections::HashMap;
use std::sync::Mutex;

use sovereign_ai::docs_host::{doc_error_message, editor_save, new_edit_session, skill_output_edit, view_fields};
use sovereign_core::content::{ContentFields, ContentImage, ContentVideo};
use sovereign_docs::{DocError, DocumentService, DocumentView, Edit, EditContent, EditSession, Origin, PiiOutcome, Revision, Trust};

use crate::tauri_commands::{ContentImageDto, ContentVideoDto, FullDocument};

/// The start of the error a save returns when the document changed since the
/// panel read it. The frontend recognises it to offer its two choices.
pub const CONFLICT_PREFIX: &str = "conflict: ";

/// One open document's editing session.
#[derive(Clone, Debug)]
pub struct SessionState {
    pub session: EditSession,
    /// The revision this session's last save produced.
    pub last_saved: Option<Revision>,
}

pub type Sessions = Mutex<HashMap<String, SessionState>>;

/// The document DTO, from the service's view: the text blocks as the body,
/// image and video blocks as media (tokens kept as `[pii:<id>]`), and the
/// revision a save must be based on.
pub fn full_document(view: &DocumentView) -> FullDocument {
    let fields = view_fields(view);
    FullDocument {
        id: view.id.clone(),
        title: view.title.clone(),
        body: fields.body,
        images: fields.images.into_iter().map(|i| ContentImageDto { path: i.path, caption: i.caption }).collect(),
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
        thread_id: view.thread_id.clone(),
        is_owned: view.trust == Trust::Owned,
        created_at: view.created_at.to_rfc3339(),
        modified_at: view.modified_at.to_rfc3339(),
        revision: view.revision.0.clone(),
    }
}

fn message(e: &DocError) -> String {
    match e {
        DocError::Conflict { .. } => format!("{CONFLICT_PREFIX}{}", doc_error_message(e)),
        other => doc_error_message(other),
    }
}

pub async fn read_document(docs: &DocumentService, id: &str) -> Result<FullDocument, String> {
    docs.read(id).await.map(|v| full_document(&v)).map_err(|e| message(&e))
}

/// What the document panel sends on a save.
pub struct PanelSave {
    pub title: String,
    pub body: String,
    pub images: Vec<ContentImageDto>,
    pub videos: Vec<ContentVideoDto>,
    /// The revision the panel read (`FullDocument::revision`).
    pub base_revision: String,
    /// The user asked for a version (Ctrl+S).
    pub explicit: bool,
}

fn images_of(dtos: &[ContentImageDto]) -> Vec<ContentImage> {
    dtos.iter().map(|i| ContentImage { path: i.path.clone(), caption: i.caption.clone() }).collect()
}

fn videos_of(dtos: &[ContentVideoDto]) -> Vec<ContentVideo> {
    dtos.iter()
        .map(|v| ContentVideo {
            path: v.path.clone(),
            caption: v.caption.clone(),
            duration_secs: v.duration_secs,
            thumbnail_path: v.thumbnail_path.clone(),
        })
        .collect()
}

/// Save the panel's state of document `id` (see the module doc). Returns the
/// document as stored after the save (personal data replaced by tokens, the
/// new revision).
pub async fn save_from_panel(
    docs: &DocumentService,
    sessions: &Sessions,
    id: &str,
    save: PanelSave,
) -> Result<FullDocument, String> {
    let view = docs.read(id).await.map_err(|e| message(&e))?;
    let base = Revision(save.base_revision);
    if view.revision != base {
        return Err(message(&DocError::Conflict { id: id.to_string(), current: view.revision }));
    }
    let current = view_fields(&view);
    if images_of(&save.images) != current.images || videos_of(&save.videos) != current.videos {
        return Err("Images and videos can't be changed from this window. Nothing was changed.".into());
    }
    let title = (save.title != view.title).then_some(save.title);
    let content = (save.body != current.body).then_some(EditContent::PlainText(save.body));
    let state = {
        let mut map = sessions.lock().map_err(|_| "editing sessions unavailable".to_string())?;
        map.entry(id.to_string())
            .or_insert_with(|| SessionState { session: new_edit_session(), last_saved: None })
            .clone()
    };
    let edit = Edit {
        base,
        title,
        content,
        origin: Origin::Editor { session: state.session },
        explicit: save.explicit,
        pasted_tokens: vec![],
    };
    let out = editor_save(docs, id, state.session, state.last_saved.as_ref(), edit)
        .await
        .map_err(|e| message(&e))?;
    if let PiiOutcome::Failed { message } = &out.pii {
        tracing::warn!("document {id} saved without a personal-data scan: {message}");
    }
    if let Ok(mut map) = sessions.lock() {
        if let Some(s) = map.get_mut(id) {
            s.last_saved = Some(out.revision.clone());
        }
    }
    read_document(docs, id).await
}

/// The panel closed: end its editing session, so its last state is a version
/// (G5). Nothing to do when the panel never saved.
pub async fn close_panel(docs: &DocumentService, sessions: &Sessions, id: &str) -> Result<(), String> {
    let state = sessions.lock().ok().and_then(|mut m| m.remove(id));
    match state {
        Some(s) => docs.end_session(id, s.session).await.map_err(|e| message(&e)),
        None => Ok(()),
    }
}

/// Restore version `commit_id` of document `id`: the current state is made a
/// version first, the restored state becomes a new version.
pub async fn restore(docs: &DocumentService, id: &str, commit_id: &str) -> Result<FullDocument, String> {
    docs.restore_version(id, commit_id).await.map_err(|e| message(&e))?;
    read_document(docs, id).await
}

/// Write a skill's `ContentUpdate` back: `input` is what the skill was given
/// (read at `base`), `output` what it returned. Returns whether anything was
/// written; the prior state is versioned first, the result after (G7).
pub async fn apply_skill(
    docs: &DocumentService,
    id: &str,
    base: Revision,
    skill: &str,
    input: &ContentFields,
    output: &ContentFields,
) -> Result<bool, String> {
    let Some(edit) = skill_output_edit(input, output)? else { return Ok(false) };
    let out = docs.apply_skill_output(id, base, skill, edit).await.map_err(|e| message(&e))?;
    Ok(out.changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_ai::docs_host::testing::{account_key, encrypted_memory_store};
    use sovereign_ai::docs_host::build_document_service;
    use sovereign_db::traits::GraphDB;
    use sovereign_docs::{NewContent, NewDocument, Provenance};

    const V1: &str = r#"{"body":"hello there","images":[{"path":"/i.png","caption":"cap"}],"videos":[]}"#;
    const V2_TWO_TEXTS: &str = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000000a01","text":"one"},{"type":"text","id":"00000000000000000000000000000a02","text":"two"}]}"#;

    struct Fixture {
        store: std::sync::Arc<sovereign_db::encrypted::EncryptedGraphDB>,
        docs: DocumentService,
        sessions: Sessions,
    }

    async fn fixture(tag: &str, with_key: bool) -> Fixture {
        let store = encrypted_memory_store(tag).await;
        let key = with_key.then(account_key);
        let docs = build_document_service(store.clone(), key, "test-device".into());
        Fixture { store, docs, sessions: Mutex::new(HashMap::new()) }
    }

    /// A document as legacy writers left it: a row with `content` written
    /// directly, no version, no trust or provenance recorded.
    async fn legacy(f: &Fixture, content: &str) -> String {
        let mut d = sovereign_db::schema::Document::new("Legacy".into(), "thread:t".into(), true);
        d.content = content.into();
        f.store.create_document(d).await.unwrap().id_string().unwrap()
    }

    fn panel(doc: &FullDocument, body: &str) -> PanelSave {
        PanelSave {
            title: doc.title.clone(),
            body: body.into(),
            images: doc.images.clone(),
            videos: doc.videos.clone(),
            base_revision: doc.revision.clone(),
            explicit: false,
        }
    }

    fn stored_is_v2(content: &str) -> bool {
        let v: serde_json::Value = serde_json::from_str(content).unwrap();
        v["format"] == "sovereign.doc" && v["v"] == 2
    }

    /// A v1 envelope and raw text saved from the panel end as v2, the prior
    /// state versioned, the image kept.
    #[tokio::test]
    async fn legacy_documents_saved_from_the_panel_end_as_v2_with_the_prior_state_versioned() {
        let f = fixture("legacy", true).await;
        for content in [V1, "raw text from an old save"] {
            let id = legacy(&f, content).await;
            let doc = read_document(&f.docs, &id).await.unwrap();
            let saved = save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "edited text")).await.unwrap();
            assert_eq!(saved.body, "edited text");
            assert_eq!(saved.images.len(), doc.images.len(), "media kept");
            let row = f.store.get_document(&id).await.unwrap();
            assert!(stored_is_v2(&row.content), "{}", row.content);
            let commits = f.store.list_document_commits(&id).await.unwrap();
            assert!(commits.iter().any(|c| c.snapshot.content == content), "prior state versioned");
        }
    }

    /// A v2 document with one text block saves; with two, a changed body is
    /// refused and nothing is written, while a title-only save goes through.
    #[tokio::test]
    async fn a_structured_document_refuses_a_text_only_save_but_keeps_its_title_save() {
        let f = fixture("v2", true).await;
        let id = legacy(&f, V2_TWO_TEXTS).await;
        let doc = read_document(&f.docs, &id).await.unwrap();
        let err = save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "collapsed")).await.unwrap_err();
        assert!(err.contains("2 blocks of text"), "{err}");
        assert_eq!(f.store.get_document(&id).await.unwrap().content, V2_TWO_TEXTS, "nothing written");

        let mut t = panel(&doc, &doc.body);
        t.title = "Renamed".into();
        let saved = save_from_panel(&f.docs, &f.sessions, &id, t).await.unwrap();
        assert_eq!(saved.title, "Renamed");
        assert_eq!(saved.body, "one\n\ntwo");
    }

    /// Personal data typed in the panel is tokenized at the save, and the DTO
    /// the save returns carries the tokens (so the next save sends no raw value).
    #[tokio::test]
    async fn the_save_runs_ingest_and_returns_the_tokenized_text() {
        let f = fixture("ingest", true).await;
        let id = legacy(&f, "").await;
        let doc = read_document(&f.docs, &id).await.unwrap();
        let saved = save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "write to alice@example.com"))
            .await
            .unwrap();
        assert!(saved.body.contains("[pii:pii_record:"), "{}", saved.body);
        assert!(!saved.body.contains("alice@example.com"));
        let row = f.store.get_document(&id).await.unwrap();
        assert!(row.pii_scanned_at.is_some());
        assert!(!row.content.contains("alice@example.com"));
    }

    /// Without the account key the hook cannot run: the text is stored as
    /// typed and the document is marked not scanned — never silently skipped.
    #[tokio::test]
    async fn without_a_key_the_document_is_stored_not_scanned() {
        let f = fixture("nokey", false).await;
        let id = legacy(&f, "").await;
        let doc = read_document(&f.docs, &id).await.unwrap();
        save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "alice@example.com")).await.unwrap();
        let row = f.store.get_document(&id).await.unwrap();
        assert!(row.pii_scanned_at.is_none());
        assert!(f.docs.read(&id).await.unwrap().plain_text().contains("alice@example.com"));
    }

    /// A save based on a revision that is no longer current is refused with
    /// the conflict prefix, and the other writer's state stays.
    #[tokio::test]
    async fn a_stale_save_is_a_conflict_and_overwrites_nothing() {
        let f = fixture("conflict", true).await;
        let id = legacy(&f, "start").await;
        let doc = read_document(&f.docs, &id).await.unwrap();
        // Another writer (a skill) saves first.
        let input = ContentFields { body: "start".into(), ..Default::default() };
        let output = ContentFields { body: "by the skill".into(), ..Default::default() };
        assert!(apply_skill(&f.docs, &id, Revision(doc.revision.clone()), "upper", &input, &output).await.unwrap());
        let err = save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "mine")).await.unwrap_err();
        assert!(err.starts_with(CONFLICT_PREFIX), "{err}");
        assert_eq!(f.docs.read(&id).await.unwrap().plain_text(), "by the skill");
    }

    /// "Keep mine" after a conflict: the panel saves on the current revision.
    /// A state a non-service writer stored under this session's name (sync,
    /// until step 4) is versioned before it is replaced.
    #[tokio::test]
    async fn keeping_mine_after_a_direct_write_versions_that_write_first() {
        let f = fixture("keepmine", true).await;
        let id = legacy(&f, "").await;
        let doc = read_document(&f.docs, &id).await.unwrap();
        let saved = save_from_panel(&f.docs, &f.sessions, &id, panel(&doc, "mine 1")).await.unwrap();
        // A direct write (as sync does today): content replaced, no version.
        f.store.update_document(&id, None, Some("from the other device")).await.unwrap();
        let err = save_from_panel(&f.docs, &f.sessions, &id, panel(&saved, "mine 2")).await.unwrap_err();
        assert!(err.starts_with(CONFLICT_PREFIX));
        // The user keeps their text: the panel reads the current revision and saves on it.
        let fresh = read_document(&f.docs, &id).await.unwrap();
        let mut keep = panel(&fresh, "mine 2");
        keep.body = "mine 2".into();
        save_from_panel(&f.docs, &f.sessions, &id, keep).await.unwrap();
        let commits = f.store.list_document_commits(&id).await.unwrap();
        assert!(
            commits.iter().any(|c| c.snapshot.content == "from the other device"),
            "the direct write is in History"
        );
        assert_eq!(f.docs.read(&id).await.unwrap().plain_text(), "mine 2");
    }

    /// Closing the panel makes its last state a version.
    #[tokio::test]
    async fn closing_the_panel_versions_its_last_state() {
        let f = fixture("close", true).await;
        let created = f
            .docs
            .create(NewDocument {
                title: "N".into(),
                thread_id: "thread:t".into(),
                content: NewContent::Text("a".into()),
                trust: Trust::Owned,
                provenance: Provenance::Written,
                origin: Origin::Assistant,
                created_at: None,
                modified_at: None,
            })
            .await
            .unwrap();
        let doc = read_document(&f.docs, &created.id).await.unwrap();
        save_from_panel(&f.docs, &f.sessions, &created.id, panel(&doc, "ab")).await.unwrap();
        let before = f.docs.versions(&created.id).await.unwrap().len();
        close_panel(&f.docs, &f.sessions, &created.id).await.unwrap();
        let after = f.docs.versions(&created.id).await.unwrap();
        assert_eq!(after.len(), before + 1, "the small autosave became a version at close");
        assert!(f.sessions.lock().unwrap().is_empty());
    }

    /// Restoring keeps the current state as a version (none of the restore
    /// sites did that before the service).
    #[tokio::test]
    async fn a_restore_versions_the_current_state_first() {
        let f = fixture("restore", true).await;
        let id = legacy(&f, "first").await;
        f.store.commit_document(&id, "old").await.unwrap();
        let first = f.store.list_document_commits(&id).await.unwrap()[0].id_string().unwrap();
        f.store.update_document(&id, None, Some("current, never versioned")).await.unwrap();
        let back = restore(&f.docs, &id, &first).await.unwrap();
        assert_eq!(back.body, "first");
        let commits = f.store.list_document_commits(&id).await.unwrap();
        assert!(commits.iter().any(|c| c.snapshot.content == "current, never versioned"));
        assert!(stored_is_v2(&f.store.get_document(&id).await.unwrap().content));
    }

    /// A skill's output: versioned before and after; an emptied text and a
    /// changed image are refused, nothing written.
    #[tokio::test]
    async fn skill_write_back_goes_through_the_service() {
        let f = fixture("skill", true).await;
        let id = legacy(&f, V1).await;
        let view = f.docs.read(&id).await.unwrap();
        let input = view_fields(&view);
        let mut emptied = input.clone();
        emptied.body = String::new();
        assert!(apply_skill(&f.docs, &id, view.revision.clone(), "s", &input, &emptied).await.is_err());
        let mut no_image = input.clone();
        no_image.images.clear();
        assert!(apply_skill(&f.docs, &id, view.revision.clone(), "s", &input, &no_image).await.is_err());
        assert_eq!(f.store.get_document(&id).await.unwrap().content, V1, "nothing written");

        let mut upper = input.clone();
        upper.body = input.body.to_uppercase();
        assert!(apply_skill(&f.docs, &id, view.revision.clone(), "upper", &input, &upper).await.unwrap());
        let after = f.docs.read(&id).await.unwrap();
        assert_eq!(after.plain_text(), "HELLO THERE");
        assert_eq!(view_fields(&after).images.len(), 1, "the image block is kept");
        let msgs: Vec<String> = f.docs.versions(&id).await.unwrap().into_iter().map(|v| v.message).collect();
        assert!(msgs.iter().any(|m| m.contains("upper")), "{msgs:?}");
        assert!(f.store.list_document_commits(&id).await.unwrap().iter().any(|c| c.snapshot.content == V1));
    }
}
