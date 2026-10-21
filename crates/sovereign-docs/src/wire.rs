//! What sync needs from the document service (C-DOC G24–G31, spec ch. 03;
//! written for C-WIRE, spec ch. 05, decided 2026-10-15). Built in step 4 of
//! the adoption; the tests in `tests/g24_*`–`g31_*` define it.
//!
//! The wire metadata travels beside a [`RemoteDocument`] in [`WireMeta`], so
//! `apply_remote` and every existing caller keep working while sync uses
//! [`DocumentService::apply_wire`].
//!
//! How it is held:
//! - **Stamps** are hybrid-logical-clock stamps, issued by
//!   [`DocumentService`] on every write and kept from the sender on every
//!   received state; each is persisted with the write it stamps (the
//!   document row, a purge marker), and the clock is initialised from them on
//!   first use, so it survives a restart without a row of its own.
//! - **Order**: a state whose previous stamp (or history) is the local stamp
//!   follows it; a state whose stamp the local history holds is older; any
//!   other differing state is concurrent — the greater stamp becomes current
//!   on both devices and the other is kept as a version on both (the losing
//!   device relays its lost state, [`DocumentService::kept_versions`]).
//! - **Purges** leave a marker with the fingerprint of the purged version; a
//!   received marker purges only an identical copy.

use std::fmt;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sovereign_db::schema::{Document, DocumentPatch, PurgeMarker};
use sovereign_db::traits::GraphDB as _;
use sovereign_db::DbError;
use sovereign_docformat::{DocumentV2, SourceForm};

use crate::service::{current_stamp_of, remote_of_row, store_err, Arrival, DocumentService, Stamping};
use crate::types::{DocError, DocId, PurgeReport, RemoteDocument, RemoteOutcome, Trust};

// ------------------------------------------------------------------ stamps --

/// A hybrid logical clock stamp (C-WIRE *Stamps instead of the wall clock*):
/// ordered by wall-clock milliseconds, then counter, then device. Every write
/// to a document and every trust decision carries one; a device never issues
/// a stamp lower than one it has seen.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Stamp {
    pub wall_ms: i64,
    pub counter: u32,
    pub device: String,
}

/// A received stamp more than this far ahead of the receiver's clock is
/// refused (C-WIRE guarantee 4). **[choice]**
pub const MAX_STAMP_AHEAD_SECS: i64 = 300;

/// A stamp as stored (`wall_ms:counter:device`).
pub(crate) fn encode_stamp(s: &Stamp) -> String {
    format!("{}:{}:{}", s.wall_ms, s.counter, s.device)
}

pub(crate) fn decode_stamp(s: &str) -> Option<Stamp> {
    let mut it = s.splitn(3, ':');
    let wall_ms = it.next()?.parse().ok()?;
    let counter = it.next()?.parse().ok()?;
    let device = it.next()?.to_string();
    Some(Stamp { wall_ms, counter, device })
}

/// What travels beside a document state, for sync (C-WIRE *What travels*).
/// Every field is optional: a peer that does not send one leaves the local
/// value as it is (C-WIRE guarantee 2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WireMeta {
    /// The document's original creation time.
    pub created_at: Option<DateTime<Utc>>,
    /// The stamp of the write that produced this state.
    pub stamp: Option<Stamp>,
    /// The stamp of the state this one replaced.
    pub prev_stamp: Option<Stamp>,
    /// The device on which the write was made, when the sender only relays it.
    pub author_device: Option<String>,
    /// The stamp of `RemoteDocument::trust_decision`.
    pub decision_stamp: Option<Stamp>,
    /// Earlier stamps of this document on the sender (bounded): what this
    /// state descends from, so a receiver several edits behind sees a
    /// fast-forward rather than a concurrent edit. Added in step 4.
    pub history: Vec<Stamp>,
}

/// What [`DocumentService::apply_wire`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireApplied {
    /// The state is now current here (created, applied, trashed, revived…).
    Applied(RemoteOutcome),
    /// A concurrent state with the lower stamp: kept as a version, not current.
    StoredAsVersion,
    /// Nothing to do: already known, older than the current state, or the
    /// purged version of a purged document.
    Ignored,
    /// The stamp is more than [`MAX_STAMP_AHEAD_SECS`] ahead of this
    /// device's clock: nothing written.
    RefusedStampAhead,
}

