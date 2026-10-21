//! What a host gives the document service (`sovereign-docs`, contract C-DOC,
//! spec ch. 03), shared by the native shell and the Tauri app — C-DOC adoption
//! step 2 (`doc/plans/C-DOC-adoption.md`).
//!
//! - [`RegexDocumentIngest`]: the personal-data hook ([`PiiIngest`]) over the
//!   regex ingest of [`crate::pii::ingest`] — the same pipeline the Tauri save
//!   path ran before (regex only, Swiss locale), now on **every** text field of
//!   every ingesting write (title, text blocks, captions, alt texts), as the
//!   contract asks (G10).
//! - [`SystemClock`], [`FileMedia`], [`LoggedEvents`]: the clock, media hashing
//!   and the change-event sink.
//! - [`build_document_service`]: the service over a persona's encrypted store.
//! - [`new_edit_session`]: an editing-session id that does not repeat across
//!   runs.
//! - Creating (adoption step 3): [`create`] (every host creator), [`new_document`],
//!   [`duplicate`] (the service's copy, G20), [`ServiceSkillDb`] (what skills
//!   create goes through the service), and the preserved raw body of a new
//!   document ([`flush_raw_bodies_of`]).
//!
//! Why this lives here: both hosts hold an `Orchestrator` and the PII pipeline
//! already lives in this crate; `sovereign-docs` takes ingest as a trait so it
//! does not depend on the AI layer.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Entity, PiiKind, PiiRecord, ReviewState, SourceKind, SourceRef};
use sovereign_db::traits::GraphDB;
use sovereign_db::DbError;
use sovereign_core::content::ContentFields;
use sovereign_docs::{
    Clock, CreateOutcome, DefaultCommitPolicy, DocError, DocumentChanged, DocumentEvents, DocumentService, Duplicate,
    DocumentView, Edit, EditContent, EditSession, IngestRequest, IngestedItem, MediaFiles, NewContent, NewDocument,
    Origin, PiiError, PiiIngest, Provenance, ReadOnlyReason, Revision, SaveOutcome, ServiceDeps, TextField,
    Trust,
};

use crate::pii::ingest::{ingest_text, PiiSink};
use crate::pii::pipeline::PipelineConfig;

// ------------------------------------------------------------------ service --

/// The document service over a persona's encrypted store, with the shipped
/// commit policy (versions by amount of change), the system clock, media
/// hashing from the file system, and — when the account key is there — the
/// regex personal-data hook. Without the key ingest cannot encrypt what it
/// finds, so the hook reports a failure and every write is stored "not
/// scanned" (G10) rather than silently skipped.
pub fn build_document_service(
    store: Arc<EncryptedGraphDB>,
    account_key: Option<Arc<AccountKey>>,
    device: String,
) -> DocumentService {
    let pii: Arc<dyn PiiIngest> = match account_key {
        Some(key) => Arc::new(RegexDocumentIngest::new(store.clone() as Arc<dyn GraphDB>, key)),
        None => Arc::new(UnavailableIngest),
    };
    DocumentService::new(ServiceDeps {
        store,
        commit_policy: Arc::new(DefaultCommitPolicy::default()),
        pii,
        media: Arc::new(FileMedia),
        clock: Arc::new(SystemClock),
        events: Arc::new(LoggedEvents),
        device,
    })
}

/// A fresh editing-session id. Random, so a session after a restart never
/// takes the id of one before it: the service lets a session replace its own
/// last state without a version, which must stay true only within one session.
pub fn new_edit_session() -> EditSession {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    EditSession(h.finish())
}

// ------------------------------------------------------------ editor saves --

/// One save from an editing session (`Origin::Editor`), with what the host
/// knows of the session: the revision its own last save produced.
///
/// When the save is based on any other revision — the first save of the
/// session, or after the host rebased the session onto a state someone else
/// wrote (a conflict the user chose to keep their text over) — the session is
/// ended first (`end_session`): a state stored under this session's name by a
/// writer that does not go through the service yet (sync applies remote states
/// directly until adoption step 4) is then made a version before the save
/// replaces it, which the service would otherwise skip as "the same session"
/// (G4). Writers that go through the service are covered by the service itself.
pub async fn editor_save(
    docs: &DocumentService,
    id: &str,
    session: EditSession,
    last_saved: Option<&Revision>,
    edit: Edit,
) -> Result<SaveOutcome, DocError> {
    if last_saved != Some(&edit.base) {
        docs.end_session(id, session).await?;
    }
    docs.save(id, edit).await
}

/// The v1 fields (body, images, videos) of a document as the service reads
/// it — what the Tauri document DTO and the skills receive. Empty when the
/// stored content is unreadable.
pub fn view_fields(view: &DocumentView) -> ContentFields {
    view.doc.as_ref().map(ContentFields::from_document).unwrap_or_default()
}

/// What a skill's `ContentUpdate` writes through the service
/// (`apply_skill_output`), from what the skill was given (`input`, read with
/// [`view_fields`]) and what it returned. `Ok(None)`: nothing to write.
/// `Err` is the user-facing reason nothing was written:
/// - a skill never writes an empty text over a non-empty one (lot 0.1);
/// - a skill that adds or removes an image or a video cannot write that back
///   yet: the output is applied as text (`EditContent::PlainText`, which keeps
///   every non-text block) until C-SKILL gives skills the block model;
/// - a document with several text blocks refuses a text-only write in the
///   service (`AmbiguousText`), see [`doc_error_message`].
pub fn skill_output_edit(input: &ContentFields, output: &ContentFields) -> Result<Option<EditContent>, &'static str> {
    if output.images != input.images || output.videos != input.videos {
        return Err("This skill changes the document's images or videos; skills can't write that back yet. \
                    Nothing was changed.");
    }
    if !input.body.trim().is_empty() && output.body.trim().is_empty() {
        return Err("The skill would have replaced the document's text with nothing. Nothing was changed.");
    }
    if output.body == input.body {
        return Ok(None);
    }
    Ok(Some(EditContent::PlainText(output.body.clone())))
}

