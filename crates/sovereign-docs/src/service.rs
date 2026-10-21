//! [`DocumentService`] — the operations of contract C-DOC. Each doc comment
//! names the guarantees (G1…, spec ch. 03) the operation is bound by;
//! `TESTS.md` maps guarantees to tests.
//!
//! How the guarantees are held, in one place:
//! - **One writer at a time.** Every write takes the service's write lock, so
//!   a save checks the revision and writes with nothing in between (G11).
//! - **A write lands whole** (G4). Versions it needs are inserted first, each
//!   in one store call, fully formed and MAC'd (`insert_commit`); then the
//!   document's title, content, times, head commit and bookkeeping are
//!   written in **one** store call (`update_document_fields`). A crash before
//!   that call leaves the prior state (and at most an extra version); after
//!   it, the new state. A create inserts its version, then the row — whole,
//!   already sealed — in one call, so a failed create lists nothing.
//! - **Restore points** (G4, G6). Before replacing a state, the service asks
//!   whether it is a version (the head commit holds it) and who wrote it
//!   (`last_writer`); anything but the editing session that wrote it makes it
//!   a version first.
//! - **Revisions** (G11) are the row's write counter and a hash of title and
//!   content: stable across reads, moved by every write of either.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use sovereign_db::encrypted::{CommitIntegrity, EncryptedGraphDB};
use sovereign_db::schema::{
    raw_to_thing, Commit, Document, DocumentPatch, DocumentSnapshot, PurgeMarker, SourceKind,
    SourceRef,
};
use sovereign_db::traits::GraphDB;
use sovereign_db::DbError;
use sovereign_docformat::{BlockId, DocumentV2, FormatError, SourceForm};
use tokio::sync::Mutex;

use crate::host::{
    change_amount, CommitContext, CommitDecision, IngestItem, IngestRequest, ServiceDeps, WritePath,
};
use crate::types::{
    token_spans, AdoptionCandidate, AdoptionPreview, AdoptionReport, AdoptionSelector, AdoptionSkip,
    Duplicate, PreviewWarning, ChangeKind, CreateOutcome, DocError, DocumentChanged, DocumentSummary,
    DocumentView, Edit, EditContent, EditSession, MacState, MigrationReport, NewContent,
    NewDocument, Origin, PastedToken, PiiOutcome, Provenance, PurgeReport, ReadOnlyReason,
    RemoteDocument, RemoteOutcome, Revision, SaveOutcome, StoredForm, TextField, TokenOccurrence,
    TokenStatus, TrashEntry, Trust, TrustDecision, VersionInfo, VersionView, Writable,
};
use crate::wire::{decode_stamp, encode_stamp, Stamp};
use crate::TRASH_RETENTION_DAYS;

/// The one door to documents. Cheap to share (`Arc`); every operation is
/// `&self` and safe to call concurrently. Writes are serialised (one write
/// lock for the service); reads are not.
pub struct DocumentService {
    pub(crate) deps: ServiceDeps,
    pub(crate) write_lock: Mutex<()>,
    /// The hybrid logical clock (C-WIRE *Stamps*): the greatest
    /// (wall-clock ms, counter) this device has issued or seen. Initialised
    /// from every stamp the store holds, on first use: each stamp is persisted
    /// with the write it stamps, so the clock survives a restart without a
    /// row of its own.
    pub(crate) hlc: std::sync::Mutex<Option<(i64, u32)>>,
    /// The greatest wall-clock reading (ms) this device has taken since the
    /// service started: "this device's clock" for the 5-minute bound on
    /// received stamps (C-WIRE 4). It never goes back, so a clock set back
    /// while running (a correction, a manual change) does not make the device
    /// refuse states it accepted a moment before. Only this device's own
    /// readings count, never a received stamp: the bound cannot be walked
    /// forward by a peer.
    pub(crate) wall_floor: std::sync::atomic::AtomicI64,
}

// ---------------------------------------------------------------- helpers --

pub(crate) fn store_err(e: DbError) -> DocError {
    DocError::Store(e.to_string())
}

fn trust_str(t: Trust) -> &'static str {
    match t {
        Trust::Owned => "owned",
        Trust::External => "external",
        Trust::Undetermined => "undetermined",
    }
}

fn parse_trust(s: &str) -> Option<Trust> {
    match s {
        "owned" => Some(Trust::Owned),
        "external" => Some(Trust::External),
        "undetermined" => Some(Trust::Undetermined),
        _ => None,
    }
}

/// A row's trust: its recorded state, or — for rows written before the
/// service — `is_owned` (G12).
fn trust_of(d: &Document) -> Trust {
    d.trust.as_deref().and_then(parse_trust).unwrap_or(if d.is_owned { Trust::Owned } else { Trust::External })
}

fn provenance_of(d: &Document) -> Provenance {
    let detail = || d.provenance_detail.clone().unwrap_or_default();
    match d.provenance_kind.as_deref() {
        Some("written") => Provenance::Written,
        Some("assistant") => Provenance::Assistant,
        Some("web") => Provenance::Web { url: detail() },
        Some("import") => Provenance::Import { source_path: detail() },
        Some("peer") => Provenance::Peer { device: detail() },
        Some("copy") => decode_copy(&detail()).unwrap_or(Provenance::Unrecorded),
        _ => Provenance::Unrecorded,
    }
}

pub(crate) fn provenance_fields(p: &Provenance) -> (Option<String>, Option<String>) {
    match p {
        Provenance::Written => (Some("written".into()), None),
        Provenance::Assistant => (Some("assistant".into()), None),
        Provenance::Web { url } => (Some("web".into()), Some(url.clone())),
        Provenance::Import { source_path } => (Some("import".into()), Some(source_path.clone())),
        Provenance::Peer { device } => (Some("peer".into()), Some(device.clone())),
        Provenance::Unrecorded => (None, None),
        // A copy nests the source's provenance (G20): the whole chain goes in
        // the detail, as length-prefixed tokens (see `provenance_tokens`).
        Provenance::Copy { .. } => (Some("copy".into()), Some(provenance_token_string(p))),
    }
}

/// A provenance as length-prefixed tokens (`<chars>:<token>`…): how a copy's
/// chain is stored, and how any provenance travels on the wire.
pub(crate) fn provenance_token_string(p: &Provenance) -> String {
    let mut out = String::new();
    for t in provenance_tokens(p) {
        out.push_str(&t.chars().count().to_string());
        out.push(':');
        out.push_str(&t);
    }
    out
}

/// A provenance as a flat list of tokens: its kind, then its detail (none
/// for written / assistant / unrecorded); a copy is `copy`, the source id,
/// then the tokens of the source's provenance.
fn provenance_tokens(p: &Provenance) -> Vec<String> {
    match p {
        Provenance::Written => vec!["written".into()],
        Provenance::Assistant => vec!["assistant".into()],
        Provenance::Unrecorded => vec!["unrecorded".into()],
        Provenance::Web { url } => vec!["web".into(), url.clone()],
        Provenance::Import { source_path } => vec!["import".into(), source_path.clone()],
        Provenance::Peer { device } => vec!["peer".into(), device.clone()],
        Provenance::Copy { of, from } => {
            let mut v = vec!["copy".to_string(), of.clone()];
            v.extend(provenance_tokens(from));
            v
        }
    }
}

pub(crate) fn decode_copy(detail: &str) -> Option<Provenance> {
    // Split `<chars>:<token>` repeatedly.
    let mut tokens = vec![];
    let mut rest = detail;
    while !rest.is_empty() {
        let (len, after) = rest.split_once(':')?;
        let len: usize = len.parse().ok()?;
        let end = after.char_indices().nth(len).map(|(i, _)| i).unwrap_or(after.len());
        if after[..end].chars().count() != len {
            return None;
        }
        tokens.push(after[..end].to_string());
        rest = &after[end..];
    }
    let mut it = tokens.into_iter();
    let p = parse_tokens(&mut it)?;
    if it.next().is_some() {
        return None;
    }
    Some(p)
}

fn parse_tokens(it: &mut impl Iterator<Item = String>) -> Option<Provenance> {
    Some(match it.next()?.as_str() {
        "written" => Provenance::Written,
        "assistant" => Provenance::Assistant,
        "unrecorded" => Provenance::Unrecorded,
        "web" => Provenance::Web { url: it.next()? },
        "import" => Provenance::Import { source_path: it.next()? },
        "peer" => Provenance::Peer { device: it.next()? },
        "copy" => {
            let of = it.next()?;
            Provenance::Copy { of, from: Box::new(parse_tokens(it)?) }
        }
        _ => return None,
    })
}

fn decision_of(d: &Document) -> Option<TrustDecision> {
    Some(TrustDecision {
        trust: parse_trust(d.trust_decision.as_deref()?)?,
        device: d.trust_decision_device.clone()?,
        decided_at: d.trust_decision_at?,
    })
}

/// Writes a trust (and the decision that set it, if any) into a patch, with
/// `is_owned` kept in agreement (owned only when owned).
/// A stored row as the state it is, for sending (`device` = this device):
/// everything that defines it, content as stored.
pub(crate) fn remote_of_row(d: &Document, device: &str) -> RemoteDocument {
    RemoteDocument {
        id: d.id_string().unwrap_or_default(),
        title: d.title.clone(),
        content: d.content.clone(),
        thread_id: d.thread_id.clone(),
        trust: Some(trust_of(d)),
        trust_decision: decision_of(d),
        provenance: Some(provenance_of(d)),
        modified_at: d.modified_at,
        deleted_at: trashed_at_of(d),
        device: device.to_string(),
    }
}

fn put_trust(p: &mut DocumentPatch, trust: Trust, decision: Option<&TrustDecision>) {
    p.trust = Some(trust_str(trust).to_string());
    p.is_owned = Some(trust == Trust::Owned);
    if let Some(dec) = decision {
        p.trust_decision = Some(Some(trust_str(dec.trust).to_string()));
        p.trust_decision_device = Some(Some(dec.device.clone()));
        p.trust_decision_at = Some(Some(dec.decided_at));
    }
}