/// The outcome of applying one remote state.
#[derive(Clone, Debug)]
pub struct WireOutcome {
    pub applied: WireApplied,
    /// The local and remote states were concurrent edits (both kept).
    pub concurrent: bool,
    /// Ingest changed the received text: the stored state differs from what
    /// arrived and must travel back.
    pub ingest_changed_text: bool,
    /// The version holding the state the remote one replaced, if any.
    pub prior_version: Option<String>,
}

impl WireOutcome {
    fn of(applied: WireApplied) -> Self {
        WireOutcome { applied, concurrent: false, ingest_changed_text: false, prior_version: None }
    }
}

/// A document's state as this device stores it, for sending (no caller reads
/// `content` itself).
#[derive(Clone)]
pub struct WireState {
    /// The state, with `device` = this device.
    pub document: RemoteDocument,
    pub meta: WireMeta,
    /// [`normalized_hash`] of this state.
    pub normalized_hash: String,
    /// The stamp was recorded by a write; `false` when it is derived for a row
    /// written before step 4 (C-WIRE weakness 25). Added in step 4.
    pub stamped: bool,
}

impl fmt::Debug for WireState {
    /// Never prints the title or content (G17).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WireState")
            .field("document", &self.document)
            .field("meta", &self.meta)
            .field("normalized_hash", &self.normalized_hash)
            .finish()
    }
}

/// A purge, as it travels: the id, the stamp of the purge and the fingerprint
/// of the purged version — never its content (C-WIRE *Deletions and purges*).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurgeMarkerWire {
    pub id: DocId,
    pub stamp: Stamp,
    pub fingerprint: String,
}

/// What a received purge marker did here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PurgeApplied {
    /// This device's copy had the purged fingerprint: purged here too.
    Purged,
    /// This device's copy differs: kept (it will travel back).
    KeptDiffers,
    /// This device had no copy: the marker is recorded.
    Recorded,
}

/// One document of a guardian backup, with its wire metadata.
#[derive(Clone)]
pub struct BackupDocument {
    pub document: RemoteDocument,
    pub meta: WireMeta,
}

/// Why a backup restore did nothing.
#[derive(Debug)]
pub enum BackupError {
    /// The workspace already holds documents (trash included): it catches up
    /// by sync, never by restore (Céline, 2026-10-15).
    NotEmpty { documents: usize },
    Doc(DocError),
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupError::NotEmpty { documents } => write!(
                f,
                "this device already holds {documents} document(s); it gets the others by sync, not by restoring a backup"
            ),
            BackupError::Doc(e) => write!(f, "{e}"),
        }
    }
}

/// What a backup restore did.
#[derive(Clone, Debug, Default)]
pub struct BackupReport {
    pub restored: Vec<DocId>,
    pub markers: usize,
}

/// A detection by the incoming-change analysis (C-WIRE guarantee 13): raised
/// only when something looks wrong, listed until dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncomingAlert {
    pub id: DocId,
    pub device: String,
    /// What was found, in words (no document text).
    pub finding: String,
    /// The version holding the state before the change.
    pub previous_version: Option<String>,
}

// ------------------------------------------------------- pure functions --

fn hex(h: impl AsRef<[u8]>) -> String {
    h.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// Feeds one length-prefixed field into a hash (no two field sequences
/// collide).
fn field(h: &mut Sha256, s: &str) {
    h.update((s.len() as u64).to_le_bytes());
    h.update(s.as_bytes());
}

/// A content string as normalised v2: a legacy form converted with the
/// migration seeded by the document id; a newer or unreadable form as stored.
fn normalized_content(id: &str, content: &str) -> String {
    match DocumentV2::read_seeded(content, id) {
        Ok((doc, form)) if !matches!(form, SourceForm::NewerVersion { .. }) => doc.to_json_string(),
        _ => content.to_string(),
    }
}

fn trust_name(t: Option<Trust>) -> &'static str {
    match t {
        Some(Trust::Owned) => "owned",
        Some(Trust::External) => "external",
        Some(Trust::Undetermined) => "undetermined",
        None => "-",
    }
}