/// A user-facing sentence for a refused write (no document text in it).
pub fn doc_error_message(e: &DocError) -> String {
    match e {
        DocError::Conflict { .. } => "This document was changed elsewhere (another window, a skill, the assistant \
                                      or a paired device) since it was opened. Nothing was overwritten."
            .into(),
        DocError::AmbiguousText { text_blocks } => format!(
            "This document has {text_blocks} blocks of text; this can only change a document with one. \
             Nothing was changed."
        ),
        DocError::ReadOnly { reason: ReadOnlyReason::NewerFormat { v }, .. } => {
            format!("This document was written by a newer version (format {v}); it can't be changed here. Nothing was changed.")
        }
        DocError::ReadOnly { reason: ReadOnlyReason::Unreadable { .. }, .. } => {
            "This document can't be read, so it can't be changed. Nothing was changed.".into()
        }
        DocError::InTrash(_) => "This document is in the trash. Nothing was changed.".into(),
        DocError::Purged(_) | DocError::NotFound(_) => "This document no longer exists.".into(),
        DocError::VersionRefused { reason, .. } => format!("This version can't be restored: {reason}."),
        other => format!("The document couldn't be saved ({other})."),
    }
}

// --------------------------------------------------------------- creating --

/// Creates a document through the service (C-DOC adoption step 3): the one
/// call every host creator makes. Then preserves the new document's raw body
/// ([`flush_raw_bodies_of`]), which the personal-data hook cannot write while the
/// service is creating it (the row does not exist yet when it ingests).
pub async fn create(docs: &DocumentService, new: NewDocument) -> Result<CreateOutcome, DocError> {
    let out = docs.create(new).await?;
    flush_raw_bodies_of(docs.store()).await;
    Ok(out)
}

/// A new document from text (markdown: one text block), created now.
pub fn new_document(
    title: impl Into<String>,
    thread_id: impl Into<String>,
    text: impl Into<String>,
    trust: Trust,
    provenance: Provenance,
    origin: Origin,
) -> NewDocument {
    NewDocument {
        title: title.into(),
        thread_id: thread_id.into(),
        content: NewContent::Text(text.into()),
        trust,
        provenance,
        origin,
        created_at: None,
        modified_at: None,
    }
}

/// The user's own new document, written here from an editor gesture (the
/// shell's and mobile's "new document", the CLI): owned, provenance
/// *written*, origin a fresh editing session.
pub fn new_user_document(title: impl Into<String>, thread_id: impl Into<String>, text: impl Into<String>) -> NewDocument {
    new_document(title, thread_id, text, Trust::Owned, Provenance::Written, Origin::Editor { session: new_edit_session() })
}

/// A page saved from the web: never owned at creation (G12; the user adopts it
/// afterwards if she wants), provenance *web* with its URL.
pub fn new_web_document(
    title: impl Into<String>,
    thread_id: impl Into<String>,
    url: impl Into<String>,
    text: impl Into<String>,
) -> NewDocument {
    new_document(
        title,
        thread_id,
        text,
        Trust::External,
        Provenance::Web { url: url.into() },
        Origin::Editor { session: new_edit_session() },
    )
}

/// Duplicates a document through the service (`DocumentService::duplicate`,
/// C-DOC G20): the source's content, its own first version, ingest, the
/// source's trust (a copy never raises or lowers it, so a copy of her own
/// document is owned) and the provenance *copy of <source>*. Its
/// personal-data tokens stay what they were: the service links the copy to
/// every record linked to the source whose token is in the source's text.
/// A trashed, purged or read-only source is refused. Then preserves the
/// copy's raw body ([`flush_raw_bodies_of`]), as [`create`] does.
pub async fn duplicate(
    docs: &DocumentService,
    source_id: &str,
    title: String,
    thread_id: String,
    origin: Origin,
) -> Result<CreateOutcome, DocError> {
    let out = docs
        .duplicate(Duplicate { source: source_id.to_string(), title: Some(title), thread_id: Some(thread_id), origin })
        .await?;
    flush_raw_bodies_of(docs.store()).await;
    Ok(out)
}

/// Skills whose documents are the user's own writing from scratch (ch. 04,
/// decided 2026-10-15: owned by default — "a journal entry"): created
/// **owned**, provenance *written*. Every other skill's new document is
/// content drawn from outside her own text (a file it imports, a WASM
/// plugin's output): **undetermined**, provenance *import* from the skill
/// (`skill:<name>`), never owned (G23). A copy is recognised separately
/// ([`ServiceSkillDb`]) and made by [`duplicate`].
pub const SKILLS_WRITING_FROM_SCRATCH: &[&str] = &["daily-journal"];

/// The trust and provenance of a new document a skill creates (not a copy).
pub fn skill_creation_trust(skill: &str) -> (Trust, Provenance) {
    if SKILLS_WRITING_FROM_SCRATCH.contains(&skill) {
        (Trust::Owned, Provenance::Written)
    } else {
        (Trust::Undetermined, Provenance::Import { source_path: format!("skill:{skill}") })
    }
}