fn writer_tag(origin: &Origin) -> String {
    match origin {
        Origin::Editor { session } => editor_tag(*session),
        Origin::Assistant => "assistant".into(),
        Origin::Skill { name } => format!("skill:{name}"),
        Origin::Import => "import".into(),
    }
}

fn editor_tag(session: EditSession) -> String {
    format!("editor:{}", session.0)
}

/// FNV-1a, 64 bits: stable across runs and versions (the revision must
/// survive a restart of the service).
fn fnv64(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            h ^= 0xff;
            h = h.wrapping_mul(0x100000001b3);
        }
        for b in p.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn revision_of(d: &Document) -> Revision {
    Revision(format!("{}-{:016x}", d.write_seq, fnv64(&[&d.title, &d.content])))
}

fn trashed_at_of(d: &Document) -> Option<DateTime<Utc>> {
    d.deleted_at.as_deref().map(|s| {
        DateTime::parse_from_rfc3339(s)
            .map(|t| t.with_timezone(&Utc))
            // A tombstone written in another form: in the trash, dated as
            // late as possible so it is never purged early.
            .unwrap_or(DateTime::<Utc>::MAX_UTC)
    })
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

/// Every text field of a document as it is about to be stored: the title,
/// every text block, every caption and alt text. Unknown blocks are not sent.
fn ingest_items(title: &str, doc: &DocumentV2) -> Vec<IngestItem> {
    let mut items = vec![IngestItem { field: TextField::Title, text: title.to_string() }];
    for b in doc.blocks() {
        if let Some(t) = b.text() {
            items.push(IngestItem { field: TextField::Text(b.id()), text: t.to_string() });
        }
        if let Some(c) = b.caption() {
            items.push(IngestItem { field: TextField::Caption(b.id()), text: c.to_string() });
        }
        if let Some(a) = b.image_alt() {
            items.push(IngestItem { field: TextField::Alt(b.id()), text: a.to_string() });
        }
    }
    items
}

/// The text of every field that can hold a token (title, texts, captions,
/// alt texts), with its field.
fn token_fields<'a>(title: &'a str, doc: Option<&'a DocumentV2>) -> Vec<(TextField, &'a str)> {
    let mut out = vec![(TextField::Title, title)];
    if let Some(doc) = doc {
        for b in doc.blocks() {
            if let Some(t) = b.text() {
                out.push((TextField::Text(b.id()), t));
            }
            if let Some(c) = b.caption() {
                out.push((TextField::Caption(b.id()), c));
            }
            if let Some(a) = b.image_alt() {
                out.push((TextField::Alt(b.id()), a));
            }
        }
    }
    out
}

/// Content from outside Sovereign (G23): the web or an import.
fn is_from_outside(p: &Provenance) -> bool {
    matches!(p, Provenance::Web { .. } | Provenance::Import { .. })
}

/// An import path as components, after `\` → `/` and dropping a trailing
/// separator (case-sensitive; TESTS.md Uncertain 28).
fn path_components(p: &str) -> Vec<String> {
    let norm = p.replace('\\', "/");
    let trimmed = norm.trim_end_matches('/');
    trimmed.split('/').map(str::to_string).collect()
}

/// Whether an import path lies inside `folder`, by path components.
fn path_inside(path: &str, folder: &str) -> bool {
    let f = path_components(folder);
    let p = path_components(path);
    p.len() > f.len() && p[..f.len()] == f[..]
}

/// The document's head commit, as far as the restore-point rule needs it.
struct Head {
    id: String,
    timestamp: DateTime<Utc>,
    /// (title, content) in plaintext; `None` when the snapshot cannot be read.
    state: Option<(String, String)>,
}

/// When the result of a write becomes a version.
enum ResultVersion {
    /// Always, with this message (every write that is not the user's editor).
    Always(String),
    /// An editor save: explicit, or when the commit policy says so (G6).
    Policy { origin: Origin, explicit: bool },
    /// Never (the migration's result, a tombstone).
    Never,
}

/// One write of a document's state, as planned by an operation.
struct Write {
    title: String,
    content: String,
    /// `last_writer` of the new state.
    writer: String,
    /// The writer is the editing session that wrote the current state: the
    /// current state may be replaced without becoming a version (G4 choice).
    same_session: bool,
    /// Message of the version of the current state, when one is needed.
    prior_message: String,
    result: ResultVersion,
    /// Everything else the write sets (times, trust, review, trash, scan).
    extra: DocumentPatch,
    /// The thread recorded in the commits.
    thread_id: String,
    /// The stamps of the new state. `None`: a local write, stamped now by
    /// this device's clock.
    stamping: Option<Stamping>,
    /// A concurrent local state that loses to the new one (C-WIRE 7): kept
    /// as a version and relayed to peers under this stamp.
    keep_prior_as_concurrent: Option<Stamp>,
    /// Leave the stamps as they are (the migration: same state, new form).
    keep_stamps: bool,
}

/// The stamps a write records (C-WIRE).
pub(crate) struct Stamping {
    pub(crate) stamp: Stamp,
    pub(crate) prev: Option<Stamp>,
    /// Stamps to add to the document's history (ancestors, kept siblings).
    pub(crate) history_add: Vec<Stamp>,
    pub(crate) author: String,
}

/// What a write did.
pub(crate) struct Written {
    /// The version made of the new state, if one was made.
    pub(crate) result: Option<String>,
    pub(crate) revision: Revision,
    /// The version holding the state the write replaced, if any.
    pub(crate) prior_holder: Option<String>,
}

/// How a remote state arrives (apply_state).
#[derive(Default)]
pub(crate) struct Arrival {
    /// The stamps the state carries; `None` = stamped now by this device.
    pub(crate) stamping: Option<Stamping>,
    pub(crate) created_at: Option<DateTime<Utc>>,
    pub(crate) decision_stamp: Option<Stamp>,
    pub(crate) keep_prior_as_concurrent: Option<Stamp>,
    /// The message of a created document's first version.
    pub(crate) message: Option<String>,
}

/// What applying a remote state did.
pub(crate) struct Arrived {
    pub(crate) outcome: RemoteOutcome,
    pub(crate) prior_holder: Option<String>,
    pub(crate) ingest_changed: bool,
}

/// The `last_writer` of a state the one-time migration wrote (no other writer
/// uses it: sync writes `sync:<device>`, editors `editor:<session>`).
pub(crate) const MIGRATION_WRITER: &str = "migration";

/// How many stamps a document remembers of its past.
pub(crate) const STAMP_HISTORY: usize = 32;
/// How many lost concurrent states a document relays.
pub(crate) const KEPT_VERSIONS: usize = 4;

/// The stamps of a patch: the new one, the previous one, the history with the
/// additions (deduplicated, bounded, newest kept), and the author.
pub(crate) fn put_stamps(p: &mut DocumentPatch, d: Option<&Document>, st: &Stamping) {
    p.stamp = Some(Some(encode_stamp(&st.stamp)));
    p.prev_stamp = Some(st.prev.as_ref().map(encode_stamp));
    let mut history: Vec<String> = d.map(|d| d.stamp_history.clone()).unwrap_or_default();
    for h in &st.history_add {
        let e = encode_stamp(h);
        if !history.contains(&e) && h != &st.stamp {
            history.push(e);
        }
    }
    if history.len() > STAMP_HISTORY {
        let cut = history.len() - STAMP_HISTORY;
        history.drain(..cut);
    }
    p.stamp_history = Some(history);
    p.author_device = Some(Some(st.author.clone()));
}

/// The stamp of a document's current state: the recorded one, or — for a row
/// written before stamps existed — one derived from `modified_at` and the
/// author (this device when unknown), never written (C-WIRE weakness 25).
pub(crate) fn current_stamp_of(d: &Document, this_device: &str) -> Stamp {
    d.stamp.as_deref().and_then(decode_stamp).unwrap_or_else(|| Stamp {
        wall_ms: d.modified_at.timestamp_millis(),
        counter: 0,
        device: d.author_device.clone().unwrap_or_else(|| this_device.to_string()),
    })
}

impl DocumentService {
    /// Builds the service over a persona's store after login.
    pub fn new(deps: ServiceDeps) -> Self {
        let started = deps.clock.now().timestamp_millis();
        Self {
            deps,
            write_lock: Mutex::new(()),
            hlc: std::sync::Mutex::new(None),
            wall_floor: std::sync::atomic::AtomicI64::new(started),
        }
    }

    /// This device's clock for the bound on received stamps: the current
    /// reading, or a greater one already taken (see `wall_floor`).
    pub(crate) fn device_clock_ms(&self) -> i64 {
        let now = self.now().timestamp_millis();
        let before = self.wall_floor.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
        before.max(now)
    }

    /// The persona's store this service writes through (hosts key per-store
    /// state on it, e.g. the pending raw bodies of `sovereign-ai`'s `docs_host`).
    pub fn store(&self) -> &EncryptedGraphDB {
        &self.deps.store
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        self.deps.clock.now()
    }

    pub(crate) fn emit(&self, id: &str, kind: ChangeKind) {
        self.deps.events.emit(DocumentChanged { id: id.to_string(), kind });
    }

    /// The purge marker of an id, when it still applies (a revoked one —
    /// the id came back as a differing copy, C-WIRE 10 — does not).
    pub(crate) async fn marker(&self, id: &str) -> Result<Option<PurgeMarker>, DocError> {
        Ok(self.store().get_purge_marker(id).await.map_err(store_err)?.filter(|m| !m.revoked))
    }

