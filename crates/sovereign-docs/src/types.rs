//! What goes in and out of the document service (spec ch. 03, *Document
//! service — contract (C-DOC)*). Plain data, the view's text accessors, and
//! `Debug` forms that print sizes, never text.

use std::fmt;

use chrono::{DateTime, Utc};
use sovereign_docformat::{BlockId, DocumentV2, SourceForm};

/// A document id, `document:<key>` (spec ch. 03, *Record identity*).
pub type DocId = String;

// ------------------------------------------------------------------ trust --

/// A document's trust state (spec ch. 04, *Ownership is not decided at
/// landing*, decided 2026-10-10). Set at creation; changed only by
/// [`crate::DocumentService::set_trust`] — never by a save, a skill, a
/// migration, a restore or sync of an existing document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trust {
    /// The user's own content: control plane, not injection-fenced.
    Owned,
    /// External content: data plane, fenced.
    External,
    /// Not yet judged by the user; **treated as external** (fenced).
    Undetermined,
}

impl Trust {
    /// True for `External` and `Undetermined` (undetermined fails closed).
    pub fn is_fenced(self) -> bool {
        !matches!(self, Trust::Owned)
    }
}

/// A trust change the user made explicitly (`set_trust`), on which device and
/// when. It travels with the document to her other devices, which apply it in
/// either direction (decided by Céline, 2026-10-11: trust across devices
/// follows who decided, not the direction). Sync without such a record never
/// changes trust.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustDecision {
    pub trust: Trust,
    /// The paired device on which the user decided.
    pub device: String,
    pub decided_at: DateTime<Utc>,
}

/// Where a document's content came from (spec ch. 04). Recorded at creation,
/// travels with sync, never changed afterwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Written here by the user.
    Written,
    /// Written by the assistant (orchestrator write tools). **[choice]**
    Assistant,
    /// Saved from the web.
    Web { url: String },
    /// Brought in by bulk import.
    Import { source_path: String },
    /// Received from one of the user's paired devices, which created it.
    Peer { device: String },
    /// A document created before provenance was recorded (every row written
    /// before the service exists). Never assigned to a new document.
    /// **[choice]**
    Unrecorded,
    /// A copy made by [`crate::DocumentService::duplicate`] (G20, specified
    /// 2026-10-15): the document copied, and where *its* content came from —
    /// the source's provenance as it was at the copy (itself possibly a
    /// copy). Copying changes neither where the content came from nor whether
    /// the user judged it hers. **[choice]**
    Copy { of: DocId, from: Box<Provenance> },
}

// ---------------------------------------------------------------- origins --

/// One editing session of one document in one editor window. Saves of the same
/// session may replace each other without a version in between (the commit
/// policy decides); a save from anything else may not (restore-point rule).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EditSession(pub u64);

/// Who is writing. Sync, restore and migration have their own operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The user's editor (autosave or explicit save).
    Editor { session: EditSession },
    /// The assistant's write tools.
    Assistant,
    /// A skill, by name.
    Skill { name: String },
    /// Bulk import.
    Import,
}

/// An opaque token naming the state a writer read. A save carries the
/// revision it was based on; a save based on anything but the current
/// revision is refused ([`DocError::Conflict`]) — two writers never silently
/// replace each other.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Revision(pub String);

impl fmt::Debug for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Revision({})", self.0)
    }
}

// ------------------------------------------------------------------ edits --

/// New content for a save.
#[derive(Clone)]
pub enum EditContent {
    /// The whole document in its stored v2 form — what the editor saves
    /// (`DocumentV2::to_json_string`). Refused if not valid v2.
    V2(String),
    /// New text for some `text` blocks, by id; every other block unchanged.
    /// Refused if an id is not a text block of the document.
    TextBlocks(Vec<(BlockId, String)>),
    /// The document's text as one string (the assistant, a skill written for
    /// v1 bodies). Accepted only when the document has **at most one** `text`
    /// block: it replaces that block (or becomes the first block when there is
    /// none). Refused with [`DocError::AmbiguousText`] otherwise — a text-only
    /// writer never collapses a structured document. Non-text blocks are kept.
    PlainText(String),
}