/// What a skill creates goes through the service (adoption step 3, C-SKILL):
/// the narrow store access a skill is given, with `create_document` routed to
/// [`create`] and every read passed to the store access the host had.
///
/// - A document whose (non-empty) content is exactly one the skill read in
///   this run is a **copy** of it ([`duplicate`]: the service's copy, the
///   source's trust, provenance *copy of*, tokens kept linked). This is how
///   the duplicate skill, and any WASM skill that copies, is recognised
///   without the skill saying so.
/// - A skill that writes the user's own text from scratch
///   ([`SKILLS_WRITING_FROM_SCRATCH`]: the daily journal) creates an
///   **owned** document, provenance *written* (ch. 04, 2026-10-15).
/// - Anything else a skill creates (file import, a WASM plugin's output) is
///   **undetermined**: not her own words, not known to be external either;
///   it fails closed (fenced) until she adopts it (ch. 04). Provenance
///   *import*, with the skill as its source (`skill:<name>`); never owned
///   (G23). Origin: the skill.
pub struct ServiceSkillDb {
    inner: Arc<dyn sovereign_skills::SkillDbAccess>,
    docs: Arc<DocumentService>,
    skill: String,
    /// (id, stored content) of every document this run read.
    read: Mutex<Vec<(String, String)>>,
}

impl ServiceSkillDb {
    pub fn new(inner: Arc<dyn sovereign_skills::SkillDbAccess>, docs: Arc<DocumentService>, skill: &str) -> Self {
        Self { inner, docs, skill: skill.to_string(), read: Mutex::new(vec![]) }
    }

    async fn create_for_skill(&self, title: &str, thread_id: &str, content: &str) -> anyhow::Result<String> {
        let origin = Origin::Skill { name: self.skill.clone() };
        // An empty content says nothing about where it came from.
        let source = (!content.trim().is_empty())
            .then(|| self.read.lock().ok())
            .flatten()
            .and_then(|r| r.iter().find(|(_, c)| c == content).map(|(id, _)| id.clone()));
        if let Some(source) = source {
            let out = duplicate(&self.docs, &source, title.to_string(), thread_id.to_string(), origin)
                .await
                .map_err(|e| anyhow::anyhow!("{}", doc_error_message(&e)))?;
            return Ok(out.id);
        }
        // The skill's content in any stored form (v2, a v1 envelope, text) as v2.
        let v2 = match sovereign_docformat::DocumentV2::read_seeded(content, title) {
            Ok((doc, form)) if !matches!(form, sovereign_docformat::SourceForm::NewerVersion { .. }) => {
                doc.to_json_string()
            }
            _ => anyhow::bail!("the skill's document is not in a form this version can store"),
        };
        let (trust, provenance) = skill_creation_trust(&self.skill);
        let new = NewDocument {
            title: title.to_string(),
            thread_id: thread_id.to_string(),
            content: NewContent::V2(v2),
            trust,
            provenance,
            origin,
            created_at: None,
            modified_at: None,
        };
        let out = create(&self.docs, new).await.map_err(|e| anyhow::anyhow!("{}", doc_error_message(&e)))?;
        Ok(out.id)
    }
}

impl sovereign_skills::SkillDbAccess for ServiceSkillDb {
    fn search_documents(&self, query: &str) -> anyhow::Result<Vec<(String, String, String)>> {
        self.inner.search_documents(query)
    }

    fn get_document(&self, id: &str) -> anyhow::Result<(String, String, String)> {
        let (title, thread_id, content) = self.inner.get_document(id)?;
        if let Ok(mut r) = self.read.lock() {
            r.push((id.to_string(), content.clone()));
        }
        Ok((title, thread_id, content))
    }

    fn list_documents(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
        self.inner.list_documents(thread_id)
    }

    fn create_document(&self, title: &str, thread_id: &str, content: &str) -> anyhow::Result<String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.create_for_skill(title, thread_id, content))
        })
    }

    fn list_relationships(&self, doc_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        self.inner.list_relationships(doc_id)
    }

    fn list_backlinks(&self, doc_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        self.inner.list_backlinks(doc_id)
    }

    fn list_all_documents_with_link_counts(&self) -> anyhow::Result<Vec<(String, String, u32, u32)>> {
        self.inner.list_all_documents_with_link_counts()
    }

    /// Finds the thread through the bare store, or creates it through the
    /// document service's one thread-write path, stamped (THREAD-sync S1).
    fn find_or_create_thread(&self, name: &str, description: &str) -> anyhow::Result<String> {
        let found = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.docs.store().find_thread_by_name(name))
        })?;
        if let Some(id) = found.and_then(|t| t.id_string()) {
            return Ok(id);
        }
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.docs.thread_create(name, description))
        })
        .map_err(|e| anyhow::anyhow!("{}", doc_error_message(&e)))
    }
}

// ------------------------------------------------------- raw body on create --

/// A document whose raw body the hook could not preserve because the service
/// was creating it: its row did not exist yet when the hook ran.
struct PendingRawBody {
    doc_id: String,
    key: Arc<AccountKey>,
    since: std::time::Instant,
}

/// How long a pending raw body waits for its row (a create that failed after
/// ingest never inserts it).
const PENDING_RAW_BODY_TTL: std::time::Duration = std::time::Duration::from_secs(600);

/// One store's pending raw bodies. Its lock is held across that store's
/// raw-body writes, so a late flush never replaces the raw body a later save
/// wrote: a save drops its document's pending entry under the same lock before
/// writing its own. It is never held across another store's I/O.
type StorePending = Arc<tokio::sync::Mutex<Vec<PendingRawBody>>>;