    /// Initialises the clock from the stamps the store holds (once).
    pub(crate) async fn ensure_clock(&self) -> Result<(), DocError> {
        if self.hlc.lock().expect("hlc lock").is_some() {
            return Ok(());
        }
        let mut max: (i64, u32) = (i64::MIN, 0);
        let mut see = |s: Option<&str>| {
            if let Some(st) = s.and_then(decode_stamp) {
                max = max.max((st.wall_ms, st.counter));
            }
        };
        for d in self.store().list_documents_including_deleted().await.map_err(store_err)? {
            see(d.stamp.as_deref());
            see(d.decision_stamp.as_deref());
            for h in &d.stamp_history {
                see(Some(h));
            }
        }
        for m in self.store().list_purge_markers().await.map_err(store_err)? {
            see(m.stamp.as_deref());
        }
        // Thread writes are stamped by the same clock (THREAD-sync S1).
        for t in self.store().list_threads_including_deleted().await.map_err(store_err)? {
            see(t.stamp.as_deref());
        }
        let mut g = self.hlc.lock().expect("hlc lock");
        if g.is_none() {
            *g = Some(max);
        }
        Ok(())
    }

    /// A new stamp from this device: never lower than any stamp issued or
    /// seen here, whatever the wall clock says (C-WIRE 4, G25).
    pub(crate) async fn next_stamp(&self) -> Result<Stamp, DocError> {
        self.ensure_clock().await?;
        let now = self.now().timestamp_millis();
        let mut g = self.hlc.lock().expect("hlc lock");
        let (w, c) = g.unwrap_or((i64::MIN, 0));
        let next = if now > w { (now, 0) } else { (w, c + 1) };
        *g = Some(next);
        Ok(Stamp { wall_ms: next.0, counter: next.1, device: self.deps.device.clone() })
    }

    /// Advances the clock past a received stamp.
    pub(crate) fn observe_stamp(&self, s: &Stamp) {
        let mut g = self.hlc.lock().expect("hlc lock");
        let cur = g.unwrap_or((i64::MIN, 0));
        if (s.wall_ms, s.counter) > cur {
            *g = Some((s.wall_ms, s.counter));
        }
    }

    /// The stamps of a local write replacing `d`'s state.
    pub(crate) async fn local_stamping(&self, d: &Document) -> Result<Stamping, DocError> {
        let prev = current_stamp_of(d, &self.deps.device);
        let stamp = self.next_stamp().await?;
        Ok(Stamping { stamp, prev: Some(prev.clone()), history_add: vec![prev], author: self.deps.device.clone() })
    }

    /// Stamps a patch for a local write of `d` (trash, trust, …).
    pub(crate) async fn stamp_local(&self, d: &Document, p: &mut DocumentPatch) -> Result<Stamp, DocError> {
        let st = self.local_stamping(d).await?;
        put_stamps(p, Some(d), &st);
        Ok(st.stamp)
    }

    /// The row of a document: `Purged` when a purge marker says so (even if a
    /// purge was interrupted and the row is still there), `NotFound` when
    /// there is no such document.
    pub(crate) async fn load(&self, id: &str) -> Result<Document, DocError> {
        match self.store().get_document(id).await {
            Ok(d) => {
                if d.deleted_at.is_some() && self.marker(id).await?.is_some() {
                    return Err(DocError::Purged(id.to_string()));
                }
                Ok(d)
            }
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => {
                if raw_to_thing(id).is_some() && self.marker(id).await?.is_some() {
                    Err(DocError::Purged(id.to_string()))
                } else {
                    Err(DocError::NotFound(id.to_string()))
                }
            }
            Err(e) => Err(store_err(e)),
        }
    }

    async fn head(&self, d: &Document) -> Result<Option<Head>, DocError> {
        let Some(cid) = d.head_commit.as_deref() else { return Ok(None) };
        let doc_id = d.id_string().unwrap_or_default();
        match self.store().get_commit(cid).await {
            Ok(c) if c.document_id == doc_id => {
                let readable = c.snapshot.content_nonce.is_none() && c.snapshot.title_nonce.is_none();
                Ok(Some(Head {
                    id: cid.to_string(),
                    timestamp: c.timestamp,
                    state: readable.then_some((c.snapshot.title, c.snapshot.content)),
                }))
            }
            Ok(_) | Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => Ok(None),
            Err(e) => Err(store_err(e)),
        }
    }