/// The normalised hash of a document state (C-WIRE *Comparing two states*):
/// title, thread, trash state, trust, trust decision, provenance, and the
/// content as v2 — a legacy form converted with the deterministic migration
/// seeded by the document id — so two copies of one document compare equal
/// whichever form each stores. Never contains the text; lowercase hex.
pub fn normalized_hash(state: &RemoteDocument) -> String {
    let mut h = Sha256::new();
    field(&mut h, "sovereign.normalized/v1");
    field(&mut h, &state.id);
    field(&mut h, &state.title);
    field(&mut h, &state.thread_id);
    field(&mut h, if state.deleted_at.is_some() { "trash" } else { "live" });
    field(&mut h, trust_name(state.trust));
    match &state.trust_decision {
        Some(d) => {
            field(&mut h, trust_name(Some(d.trust)));
            field(&mut h, &d.device);
            field(&mut h, &d.decided_at.timestamp_micros().to_string());
        }
        None => field(&mut h, "-"),
    }
    match &state.provenance {
        Some(p) => {
            let (kind, detail) = crate::service::provenance_fields(p);
            field(&mut h, kind.as_deref().unwrap_or("unrecorded"));
            field(&mut h, detail.as_deref().unwrap_or(""));
        }
        None => field(&mut h, "-"),
    }
    field(&mut h, &normalized_content(&state.id, &state.content));
    hex(h.finalize())
}

/// The fingerprint a purge marker carries: a hash of the purged version's
/// **title and content** (content normalised to v2, seeded by `id`) and
/// nothing else — not the thread, trash state, trust or times. **[choice]**
pub fn purge_fingerprint(id: &str, title: &str, content: &str) -> String {
    let mut h = Sha256::new();
    field(&mut h, "sovereign.purge/v1");
    field(&mut h, title);
    field(&mut h, &normalized_content(id, content));
    hex(h.finalize())
}

/// A provenance as it travels inside a sealed envelope (length-prefixed
/// tokens; copies nested). [`decode_provenance`] reads it back.
pub fn encode_provenance(p: &crate::types::Provenance) -> String {
    crate::service::provenance_token_string(p)
}

/// The provenance [`encode_provenance`] wrote; `None` if malformed.
pub fn decode_provenance(s: &str) -> Option<crate::types::Provenance> {
    crate::service::decode_copy(s)
}

/// A purge marker as it travels, when it carries a stamp and a fingerprint
/// (markers from before step 4 do not, and do not travel).
pub fn purge_marker_wire(m: &PurgeMarker) -> Option<PurgeMarkerWire> {
    if m.revoked {
        return None;
    }
    Some(PurgeMarkerWire {
        id: m.document_id.clone(),
        stamp: decode_stamp(m.stamp.as_deref()?)?,
        fingerprint: m.fingerprint.clone()?,
    })
}

/// The wire metadata of a stored row.
fn meta_of_row(d: &Document, this_device: &str) -> WireMeta {
    WireMeta {
        created_at: Some(d.created_at),
        stamp: Some(current_stamp_of(d, this_device)),
        prev_stamp: d.prev_stamp.as_deref().and_then(decode_stamp),
        author_device: Some(d.author_device.clone().unwrap_or_else(|| this_device.to_string())),
        decision_stamp: d.decision_stamp.as_deref().and_then(decode_stamp),
        history: d.stamp_history.iter().filter_map(|s| decode_stamp(s)).collect(),
    }
}

/// A backed-up document row (a guardian backup snapshot holds rows) as a
/// [`BackupDocument`], with `device` = the device that made the backup.
pub fn backup_document_from_row(row: &Document, device: &str) -> BackupDocument {
    BackupDocument { document: remote_of_row(row, device), meta: meta_of_row(row, device) }
}

/// Every stamp a row knows besides its current one: its ancestors and the
/// concurrent states it keeps as versions.
fn known_stamps(d: &Document) -> impl Iterator<Item = Stamp> + '_ {
    d.stamp_history
        .iter()
        .map(String::as_str)
        .chain(d.kept_versions.iter().filter_map(|e| e.split_once('\u{1f}').map(|(s, _)| s)))
        .filter_map(decode_stamp)
}

fn stamping_of(meta: &WireMeta, stamp: Stamp, author: String) -> Stamping {
    let mut history_add = meta.history.clone();
    if let Some(p) = &meta.prev_stamp {
        history_add.push(p.clone());
    }
    Stamping { stamp, prev: meta.prev_stamp.clone(), history_add, author }
}