/// The pending lists, one per store (BACKLOG §9: a single process-wide list
/// flushed under one lock let a write in one store write another store's
/// pending raw body, and wait forever on a store whose runtime was gone).
/// Keyed by the store's data pointer; the `Weak` keeps no store alive
/// but keeps that address from being reused while the entry exists. Entries
/// of dropped stores are removed on every lookup. This map's own lock is never
/// held across an await.
fn raw_bodies() -> &'static Mutex<HashMap<usize, (Weak<dyn GraphDB>, StorePending)>> {
    static PENDING: std::sync::OnceLock<Mutex<HashMap<usize, (Weak<dyn GraphDB>, StorePending)>>> =
        std::sync::OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A store's identity: its data pointer (not `Arc::ptr_eq` on a `dyn`, whose
/// vtable pointers may differ for the same store).
fn store_key(store: &dyn GraphDB) -> usize {
    store as *const dyn GraphDB as *const () as usize
}

/// The pending map, with the entries of dropped stores removed.
fn live_raw_bodies() -> std::sync::MutexGuard<'static, HashMap<usize, (Weak<dyn GraphDB>, StorePending)>> {
    let mut map = raw_bodies().lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, (weak, _)| weak.strong_count() > 0);
    map
}

/// `store`'s pending list, if it has one.
fn pending_of(store: &dyn GraphDB) -> Option<StorePending> {
    live_raw_bodies().get(&store_key(store)).map(|(_, list)| list.clone())
}

/// `store`'s pending list, created if it has none.
fn pending_or_new(store: &Arc<dyn GraphDB>) -> StorePending {
    let mut map = live_raw_bodies();
    let entry = map.entry(store_key(store.as_ref())).or_insert_with(|| (Arc::downgrade(store), StorePending::default()));
    entry.1.clone()
}

/// Preserves the raw body of every document created in `store` since its last
/// flush (adoption step 2 left this open: "on a create no raw body is
/// preserved"). The raw body is rebuilt from what was stored: the text blocks,
/// with the tokens of the document's own records expanded back to their values
/// — what the hook was given, since a created document's only records are the
/// ones its ingest just made. Called by [`create`], at the start of every
/// ingest, and by hosts after a batch of creates (bulk import). An entry whose
/// row is not there yet waits ([`PENDING_RAW_BODY_TTL`]). Only `store`'s own
/// entries are touched.
pub async fn flush_raw_bodies_of(store: &dyn GraphDB) {
    let Some(list) = pending_of(store) else { return };
    let mut pending = list.lock().await;
    flush_list(store, &mut pending).await;
}

async fn flush_list(store: &dyn GraphDB, pending: &mut Vec<PendingRawBody>) {
    let mut keep = vec![];
    for p in pending.drain(..) {
        match preserve_raw_body(store, &p.key, &p.doc_id).await {
            Ok(()) => {}
            Err(RawBodyError::NoRowYet) if p.since.elapsed() < PENDING_RAW_BODY_TTL => keep.push(p),
            Err(e) => tracing::debug!("document {}: raw body not preserved ({e})", p.doc_id),
        }
    }
    *pending = keep;
}

#[derive(Debug)]
enum RawBodyError {
    NoRowYet,
    Other(String),
}

impl std::fmt::Display for RawBodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RawBodyError::NoRowYet => f.write_str("the document does not exist"),
            RawBodyError::Other(e) => f.write_str(e),
        }
    }
}

/// Writes the raw body of `doc_id` from its stored text (see
/// [`flush_raw_bodies_of`]); the scan time the service recorded is kept.
async fn preserve_raw_body(db: &dyn GraphDB, key: &AccountKey, doc_id: &str) -> Result<(), RawBodyError> {
    let other = |e: String| RawBodyError::Other(e);
    let d = match db.get_document(doc_id).await {
        Ok(d) => d,
        Err(DbError::NotFound(_)) => return Err(RawBodyError::NoRowYet),
        Err(e) => return Err(other(e.to_string())),
    };
    let known = known_records(db, key, doc_id).await.map_err(other)?;
    let raw = expand_tokens(&sovereign_docformat::plain_text_of(&d.content), &known);
    let (enc, nonce) = EncryptedBlob::encrypt_str(&raw, key).map_err(|e| other(e.to_string()))?.into_pair();
    match db.update_document_pii_fields(doc_id, Some(&enc), Some(&nonce), d.pii_scanned_at).await {
        Ok(()) => Ok(()),
        Err(DbError::NotFound(_)) => Err(RawBodyError::NoRowYet),
        Err(e) => Err(other(e.to_string())),
    }
}

/// The discovered records linked to `doc_id`, with their values.
async fn known_records(db: &dyn GraphDB, key: &AccountKey, doc_id: &str) -> Result<Vec<Known>, String> {
    let records = db.list_pii_records(None, None, Some(false)).await.map_err(|e| e.to_string())?;
    Ok(records
        .into_iter()
        .filter(|r| r.sources.iter().any(|s| is_doc(s, doc_id)))
        .filter_map(|r| {
            let id = r.id_string()?;
            let value = decrypt_value(&r, key)?;
            Some(Known { id, kind: r.kind, review_state: r.review_state, value })
        })
        .collect())
}

// -------------------------------------------------------------- small deps --