/// A personal-data token that entered the document through a paste of a
/// genuine Sovereign clipboard payload (spec ch. 12, C-TEXT, *Clipboard*):
/// the record and the document it was copied from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastedToken {
    pub record_id: String,
    pub from_document: DocId,
}

/// One save.
#[derive(Clone)]
pub struct Edit {
    /// The revision the writer read.
    pub base: Revision,
    /// A new title, if it changed.
    pub title: Option<String>,
    /// New content, if it changed.
    pub content: Option<EditContent>,
    pub origin: Origin,
    /// The user asked for a version now (Ctrl+S).
    pub explicit: bool,
    /// Tokens pasted from another Sovereign document. Honoured only for
    /// [`Origin::Editor`]; see [`crate::DocumentService::save`].
    pub pasted_tokens: Vec<PastedToken>,
}

/// Content of a new document.
#[derive(Clone)]
pub enum NewContent {
    /// Markdown text: one `text` block.
    Text(String),
    /// A whole document in stored v2 form.
    V2(String),
}

/// A document to create.
#[derive(Clone)]
pub struct NewDocument {
    pub title: String,
    pub thread_id: String,
    pub content: NewContent,
    pub trust: Trust,
    pub provenance: Provenance,
    pub origin: Origin,
    /// Original times (import). `None` = the clock's now.
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
}

/// The winning state of a document as decided by sync (ch. 05), to be applied
/// here. The service does not decide who wins; it applies without losing
/// anything.
#[derive(Clone)]
pub struct RemoteDocument {
    pub id: DocId,
    pub title: String,
    /// The content exactly as the peer stores it (any stored form).
    pub content: String,
    pub thread_id: String,
    /// The sender's current trust. `None` when the peer predates the field
    /// (ch. 05: fields added with defaults). Used only when the document is
    /// created here; it never changes the trust of an existing document.
    pub trust: Option<Trust>,
    /// The sender's last explicit trust decision, if any (wire field to add
    /// with a default — open in C-WIRE). The only thing in sync that changes
    /// the trust of an existing document, in either direction; the later
    /// decision wins.
    pub trust_decision: Option<TrustDecision>,
    pub provenance: Option<Provenance>,
    pub modified_at: DateTime<Utc>,
    /// `Some` = the peer holds the document in its trash (a tombstone).
    pub deleted_at: Option<DateTime<Utc>>,
    /// The paired device the state came from.
    pub device: String,
}

/// What applying a remote state did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteOutcome {
    Created,
    Applied,
    Trashed,
    RestoredFromTrash,
    Unchanged,
    /// The document was purged here; the remote state is not applied.
    IgnoredPurged,
}

// ------------------------------------------------------------------ views --

/// Whether a document can be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Writable {
    Yes,
    ReadOnly(ReadOnlyReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// Written in a newer format version: shown, never rewritten.
    NewerFormat { v: u64 },
    /// Claims to be a Sovereign document but cannot be read: never rewritten.
    Unreadable { message: String },
}

/// How text in token form `[pii:<record_id>]` is to be treated in one
/// document (spec ch. 09, *Where a token resolves*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenStatus {
    /// A real token here: shown as its label (e.g. `[Email]`), atomic.
    Token { label: String },
    /// Linked here, but its record is deleted or cannot be read: a placeholder.
    Placeholder,
    /// Not linked to this document (or no such record): ordinary text.
    NotAToken,
}

/// A piece of a document that holds text: what personal-data ingest receives
/// (decided 2026-10-11: titles, captions and alt texts too) and where a token
/// can occur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextField {
    Title,
    /// The text of a `text` block.
    Text(BlockId),
    /// The caption of an `image` or `video` block.
    Caption(BlockId),
    /// The alt text of an `image` block.
    Alt(BlockId),
}

/// One occurrence of token-form text in the title, a `text` block, a caption
/// or an alt text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenOccurrence {
    pub field: TextField,
    pub record_id: String,
    pub status: TokenStatus,
}