// --------------------------------------------------------- the service --

impl DocumentService {
    /// The stored state of a row, for sending.
    fn wire_state(&self, d: &Document) -> WireState {
        let document = remote_of_row(d, &self.deps.device);
        let normalized_hash = normalized_hash(&document);
        WireState { document, meta: meta_of_row(d, &self.deps.device), normalized_hash, stamped: d.stamp.is_some() }
    }

    /// Applies a remote state with its wire metadata (G24–G27, G31). Like
    /// `apply_remote`, plus: the creation time, stamps and author device are
    /// kept; order is by stamp (a stamp too far ahead is refused); a
    /// concurrent state with the lower stamp is stored as a version without
    /// becoming current; a purged id comes back only as a copy whose
    /// fingerprint differs from the purged one; no review flag is raised.
    pub async fn apply_wire(&self, remote: RemoteDocument, meta: WireMeta) -> Result<WireOutcome, DocError> {
        let id = remote.id.clone();
        let author = meta.author_device.clone().unwrap_or_else(|| remote.device.clone());
        let rstamp = meta.stamp.clone().unwrap_or_else(|| Stamp {
            wall_ms: remote.modified_at.timestamp_millis(),
            counter: 0,
            device: author.clone(),
        });
        if rstamp.wall_ms > self.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000 {
            return Ok(WireOutcome::of(WireApplied::RefusedStampAhead));
        }
        let _w = self.write_lock.lock().await;
        if sovereign_db::schema::raw_to_thing(&id).is_none() {
            return Err(DocError::NotFound(id));
        }
        self.ensure_clock().await?;
        let local = match self.store().get_document(&id).await {
            Ok(d) => Some(d),
            Err(DbError::NotFound(_)) => None,
            Err(e) => return Err(store_err(e)),
        };
        if let Some(m) = self.marker(&id).await? {
            // A purged id: the purged version never comes back; a differing
            // copy does, as a new arrival, and the marker is dropped here.
            let fp = purge_fingerprint(&id, &remote.title, &remote.content);
            if local.is_some() || m.fingerprint.as_deref().is_none_or(|f| f == fp) {
                return Ok(WireOutcome::of(WireApplied::Ignored));
            }
            self.store()
                .set_purge_marker(PurgeMarker { revoked: true, ..m })
                .await
                .map_err(store_err)?;
        }
        self.observe_stamp(&rstamp);
        if let Some(ds) = &meta.decision_stamp {
            self.observe_stamp(ds);
        }
        let arrival = |keep: Option<Stamp>| Arrival {
            stamping: Some(stamping_of(&meta, rstamp.clone(), author.clone())),
            created_at: meta.created_at,
            decision_stamp: meta.decision_stamp.clone(),
            keep_prior_as_concurrent: keep,
            message: None,
        };
        let Some(d) = local else {
            let a = self.apply_state(&remote, None, arrival(None)).await?;
            return Ok(WireOutcome {
                applied: WireApplied::Applied(a.outcome),
                concurrent: false,
                ingest_changed_text: a.ingest_changed,
                prior_version: None,
            });
        };

        let lstamp = current_stamp_of(&d, &self.deps.device);
        if rstamp == lstamp {
            return Ok(WireOutcome::of(WireApplied::Ignored));
        }
        let local_state = remote_of_row(&d, &self.deps.device);
        if normalized_hash(&local_state) == normalized_hash(&remote) {
            return Ok(WireOutcome::of(WireApplied::Ignored));
        }
        let older = known_stamps(&d).any(|h| h == rstamp)
            || d.prev_stamp.as_deref().and_then(decode_stamp).as_ref() == Some(&rstamp);
        if older {
            return Ok(WireOutcome::of(WireApplied::Ignored));
        }
        // A state never stamped here (written before step 4) has no history
        // to be concurrent with: what follows it is a fast-forward.
        let follows = meta.prev_stamp.as_ref() == Some(&lstamp) || meta.history.contains(&lstamp) || d.stamp.is_none();
        if follows || rstamp > lstamp {
            let concurrent = !follows;
            let a = self.apply_state(&remote, Some(d), arrival(concurrent.then(|| lstamp.clone()))).await?;
            let applied = match a.outcome {
                RemoteOutcome::Unchanged => WireApplied::Ignored,
                o => WireApplied::Applied(o),
            };
            return Ok(WireOutcome {
                applied,
                concurrent,
                ingest_changed_text: a.ingest_changed,
                prior_version: a.prior_holder,
            });
        }
        // Concurrent, with the lower stamp: kept as a version, not current.
        self.keep_as_version(&d, &remote, &rstamp, &author).await?;
        Ok(WireOutcome { applied: WireApplied::StoredAsVersion, concurrent: true, ingest_changed_text: false, prior_version: None })
    }