/// Wall-clock time.
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// SHA-256 of a media file, read from the file system.
pub struct FileMedia;
impl MediaFiles for FileMedia {
    fn sha256(&self, path: &str) -> Option<String> {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(path).ok()?;
        let digest = Sha256::digest(&bytes);
        Some(digest.iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Change events go to the log (no listener consumes them yet: the shell and
/// the app refresh what they show after their own writes).
pub struct LoggedEvents;
impl DocumentEvents for LoggedEvents {
    fn emit(&self, event: DocumentChanged) {
        tracing::debug!("document {} changed: {:?}", event.id, event.kind);
    }
}

/// The hook when no account key is installed: ingest cannot run.
pub struct UnavailableIngest;
#[async_trait]
impl PiiIngest for UnavailableIngest {
    async fn ingest(&self, _request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError> {
        Err(PiiError { message: "personal-data scan unavailable: no account key".into() })
    }
}

// ---------------------------------------------------------------- PII hook --

/// The personal-data hook over the regex ingest (`ingest_text`).
///
/// Per text field it runs the pipeline and returns the field's canonical text
/// (values replaced by `[pii:<record_id>]`). What it adds to `ingest_text`,
/// because the service calls it on every save rather than once per document:
///
/// - **No duplicate records.** A value already recorded for this document (same
///   kind, same review state, same value) gets the existing record's id instead
///   of a new record. Without it, each autosave would add a record for every
///   finding left in the text (low-confidence findings are not replaced by a
///   token, so they are found again), and for every value still shown raw in an
///   editor that has not reloaded the tokenized text.
/// - **The preserved raw body** (`body_raw_*`, read by the "raw original"
///   reveal): the document's text blocks as given, joined like
///   `DocumentView::plain_text`, with the tokens of this document's records
///   expanded back to their values — so the raw original keeps the values
///   tokenized by earlier saves instead of losing them at the second save.
///   Titles, captions and alt texts are not in it (each value found there is
///   still kept, encrypted, in its record). When the document row does not exist
///   yet (a create: the service ingests before it inserts the row) the document
///   is left pending and its raw body is written once the row exists
///   ([`flush_raw_bodies_of`], adoption step 3) — the hook contract has no call
///   after the row is inserted.
///
/// Source spans are offsets into the field's text, not into a flat body
/// (TESTS.md weakness 4: spans are meaningless for v2 anyway).
pub struct RegexDocumentIngest {
    db: Arc<dyn GraphDB>,
    account_key: Arc<AccountKey>,
    config: PipelineConfig,
}

impl RegexDocumentIngest {
    pub fn new(db: Arc<dyn GraphDB>, account_key: Arc<AccountKey>) -> Self {
        // As the Tauri save path did (PII-001): Swiss locale, regex only.
        let config = PipelineConfig { locale: crate::pii::Locale::Swiss, ..PipelineConfig::default() };
        Self { db, account_key, config }
    }

    /// The discovered records linked to `doc_id`, with their values.
    async fn known(&self, doc_id: &str) -> Result<Vec<Known>, String> {
        known_records(self.db.as_ref(), &self.account_key, doc_id).await
    }
}

fn is_doc(s: &SourceRef, doc_id: &str) -> bool {
    s.source_kind == SourceKind::Document && s.source_id == doc_id
}

fn decrypt_value(r: &PiiRecord, key: &AccountKey) -> Option<String> {
    EncryptedBlob::from_pair(r.value_encrypted.clone(), r.value_nonce.clone()).decrypt_to_string(key).ok()
}

/// A recorded value of this document.
#[derive(Clone)]
struct Known {
    id: String,
    kind: PiiKind,
    review_state: ReviewState,
    value: String,
}

#[async_trait]
impl PiiIngest for RegexDocumentIngest {
    async fn ingest(&self, request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError> {
        let fail = |what: &str, e: String| PiiError { message: format!("{what}: {e}") };
        let doc_id = request.document_id.clone();
        // Raw bodies of documents created since the last call (their rows
        // exist now).
        flush_raw_bodies_of(self.db.as_ref()).await;
        let entities = self.db.list_entities().await.map_err(|e| fail("entities", e.to_string()))?;
        let contacts = self.db.list_contacts().await.map_err(|e| fail("contacts", e.to_string()))?;
        let known = self.known(&doc_id).await.map_err(|e| fail("records", e))?;
        let sink = DedupSink {
            db: self.db.clone(),
            doc_id: doc_id.clone(),
            account_key: self.account_key.clone(),
            known: Mutex::new(known),
            reused: Mutex::new(HashSet::new()),
        };

        let mut out = vec![];
        for item in &request.items {
            let result = ingest_text(
                &item.text,
                &doc_id,
                SourceKind::Document,
                &self.config,
                None,
                None,
                &entities,
                &contacts,
                &sink,
                &self.account_key,
            )
            .await
            .map_err(|e| fail("ingest", e.to_string()))?;
            if result.canonical_body != item.text {
                out.push(IngestedItem { field: item.field, text: result.canonical_body });
            }
        }

        // The preserved raw body: the text blocks as given, this document's
        // tokens expanded.
        let known = sink.known.lock().map(|k| k.clone()).unwrap_or_default();
        let raw = request
            .items
            .iter()
            .filter(|i| matches!(i.field, TextField::Text(_)))
            .map(|i| expand_tokens(&i.text, &known))
            .collect::<Vec<_>>()
            .join("\n\n");
        let blob = EncryptedBlob::encrypt_str(&raw, &self.account_key).map_err(|e| fail("raw body", e.to_string()))?;
        let (enc, nonce) = blob.into_pair();
        // Under this store's pending lock: this write supersedes a pending one
        // for the same document (same store, same id), whichever task flushes
        // next.
        let list = pending_or_new(&self.db);
        let mut pending = list.lock().await;
        pending.retain(|p| p.doc_id != doc_id);
        match self.db.update_document_pii_fields(&doc_id, Some(&enc), Some(&nonce), Some(Utc::now())).await {
            Ok(()) => {}
            // A create: the row is inserted after ingest. Preserved once it
            // exists (`flush_raw_bodies_of`).
            Err(DbError::NotFound(_)) => pending.push(PendingRawBody {
                doc_id: doc_id.clone(),
                key: self.account_key.clone(),
                since: std::time::Instant::now(),
            }),
            Err(e) => return Err(fail("raw body", e.to_string())),
        }
        drop(pending);
        Ok(out)
    }
}

/// `text` with each `[pii:<id>]` whose record is in `known` replaced by its
/// value; any other token-form text kept as it is.
fn expand_tokens(text: &str, known: &[Known]) -> String {
    const OPEN: &str = "[pii:";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(']') else { break };
        let id = &after[..end];
        out.push_str(&rest[..start]);
        match known.iter().find(|k| k.id == id) {
            Some(k) => out.push_str(&k.value),
            None => out.push_str(&rest[start..start + OPEN.len() + end + 1]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The sink `ingest_text` writes through: a value already recorded for this
/// document is answered with the existing record instead of a new one.
struct DedupSink {
    db: Arc<dyn GraphDB>,
    doc_id: String,
    account_key: Arc<AccountKey>,
    known: Mutex<Vec<Known>>,
    /// Ids answered from `known`: their sources are merged, never replaced.
    reused: Mutex<HashSet<String>>,
}

#[async_trait]
impl PiiSink for DedupSink {
    async fn create_pii_record(&self, record: PiiRecord) -> anyhow::Result<String> {
        let value = decrypt_value(&record, &self.account_key);
        if let Some(v) = &value {
            let found = self.known.lock().ok().and_then(|k| {
                k.iter()
                    .find(|k| k.kind == record.kind && k.review_state == record.review_state && &k.value == v)
                    .map(|k| k.id.clone())
            });
            if let Some(id) = found {
                if let Ok(mut r) = self.reused.lock() {
                    r.insert(id.clone());
                }
                return Ok(id);
            }
        }
        let kind = record.kind.clone();
        let review_state = record.review_state.clone();
        let created = self.db.create_pii_record(record).await.map_err(|e| anyhow::anyhow!("create_pii_record: {e}"))?;
        let id = created.id_string().ok_or_else(|| anyhow::anyhow!("create_pii_record returned no id"))?;
        if let (Some(value), Ok(mut k)) = (value, self.known.lock()) {
            k.push(Known { id: id.clone(), kind, review_state, value });
        }
        Ok(id)
    }

    async fn update_pii_record_sources(&self, id: &str, sources: Vec<SourceRef>) -> anyhow::Result<()> {
        let reused = self.reused.lock().map(|r| r.contains(id)).unwrap_or(false);
        let sources = if reused {
            // Keep every other document's link (and this one's, already there).
            let rec = self.db.get_pii_record(id).await.map_err(|e| anyhow::anyhow!("get_pii_record: {e}"))?;
            if rec.sources.iter().any(|s| is_doc(s, &self.doc_id)) {
                return Ok(());
            }
            let mut merged = rec.sources;
            merged.extend(sources);
            merged
        } else {
            sources
        };
        self.db
            .update_pii_record_sources(id, sources)
            .await
            .map_err(|e| anyhow::anyhow!("update_pii_record_sources: {e}"))
    }

    async fn create_entity(&self, entity: Entity) -> anyhow::Result<String> {
        let created = self.db.create_entity(entity).await.map_err(|e| anyhow::anyhow!("create_entity: {e}"))?;
        created.id_string().ok_or_else(|| anyhow::anyhow!("create_entity returned no id"))
    }
}

/// For the hosts' integration tests: a real encrypted store over an in-memory
/// SurrealDB (key databases in the temp directory), and an account key.
#[doc(hidden)]
pub mod testing {
    use std::sync::Arc;

    use sovereign_crypto::account_key::AccountKey;
    use sovereign_crypto::device_key::DeviceKey;
    use sovereign_crypto::index_key::IndexKey;
    use sovereign_crypto::kek::Kek;
    use sovereign_crypto::key_db::KeyDatabase;
    use sovereign_crypto::master_key::MasterKey;
    use sovereign_db::encrypted::EncryptedGraphDB;
    use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
    use sovereign_db::traits::GraphDB;

    pub async fn encrypted_memory_store(tag: &str) -> Arc<EncryptedGraphDB> {
        let raw = SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store");
        raw.connect().await.expect("connect");
        raw.init_schema().await.expect("schema");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        // A process-wide counter too: two stores made in the same clock tick
        // with the same tag would share key files and race on their `.tmp`
        // writes (BACKLOG §9, the rule of 2026-10-15).
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir();
        let kdb = |s: &str| {
            Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
                dir.join(format!("docs-host-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
            )))
        };
        let mk = MasterKey::generate();
        let device_key = DeviceKey::derive(&mk, "test-device").expect("device key");
        Arc::new(EncryptedGraphDB::new(
            Arc::new(raw) as Arc<dyn GraphDB>,
            kdb("doc"),
            kdb("msg"),
            kdb("thr"),
            kdb("conv"),
            kdb("con"),
            kdb("shr"),
            Arc::new(Kek::generate()),
            Arc::new(IndexKey::generate()),
            Arc::new(device_key),
        ))
    }

    pub fn account_key() -> Arc<AccountKey> {
        let mk = MasterKey::from_passphrase(b"docs-host-testing", b"salt").expect("master key");
        Arc::new(AccountKey::derive(&mk).expect("account key"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::master_key::MasterKey;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_docformat::BlockId;
    use sovereign_docs::{TokenStatus, WritePath};

    fn key() -> Arc<AccountKey> {
        let mk = MasterKey::from_passphrase(b"docs-host-test", b"salt").unwrap();
        Arc::new(AccountKey::derive(&mk).unwrap())
    }

    fn request(doc: &str, items: Vec<(TextField, &str)>) -> IngestRequest {
        IngestRequest {
            document_id: doc.into(),
            path: WritePath::Save,
            items: items
                .into_iter()
                .map(|(field, text)| sovereign_docs::IngestItem { field, text: text.into() })
                .collect(),
        }
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    async fn records_of(db: &MockGraphDB, doc: &str) -> Vec<PiiRecord> {
        db.list_pii_records(None, None, Some(false))
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.sources.iter().any(|s| is_doc(s, doc)))
            .collect()
    }

    /// Every text field goes through the regex pipeline; a value found in a
    /// title, a text block or a caption comes back as a token.
    #[test]
    fn every_text_field_is_tokenized() {
        rt().block_on(async {
            let db = Arc::new(MockGraphDB::new());
            let hook = RegexDocumentIngest::new(db.clone(), key());
            let b = BlockId::derived("t", 0);
            let answer = hook
                .ingest(request(
                    "document:d1",
                    vec![
                        (TextField::Title, "mail alice@example.com"),
                        (TextField::Text(b), "write to bob@example.org today"),
                        (TextField::Caption(b), "nothing here"),
                    ],
                ))
                .await
                .unwrap();
            assert_eq!(answer.len(), 2, "two fields held a value");
            for a in &answer {
                assert!(a.text.contains("[pii:pii_record:"), "{:?}", a.field);
                assert!(!a.text.contains("@example."), "{:?}", a.field);
            }
            assert_eq!(records_of(&db, "document:d1").await.len(), 2);
        });
    }

    /// The service calls the hook on every save: the same value saved again
    /// (an editor still showing it raw) gets the same record, not a new one.
    #[test]
    fn saving_the_same_value_again_adds_no_record() {
        rt().block_on(async {
            let db = Arc::new(MockGraphDB::new());
            let hook = RegexDocumentIngest::new(db.clone(), key());
            let b = BlockId::derived("t", 0);
            let first = hook
                .ingest(request("document:d1", vec![(TextField::Text(b), "mail alice@example.com")]))
                .await
                .unwrap();
            let second = hook
                .ingest(request("document:d1", vec![(TextField::Text(b), "mail alice@example.com, again")]))
                .await
                .unwrap();
            assert_eq!(records_of(&db, "document:d1").await.len(), 1);
            let token = |t: &str| t[t.find("[pii:").unwrap()..].split(']').next().unwrap().to_string();
            assert_eq!(token(&first[0].text), token(&second[0].text));
        });
    }

    /// A value already recorded for another document is not merged into it:
    /// records are per document here (the paste path links across documents).
    #[test]
    fn another_documents_record_is_not_reused() {
        rt().block_on(async {
            let db = Arc::new(MockGraphDB::new());
            let hook = RegexDocumentIngest::new(db.clone(), key());
            let b = BlockId::derived("t", 0);
            hook.ingest(request("document:a", vec![(TextField::Text(b), "alice@example.com")])).await.unwrap();
            hook.ingest(request("document:b", vec![(TextField::Text(b), "alice@example.com")])).await.unwrap();
            assert_eq!(records_of(&db, "document:a").await.len(), 1);
            assert_eq!(records_of(&db, "document:b").await.len(), 1);
        });
    }

    #[test]
    fn tokens_of_known_records_expand_and_others_stay() {
        let known = vec![Known {
            id: "pii_record:x".into(),
            kind: PiiKind::Email,
            review_state: ReviewState::Confirmed,
            value: "a@b.example".into(),
        }];
        assert_eq!(
            expand_tokens("to [pii:pii_record:x] and [pii:pii_record:y] [pii:", &known),
            "to a@b.example and [pii:pii_record:y] [pii:"
        );
    }

    // ------------------------------------------------- adoption step 3 --

    /// A service over an encrypted in-memory store with the regex hook.
    async fn service(tag: &str) -> (Arc<EncryptedGraphDB>, Arc<DocumentService>, Arc<AccountKey>) {
        let store = testing::encrypted_memory_store(tag).await;
        let ak = testing::account_key();
        let docs = Arc::new(build_document_service(store.clone(), Some(ak.clone()), "test".into()));
        (store, docs, ak)
    }

    /// The decrypted raw body of `id`, if one is preserved.
    async fn raw_body(store: &EncryptedGraphDB, ak: &AccountKey, id: &str) -> Option<String> {
        let d = store.get_document(id).await.unwrap();
        let blob = EncryptedBlob::from_pair(d.body_raw_encrypted?, d.body_raw_nonce?);
        Some(blob.decrypt_to_string(ak).unwrap())
    }

    /// Step 2 left this open: on a create the hook runs before the row
    /// exists, so no raw body was preserved. Now a created document has its
    /// raw body — the text as given, values inline — once `create` returns.
    #[tokio::test]
    async fn a_created_document_keeps_its_raw_body() {
        let (store, docs, ak) = service("create-raw").await;
        let new = new_user_document("Note", "thread:t", "write to alice@example.com today");
        let out = create(&docs, new).await.unwrap();
        assert_eq!(out.pii, sovereign_docs::PiiOutcome::Scanned);
        let view = docs.read(&out.id).await.unwrap();
        assert!(!view.plain_text().contains("alice@example.com"), "tokenized");
        assert!(matches!(view.tokens[0].status, TokenStatus::Token { .. }));
        assert_eq!(raw_body(&store, &ak, &out.id).await.as_deref(), Some("write to alice@example.com today"));
    }

    /// A document created straight through the service (bulk import) gets
    /// its raw body at the next ingest, or at an explicit flush.
    #[tokio::test]
    async fn a_raw_body_left_pending_is_written_by_the_next_flush() {
        let (store, docs, ak) = service("pending-raw").await;
        let new = new_user_document("Note", "thread:t", "call bob@example.org");
        let out = docs.create(new).await.unwrap();
        flush_raw_bodies_of(store.as_ref()).await;
        assert_eq!(raw_body(&store, &ak, &out.id).await.as_deref(), Some("call bob@example.org"));
    }

    /// A later save's raw body is not replaced by an older pending one.
    #[tokio::test]
    async fn a_save_supersedes_a_pending_raw_body() {
        let (store, docs, ak) = service("supersede-raw").await;
        let out = docs.create(new_user_document("Note", "thread:t", "first carol@example.net")).await.unwrap();
        let base = docs.read(&out.id).await.unwrap().revision;
        let edit = Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText("second dave@example.net".into())),
            origin: sovereign_docs::Origin::Assistant,
            explicit: false,
            pasted_tokens: vec![],
        };
        docs.save(&out.id, edit).await.unwrap();
        flush_raw_bodies_of(store.as_ref()).await;
        assert_eq!(raw_body(&store, &ak, &out.id).await.as_deref(), Some("second dave@example.net"));
    }

    /// A copy is the service's duplicate: its own document with its own
    /// version, the source's trust, provenance *copy of*; an adopted web page's
    /// copy stays owned (her decision made it hers, G20).
    #[tokio::test]
    async fn duplicate_copies_content_and_trust_and_records_the_copy() {
        let (_store, docs, _) = service("dup").await;
        let src = create(&docs, new_web_document("Page", "thread:t", "https://example.org/a", "Some text"))
            .await
            .unwrap();
        let copy = duplicate(&docs, &src.id, "Copy of Page".into(), "thread:u".into(), Origin::Skill { name: "d".into() })
            .await
            .unwrap();
        assert_ne!(copy.id, src.id);
        let view = docs.read(&copy.id).await.unwrap();
        assert_eq!((view.title.as_str(), view.thread_id.as_str()), ("Copy of Page", "thread:u"));
        assert_eq!(view.plain_text(), docs.read(&src.id).await.unwrap().plain_text());
        assert_eq!(view.trust, Trust::External);
        let web = Provenance::Web { url: "https://example.org/a".into() };
        assert_eq!(view.provenance, Provenance::Copy { of: src.id.clone(), from: Box::new(web) });
        assert_eq!(docs.versions(&copy.id).await.unwrap().len(), 1);

        docs.set_trust(&src.id, Trust::Owned).await.unwrap();
        let copy = duplicate(&docs, &src.id, "Again".into(), "thread:t".into(), Origin::Skill { name: "d".into() })
            .await
            .unwrap();
        assert_eq!(docs.read(&copy.id).await.unwrap().trust, Trust::Owned);
    }

    /// Weakness 10, closed by G20: a document with tokens is copied, and its
    /// tokens stay tokens in the copy.
    #[tokio::test]
    async fn duplicate_keeps_tokens_linked() {
        let (_store, docs, _) = service("dup-tokens").await;
        let src = create(&docs, new_user_document("Card", "thread:t", "mail erin@example.com")).await.unwrap();
        let copy = duplicate(&docs, &src.id, "Copy".into(), "thread:t".into(), Origin::Skill { name: "d".into() })
            .await
            .unwrap();
        let view = docs.read(&copy.id).await.unwrap();
        assert!(!view.plain_text().contains("erin@example.com"));
        assert!(!view.tokens.is_empty());
        assert!(view.tokens.iter().all(|t| matches!(t.status, TokenStatus::Token { .. })), "{:?}", view.tokens);
    }

    /// What a skill creates goes through the service: the journal is the
    /// user's own writing (owned, written); a file a skill imports is
    /// undetermined, provenance import from the skill; a document whose
    /// content is one the skill read is a copy of it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_skill_creates_through_the_service() {
        use sovereign_skills::SkillDbAccess;
        let (store, docs, _) = service("skill-create").await;
        let inner = sovereign_skills::wrap_db(store.clone());
        let db = ServiceSkillDb::new(inner, docs.clone(), "daily-journal");

        let id = db.create_document("2026-10-14", "thread:j", "# 2026-10-14\n\n").unwrap();
        let view = docs.read(&id).await.unwrap();
        assert_eq!(view.trust, Trust::Owned, "a journal entry is her own writing");
        assert_eq!(view.provenance, Provenance::Written);
        assert_eq!(view.plain_text().trim(), "# 2026-10-14");
        assert_eq!(docs.versions(&id).await.unwrap().len(), 1);

        let importer = ServiceSkillDb::new(sovereign_skills::wrap_db(store.clone()), docs.clone(), "file-import");
        let id = importer.create_document("notes.txt", "", "text from a file").unwrap();
        let view = docs.read(&id).await.unwrap();
        assert_eq!(view.trust, Trust::Undetermined, "a file a skill imports is not owned (G23)");
        assert_eq!(view.provenance, Provenance::Import { source_path: "skill:file-import".into() });

        let owned = create(&docs, new_user_document("Mine", "thread:t", "my words")).await.unwrap();
        let (title, thread, content) = db.get_document(&owned.id).unwrap();
        let copy = db.create_document(&format!("Copy of {title}"), &thread, &content).unwrap();
        let view = docs.read(&copy).await.unwrap();
        assert_eq!(view.trust, Trust::Owned, "a copy keeps the source's trust");
        assert_eq!(view.provenance, Provenance::Copy { of: owned.id.clone(), from: Box::new(Provenance::Written) });
        assert_eq!(view.plain_text(), "my words");
    }
}
