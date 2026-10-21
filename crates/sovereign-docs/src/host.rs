//! What the document service is given, and nothing else (spec ch. 03,
//! *Document service — contract (C-DOC)*, *What it is given*).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::encrypted::EncryptedGraphDB;

use crate::types::{DocId, DocumentChanged, Origin, TextField};
use sovereign_docformat::BlockId;

/// Everything the service is given.
pub struct ServiceDeps {
    /// The persona's store, **through the encrypted layer**. The type is the
    /// guarantee that every commit the service makes is MAC'd and every body it
    /// writes is sealed (ch. 07): there is no constructor over a plain store.
    pub store: Arc<EncryptedGraphDB>,
    pub commit_policy: Arc<dyn CommitPolicy>,
    pub pii: Arc<dyn PiiIngest>,
    pub media: Arc<dyn MediaFiles>,
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn DocumentEvents>,
    /// This device's name, recorded on the user's trust decisions
    /// ([`crate::TrustDecision`]).
    pub device: String,
}

// ---------------------------------------------------------------- clock --

/// Wall-clock time, given so tests are deterministic. Sets `created_at` /
/// `modified_at` of user writes and decides when the trash expires. **It never
/// decides when a version is made** (decided 2026-10-11).
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

// --------------------------------------------------------------- events --

/// Where [`DocumentChanged`] goes (the shell, mobile, sync's change feed).
pub trait DocumentEvents: Send + Sync {
    fn emit(&self, event: DocumentChanged);
}

// ---------------------------------------------------------- commit policy --

/// What the policy is asked after a save from an editor session. There is no
/// time and no save count in it: versions are made by amount of change, never
/// by time (decided by Céline, 2026-10-11).
#[derive(Clone, Debug)]
pub struct CommitContext {
    pub document_id: DocId,
    pub origin: Origin,
    /// The user asked for a version (Ctrl+S).
    pub explicit: bool,
    /// The amount of change between the document's last version and the state
    /// just saved, measured by [`change_amount`] (computed by the service).
    pub change_since_version: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitDecision {
    /// Make a version of the state just saved.
    Version,
    /// Not yet.
    NotYet,
}

/// When an editor save becomes a version. The policy only *adds* versions:
/// the restore-point rule, the initial version, explicit saves, the end of a
/// session and non-editor writes make versions whatever it answers. Non-editor
/// writes (assistant, skill, import, sync, restore, migration) are not asked.
pub trait CommitPolicy: Send + Sync {
    fn decide(&self, ctx: &CommitContext) -> CommitDecision;
}

/// The shipped policy (spec ch. 03, *Auto-commit policy*, decided 2026-10-11):
/// a version when the amount of change since the last version reaches
/// `threshold`; an explicit save always. Nothing else — not time, not the
/// number of saves.
#[derive(Clone, Copy, Debug)]
pub struct DefaultCommitPolicy {
    /// Change units (see [`change_amount`]) that make a version.
    pub threshold: u64,
}

impl DefaultCommitPolicy {
    /// **To calibrate** (Céline, 2026-10-11: "to be evaluated"). A starting
    /// point, not a decision: roughly a short paragraph of typing.
    pub const DEFAULT_THRESHOLD: u64 = 500;