    /// Stores a remote state as a version of `d` without making it current,
    /// and remembers its stamp (so it is never stored twice).
    async fn keep_as_version(&self, d: &Document, remote: &RemoteDocument, stamp: &Stamp, author: &str) -> Result<(), DocError> {
        let id = remote.id.clone();
        let content = normalized_content(&id, &remote.content);
        let head = d.head_commit.clone();
        let after = match &head {
            Some(h) => self.store().get_commit(h).await.ok().map(|c| c.timestamp),
            None => None,
        };
        let c = self
            .make_version(&id, head, after, &format!("Concurrent edit from {author}"), &remote.title, &content, &remote.thread_id)
            .await?;
        // A sibling, not an ancestor: remembered (and relayed) apart from the
        // history, so a peer never reads it as something this state follows.
        let mut kept = d.kept_versions.clone();
        kept.push(format!("{}\u{1f}{}", encode_stamp(stamp), c.id_string().unwrap_or_default()));
        if kept.len() > crate::service::KEPT_VERSIONS {
            let cut = kept.len() - crate::service::KEPT_VERSIONS;
            kept.drain(..cut);
        }
        let patch = DocumentPatch { kept_versions: Some(kept), ..Default::default() };
        self.store().update_document_fields(&id, patch).await.map_err(store_err)
    }

    /// Applies a concurrent state a peer lost and relays (C-WIRE 7): stored as
    /// a version here, never made current; ignored when already known.
    pub async fn apply_wire_version(&self, remote: RemoteDocument, meta: WireMeta) -> Result<WireOutcome, DocError> {
        let Some(stamp) = meta.stamp.clone() else { return Ok(WireOutcome::of(WireApplied::Ignored)) };
        if stamp.wall_ms > self.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000 {
            return Ok(WireOutcome::of(WireApplied::RefusedStampAhead));
        }
        let _w = self.write_lock.lock().await;
        if self.marker(&remote.id).await?.is_some() {
            return Ok(WireOutcome::of(WireApplied::Ignored));
        }
        let d = match self.store().get_document(&remote.id).await {
            Ok(d) => d,
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => return Ok(WireOutcome::of(WireApplied::Ignored)),
            Err(e) => return Err(store_err(e)),
        };
        let known = current_stamp_of(&d, &self.deps.device) == stamp || known_stamps(&d).any(|h| h == stamp);
        if known {
            return Ok(WireOutcome::of(WireApplied::Ignored));
        }
        self.ensure_clock().await?;
        self.observe_stamp(&stamp);
        let author = meta.author_device.clone().unwrap_or_else(|| remote.device.clone());
        self.keep_as_version(&d, &remote, &stamp, &author).await?;
        Ok(WireOutcome { applied: WireApplied::StoredAsVersion, concurrent: true, ingest_changed_text: false, prior_version: None })
    }

    /// The concurrent states this device lost and keeps as versions (C-WIRE
    /// 7), for relaying with the document's current state.
    pub async fn kept_versions(&self, id: &str) -> Result<Vec<WireState>, DocError> {
        let d = self.load(id).await?;
        let mut out = vec![];
        for entry in &d.kept_versions {
            let Some((stamp, commit)) = entry.split_once('\u{1f}') else { continue };
            let Some(stamp) = decode_stamp(stamp) else { continue };
            let Ok(c) = self.store().get_commit(commit).await else { continue };
            if c.document_id != id || c.snapshot.content_nonce.is_some() {
                continue;
            }
            let mut document = remote_of_row(&d, &self.deps.device);
            document.title = c.snapshot.title.clone();
            document.content = c.snapshot.content.clone();
            if !c.snapshot.thread_id.is_empty() {
                document.thread_id = c.snapshot.thread_id.clone();
            }
            let normalized_hash = normalized_hash(&document);
            let meta = WireMeta {
                stamp: Some(stamp.clone()),
                author_device: Some(stamp.device.clone()),
                ..WireMeta::default()
            };
            out.push(WireState { document, meta, normalized_hash, stamped: true });
        }
        Ok(out)
    }