/// What a reader gets instead of `content`: the typed view. No caller parses
/// stored content.
pub struct DocumentView {
    pub id: DocId,
    pub title: String,
    pub thread_id: String,
    pub trust: Trust,
    /// The last explicit user decision on trust, here or on a paired device;
    /// `None` when trust is still what creation set.
    pub trust_decision: Option<TrustDecision>,
    pub provenance: Provenance,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    /// `Some` while the document is in the trash.
    pub trashed_at: Option<DateTime<Utc>>,
    pub revision: Revision,
    /// The form found in storage (V2 once migrated).
    pub stored_form: StoredForm,
    pub writable: Writable,
    /// The blocks. `None` only when the stored content is unreadable.
    pub doc: Option<DocumentV2>,
    /// Every token-form occurrence, with what it is in this document.
    pub tokens: Vec<TokenOccurrence>,
    /// The current text has been through personal-data ingest.
    pub pii_scanned: bool,
    /// A paired device's change awaits review (ch. 05).
    pub peer_review_pending: bool,
}

/// The stored form a read found (mirrors [`SourceForm`], plus unreadable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoredForm {
    Read(SourceForm),
    Unreadable,
}

impl DocumentView {
    /// For search, the assistant and skills: the text blocks joined by one
    /// blank line, tokens in their canonical form `[pii:<id>]` (ch. 09: the
    /// model always receives the canonical form). Empty when unreadable.
    pub fn plain_text(&self) -> String {
        self.doc.as_ref().map(DocumentV2::plain_text).unwrap_or_default()
    }

    /// For display without the editor: as [`plain_text`](Self::plain_text),
    /// with each occurrence whose status is `Token` replaced by its label and
    /// each `Placeholder` by a placeholder; `NotAToken` occurrences stay as
    /// typed.
    pub fn labelled_text(&self) -> String {
        let Some(doc) = &self.doc else { return String::new() };
        let mut out = String::new();
        let mut first = true;
        for b in doc.blocks() {
            let Some(text) = b.text() else { continue };
            if !first {
                out.push_str("\n\n");
            }
            first = false;
            out.push_str(&self.labelled(TextField::Text(b.id()), text));
        }
        out
    }

    /// The title with the same treatment as [`labelled_text`](Self::labelled_text)
    /// (titles go through ingest, so they can hold tokens). `title` holds the
    /// canonical form.
    pub fn labelled_title(&self) -> String {
        self.labelled(TextField::Title, &self.title)
    }

    /// `text` (the content of `field`) with each occurrence the view reports
    /// as a token replaced by its label, and each placeholder by
    /// [`PLACEHOLDER`]; other token-form text stays as typed.
    fn labelled(&self, field: TextField, text: &str) -> String {
        let status = |rid: &str| {
            self.tokens.iter().find(|t| t.field == field && t.record_id == rid).map(|t| &t.status)
        };
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for (span, rid) in token_spans(text) {
            let replacement = match status(rid) {
                Some(TokenStatus::Token { label }) => label.as_str(),
                Some(TokenStatus::Placeholder) => PLACEHOLDER,
                _ => continue,
            };
            out.push_str(&text[last..span.start]);
            out.push_str(replacement);
            last = span.end;
        }
        out.push_str(&text[last..]);
        out
    }
}

/// What a token whose record is deleted or unreadable shows as, where it is
/// linked (spec ch. 09, *Where a token resolves*).
pub const PLACEHOLDER: &str = "[removed]";

/// The occurrences of token-form text `[pii:<record_id>]` in `text`: each
/// one's byte span and record id. A record id is non-empty and holds no
/// whitespace and no brackets.
pub(crate) fn token_spans(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    const OPEN: &str = "[pii:";
    let mut out = vec![];
    let mut from = 0;
    while let Some(rel) = text[from..].find(OPEN) {
        let start = from + rel;
        let id_start = start + OPEN.len();
        let rest = &text[id_start..];
        let end = rest.find(|c: char| c == ']' || c == '[' || c.is_whitespace());
        match end {
            Some(e) if e > 0 && rest[e..].starts_with(']') => {
                out.push((start..id_start + e + 1, &rest[..e]));
                from = id_start + e + 1;
            }
            _ => from = id_start,
        }
    }
    out
}