    /// Inserts a version (fully formed, MAC'd by the encrypted layer) of
    /// `(title, content)`, timed after `after` so the newest version is always
    /// the head.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn make_version(
        &self,
        doc_id: &str,
        parent: Option<String>,
        after: Option<DateTime<Utc>>,
        message: &str,
        title: &str,
        content: &str,
        thread_id: &str,
    ) -> Result<Commit, DocError> {
        let mut timestamp = Utc::now();
        if let Some(prev) = after {
            if timestamp <= prev {
                timestamp = prev + Duration::microseconds(1);
            }
        }
        let commit = Commit {
            id: None,
            document_id: doc_id.to_string(),
            parent_commit: parent,
            message: message.to_string(),
            timestamp,
            snapshot: DocumentSnapshot {
                document_id: doc_id.to_string(),
                title: title.to_string(),
                content: content.to_string(),
                thread_id: thread_id.to_string(),
                content_nonce: None,
                title_nonce: None,
                title_token_hashes: vec![],
                deleted_at: None,
            },
            signature: None,
        };
        self.store().insert_commit(commit).await.map_err(store_err)
    }

    /// Fills `ref.sha256` of media blocks that have none and whose file can
    /// be read (G18). A present hash is never changed.
    fn fill_media_hashes(&self, doc: &mut DocumentV2) {
        let missing: Vec<(BlockId, String)> = doc
            .blocks()
            .iter()
            .filter(|b| b.media_sha256().is_none())
            .filter_map(|b| b.media_path().map(|p| (b.id(), p.to_string())))
            .collect();
        for (id, path) in missing {
            if let Some(h) = self.deps.media.sha256(&path) {
                let _ = doc.set_media_sha256(id, &h);
            }
        }
    }

    /// Runs personal-data ingest on every text field (G10). On success the
    /// fields hold the hook's canonical text; on failure — or an answer naming
    /// a field it was not given — they are left as given.
    async fn ingest(&self, id: &str, path: WritePath, title: &mut String, doc: &mut DocumentV2) -> PiiOutcome {
        let items = ingest_items(title, doc);
        let given: HashSet<TextField> = items.iter().map(|i| i.field).collect();
        let request = IngestRequest { document_id: id.to_string(), path, items };
        let answer = match self.deps.pii.ingest(request).await {
            Ok(a) => a,
            Err(e) => return PiiOutcome::Failed { message: e.message },
        };
        if answer.iter().any(|a| !given.contains(&a.field)) {
            return PiiOutcome::Failed {
                message: "the personal-data hook answered a field it was not given".into(),
            };
        }
        for a in answer {
            let r = match a.field {
                TextField::Title => {
                    *title = a.text;
                    Ok(())
                }
                TextField::Text(b) => doc.set_text(b, &a.text),
                TextField::Caption(b) => doc.set_caption(b, &a.text),
                TextField::Alt(b) => doc.set_image_alt(b, &a.text),
            };
            if r.is_err() {
                return PiiOutcome::Failed { message: "the personal-data hook's answer could not be applied".into() };
            }
        }
        PiiOutcome::Scanned
    }

    fn scan_mark(&self, pii: &PiiOutcome, patch: &mut DocumentPatch) {
        match pii {
            PiiOutcome::Scanned => patch.pii_scanned_at = Some(Some(self.now())),
            PiiOutcome::Failed { .. } => patch.pii_scanned_at = Some(None),
            PiiOutcome::NotRun => {}
        }
    }

    /// Applies one write: a version of the current state when the
    /// restore-point rule asks for it, a version of the new state when the
    /// write's kind does, then the row in one store call.
    async fn apply_write(&self, id: &str, current: &Document, w: Write) -> Result<Written, DocError> {
        let head = self.head(current).await?;
        let current_state = (current.title.clone(), current.content.clone());
        let current_is_version = head.as_ref().and_then(|h| h.state.as_ref()) == Some(&current_state);

        let mut parent = head.as_ref().map(|h| h.id.clone());
        let mut after = head.as_ref().map(|h| h.timestamp);
        let mut last_version: Option<(String, String)> = head.as_ref().and_then(|h| h.state.clone());
        let mut prior_holder = if current_is_version { parent.clone() } else { None };

        if !current_is_version && !w.same_session {
            let c = self
                .make_version(id, parent.clone(), after, &w.prior_message, &current.title, &current.content, &current.thread_id)
                .await?;
            let cid = c.id_string().unwrap_or_default();
            after = Some(c.timestamp);
            parent = Some(cid.clone());
            last_version = Some(current_state.clone());
            prior_holder = Some(cid);
        }

        let new_state = (w.title.clone(), w.content.clone());
        let make_result = match &w.result {
            ResultVersion::Always(_) => true,
            ResultVersion::Never => false,
            ResultVersion::Policy { origin, explicit } => {
                let (vt, vc) = last_version.clone().unwrap_or_default();
                let ctx = CommitContext {
                    document_id: id.to_string(),
                    origin: origin.clone(),
                    explicit: *explicit,
                    change_since_version: change_amount(&vt, &vc, &w.title, &w.content),
                };
                let decided = self.deps.commit_policy.decide(&ctx) == CommitDecision::Version;
                *explicit || decided
            }
        };
        let mut result = None;
        // No version repeats the one before it (G6).
        if make_result && last_version.as_ref() != Some(&new_state) {
            let message = match &w.result {
                ResultVersion::Always(m) => m.clone(),
                ResultVersion::Policy { explicit: true, .. } => "Saved".to_string(),
                _ => "Autosaved".to_string(),
            };
            let c = self.make_version(id, parent.clone(), after, &message, &w.title, &w.content, &w.thread_id).await?;
            let cid = c.id_string().unwrap_or_default();
            parent = Some(cid.clone());
            result = Some(cid);
        }

        let mut patch = w.extra;
        if !w.keep_stamps {
            let stamping = match w.stamping {
                Some(st) => st,
                None => self.local_stamping(current).await?,
            };
            put_stamps(&mut patch, Some(current), &stamping);
        }
        if let (Some(lost), Some(holder)) = (&w.keep_prior_as_concurrent, &prior_holder) {
            let mut kept = current.kept_versions.clone();
            kept.push(format!("{}\u{1f}{}", encode_stamp(lost), holder));
            if kept.len() > KEPT_VERSIONS {
                let cut = kept.len() - KEPT_VERSIONS;
                kept.drain(..cut);
            }
            patch.kept_versions = Some(kept);
        }
        patch.title = Some(w.title.clone());
        patch.content = Some(w.content.clone());
        if parent != current.head_commit {
            patch.head_commit = Some(parent);
        }
        let write_seq = current.write_seq + 1;
        patch.write_seq = Some(write_seq);
        patch.last_writer = Some(Some(w.writer));
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;

        let mut after_doc = current.clone();
        after_doc.title = w.title;
        after_doc.content = w.content;
        after_doc.write_seq = write_seq;
        Ok(Written { result, revision: revision_of(&after_doc), prior_holder })
    }

    /// A document's stored content read for writing: refused when read-only.
    fn writable_doc(&self, id: &str, d: &Document) -> Result<DocumentV2, DocError> {
        match DocumentV2::read_seeded(&d.content, id) {
            Ok((_, SourceForm::NewerVersion { v })) => {
                Err(DocError::ReadOnly { id: id.to_string(), reason: ReadOnlyReason::NewerFormat { v } })
            }
            Ok((doc, _)) => Ok(doc),
            Err(FormatError { message }) => {
                Err(DocError::ReadOnly { id: id.to_string(), reason: ReadOnlyReason::Unreadable { message } })
            }
        }
    }

    // ------------------------------------------------------------ create --

    /// Creates a document (G1 one form written, G4 initial commit, G9 ingest,
    /// G11 trust and provenance set here, G14 event `Created`). The new
    /// document has exactly one version, of its initial state.
    pub async fn create(&self, new: NewDocument) -> Result<CreateOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        // G23: nothing from outside (the web, an import) is created owned.
        if new.trust == Trust::Owned && is_from_outside(&new.provenance) {
            return Err(DocError::TrustNotAllowed { trust: Trust::Owned });
        }
        // G20: only `duplicate` makes a copy.
        if matches!(new.provenance, Provenance::Copy { .. }) {
            return Err(DocError::InvalidContent("a copy is made by duplicate, not by create".into()));
        }
        if new.provenance == Provenance::Unrecorded {
            return Err(DocError::InvalidContent(
                "provenance \"unrecorded\" is never assigned to a new document".into(),
            ));
        }
        let id = format!("document:{}", BlockId::random().to_hex());
        let mut doc = match &new.content {
            NewContent::Text(t) => DocumentV2::from_text(t, &id),
            NewContent::V2(s) => match DocumentV2::read(s) {
                Ok((doc, SourceForm::V2)) => doc,
                _ => return Err(DocError::InvalidContent("not a v2 document".into())),
            },
        };
        let mut title = new.title.clone();
        self.fill_media_hashes(&mut doc);
        let path = if new.origin == Origin::Import { WritePath::Import } else { WritePath::Create };
        let pii = self.ingest(&id, path, &mut title, &mut doc).await;
        let content = doc.to_json_string();
        let now = self.now();
        let created_at = new.created_at.unwrap_or(now);
        let modified_at = new.modified_at.unwrap_or(now);

        let version = self.make_version(&id, None, None, "Created", &title, &content, &new.thread_id).await?;
        let version_id = version.id_string().unwrap_or_default();

        let (pk, pd) = provenance_fields(&new.provenance);
        let mut row = Document::new(title, new.thread_id.clone(), new.trust == Trust::Owned);
        row.id = raw_to_thing(&id);
        row.content = content;
        row.created_at = created_at;
        row.modified_at = modified_at;
        row.head_commit = Some(version_id.clone());
        row.trust = Some(trust_str(new.trust).to_string());
        row.provenance_kind = pk;
        row.provenance_detail = pd;
        row.pii_scanned_at = matches!(pii, PiiOutcome::Scanned).then_some(now);
        row.write_seq = 1;
        row.last_writer = Some(writer_tag(&new.origin));
        row.stamp = Some(encode_stamp(&self.next_stamp().await?));
        row.author_device = Some(self.deps.device.clone());
        if let Provenance::Web { url } = &new.provenance {
            row.source_url = Some(url.clone());
        }
        let revision = revision_of(&row);
        let inserted = self.store().create_document_with_id(row).await.map_err(store_err)?;
        if !inserted {
            return Err(DocError::Store(format!("document id {id} already exists")));
        }
        self.emit(&id, ChangeKind::Created);
        Ok(CreateOutcome { id, revision, version: version_id, pii })
    }

    // -------------------------------------------------------------- read --

    /// The typed view of a document (G15). Readable whatever the stored form;
    /// a trashed document is readable (`trashed_at` set); a purged one is
    /// [`DocError::Purged`]. Reading never writes.
    pub async fn read(&self, id: &str) -> Result<DocumentView, DocError> {
        let d = self.load(id).await?;
        let (doc, stored_form, writable) = match DocumentV2::read_seeded(&d.content, id) {
            Ok((doc, form)) => {
                let writable = match &form {
                    SourceForm::NewerVersion { v } => Writable::ReadOnly(ReadOnlyReason::NewerFormat { v: *v }),
                    _ => Writable::Yes,
                };
                (Some(doc), StoredForm::Read(form), writable)
            }
            Err(e) => (None, StoredForm::Unreadable, Writable::ReadOnly(ReadOnlyReason::Unreadable { message: e.message })),
        };
        let mut statuses: HashMap<String, TokenStatus> = HashMap::new();
        let mut tokens = vec![];
        for (field, text) in token_fields(&d.title, doc.as_ref()) {
            for (_, rid) in token_spans(text) {
                if !statuses.contains_key(rid) {
                    let st = self.token_status(id, rid).await?;
                    statuses.insert(rid.to_string(), st);
                }
                tokens.push(TokenOccurrence { field, record_id: rid.to_string(), status: statuses[rid].clone() });
            }
        }
        Ok(DocumentView {
            id: id.to_string(),
            trust: trust_of(&d),
            trust_decision: decision_of(&d),
            provenance: provenance_of(&d),
            created_at: d.created_at,
            modified_at: d.modified_at,
            trashed_at: trashed_at_of(&d),
            revision: revision_of(&d),
            stored_form,
            writable,
            doc,
            tokens,
            pii_scanned: d.pii_scanned_at.is_some(),
            peer_review_pending: d.peer_review_pending,
            thread_id: d.thread_id,
            title: d.title,
        })
    }

    /// Documents not in the trash, optionally of one thread.
    pub async fn list(&self, thread_id: Option<&str>) -> Result<Vec<DocumentSummary>, DocError> {
        let docs = self.store().list_documents(thread_id).await.map_err(store_err)?;
        Ok(docs
            .into_iter()
            .filter(|d| d.deleted_at.is_none())
            .map(|d| DocumentSummary {
                id: d.id_string().unwrap_or_default(),
                trust: trust_of(&d),
                created_at: d.created_at,
                modified_at: d.modified_at,
                thread_id: d.thread_id,
                title: d.title,
            })
            .collect())
    }

    /// What text in token form `[pii:<record_id>]` is in document `doc_id`
    /// (G13, the resolver rule).
    pub async fn token_status(&self, doc_id: &str, record_id: &str) -> Result<TokenStatus, DocError> {
        let Ok(rec) = self.store().get_pii_record(record_id).await else {
            return Ok(TokenStatus::NotAToken);
        };
        let linked = rec.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == doc_id);
        if !linked {
            return Ok(TokenStatus::NotAToken);
        }
        if rec.deleted_at.is_some() {
            return Ok(TokenStatus::Placeholder);
        }
        Ok(TokenStatus::Token { label: format!("[{:?}]", rec.kind) })
    }

    // -------------------------------------------------------------- save --

    /// Saves an edit (G1, G3, G5, G7, G9, G10, G11, G12, G14). Refused when
    /// `edit.base` is not the current revision ([`DocError::Conflict`]), when
    /// the document is read-only, trashed or purged; a refused save writes
    /// nothing. A save identical to the stored state writes nothing, makes no
    /// version and emits nothing (`changed == false`). `pasted_tokens` link a
    /// record to this document only from an editor, only for tokens present in
    /// the saved title or text fields, and only when the record is linked to
    /// `from_document`. An editor save becomes a version by amount of change
    /// (the commit policy), never by time.
    pub async fn save(&self, id: &str, edit: Edit) -> Result<SaveOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        self.save_locked(id, edit).await
    }

    async fn save_locked(&self, id: &str, edit: Edit) -> Result<SaveOutcome, DocError> {
        let d = self.load(id).await?;
        if d.deleted_at.is_some() {
            return Err(DocError::InTrash(id.to_string()));
        }
        let mut doc = self.writable_doc(id, &d)?;
        let current = revision_of(&d);
        if edit.base != current {
            return Err(DocError::Conflict { id: id.to_string(), current });
        }

        match edit.content {
            None => {}
            Some(EditContent::V2(s)) => match DocumentV2::read(&s) {
                Ok((new_doc, SourceForm::V2)) => doc = new_doc,
                _ => return Err(DocError::InvalidContent("not a v2 document".into())),
            },
            Some(EditContent::TextBlocks(list)) => {
                for (bid, text) in &list {
                    if doc.block(*bid).and_then(|b| b.text()).is_none() {
                        return Err(DocError::NoSuchTextBlock(*bid));
                    }
                    doc.set_text(*bid, text).map_err(|_| DocError::NoSuchTextBlock(*bid))?;
                }
            }
            Some(EditContent::PlainText(text)) => {
                let text_blocks: Vec<BlockId> =
                    doc.blocks().iter().filter(|b| b.text().is_some()).map(|b| b.id()).collect();
                match text_blocks.as_slice() {
                    [] => {
                        let new_id = BlockId::derived(&format!("{id}#text-{}", d.write_seq), 0);
                        doc.insert_text_block(0, new_id, &text)
                            .map_err(|_| DocError::InvalidContent("document cannot take text".into()))?;
                    }
                    [one] => {
                        doc.set_text(*one, &text).map_err(|_| DocError::NoSuchTextBlock(*one))?;
                    }
                    many => return Err(DocError::AmbiguousText { text_blocks: many.len() }),
                }
            }
        }
        let mut title = edit.title.clone().unwrap_or_else(|| d.title.clone());
        self.fill_media_hashes(&mut doc);
        if title == d.title && doc.to_json_string() == d.content {
            return Ok(SaveOutcome { revision: current, changed: false, version: None, pii: PiiOutcome::NotRun });
        }

        let path = match &edit.origin {
            Origin::Skill { .. } => WritePath::Skill,
            Origin::Import => WritePath::Import,
            Origin::Editor { .. } | Origin::Assistant => WritePath::Save,
        };
        let pii = self.ingest(id, path, &mut title, &mut doc).await;
        let content = doc.to_json_string();
        if title == d.title && content == d.content {
            return Ok(SaveOutcome { revision: current, changed: false, version: None, pii });
        }

        let mut extra = DocumentPatch { modified_at: Some(self.now()), ..Default::default() };
        self.scan_mark(&pii, &mut extra);
        let (same_session, result) = match &edit.origin {
            Origin::Editor { session } => (
                d.last_writer.as_deref() == Some(editor_tag(*session).as_str()),
                ResultVersion::Policy { origin: edit.origin.clone(), explicit: edit.explicit },
            ),
            Origin::Assistant => (false, ResultVersion::Always("Assistant edit".into())),
            Origin::Skill { name } => (false, ResultVersion::Always(format!("Skill: {name}"))),
            Origin::Import => (false, ResultVersion::Always("Import".into())),
        };
        let writer = writer_tag(&edit.origin);
        let w = Write {
            title: title.clone(),
            content,
            prior_message: format!("Before {writer}"),
            writer,
            same_session,
            result,
            extra,
            thread_id: d.thread_id.clone(),
            stamping: None,
            keep_prior_as_concurrent: None,
            keep_stamps: false,
        };
        let written = self.apply_write(id, &d, w).await?;

        if matches!(edit.origin, Origin::Editor { .. }) && !edit.pasted_tokens.is_empty() {
            self.link_pasted_tokens(id, &title, &doc, &edit.pasted_tokens).await?;
        }
        self.emit(id, ChangeKind::Saved);
        Ok(SaveOutcome { revision: written.revision, changed: true, version: written.result, pii })
    }

    /// G14: links `id` to each pasted record whose token is in the saved
    /// fields and that is linked to the document it was copied from.
    async fn link_pasted_tokens(
        &self,
        id: &str,
        title: &str,
        doc: &DocumentV2,
        pasted: &[PastedToken],
    ) -> Result<(), DocError> {
        let fields = token_fields(title, Some(doc));
        for p in pasted {
            let present = fields.iter().any(|(_, text)| token_spans(text).iter().any(|(_, rid)| *rid == p.record_id));
            if !present {
                continue;
            }
            let Ok(rec) = self.store().get_pii_record(&p.record_id).await else { continue };
            let is_doc = |s: &SourceRef, doc: &str| s.source_kind == SourceKind::Document && s.source_id == doc;
            if !rec.sources.iter().any(|s| is_doc(s, &p.from_document)) || rec.sources.iter().any(|s| is_doc(s, id)) {
                continue;
            }
            let mut sources = rec.sources.clone();
            sources.push(SourceRef { source_kind: SourceKind::Document, source_id: id.to_string(), span_start: 0, span_end: 0 });
            self.store().update_pii_record_sources(&p.record_id, sources).await.map_err(store_err)?;
        }
        Ok(())
    }

    /// Applies a skill's output (G6): a version of the prior state is made
    /// first (unless already one), the output is saved, and a version of the
    /// result is made, named after the skill. Same refusals as `save`.
    pub async fn apply_skill_output(
        &self,
        id: &str,
        base: Revision,
        skill: &str,
        output: EditContent,
    ) -> Result<SaveOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        self.save_locked(id, Edit {
            base,
            title: None,
            content: Some(output),
            origin: Origin::Skill { name: skill.to_string() },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
    }

    /// Ends an editing session (window closed, app quitting): if the
    /// document's current state was written by `session` and is not a version
    /// yet, it becomes one (G5). Never fails because there is nothing to do.
    pub async fn end_session(&self, id: &str, session: EditSession) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = match self.load(id).await {
            Ok(d) => d,
            Err(DocError::NotFound(_)) | Err(DocError::Purged(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        if d.last_writer.as_deref() != Some(editor_tag(session).as_str()) {
            return Ok(());
        }
        let head = self.head(&d).await?;
        let state = (d.title.clone(), d.content.clone());
        if head.as_ref().and_then(|h| h.state.as_ref()) == Some(&state) {
            return Ok(());
        }
        let c = self
            .make_version(
                id,
                head.as_ref().map(|h| h.id.clone()),
                head.as_ref().map(|h| h.timestamp),
                "End of editing session",
                &d.title,
                &d.content,
                &d.thread_id,
            )
            .await?;
        let patch = DocumentPatch { head_commit: Some(c.id_string()), ..Default::default() };
        self.store().update_document_fields(id, patch).await.map_err(store_err)
    }

    // ------------------------------------------------------------- trust --

    /// Changes a document's trust, on an explicit user gesture only (G12).
    /// Both directions. Records the decision (this device, the clock's time) so
    /// it travels to the user's other devices. Emits `TrustChanged` when it
    /// changed; setting the current trust records nothing.
    pub async fn set_trust(&self, id: &str, trust: Trust) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if trust_of(&d) == trust {
            return Ok(());
        }
        let decision = TrustDecision { trust, device: self.deps.device.clone(), decided_at: self.now() };
        let mut patch = DocumentPatch::default();
        put_trust(&mut patch, trust, Some(&decision));
        let stamp = self.stamp_local(&d, &mut patch).await?;
        patch.decision_stamp = Some(Some(encode_stamp(&stamp)));
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::TrustChanged);
        Ok(())
    }

    // ---------------------------------------------------------- versions --

    /// The document's versions, newest first, with their integrity.
    pub async fn versions(&self, id: &str) -> Result<Vec<VersionInfo>, DocError> {
        self.load(id).await?;
        let commits = self.store().list_document_commits_checked(id).await.map_err(store_err)?;
        Ok(commits
            .into_iter()
            .map(|(c, integrity)| VersionInfo {
                commit_id: c.id_string().unwrap_or_default(),
                message: c.message,
                timestamp: c.timestamp,
                mac: match integrity {
                    CommitIntegrity::Verified => MacState::Verified,
                    CommitIntegrity::Unsigned => MacState::Unsigned,
                    CommitIntegrity::Invalid => MacState::Invalid,
                },
            })
            .collect())
    }

    /// A commit of document `id`, with its integrity; refused when it is not
    /// one of the document's.
    async fn version_of(&self, id: &str, commit_id: &str) -> Result<(Commit, CommitIntegrity), DocError> {
        let refused = |reason: &str| DocError::VersionRefused { commit_id: commit_id.to_string(), reason: reason.to_string() };
        match self.store().get_commit_checked(commit_id).await {
            Ok((c, _)) if c.document_id != id => Err(refused("the version belongs to another document")),
            Ok(found) => Ok(found),
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => Err(refused("no such version")),
            Err(e) => Err(store_err(e)),
        }
    }

    /// A version's content, for preview. Refuses a commit of another document.
    pub async fn read_version(&self, id: &str, commit_id: &str) -> Result<VersionView, DocError> {
        self.load(id).await?;
        let (c, _) = self.version_of(id, commit_id).await?;
        let readable = c.snapshot.content_nonce.is_none() && c.snapshot.title_nonce.is_none();
        let doc = if readable { DocumentV2::read_seeded(&c.snapshot.content, id).ok().map(|(d, _)| d) } else { None };
        Ok(VersionView { commit_id: commit_id.to_string(), title: if readable { c.snapshot.title } else { String::new() }, doc })
    }

    /// Restores a version (G5, G8, G9, G14 `VersionRestored`). The current
    /// state is made a version first unless it already is one; the restored
    /// state is written in v2 (a legacy snapshot is converted; a newer-format
    /// snapshot is written verbatim) and becomes a new version. Refused for a
    /// commit whose MAC does not verify, a commit of another document, a
    /// trashed or purged document.
    pub async fn restore_version(&self, id: &str, commit_id: &str) -> Result<SaveOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if d.deleted_at.is_some() {
            return Err(DocError::InTrash(id.to_string()));
        }
        let (c, integrity) = self.version_of(id, commit_id).await?;
        let refused = |reason: &str| DocError::VersionRefused { commit_id: commit_id.to_string(), reason: reason.to_string() };
        if integrity == CommitIntegrity::Invalid {
            return Err(refused("its integrity check failed"));
        }
        if c.snapshot.content_nonce.is_some() || c.snapshot.title_nonce.is_some() {
            return Err(refused("its content cannot be decrypted"));
        }
        let mut title = c.snapshot.title.clone();
        let (content, pii) = match DocumentV2::read_seeded(&c.snapshot.content, id) {
            Ok((mut doc, form)) if !matches!(form, SourceForm::NewerVersion { .. }) => {
                self.fill_media_hashes(&mut doc);
                let pii = self.ingest(id, WritePath::RestoreVersion, &mut title, &mut doc).await;
                (doc.to_json_string(), pii)
            }
            // Newer-format or unreadable: written back exactly as it was.
            _ => (c.snapshot.content.clone(), PiiOutcome::NotRun),
        };
        if title == d.title && content == d.content {
            return Ok(SaveOutcome { revision: revision_of(&d), changed: false, version: None, pii });
        }
        let mut extra = DocumentPatch { modified_at: Some(self.now()), ..Default::default() };
        match &pii {
            PiiOutcome::NotRun => extra.pii_scanned_at = Some(None),
            other => self.scan_mark(other, &mut extra),
        }
        let w = Write {
            title,
            content,
            writer: "restore".into(),
            same_session: false,
            prior_message: format!("Before restoring {commit_id}"),
            result: ResultVersion::Always(format!("Restored from {commit_id}")),
            extra,
            thread_id: d.thread_id.clone(),
            stamping: None,
            keep_prior_as_concurrent: None,
            keep_stamps: false,
        };
        let written = self.apply_write(id, &d, w).await?;
        self.emit(id, ChangeKind::VersionRestored);
        Ok(SaveOutcome { revision: written.revision, changed: true, version: written.result, pii })
    }

    // ------------------------------------------------------------- trash --

    /// Moves a document to the trash (G12, G14 `Trashed`). Content, versions,
    /// links and personal-data sources are kept; the document leaves every
    /// list, and its suggested links leave the pending suggestions, until it is
    /// restored or purged.
    pub async fn trash(&self, id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        self.trash_locked(id).await
    }

    /// [`DocumentService::trash`], for a caller already holding the write
    /// lock (the S5-12 resolutions, which check and write under one lock).
    pub(crate) async fn trash_locked(&self, id: &str) -> Result<(), DocError> {
        let d = self.load(id).await?;
        if d.deleted_at.is_some() {
            return Ok(());
        }
        let mut patch = DocumentPatch { deleted_at: Some(Some(rfc3339(self.now()))), ..Default::default() };
        self.stamp_local(&d, &mut patch).await?;
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::Trashed);
        Ok(())
    }

    /// Brings a document back from the trash, exactly as it was (G12).
    pub async fn restore_from_trash(&self, id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if d.deleted_at.is_none() {
            return Err(DocError::NotInTrash(id.to_string()));
        }
        let mut patch = DocumentPatch { deleted_at: Some(None), ..Default::default() };
        self.stamp_local(&d, &mut patch).await?;
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::RestoredFromTrash);
        Ok(())
    }

    /// The documents in the trash (and not being purged), oldest first in
    /// store order.
    async fn trashed(&self) -> Result<Vec<Document>, DocError> {
        let docs = self.store().list_documents_including_deleted().await.map_err(store_err)?;
        let purging: HashSet<String> = self
            .store()
            .list_purge_markers()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|m| !m.revoked)
            .map(|m| m.document_id)
            .collect();
        Ok(docs
            .into_iter()
            .filter(|d| d.deleted_at.is_some())
            .filter(|d| !purging.contains(&d.id_string().unwrap_or_default()))
            .collect())
    }

    /// The trash, newest first.
    pub async fn list_trash(&self) -> Result<Vec<TrashEntry>, DocError> {
        let mut entries: Vec<TrashEntry> = self
            .trashed()
            .await?
            .into_iter()
            .map(|d| TrashEntry {
                id: d.id_string().unwrap_or_default(),
                trashed_at: trashed_at_of(&d).unwrap_or(DateTime::<Utc>::MAX_UTC),
                title: d.title,
                thread_id: d.thread_id,
            })
            .collect();
        entries.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at));
        Ok(entries)
    }

    /// Empties the trash: purges every trashed document now (G12).
    pub async fn empty_trash(&self) -> Result<PurgeReport, DocError> {
        let _w = self.write_lock.lock().await;
        let mut report = PurgeReport::default();
        self.finish_interrupted_purges(&mut report).await?;
        for d in self.trashed().await? {
            self.purge_one(&d.id_string().unwrap_or_default(), &mut report).await?;
        }
        Ok(report)
    }

    /// Purges one document from the trash now. **[choice]** — the trash view's
    /// per-item "delete forever"; refused for a document not in the trash.
    pub async fn purge_from_trash(&self, id: &str) -> Result<PurgeReport, DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if d.deleted_at.is_none() {
            return Err(DocError::NotInTrash(id.to_string()));
        }
        let mut report = PurgeReport::default();
        self.purge_one(id, &mut report).await?;
        Ok(report)
    }

    /// Purges what has been in the trash for 30 days or more by the clock.
    /// Run periodically by every host.
    pub async fn purge_expired(&self) -> Result<PurgeReport, DocError> {
        let _w = self.write_lock.lock().await;
        let mut report = PurgeReport::default();
        self.finish_interrupted_purges(&mut report).await?;
        let cutoff = self.now() - Duration::days(TRASH_RETENTION_DAYS);
        for d in self.trashed().await? {
            if trashed_at_of(&d).is_some_and(|t| t <= cutoff) {
                self.purge_one(&d.id_string().unwrap_or_default(), &mut report).await?;
            }
        }
        Ok(report)
    }

    /// Whether a document id was purged here. Sync never brings it back.
    pub async fn is_purged(&self, id: &str) -> Result<bool, DocError> {
        if raw_to_thing(id).is_none() {
            return Ok(false);
        }
        Ok(self.marker(id).await?.is_some())
    }

    /// Completes the purges a crash interrupted (G13).
    async fn finish_interrupted_purges(&self, report: &mut PurgeReport) -> Result<(), DocError> {
        for m in self.store().list_purge_markers().await.map_err(store_err)? {
            if !m.complete && !m.revoked {
                self.purge_one(&m.document_id, report).await?;
            }
        }
        Ok(())
    }

    /// Purges one document: marker first (so the purge sticks and an
    /// interruption is resumed), then everything hanging off it, then the
    /// row, then its key; the marker is completed last. Every step is
    /// idempotent.
    pub(crate) async fn purge_one(&self, id: &str, report: &mut PurgeReport) -> Result<(), DocError> {
        self.purge_one_as(id, None, report).await
    }

    /// Purges one document, recording the marker's stamp and the purged
    /// version's fingerprint (C-WIRE 10) — the received ones when a peer's
    /// marker purges it here, else a new stamp and this copy's fingerprint.
    pub(crate) async fn purge_one_as(
        &self,
        id: &str,
        received: Option<(Stamp, String)>,
        report: &mut PurgeReport,
    ) -> Result<(), DocError> {
        let store = self.store();
        let (purged_at, stamp, fingerprint) = match (self.marker(id).await?, received) {
            (Some(m), _) => (m.purged_at, m.stamp, m.fingerprint),
            (None, Some((st, fp))) => (self.now(), Some(encode_stamp(&st)), Some(fp)),
            (None, None) => {
                let fp = match store.get_document(id).await {
                    Ok(d) => Some(crate::wire::purge_fingerprint(id, &d.title, &d.content)),
                    Err(_) => None,
                };
                (self.now(), Some(encode_stamp(&self.next_stamp().await?)), fp)
            }
        };
        let marker = |complete| PurgeMarker {
            id: None,
            document_id: id.to_string(),
            purged_at,
            complete,
            stamp: stamp.clone(),
            fingerprint: fingerprint.clone(),
            revoked: false,
        };
        store.set_purge_marker(marker(false)).await.map_err(store_err)?;

        report.commits += store.delete_document_commits(id).await.map_err(store_err)? as usize;
        report.relationships += store.delete_document_relationships(id).await.map_err(store_err)? as usize;
        report.suggested_links += store.delete_document_suggested_links(id).await.map_err(store_err)? as usize;

        let is_this = |s: &SourceRef| s.source_kind == SourceKind::Document && s.source_id == id;
        for rec in store.list_pii_records_including_deleted().await.map_err(store_err)? {
            if !rec.sources.iter().any(is_this) {
                continue;
            }
            let Some(rid) = rec.id_string() else { continue };
            let removed = rec.sources.iter().filter(|s| is_this(s)).count();
            let remaining: Vec<SourceRef> = rec.sources.iter().filter(|s| !is_this(s)).cloned().collect();
            if remaining.is_empty() && !rec.stored_secret {
                // A discovered record left with no source goes; a vault
                // entry never does.
                store.delete_pii_record(&rid).await.map_err(store_err)?;
                report.pii_records += 1;
            } else {
                store.update_pii_record_sources(&rid, remaining).await.map_err(store_err)?;
            }
            report.pii_sources += removed;
        }

        match store.delete_document(id).await {
            Ok(()) | Err(DbError::NotFound(_)) => {}
            Err(e) => return Err(store_err(e)),
        }
        store.shred_document_key(id).await.map_err(store_err)?;
        store.set_purge_marker(marker(true)).await.map_err(store_err)?;
        report.documents.push(id.to_string());
        self.emit(id, ChangeKind::Purged);
        Ok(())
    }

    // -------------------------------------------------------------- sync --

    /// Applies the winning state of a document from a paired device (G7, G9,
    /// G11, G12, G14 `RemoteApplied`/`Created`/`Trashed`). The local state is
    /// made a version first. No review flag is raised (C-WIRE 13, decided
    /// 2026-10-15): the replaced state is in History, and the incoming-change
    /// analysis raises an alert only on a detection. A purged id is ignored.
    /// Trust of an existing document changes only through
    /// `remote.trust_decision`, when it is later than the local one — in
    /// either direction. The state is stamped now by this device; sync itself
    /// goes through [`DocumentService::apply_wire`], which keeps the sender's
    /// stamps and orders by them.
    pub async fn apply_remote(&self, remote: RemoteDocument) -> Result<RemoteOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        let id = remote.id.clone();
        if raw_to_thing(&id).is_none() {
            return Err(DocError::NotFound(id));
        }
        if self.marker(&id).await?.is_some() {
            return Ok(RemoteOutcome::IgnoredPurged);
        }
        let local = match self.store().get_document(&id).await {
            Ok(d) => Some(d),
            Err(DbError::NotFound(_)) => None,
            Err(e) => return Err(store_err(e)),
        };
        Ok(self.apply_state(&remote, local, Arrival::default()).await?.outcome)
    }

    /// Applies a remote state over `local` (or creates it), under the write
    /// lock. Shared by `apply_remote`, `apply_wire` and the backup restore.
    pub(crate) async fn apply_state(
        &self,
        remote: &RemoteDocument,
        local: Option<Document>,
        mut a: Arrival,
    ) -> Result<Arrived, DocError> {
        let id = remote.id.clone();
        // The remote content as this device stores it: v2 (a legacy form
        // converted deterministically), or a newer / unreadable form verbatim.
        let mut title = remote.title.clone();
        let mut parsed = match DocumentV2::read_seeded(&remote.content, &id) {
            Ok((mut doc, form)) if !matches!(form, SourceForm::NewerVersion { .. }) => {
                self.fill_media_hashes(&mut doc);
                Some(doc)
            }
            _ => None,
        };
        let converted = parsed.as_ref().map(DocumentV2::to_json_string).unwrap_or_else(|| remote.content.clone());

        let Some(d) = local else {
            return self.create_from_remote(remote, title, parsed, converted, a).await;
        };

        // Trust moves only with a decision later than the local one: by the
        // decisions' stamps when both have one, else by their recorded time.
        let local_decision = decision_of(&d);
        let local_decision_stamp = d.decision_stamp.as_deref().and_then(decode_stamp);
        let decision = remote
            .trust_decision
            .as_ref()
            .filter(|r| match (&local_decision, &a.decision_stamp, &local_decision_stamp) {
                (None, _, _) => true,
                (Some(_), Some(rs), Some(ls)) => rs > ls,
                (Some(l), _, _) => r.decided_at > l.decided_at,
            })
            .filter(|r| Some(*r) != local_decision.as_ref());
        let mut trust_patch = DocumentPatch::default();
        if let Some(dec) = decision {
            put_trust(&mut trust_patch, dec.trust, Some(dec));
            trust_patch.decision_stamp = Some(a.decision_stamp.as_ref().map(encode_stamp));
        }

        let local_trashed = d.deleted_at.is_some();
        let remote_trashed = remote.deleted_at.is_some();
        let same_state = remote.title == d.title
            && converted == d.content
            && remote.thread_id == d.thread_id
            && remote_trashed == local_trashed;
        if same_state {
            if decision.is_none() {
                return Ok(Arrived { outcome: RemoteOutcome::Unchanged, prior_holder: None, ingest_changed: false });
            }
            let mut patch = trust_patch;
            let stamping = match a.stamping.take() {
                Some(st) => st,
                None => self.local_stamping(&d).await?,
            };
            put_stamps(&mut patch, Some(&d), &stamping);
            self.store().update_document_fields(&id, patch).await.map_err(store_err)?;
            self.emit(&id, ChangeKind::RemoteApplied);
            return Ok(Arrived { outcome: RemoteOutcome::Applied, prior_holder: None, ingest_changed: false });
        }

        let before_ingest = (title.clone(), converted.clone());
        let pii = match parsed.as_mut() {
            Some(doc) => self.ingest(&id, WritePath::Sync, &mut title, doc).await,
            None => PiiOutcome::NotRun,
        };
        let content = parsed.as_ref().map(DocumentV2::to_json_string).unwrap_or(converted);
        let ingest_changed = (title.clone(), content.clone()) != before_ingest;
        let stamping = match a.stamping.take() {
            // The receiver's rewrite is its own write (C-WIRE 5, decision on
            // weakness 26): a new stamp from this device, after the sender's.
            Some(st) if ingest_changed => {
                let mut history_add = st.history_add.clone();
                history_add.push(st.stamp.clone());
                Some(Stamping {
                    stamp: self.next_stamp().await?,
                    prev: Some(st.stamp),
                    history_add,
                    author: self.deps.device.clone(),
                })
            }
            other => other,
        };
        let mut extra = trust_patch;
        extra.modified_at = Some(remote.modified_at);
        extra.thread_id = Some(remote.thread_id.clone());
        match &pii {
            PiiOutcome::NotRun => extra.pii_scanned_at = Some(None),
            other => self.scan_mark(other, &mut extra),
        }
        if remote_trashed && !local_trashed {
            let when = remote.deleted_at.unwrap_or_else(|| self.now());
            extra.deleted_at = Some(Some(rfc3339(when)));
        } else if !remote_trashed && local_trashed {
            extra.deleted_at = Some(None);
        }
        let device = remote.device.clone();
        let w = Write {
            title,
            content,
            writer: format!("sync:{device}"),
            same_session: false,
            prior_message: format!("Before sync from {device}"),
            result: ResultVersion::Always(format!("Synced from {device}")),
            extra,
            thread_id: remote.thread_id.clone(),
            stamping,
            keep_prior_as_concurrent: a.keep_prior_as_concurrent.take(),
            keep_stamps: false,
        };
        let written = self.apply_write(&id, &d, w).await?;
        let outcome = if remote_trashed && !local_trashed {
            self.emit(&id, ChangeKind::Trashed);
            RemoteOutcome::Trashed
        } else if !remote_trashed && local_trashed {
            self.emit(&id, ChangeKind::RemoteApplied);
            RemoteOutcome::RestoredFromTrash
        } else {
            self.emit(&id, ChangeKind::RemoteApplied);
            RemoteOutcome::Applied
        };
        Ok(Arrived { outcome, prior_holder: written.prior_holder, ingest_changed })
    }

    /// A document this device does not have: created under its origin id,
    /// with the sender's trust and provenance (creation rules hold: content
    /// from outside is not owned without a decision), and one version.
    async fn create_from_remote(
        &self,
        remote: &RemoteDocument,
        mut title: String,
        mut parsed: Option<DocumentV2>,
        converted: String,
        mut a: Arrival,
    ) -> Result<Arrived, DocError> {
        let id = remote.id.clone();
        let provenance = remote.provenance.clone().unwrap_or(Provenance::Peer { device: remote.device.clone() });
        let (mut trust, decision) = match &remote.trust_decision {
            Some(dec) => (dec.trust, Some(dec.clone())),
            None => (remote.trust.unwrap_or(Trust::Undetermined), None),
        };
        if trust == Trust::Owned && is_from_outside(&provenance) && decision.is_none() {
            trust = Trust::Undetermined;
        }
        let before_ingest = (title.clone(), converted.clone());
        let pii = match parsed.as_mut() {
            Some(doc) => self.ingest(&id, WritePath::Sync, &mut title, doc).await,
            None => PiiOutcome::NotRun,
        };
        let content = parsed.as_ref().map(DocumentV2::to_json_string).unwrap_or(converted);
        let ingest_changed = (title.clone(), content.clone()) != before_ingest;
        let stamping = match a.stamping.take() {
            Some(st) if ingest_changed => {
                let mut history_add = st.history_add.clone();
                history_add.push(st.stamp.clone());
                Stamping { stamp: self.next_stamp().await?, prev: Some(st.stamp), history_add, author: self.deps.device.clone() }
            }
            Some(st) => st,
            None => Stamping {
                stamp: self.next_stamp().await?,
                prev: None,
                history_add: vec![],
                author: remote.device.clone(),
            },
        };
        let message = a.message.take().unwrap_or_else(|| format!("Received from {}", remote.device));
        let version = self.make_version(&id, None, None, &message, &title, &content, &remote.thread_id).await?;
        let (pk, pd) = provenance_fields(&provenance);
        let mut row = Document::new(title, remote.thread_id.clone(), trust == Trust::Owned);
        row.id = raw_to_thing(&id);
        row.content = content;
        row.created_at = a.created_at.unwrap_or(remote.modified_at);
        row.modified_at = remote.modified_at;
        row.head_commit = version.id_string();
        row.trust = Some(trust_str(trust).to_string());
        if let Some(dec) = &decision {
            row.trust_decision = Some(trust_str(dec.trust).to_string());
            row.trust_decision_device = Some(dec.device.clone());
            row.trust_decision_at = Some(dec.decided_at);
            row.decision_stamp = a.decision_stamp.as_ref().map(encode_stamp);
        }
        row.provenance_kind = pk;
        row.provenance_detail = pd;
        row.pii_scanned_at = matches!(pii, PiiOutcome::Scanned).then_some(self.now());
        row.deleted_at = remote.deleted_at.map(rfc3339);
        row.write_seq = 1;
        row.last_writer = Some(format!("sync:{}", remote.device));
        let mut p = DocumentPatch::default();
        put_stamps(&mut p, None, &stamping);
        row.stamp = p.stamp.flatten();
        row.prev_stamp = p.prev_stamp.flatten();
        row.stamp_history = p.stamp_history.unwrap_or_default();
        row.author_device = p.author_device.flatten();
        let inserted = self.store().create_document_with_id(row).await.map_err(store_err)?;
        if !inserted {
            return Err(DocError::Store(format!("document id {id} appeared during sync")));
        }
        self.emit(&id, ChangeKind::Created);
        Ok(Arrived { outcome: RemoteOutcome::Created, prior_holder: None, ingest_changed })
    }

    // --------------------------------------------------------- migration --

    /// The one-time migration (G2): every legacy document (v1 envelope, raw
    /// text, empty) is converted to v2 after a version of its prior state;
    /// newer-format and unreadable documents are left untouched; trashed
    /// documents are migrated too and stay trashed. Idempotent. Changes no
    /// title, trust, provenance, thread or time, and runs no ingest.
    pub async fn migrate_all(&self) -> Result<MigrationReport, DocError> {
        let _w = self.write_lock.lock().await;
        let mut report = MigrationReport::default();
        let purging: HashSet<String> = self
            .store()
            .list_purge_markers()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|m| !m.revoked)
            .map(|m| m.document_id)
            .collect();
        let docs = self.store().list_documents_including_deleted().await.map_err(store_err)?;
        for d in docs {
            let Some(id) = d.id_string() else { continue };
            if purging.contains(&id) {
                continue;
            }
            match DocumentV2::read_seeded(&d.content, &id) {
                Ok((_, SourceForm::V2)) => {
                    report.already_v2 += 1;
                    if d.last_writer.as_deref() == Some(MIGRATION_WRITER) {
                        report.previously_migrated.push(id);
                    }
                }
                Ok((_, SourceForm::NewerVersion { .. })) => report.read_only.push(id),
                Err(_) => {
                    report.unreadable.push(id.clone());
                    report.read_only.push(id);
                }
                Ok((mut doc, _legacy)) => {
                    self.fill_media_hashes(&mut doc);
                    let w = Write {
                        title: d.title.clone(),
                        content: doc.to_json_string(),
                        writer: MIGRATION_WRITER.into(),
                        same_session: false,
                        prior_message: "Before migration to format v2".into(),
                        result: ResultVersion::Never,
                        // No time, title, trust or thread changes: the
                        // timeline does not move.
                        extra: DocumentPatch::default(),
                        thread_id: d.thread_id.clone(),
                        // The migration changes the stored form, not the
                        // state: its stamps stay as they are (C-WIRE 3) —
                        // a row never stamped stays unstamped.
                        stamping: None,
                        keep_stamps: true,
                        keep_prior_as_concurrent: None,
                    };
                    match self.apply_write(&id, &d, w).await {
                        Ok(_) => {
                            self.emit(&id, ChangeKind::Migrated);
                            report.migrated.push(id);
                        }
                        Err(e) => report.failed.push((id, e.to_string())),
                    }
                }
            }
        }
        Ok(report)
    }

    // ------------------------------------------------ duplicate (G20) --

    /// Makes a copy of a document (G20): its content as stored, the requested
    /// title and thread (the source's by default), the source's trust —
    /// copying never raises or lowers it — and the provenance
    /// `Copy { of, from }`. The copy has its own first version, goes through
    /// ingest (G10), and is linked to every personal-data record that is linked
    /// to the source and whose token is in the source's text fields, so each
    /// token is what it was in the source. The source is unchanged. Refused for
    /// a trashed, purged or read-only source.
    pub async fn duplicate(&self, request: Duplicate) -> Result<CreateOutcome, DocError> {
        let _w = self.write_lock.lock().await;
        let source = request.source.clone();
        let d = self.load(&source).await?;
        if d.deleted_at.is_some() {
            return Err(DocError::InTrash(source));
        }
        let mut doc = self.writable_doc(&source, &d)?;

        // The records to link: linked to the source, and in its text fields
        // (title, texts, captions, alt texts) — read before the copy's ingest.
        let mut to_link: Vec<String> = vec![];
        for (_, text) in token_fields(&d.title, Some(&doc)) {
            for (_, rid) in token_spans(text) {
                if to_link.iter().any(|r| r == rid) {
                    continue;
                }
                if let Ok(rec) = self.store().get_pii_record(rid).await {
                    if rec.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == source) {
                        to_link.push(rid.to_string());
                    }
                }
            }
        }

        let id = format!("document:{}", BlockId::random().to_hex());
        let mut title = request.title.clone().unwrap_or_else(|| d.title.clone());
        let thread_id = request.thread_id.clone().unwrap_or_else(|| d.thread_id.clone());
        let trust = trust_of(&d);
        let provenance = Provenance::Copy { of: source.clone(), from: Box::new(provenance_of(&d)) };
        self.fill_media_hashes(&mut doc);
        let pii = self.ingest(&id, WritePath::Create, &mut title, &mut doc).await;
        let content = doc.to_json_string();
        let now = self.now();

        let version = self
            .make_version(&id, None, None, &format!("Copy of {source}"), &title, &content, &thread_id)
            .await?;
        let version_id = version.id_string().unwrap_or_default();

        let (pk, pd) = provenance_fields(&provenance);
        let mut row = Document::new(title, thread_id, trust == Trust::Owned);
        row.id = raw_to_thing(&id);
        row.content = content;
        row.created_at = now;
        row.modified_at = now;
        row.head_commit = Some(version_id.clone());
        row.trust = Some(trust_str(trust).to_string());
        row.provenance_kind = pk;
        row.provenance_detail = pd;
        row.pii_scanned_at = matches!(pii, PiiOutcome::Scanned).then_some(now);
        row.write_seq = 1;
        row.last_writer = Some(writer_tag(&request.origin));
        row.stamp = Some(encode_stamp(&self.next_stamp().await?));
        row.author_device = Some(self.deps.device.clone());
        let revision = revision_of(&row);
        let inserted = self.store().create_document_with_id(row).await.map_err(store_err)?;
        if !inserted {
            return Err(DocError::Store(format!("document id {id} already exists")));
        }

        // Each token stays what it was in the source: a token (live record) or
        // a placeholder (deleted record) — G14's conditions, met by the source.
        for rid in to_link {
            let Ok(rec) = self.store().get_pii_record(&rid).await else { continue };
            if rec.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == id) {
                continue;
            }
            let mut sources = rec.sources.clone();
            sources.push(SourceRef { source_kind: SourceKind::Document, source_id: id.clone(), span_start: 0, span_end: 0 });
            self.store().update_pii_record_sources(&rid, sources).await.map_err(store_err)?;
        }
        self.emit(&id, ChangeKind::Created);
        Ok(CreateOutcome { id, revision, version: version_id, pii })
    }

    // ---------------------------------------- adoption gestures (G21, G22) --

    /// What an adoption gesture would change (G21): every document the
    /// selector names *now* that is a candidate — not trashed, not owned;
    /// thread, import-folder and date gestures take undetermined documents
    /// only, an explicit selection undetermined and external ones (Céline,
    /// 2026-10-15). Changes nothing.
    pub async fn preview_adoption(&self, selector: AdoptionSelector) -> Result<AdoptionPreview, DocError> {
        let candidate = |d: &Document| AdoptionCandidate {
            id: d.id_string().unwrap_or_default(),
            title: d.title.clone(),
            trust: trust_of(d),
            created_at: d.created_at,
            modified_at: d.modified_at,
        };
        let undetermined = |d: &Document| d.deleted_at.is_none() && trust_of(d) == Trust::Undetermined;
        let mut warnings = vec![];
        let candidates: Vec<AdoptionCandidate> = match &selector {
            AdoptionSelector::Documents(ids) => {
                let mut out: Vec<AdoptionCandidate> = vec![];
                for id in ids {
                    if out.iter().any(|c| &c.id == id) || raw_to_thing(id).is_none() {
                        continue;
                    }
                    match self.store().get_document(id).await {
                        Ok(d) if d.deleted_at.is_none() && trust_of(&d) != Trust::Owned => out.push(candidate(&d)),
                        Ok(_) | Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => {}
                        Err(e) => return Err(store_err(e)),
                    }
                }
                out
            }
            AdoptionSelector::Thread(thread) => {
                let docs = self.store().list_documents(Some(thread)).await.map_err(store_err)?;
                docs.iter().filter(|d| undetermined(d)).map(candidate).collect()
            }
            AdoptionSelector::ImportFolder(folder) => {
                let docs = self.store().list_documents(None).await.map_err(store_err)?;
                docs.iter()
                    .filter(|d| undetermined(d))
                    .filter(|d| matches!(provenance_of(d), Provenance::Import { source_path } if path_inside(&source_path, folder)))
                    .map(candidate)
                    .collect()
            }
            AdoptionSelector::CreatedBefore(at) => {
                warnings.push(PreviewWarning::DateIsNotAuthorship);
                let docs = self.store().list_documents(None).await.map_err(store_err)?;
                docs.iter().filter(|d| undetermined(d) && d.created_at < *at).map(candidate).collect()
            }
            AdoptionSelector::ModifiedBefore(at) => {
                warnings.push(PreviewWarning::DateIsNotAuthorship);
                let docs = self.store().list_documents(None).await.map_err(store_err)?;
                docs.iter().filter(|d| undetermined(d) && d.modified_at < *at).map(candidate).collect()
            }
        };
        Ok(AdoptionPreview { selector, candidates, warnings })
    }

    /// Applies a preview (G22): adopts exactly the previewed documents that
    /// are still candidates — never one the selector would name now but the
    /// preview did not — each with its own trust decision (owned, this
    /// device, now), one `TrustChanged` event each. Each document is adopted
    /// whole or not at all: trust, decision and `is_owned` go in one store
    /// write. One decision time for the whole apply.
    pub async fn apply_adoption(&self, preview: &AdoptionPreview) -> Result<AdoptionReport, DocError> {
        let _w = self.write_lock.lock().await;
        let decision = TrustDecision { trust: Trust::Owned, device: self.deps.device.clone(), decided_at: self.now() };
        let mut report = AdoptionReport::default();
        for c in &preview.candidates {
            let id = c.id.clone();
            let d = match self.load(&id).await {
                Ok(d) => d,
                Err(DocError::Purged(_)) => {
                    report.skipped.push((id, AdoptionSkip::Purged));
                    continue;
                }
                Err(DocError::NotFound(_)) => {
                    report.skipped.push((id, AdoptionSkip::NotFound));
                    continue;
                }
                Err(e) => return Err(e),
            };
            if d.deleted_at.is_some() {
                report.skipped.push((id, AdoptionSkip::InTrash));
                continue;
            }
            if trust_of(&d) == Trust::Owned {
                report.skipped.push((id, AdoptionSkip::AlreadyOwned));
                continue;
            }
            let mut patch = DocumentPatch::default();
            put_trust(&mut patch, Trust::Owned, Some(&decision));
            let stamp = self.stamp_local(&d, &mut patch).await?;
            patch.decision_stamp = Some(Some(encode_stamp(&stamp)));
            self.store().update_document_fields(&id, patch).await.map_err(store_err)?;
            self.emit(&id, ChangeKind::TrustChanged);
            report.adopted.push(id);
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_provenance_round_trips_through_storage() {
        let deep = Provenance::Copy {
            of: "document:b".into(),
            from: Box::new(Provenance::Copy {
                of: "document:a".into(),
                from: Box::new(Provenance::Web { url: "https://ex.org/a:b?x=1:2 é".into() }),
            }),
        };
        for p in [deep, Provenance::Copy { of: "document:c".into(), from: Box::new(Provenance::Unrecorded) }] {
            let (kind, detail) = provenance_fields(&p);
            let mut d = Document::new(String::new(), String::new(), false);
            d.provenance_kind = kind;
            d.provenance_detail = detail;
            assert_eq!(provenance_of(&d), p);
        }
    }

    #[test]
    fn import_paths_compare_by_components() {
        assert!(path_inside("/in/corpus/a.md", "/in/corpus/"));
        assert!(path_inside("C:\\in\\win\\d.md", "C:/in/win"));
        assert!(!path_inside("/in/corpusX/c.md", "/in/corpus"));
        assert!(!path_inside("/in/corpus", "/in/corpus"));
        assert!(!path_inside("skill:file-import", "/in"));
    }
}