    /// The stored state of one document, for sending (G29). Trashed documents
    /// included; a purged one is [`DocError::Purged`].
    pub async fn stored_state(&self, id: &str) -> Result<WireState, DocError> {
        let d = self.load(id).await?;
        Ok(self.wire_state(&d))
    }

    /// Every stored state, trashed ones included (the manifest's source).
    pub async fn stored_states(&self) -> Result<Vec<WireState>, DocError> {
        let purging: std::collections::HashSet<String> = self
            .store()
            .list_purge_markers()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|m| !m.revoked)
            .map(|m| m.document_id)
            .collect();
        Ok(self
            .store()
            .list_documents_including_deleted()
            .await
            .map_err(store_err)?
            .iter()
            .filter(|d| !purging.contains(&d.id_string().unwrap_or_default()))
            .map(|d| self.wire_state(d))
            .collect())
    }

    /// The purge markers this device knows (G27).
    pub async fn purge_markers(&self) -> Result<Vec<PurgeMarkerWire>, DocError> {
        Ok(self
            .store()
            .list_purge_markers()
            .await
            .map_err(store_err)?
            .iter()
            .filter(|m| m.complete)
            .filter_map(purge_marker_wire)
            .collect())
    }

    /// Applies a purge marker received from a paired device (G27).
    pub async fn apply_purge_marker(&self, marker: PurgeMarkerWire) -> Result<PurgeApplied, DocError> {
        let _w = self.write_lock.lock().await;
        self.ensure_clock().await?;
        self.observe_stamp(&marker.stamp);
        let id = marker.id.clone();
        match self.store().get_document(&id).await {
            Ok(d) => {
                if purge_fingerprint(&id, &d.title, &d.content) != marker.fingerprint {
                    return Ok(PurgeApplied::KeptDiffers);
                }
                let mut report = PurgeReport::default();
                self.purge_one_as(&id, Some((marker.stamp.clone(), marker.fingerprint.clone())), &mut report).await?;
                Ok(PurgeApplied::Purged)
            }
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => {
                if self.marker(&id).await?.is_none() {
                    self.record_marker(&marker).await?;
                }
                Ok(PurgeApplied::Recorded)
            }
            Err(e) => Err(store_err(e)),
        }
    }

    async fn record_marker(&self, m: &PurgeMarkerWire) -> Result<(), DocError> {
        self.store()
            .set_purge_marker(PurgeMarker {
                id: None,
                document_id: m.id.clone(),
                purged_at: self.now(),
                complete: true,
                stamp: Some(encode_stamp(&m.stamp)),
                fingerprint: Some(m.fingerprint.clone()),
                revoked: false,
            })
            .await
            .map_err(store_err)
    }

    /// Restores a guardian backup (G30): refused with
    /// [`BackupError::NotEmpty`] on a workspace that holds any document;
    /// otherwise every document comes back with one version "Restored from
    /// backup", and the markers are recorded.
    pub async fn restore_backup(
        &self,
        documents: Vec<BackupDocument>,
        markers: Vec<PurgeMarkerWire>,
    ) -> Result<BackupReport, BackupError> {
        let _w = self.write_lock.lock().await;
        let held = self.store().list_documents_including_deleted().await.map_err(|e| BackupError::Doc(store_err(e)))?;
        if !held.is_empty() {
            return Err(BackupError::NotEmpty { documents: held.len() });
        }
        self.ensure_clock().await.map_err(BackupError::Doc)?;
        let mut report = BackupReport::default();
        for b in documents {
            let id = b.document.id.clone();
            let author = b.meta.author_device.clone().unwrap_or_else(|| b.document.device.clone());
            let stamping = b.meta.stamp.clone().map(|st| {
                self.observe_stamp(&st);
                stamping_of(&b.meta, st, author.clone())
            });
            let a = Arrival {
                stamping,
                created_at: b.meta.created_at,
                decision_stamp: b.meta.decision_stamp.clone(),
                keep_prior_as_concurrent: None,
                message: Some("Restored from backup".into()),
            };
            self.apply_state(&b.document, None, a).await.map_err(BackupError::Doc)?;
            report.restored.push(id);
        }
        for m in markers {
            self.observe_stamp(&m.stamp);
            if self.marker(&m.id).await.map_err(BackupError::Doc)?.is_none() {
                self.record_marker(&m).await.map_err(BackupError::Doc)?;
                report.markers += 1;
            }
        }
        Ok(report)
    }

    /// Records an alert from the incoming-change analysis (G31). One per
    /// document: a later alert replaces an open one.
    pub async fn raise_alert(&self, alert: IncomingAlert) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        self.load(&alert.id).await?;
        let patch = DocumentPatch {
            alert_device: Some(Some(alert.device)),
            alert_finding: Some(Some(alert.finding)),
            alert_previous_version: Some(alert.previous_version),
            alert_raised_at: Some(Some(self.now())),
            ..Default::default()
        };
        self.store().update_document_fields(&alert.id, patch).await.map_err(store_err)
    }

    /// The alerts not yet dismissed, newest first.
    pub async fn alerts(&self) -> Result<Vec<IncomingAlert>, DocError> {
        let mut out: Vec<(Option<DateTime<Utc>>, IncomingAlert)> = self
            .store()
            .list_documents_including_deleted()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter_map(|d| {
                let finding = d.alert_finding.clone()?;
                Some((
                    d.alert_raised_at,
                    IncomingAlert {
                        id: d.id_string().unwrap_or_default(),
                        device: d.alert_device.clone().unwrap_or_default(),
                        finding,
                        previous_version: d.alert_previous_version.clone(),
                    },
                ))
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(out.into_iter().map(|(_, a)| a).collect())
    }

    /// Dismisses a document's alert; changes nothing in the document.
    pub async fn dismiss_alert(&self, id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        match self.store().get_document(id).await {
            Ok(_) => {}
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => return Ok(()),
            Err(e) => return Err(store_err(e)),
        }
        let patch = DocumentPatch {
            alert_device: Some(None),
            alert_finding: Some(None),
            alert_previous_version: Some(None),
            alert_raised_at: Some(None),
            ..Default::default()
        };
        self.store().update_document_fields(id, patch).await.map_err(store_err)
    }

    /// Whether a document has an open alert.
    pub async fn has_alert(&self, id: &str) -> Result<bool, DocError> {
        Ok(self.load(id).await?.alert_finding.is_some())
    }

    // ------------------------------------------------- model verdicts --

    /// Queues the model's verdict on the change just applied to `id` from
    /// `device` (C-WIRE 13, spec v2.0.22): the deterministic checks ran
    /// during sync; the model runs afterwards, off the sync path. The queue
    /// entry is stored on the document row (no document text: the device,
    /// the version before the change, the stamp of the received state), so
    /// it survives a restart. One entry per document: a later change to the
    /// same document replaces it (its own previous version then holds the
    /// state the earlier entry was about).
    pub async fn queue_model_check(&self, id: &str, device: &str, previous_version: Option<String>) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        let check = ModelCheck {
            id: id.to_string(),
            device: device.to_string(),
            previous_version,
            stamp: encode_stamp(&current_stamp_of(&d, &self.deps.device)),
        };
        let patch = DocumentPatch { model_check: Some(Some(check.to_stored())), ..Default::default() };
        self.store().update_document_fields(id, patch).await.map_err(store_err)
    }

    /// Every queued model verdict (the ones a quit interrupted included).
    pub async fn pending_model_checks(&self) -> Result<Vec<ModelCheck>, DocError> {
        Ok(self
            .store()
            .list_documents_including_deleted()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter_map(|d| {
                let id = d.id_string()?;
                ModelCheck::from_stored(&id, d.model_check.as_deref()?)
            })
            .collect())
    }

    /// What the model reads of a queued change: the previous version and
    /// the current state, as [`analysis_text`]. `None` when the received
    /// state is no longer current (replaced, trashed or purged since): the
    /// entry is then finished without a model verdict.
    pub async fn model_check_texts(&self, check: &ModelCheck) -> Result<Option<(String, String)>, DocError> {
        let d = match self.load(&check.id).await {
            Ok(d) => d,
            Err(DocError::NotFound(_)) | Err(DocError::Purged(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        if d.deleted_at.is_some() || encode_stamp(&current_stamp_of(&d, &self.deps.device)) != check.stamp {
            return Ok(None);
        }
        let current = self.read(&check.id).await?;
        let incoming = analysis_text(&current.title, current.doc.as_ref());
        let prior = match &check.previous_version {
            Some(c) => match self.read_version(&check.id, c).await {
                Ok(v) => analysis_text(&v.title, v.doc.as_ref()),
                Err(_) => String::new(),
            },
            None => String::new(),
        };
        Ok(Some((prior, incoming)))
    }

    /// Ends a queued model verdict. Only the entry `check` describes is
    /// removed: one queued since for a later change stays.
    pub async fn finish_model_check(&self, check: &ModelCheck) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = match self.store().get_document(&check.id).await {
            Ok(d) => d,
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => return Ok(()),
            Err(e) => return Err(store_err(e)),
        };
        let same = d.model_check.as_deref().and_then(|s| ModelCheck::from_stored(&check.id, s)).is_some_and(|c| c == *check);
        if !same {
            return Ok(());
        }
        let patch = DocumentPatch { model_check: Some(None), ..Default::default() };
        self.store().update_document_fields(&check.id, patch).await.map_err(store_err)
    }
}

/// A received change waiting for the model's verdict (see
/// [`DocumentService::queue_model_check`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelCheck {
    pub id: DocId,
    /// The device the change came from.
    pub device: String,
    /// The version holding the state before the change (`None`: a first
    /// arrival).
    pub previous_version: Option<String>,
    /// The stamp of the received state, as stored.
    pub stamp: String,
}

impl ModelCheck {
    fn to_stored(&self) -> String {
        serde_json::json!({"device": self.device, "previous_version": self.previous_version, "stamp": self.stamp}).to_string()
    }

    fn from_stored(id: &str, s: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(s).ok()?;
        Some(Self {
            id: id.to_string(),
            device: v["device"].as_str()?.to_string(),
            previous_version: v["previous_version"].as_str().map(str::to_string),
            stamp: v["stamp"].as_str()?.to_string(),
        })
    }
}

/// What the incoming-change analysis reads of a document state: the title,
/// the text blocks (tokens in canonical form), then every caption and alt
/// text, then the stored bytes of any block this version does not render as
/// text (a table, or a block it does not know). So nothing a peer can put in
/// a document escapes the injection scan. Used by sync for the checks during
/// sync and by the background model verdict.
pub fn analysis_text(title: &str, doc: Option<&DocumentV2>) -> String {
    let mut out = title.to_string();
    let Some(doc) = doc else { return out };
    let body = doc.plain_text();
    if !body.is_empty() {
        out.push_str("\n\n");
        out.push_str(&body);
    }
    for b in doc.blocks() {
        let extra = if b.text().is_some() {
            None
        } else if b.media_path().is_none() {
            Some(b.to_json_string())
        } else {
            let parts: Vec<&str> = [b.caption(), b.image_alt()].into_iter().flatten().filter(|s| !s.is_empty()).collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        };
        if let Some(extra) = extra {
            out.push_str("\n\n");
            out.push_str(&extra);
        }
    }
    out
}

#[cfg(test)]
mod analysis_text_tests {
    use super::analysis_text;
    use sovereign_docformat::DocumentV2;

    /// Nothing a peer can put in a document escapes the scan: the title, the
    /// text, a caption, an alt text, and a block this version does not know.
    #[test]
    fn reads_title_text_captions_alt_and_unknown_blocks() {
        let json = r#"{"format":"sovereign.doc","v":2,"blocks":[
            {"type":"text","id":"00000000000000000000000000001b01","text":"body words"},
            {"type":"image","id":"00000000000000000000000000001b02","ref":{"path":"/i.png"},"alt":"alt words","caption":"caption words"},
            {"type":"x-future","id":"00000000000000000000000000001b03","note":"future words"}]}"#;
        let d = DocumentV2::read(json).expect("v2").0;
        let t = analysis_text("title words", Some(&d));
        for w in ["title words", "body words", "alt words", "caption words", "future words"] {
            assert!(t.contains(w), "{w} missing from {t:?}");
        }
        assert_eq!(analysis_text("only a title", None), "only a title");
    }
}