    pub fn new(threshold: u64) -> Self {
        Self { threshold }
    }
}

impl Default for DefaultCommitPolicy {
    fn default() -> Self {
        Self::new(Self::DEFAULT_THRESHOLD)
    }
}

impl CommitPolicy for DefaultCommitPolicy {
    fn decide(&self, ctx: &CommitContext) -> CommitDecision {
        if ctx.explicit || ctx.change_since_version >= self.threshold {
            CommitDecision::Version
        } else {
            CommitDecision::NotYet
        }
    }
}

/// Weight of one non-text block added, removed or changed, in change units.
/// **To calibrate.** **[choice]** A non-text block (an image, a video, a table,
/// a block this version does not know) has no characters to count, and
/// replacing a picture is a change the user would want a version of.
pub const NON_TEXT_BLOCK_WEIGHT: u64 = 100;

/// The amount of change between two states of a document (title, stored
/// content) — the measure the commit policy is given. **[choice]**, decided
/// 2026-10-11 to be "evaluated"; the proposal:
///
/// - It is a **distance between two states**, not a count of keystrokes: the
///   state at the last version and the state now. It needs no counter that a
///   crash could lose, gives the same answer on every device, and typing then
///   erasing the same words is not a change worth a version.
/// - For the title and each `text` block (matched by block id), the Unicode
///   scalar values outside the longest common prefix and suffix, on both sides —
///   what was deleted plus what was inserted, for one edited region (an upper
///   bound when several distant regions changed). Linear time, no diff
///   algorithm. A text block present on one side only counts its whole length.
/// - Every non-text block added, removed or changed counts
///   [`NON_TEXT_BLOCK_WEIGHT`]; a change in the relative order of the blocks
///   present on both sides counts it once more.
/// - Unreadable or newer-format content: the measure of its stored bytes as one
///   text (the document is never written by an editor anyway).
///
/// Guarantees (tested): zero exactly when the states are equal; symmetric;
/// growing a change never makes it smaller; it depends on the two states only.
pub fn change_amount(before_title: &str, before_content: &str, after_title: &str, after_content: &str) -> u64 {
    if before_title == after_title && before_content == after_content {
        return 0;
    }
    let total = affix_distance(before_title, after_title) + content_change(before_content, after_content);
    // The states differ but nothing above counted it (a byte outside every
    // block — an unknown top-level field, the envelope's spacing — or a text
    // block's field other than its text): still a change, the smallest one.
    total.max(1)
}

/// Unicode scalar values outside the longest common prefix and suffix of `a`
/// and `b`, on both sides: deleted plus inserted, for one edited region.
fn affix_distance(a: &str, b: &str) -> u64 {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prefix = 0;
    while prefix < a.len() && prefix < b.len() && a[prefix] == b[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < a.len() - prefix
        && suffix < b.len() - prefix
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }
    ((a.len() - prefix - suffix) + (b.len() - prefix - suffix)) as u64
}

/// One block as the measure sees it: its text (text blocks) and its bytes.
struct MeasuredBlock {
    text: Option<String>,
    json: String,
}

fn find(list: &[(BlockId, MeasuredBlock)], id: BlockId) -> Option<&MeasuredBlock> {
    list.iter().find(|(i, _)| *i == id).map(|(_, m)| m)
}

fn content_change(before: &str, after: &str) -> u64 {
    if before == after {
        return 0;
    }
    // A fixed seed: the measure depends on the two states only (a legacy form
    // gets the same block ids on both sides).
    let read = |s: &str| match sovereign_docformat::DocumentV2::read_seeded(s, "") {
        Ok((doc, form)) if !matches!(form, sovereign_docformat::SourceForm::NewerVersion { .. }) => Some(
            doc.blocks()
                .iter()
                .map(|b| (b.id(), MeasuredBlock { text: b.text().map(str::to_string), json: b.to_json_string() }))
                .collect::<Vec<_>>(),
        ),
        _ => None,
    };
    let (Some(a), Some(b)) = (read(before), read(after)) else {
        // Unreadable or newer-format content: its stored bytes as one text.
        return affix_distance(before, after);
    };
    let chars = |t: &str| t.chars().count() as u64;
    let mut total = 0u64;
    let mut seen = std::collections::HashSet::new();
    for (id, _) in a.iter().chain(b.iter()) {
        if !seen.insert(*id) {
            continue;
        }
        total += match (find(&a, *id), find(&b, *id)) {
            (Some(x), Some(y)) => match (&x.text, &y.text) {
                (Some(tx), Some(ty)) => affix_distance(tx, ty),
                (None, None) => {
                    if x.json != y.json {
                        NON_TEXT_BLOCK_WEIGHT
                    } else {
                        0
                    }
                }
                (Some(t), None) | (None, Some(t)) => chars(t) + NON_TEXT_BLOCK_WEIGHT,
            },
            (Some(x), None) | (None, Some(x)) => match &x.text {
                Some(t) => chars(t),
                None => NON_TEXT_BLOCK_WEIGHT,
            },
            (None, None) => 0,
        };
    }
    // A change in the relative order of the blocks present on both sides.
    let common_a: Vec<BlockId> = a.iter().map(|(i, _)| *i).filter(|i| find(&b, *i).is_some()).collect();
    let common_b: Vec<BlockId> = b.iter().map(|(i, _)| *i).filter(|i| find(&a, *i).is_some()).collect();
    if common_a != common_b {
        total += NON_TEXT_BLOCK_WEIGHT;
    }
    total
}

// --------------------------------------------------------------- PII hook --

/// Which write is being ingested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WritePath {
    Create,
    Save,
    Skill,
    Import,
    Sync,
    RestoreVersion,
}

/// One piece of a document's text as it is about to be stored: its title, the
/// text of a `text` block, or the caption or alt text of a media block.
#[derive(Clone, PartialEq, Eq)]
pub struct IngestItem {
    pub field: TextField,
    pub text: String,
}

/// One ingest call: **every** text field of the document as it is about to be
/// stored — the title, every `text` block, every caption and alt text (decided
/// 2026-10-11) — not only those that changed, so no field escapes. Blocks this
/// version does not know are not sent (their fields are unknown).
#[derive(Clone)]
pub struct IngestRequest {
    pub document_id: DocId,
    pub path: WritePath,
    pub items: Vec<IngestItem>,
}

/// A field's canonical text after ingest (values replaced by tokens).
#[derive(Clone, PartialEq, Eq)]
pub struct IngestedItem {
    pub field: TextField,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiiError {
    pub message: String,
}

/// Personal-data ingest (spec ch. 09, *The "no stray PII" ingest invariant*),
/// implemented by the PII ingest service (contract C-PII, lot 2d). It writes
/// the records, the source references of what it found and the preserved raw
/// body itself; it returns each field's canonical text. The service stores
/// exactly the returned texts and refuses an answer naming a field it was not
/// given. A field left out of the answer is stored as given.
#[async_trait]
pub trait PiiIngest: Send + Sync {
    async fn ingest(&self, request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError>;
}

// ------------------------------------------------------------------ media --

/// Reads media files for the service (the editor never reads files, ch. 03).
pub trait MediaFiles: Send + Sync {
    /// SHA-256 of the file at `path`, lowercase hex; `None` if it cannot be read.
    fn sha256(&self, path: &str) -> Option<String>;
}

// Debug without the text (C-TEXT g3; guarantee G17): sizes, never text.

impl std::fmt::Debug for IngestItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestItem").field("field", &self.field).field("text_bytes", &self.text.len()).finish()
    }
}

impl std::fmt::Debug for IngestedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestedItem").field("field", &self.field).field("text_bytes", &self.text.len()).finish()
    }
}

impl std::fmt::Debug for IngestRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestRequest")
            .field("document_id", &self.document_id)
            .field("path", &self.path)
            .field("items", &self.items)
            .finish()
    }
}