/// One line of a document list.
#[derive(Clone)]
pub struct DocumentSummary {
    pub id: DocId,
    pub title: String,
    pub thread_id: String,
    pub trust: Trust,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

/// One line of the trash.
#[derive(Clone)]
pub struct TrashEntry {
    pub id: DocId,
    pub title: String,
    pub trashed_at: DateTime<Utc>,
    /// The thread the document was in when trashed (C-DOC adoption step 5,
    /// S5-5: the trash view shows its name, or the fallback lane's if the
    /// thread is gone). Additive.
    pub thread_id: String,
}

/// A commit's integrity as checked on read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacState {
    Verified,
    /// A legacy commit made before commits were MAC'd.
    Unsigned,
    /// The MAC does not verify: shown, never restored.
    Invalid,
}

/// One version in a document's history, newest first.
#[derive(Clone, Debug)]
pub struct VersionInfo {
    pub commit_id: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub mac: MacState,
}

/// A version's content, for preview.
pub struct VersionView {
    pub commit_id: String,
    pub title: String,
    /// `None` when the snapshot's content is unreadable.
    pub doc: Option<DocumentV2>,
}

// --------------------------------------------------------------- outcomes --

/// What personal-data ingest did on a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PiiOutcome {
    /// The hook ran; the stored text is its canonical output.
    Scanned,
    /// The hook failed; the content was stored as given and the document is
    /// marked not scanned (the sweep and read-time masking cover it, ch. 09).
    /// Decided by Céline, 2026-10-11.
    Failed { message: String },
    /// No text field was written (identical save, a newer-format document
    /// stored verbatim).
    NotRun,
}

/// Result of a save, a skill output, a restore.
#[derive(Clone, Debug)]
pub struct SaveOutcome {
    /// The revision now current.
    pub revision: Revision,
    /// False when the save was identical to the stored state (nothing written).
    pub changed: bool,
    /// The commit made of the new state, if one was made.
    pub version: Option<String>,
    pub pii: PiiOutcome,
}

/// Result of a create.
#[derive(Clone, Debug)]
pub struct CreateOutcome {
    pub id: DocId,
    pub revision: Revision,
    /// The initial commit: every document has a version from its creation.
    pub version: String,
    pub pii: PiiOutcome,
}

/// What a purge removed. Counts are for the documents listed.
#[derive(Clone, Debug, Default)]
pub struct PurgeReport {
    pub documents: Vec<DocId>,
    pub commits: usize,
    pub relationships: usize,
    pub suggested_links: usize,
    /// Source references removed from personal-data records.
    pub pii_sources: usize,
    /// Discovered personal-data records removed because their last source
    /// was purged.
    pub pii_records: usize,
}

/// What the one-time migration did.
#[derive(Clone, Debug, Default)]
pub struct MigrationReport {
    pub migrated: Vec<DocId>,
    pub already_v2: usize,
    /// Newer-format or unreadable documents, left untouched.
    pub read_only: Vec<DocId>,
    /// The unreadable ones among `read_only`: content that claims to be a
    /// Sovereign document and cannot be read (a newer format is not counted
    /// here). Adoption step 6 tells the person how many (M3).
    pub unreadable: Vec<DocId>,
    /// The documents found in format v2 that an earlier run of the migration
    /// converted and nothing has written since (their `last_writer` is still
    /// the migration's): what an interrupted run converted, counted by the
    /// run that finishes (adoption step 6, M3). Included in `already_v2`.
    pub previously_migrated: Vec<DocId>,
    pub failed: Vec<(DocId, String)>,
}

// ----------------------------------------------------------------- events --

/// What changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Created,
    Saved,
    Migrated,
    Trashed,
    RestoredFromTrash,
    Purged,
    VersionRestored,
    TrustChanged,
    RemoteApplied,
}

/// Emitted once per completed change, after it is stored; never for a change
/// that failed or did nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentChanged {
    pub id: DocId,
    pub kind: ChangeKind,
}

// -------------------------------------------- duplicate (G20, 2026-10-15) --

/// A copy to make of a document (spec ch. 03, C-DOC G20).
#[derive(Clone)]
pub struct Duplicate {
    pub source: DocId,
    /// `None` = the source's title.
    pub title: Option<String>,
    /// `None` = the source's thread.
    pub thread_id: Option<String>,
    /// Who asks for the copy (the user's editor, the assistant, a skill).
    pub origin: Origin,
}

impl fmt::Debug for Duplicate {
    /// The title's size, never the title (G17).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Duplicate")
            .field("source", &self.source)
            .field("title_bytes", &self.title.as_ref().map(String::len))
            .field("thread_id", &self.thread_id)
            .field("origin", &self.origin)
            .finish()
    }
}

// ---------------------------------- adoption gestures (G21, G22, 2026-10-15) --

/// Which documents an adoption gesture is about (spec ch. 04, *Decided
/// 2026-10-15*). Always resolved to a list of documents *now*: never a
/// standing rule. Only an explicit selection (`Documents`) can name an
/// external document; every other gesture takes undetermined documents only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdoptionSelector {
    /// One document, or a selection made in bulk.
    Documents(Vec<DocId>),
    /// Every document of a thread.
    Thread(String),
    /// Every document of one import: provenance `Import { source_path }`
    /// whose path lies inside this folder (path components compared after
    /// turning `\` into `/` and dropping a trailing separator). Copies of
    /// imported documents are not part of the import. **[choice]**
    ImportFolder(String),
    /// Every **undetermined** document whose original creation date is before
    /// this instant.
    CreatedBefore(DateTime<Utc>),
    /// Every **undetermined** document whose last modification is before this
    /// instant.
    ModifiedBefore(DateTime<Utc>),
}

/// Something the preview must tell the user before she applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewWarning {
    /// "A date is not proof of authorship": a file's dates come from wherever
    /// it came from (ch. 04). Carried by every date gesture's preview.
    DateIsNotAuthorship,
}

/// One document a gesture would adopt.
#[derive(Clone)]
pub struct AdoptionCandidate {
    pub id: DocId,
    pub title: String,
    /// Its trust now (undetermined or external).
    pub trust: Trust,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl fmt::Debug for AdoptionCandidate {
    /// The title's size, never the title (G17).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdoptionCandidate")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("trust", &self.trust)
            .finish()
    }
}

/// What a gesture would change, computed without changing anything. Applying
/// it adopts these documents and no others.
#[derive(Clone, Debug)]
pub struct AdoptionPreview {
    pub selector: AdoptionSelector,
    pub candidates: Vec<AdoptionCandidate>,
    pub warnings: Vec<PreviewWarning>,
}

/// Why a previewed document was not adopted when the preview was applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdoptionSkip {
    /// Owned by the time of the apply (adopted meanwhile, here or on a paired
    /// device).
    AlreadyOwned,
    InTrash,
    Purged,
    NotFound,
}

/// What applying a preview did.
#[derive(Clone, Debug, Default)]
pub struct AdoptionReport {
    pub adopted: Vec<DocId>,
    pub skipped: Vec<(DocId, AdoptionSkip)>,
}

// ----------------------------------------------------------------- errors --

#[derive(Debug, thiserror::Error)]
pub enum DocError {
    #[error("no such document: {0}")]
    NotFound(DocId),
    #[error("document is in the trash: {0}")]
    InTrash(DocId),
    #[error("document is not in the trash: {0}")]
    NotInTrash(DocId),
    #[error("document was purged: {0}")]
    Purged(DocId),
    #[error("document is read-only: {id}")]
    ReadOnly { id: DocId, reason: ReadOnlyReason },
    #[error("document changed since it was read: {id}")]
    Conflict { id: DocId, current: Revision },
    #[error("content is not a valid document: {0}")]
    InvalidContent(String),
    #[error("text-only write to a document with {text_blocks} text blocks")]
    AmbiguousText { text_blocks: usize },
    #[error("no text block {0:?}")]
    NoSuchTextBlock(BlockId),
    #[error("trust {trust:?} is not allowed for this provenance")]
    TrustNotAllowed { trust: Trust },
    #[error("version {commit_id} refused: {reason}")]
    VersionRefused { commit_id: String, reason: String },
    #[error("storage: {0}")]
    Store(String),
    /// A resolution of a document whose thread was deleted (S5-12, W4) that
    /// was refused, nothing changed; the text says why, for the notice (no
    /// document text in it).
    #[error("{0}")]
    OrphanRefused(String),
}

// ------------------------------------------------- Debug without the text --
//
// Types that carry document text print sizes, never the text (spec ch. 12,
// C-TEXT g3: a `{:?}` in a log line is a log line). Guarantee G17.

impl fmt::Debug for EditContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditContent::V2(s) => f.debug_struct("V2").field("bytes", &s.len()).finish(),
            EditContent::TextBlocks(list) => f
                .debug_struct("TextBlocks")
                .field("blocks", &list.iter().map(|(id, t)| (id.to_hex(), t.len())).collect::<Vec<_>>())
                .finish(),
            EditContent::PlainText(s) => f.debug_struct("PlainText").field("bytes", &s.len()).finish(),
        }
    }
}

impl fmt::Debug for NewContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NewContent::Text(s) => f.debug_struct("Text").field("bytes", &s.len()).finish(),
            NewContent::V2(s) => f.debug_struct("V2").field("bytes", &s.len()).finish(),
        }
    }
}

impl fmt::Debug for Edit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edit")
            .field("base", &self.base)
            .field("title_bytes", &self.title.as_ref().map(String::len))
            .field("content", &self.content)
            .field("origin", &self.origin)
            .field("explicit", &self.explicit)
            .field("pasted_tokens", &self.pasted_tokens.len())
            .finish()
    }
}

impl fmt::Debug for NewDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewDocument")
            .field("title_bytes", &self.title.len())
            .field("thread_id", &self.thread_id)
            .field("content", &self.content)
            .field("trust", &self.trust)
            .field("origin", &self.origin)
            .field("created_at", &self.created_at)
            .field("modified_at", &self.modified_at)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for RemoteDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteDocument")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("content_bytes", &self.content.len())
            .field("thread_id", &self.thread_id)
            .field("trust", &self.trust)
            .field("trust_decision", &self.trust_decision)
            .field("modified_at", &self.modified_at)
            .field("deleted_at", &self.deleted_at)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for DocumentView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentView")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("thread_id", &self.thread_id)
            .field("trust", &self.trust)
            .field("revision", &self.revision)
            .field("stored_form", &self.stored_form)
            .field("writable", &self.writable)
            .field("doc", &self.doc)
            .field("tokens", &self.tokens.len())
            .field("trashed_at", &self.trashed_at)
            .field("pii_scanned", &self.pii_scanned)
            .field("peer_review_pending", &self.peer_review_pending)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for VersionView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VersionView")
            .field("commit_id", &self.commit_id)
            .field("title_bytes", &self.title.len())
            .field("doc", &self.doc)
            .finish()
    }
}

impl fmt::Debug for DocumentSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentSummary")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("thread_id", &self.thread_id)
            .field("trust", &self.trust)
            .field("created_at", &self.created_at)
            .field("modified_at", &self.modified_at)
            .finish()
    }
}

impl fmt::Debug for TrashEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrashEntry")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("trashed_at", &self.trashed_at)
            .field("thread_id", &self.thread_id)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::token_spans;

    #[test]
    fn token_spans_find_record_ids_with_colons() {
        let t = "a [pii:pii_record:x1] b [pii:] c [pii:pii_record:y2]";
        let found: Vec<&str> = token_spans(t).into_iter().map(|(_, r)| r).collect();
        assert_eq!(found, vec!["pii_record:x1", "pii_record:y2"]);
        let (span, _) = &token_spans(t)[0];
        assert_eq!(&t[span.clone()], "[pii:pii_record:x1]");
    }
}
