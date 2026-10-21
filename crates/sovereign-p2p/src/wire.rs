//! C-WIRE — the sync wire contract (spec ch. 05, *Sync wire — contract
//! (C-WIRE)*, decided with Céline on 2026-10-15). Built in step 4 of the
//! C-DOC adoption; the tests in `tests/wire_*.rs` define it.
//!
//! What is here: the document service handed to sync (every document state is
//! read from it and applied through it — sync writes no document itself), the
//! incoming-change analysis handed to sync (silent unless suspicious), the
//! document plan of a round (normalised hashes and stamps; the older-build
//! refusal), the wire state sealed beside each document snapshot, purge
//! markers, the list of row tables a sync overwrite can be restored from, and
//! the backup restore through the service.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use sovereign_db::GraphDB;
use sovereign_docs::{
    BackupError, BackupReport, DocumentService, IncomingAlert, PurgeApplied, PurgeMarkerWire, RemoteDocument,
    Stamp, Trust, TrustDecision, WireApplied, WireMeta, WireState,
};

use crate::backup::BackupSnapshot;
use crate::error::{P2pError, P2pResult};
use crate::protocol::manifest::{DocumentWireEntry, PurgeMarkerEntry, SyncManifest, WireStamp};
use crate::protocol::sync::{EncryptedCommit, SyncTable};
use crate::sync_service::{public_key_from_peer_id, seal_snapshot, sign_commit, transport_to_snapshot, verify_commit, SyncService};

/// The wire version this build speaks (manifest field `wire_version`). A peer
/// whose manifest has none is on an older build.
pub const WIRE_VERSION: u32 = 1;

/// The incoming-change analysis (C-WIRE guarantee 13; `peer_audit.rs` in
/// `sovereign-ai` implements it — sync cannot depend on that crate). Run on
/// every document change received from a paired device; `None` = nothing
/// suspicious (no alert), `Some(finding)` = an alert, in words, no document
/// text.
#[async_trait]
pub trait IncomingAnalysis: Send + Sync {
    async fn analyse(&self, prior_text: &str, incoming_text: &str) -> Option<String>;
}

impl SyncService {
    /// Gives sync the persona's document service: from then on every document
    /// state is read from it and applied through it (C-WIRE guarantees 1–11).
    pub fn with_documents(mut self, documents: Arc<DocumentService>) -> Self {
        self.documents = Some(documents);
        self
    }

    /// Gives sync the incoming-change analysis (C-WIRE guarantee 13).
    pub fn with_analysis(mut self, analysis: Arc<dyn IncomingAnalysis>) -> Self {
        self.analysis = Some(analysis);
        self
    }

    /// Applies the purge markers a peer's manifest carries (C-WIRE guarantee
    /// 10), through the document service. A peer on an older build carries
    /// none. Returns how many markers changed something here.
    pub async fn apply_purge_markers(&self, remote: &SyncManifest) -> P2pResult<usize> {
        let Some(docs) = &self.documents else { return Ok(0) };
        if remote.wire_version.is_none() {
            return Ok(0);
        }
        let known: HashSet<(String, String)> = docs
            .purge_markers()
            .await
            .map_err(doc_err)?
            .into_iter()
            .map(|m| (m.id, m.fingerprint))
            .collect();
        let mut changed = 0;
        for m in &remote.purge_markers {
            if known.contains(&(m.doc_id.clone(), m.fingerprint.clone())) {
                continue;
            }
            let marker = PurgeMarkerWire { id: m.doc_id.clone(), stamp: stamp_of(&m.stamp), fingerprint: m.fingerprint.clone() };
            match docs.apply_purge_marker(marker).await.map_err(doc_err)? {
                PurgeApplied::Purged | PurgeApplied::Recorded => changed += 1,
                PurgeApplied::KeptDiffers => {}
            }
        }
        Ok(changed)
    }

    /// The document part of this device's manifest: per document the
    /// normalised hash and the stamps, and the purge markers (C-WIRE).
    pub(crate) async fn manifest_documents(&self, docs: &DocumentService, manifest: &mut SyncManifest) -> P2pResult<()> {
        for st in docs.stored_states().await.map_err(doc_err)? {
            let Some(stamp) = st.meta.stamp.as_ref() else { continue };
            manifest.document_wire.push(DocumentWireEntry {
                doc_id: st.document.id.clone(),
                normalized_hash: st.normalized_hash.clone(),
                stamp: wire_stamp(stamp),
                prev_stamp: st.meta.prev_stamp.as_ref().map(wire_stamp),
                history: st.meta.history.iter().map(wire_stamp).collect(),
                stamped: st.stamped,
            });
        }
        for m in docs.purge_markers().await.map_err(doc_err)? {
            manifest.purge_markers.push(PurgeMarkerEntry {
                doc_id: m.id,
                stamp: wire_stamp(&m.stamp),
                fingerprint: m.fingerprint,
            });
        }
        Ok(())
    }

    /// Seals a document's current state — and the concurrent states this
    /// device lost and keeps as versions — for `peer` (C-WIRE 1, 7, 16): the
    /// snapshot and the wire state each AEAD-sealed under the pair key, the
    /// signature covering both.
    pub(crate) async fn seal_wire_states(
        &self,
        docs: &DocumentService,
        doc_id: &str,
        key: &[u8; 32],
    ) -> P2pResult<Vec<EncryptedCommit>> {
        let current = docs.stored_state(doc_id).await.map_err(doc_err)?;
        let mut out = vec![self.seal_wire_state(&current, false, key)?];
        for kept in docs.kept_versions(doc_id).await.map_err(doc_err)? {
            out.push(self.seal_wire_state(&kept, true, key)?);
        }
        Ok(out)
    }

    fn seal_wire_state(&self, st: &WireState, kept: bool, key: &[u8; 32]) -> P2pResult<EncryptedCommit> {
        let d = &st.document;
        let snapshot = sovereign_db::schema::DocumentSnapshot {
            document_id: d.id.clone(),
            title: d.title.clone(),
            content: d.content.clone(),
            thread_id: d.thread_id.clone(),
            content_nonce: None,
            title_nonce: None,
            title_token_hashes: Vec::new(),
            deleted_at: d.deleted_at.map(|t| t.to_rfc3339()),
        };
        let mut commit = seal_snapshot(String::new(), d.id.clone(), d.modified_at.to_rfc3339(), snapshot, key)?;
        let wire = SealedWire::of(st, kept);
        let json = serde_json::to_vec(&wire).map_err(|e| P2pError::SyncError(format!("wire serialize: {e}")))?;
        let (ct, nonce) = sovereign_crypto::aead::encrypt(&json, key)
            .map_err(|e| P2pError::SyncError(format!("wire encrypt: {e}")))?;
        commit.sealed_wire = base64::engine::general_purpose::STANDARD.encode(ct);
        commit.wire_nonce = base64::engine::general_purpose::STANDARD.encode(nonce);
        commit.version_device = self.device_id().to_string();
        sign_commit(&mut commit, &self.keypair)?;
        Ok(commit)
    }

    /// Applies received document envelopes through the document service
    /// (C-WIRE 1–13). Envelopes without a wire state (a peer on an older
    /// build, or one stripped in transit) are refused. Every applied change
    /// goes through the incoming-change analysis, which raises an alert only
    /// on a detection. Returns how many documents changed here.
    pub(crate) async fn apply_wire_commits(
        &self,
        docs: &DocumentService,
        commits: Vec<EncryptedCommit>,
        sender: &PeerId,
    ) -> P2pResult<u32> {
        let key = self.pair_key_for(sender)?;
        let sender_key = public_key_from_peer_id(sender).ok_or_else(|| {
            P2pError::SyncError(format!("cannot extract a public key from sender peer id {sender}; rejecting commits"))
        })?;
        let sender_id = sender.to_string();
        let mut opened = vec![];
        for ec in commits {
            if !verify_commit(&ec, &sender_key) || ec.version_device != sender_id {
                tracing::warn!("rejecting document envelope for {}: bad signature or author", ec.document_id);
                continue;
            }
            if ec.sealed_wire.is_empty() || ec.wire_nonce.is_empty() {
                // C-WIRE 12: no document state from an older build.
                tracing::warn!("refusing document {} from {sender_id}: no wire state (older build)", ec.document_id);
                continue;
            }
            let wire = match open_wire(&ec, &key) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("refusing document {}: {e}", ec.document_id);
                    continue;
                }
            };
            let snapshot = match transport_to_snapshot(&ec, &key) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("refusing document {}: {e}", ec.document_id);
                    continue;
                }
            };
            if snapshot.document_id != ec.document_id {
                tracing::warn!("refusing document {}: snapshot names another document", ec.document_id);
                continue;
            }
            opened.push((ec, wire, snapshot));
        }
        // Current states first, then the lost concurrent states relayed with them.
        opened.sort_by_key(|(_, w, _)| w.kept);

        let mut changed = HashSet::new();
        for (ec, wire, snapshot) in opened {
            let modified_at = chrono::DateTime::parse_from_rfc3339(&ec.timestamp)
                .map(|t| t.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());
            let deleted_at = snapshot
                .deleted_at
                .as_deref()
                .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|t| t.with_timezone(&chrono::Utc)).unwrap_or(modified_at));
            let thread_id = if snapshot.thread_id.is_empty() { "default".to_string() } else { snapshot.thread_id };
            let remote = RemoteDocument {
                id: ec.document_id.clone(),
                title: snapshot.title,
                content: snapshot.content,
                thread_id,
                trust: wire.trust.as_deref().and_then(trust_of),
                trust_decision: wire.decision.as_ref().and_then(|d| d.to_decision()),
                provenance: wire.provenance.as_deref().and_then(sovereign_docs::decode_provenance),
                modified_at,
                deleted_at,
                device: sender_id.clone(),
            };
            let meta = WireMeta {
                created_at: wire.created_at,
                stamp: wire.stamp.as_ref().map(stamp_of),
                prev_stamp: wire.prev_stamp.as_ref().map(stamp_of),
                author_device: wire.author_device.clone(),
                decision_stamp: wire.decision_stamp.as_ref().map(stamp_of),
                history: wire.history.iter().map(stamp_of).collect(),
            };
            if wire.kept {
                docs.apply_wire_version(remote, meta).await.map_err(doc_err)?;
                continue;
            }
            let id = remote.id.clone();
            let prior_text = match docs.read(&id).await {
                Ok(v) => analysis_text(&v),
                Err(_) => String::new(),
            };
            let out = docs.apply_wire(remote, meta).await.map_err(doc_err)?;
            if let WireApplied::Applied(_) = out.applied {
                changed.insert(id.clone());
                if let Some(analysis) = &self.analysis {
                    let incoming = docs.read(&id).await.map(|v| analysis_text(&v)).unwrap_or_default();
                    if let Some(finding) = analysis.analyse(&prior_text, &incoming).await {
                        let alert = IncomingAlert {
                            id: id.clone(),
                            device: sender_id.clone(),
                            finding,
                            previous_version: out.prior_version.clone(),
                        };
                        if let Err(e) = docs.raise_alert(alert).await {
                            tracing::warn!("could not record the alert on {id}: {e}");
                        }
                    }
                }
                // The model's verdict, off the sync path (spec v2.0.22): only
                // queued here, never awaited.
                if self.model_verdicts().is_some() {
                    if let Err(e) = docs.queue_model_check(&id, &sender_id, out.prior_version.clone()).await {
                        tracing::warn!("could not queue the model's verdict on {id}: {e}");
                    }
                }
            }
        }
        if let Some(mv) = self.model_verdicts() {
            mv.wake();
        }
        Ok(changed.len() as u32)
    }

    /// Gives sync the model's verdict on received changes, run in the
    /// background (C-WIRE 13, spec v2.0.22). Settable after construction:
    /// the host may build the model later than sync.
    pub fn set_model_verdicts(&self, verdicts: Arc<ModelVerdicts>) {
        if let Ok(mut g) = self.model_verdicts.write() {
            *g = Some(verdicts);
        }
    }

    fn model_verdicts(&self) -> Option<Arc<ModelVerdicts>> {
        self.model_verdicts.read().ok().and_then(|g| g.clone())
    }
}

// ---------------------------------------------------------- model verdicts --

/// The model's verdict on one received change. Implemented in `sovereign-ai`
/// over the orchestrator's router model; sync cannot depend on that crate.
#[async_trait]
pub trait ModelJudge: Send + Sync {
    /// `None`: no model is available now, the change stays queued.
    /// `Some(None)`: the model found nothing (or could not judge; the
    /// deterministic checks already ran). `Some(Some(finding))`: an alert, in
    /// words, no document text.
    async fn judge(&self, prior_text: &str, incoming_text: &str) -> Option<Option<String>>;
}

/// What one pass over the queue did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ModelPass {
    /// Changes the model judged.
    pub judged: usize,
    /// Alerts the model raised.
    pub alerts: usize,
    /// Changes finished without a verdict: the received state was replaced,
    /// trashed or purged before the model reached it.
    pub superseded: usize,
    /// Changes left queued because no model is available.
    pub waiting: usize,
}

/// The background worker giving every received document change the model's
/// verdict (C-WIRE 13, spec v2.0.22, decided by Céline 2026-10-15). Sync only
/// queues (a field on the document row, so a quit loses nothing: the queue
/// is taken up again when the worker starts) and wakes the worker; the
/// worker waits on the model, never sync.
pub struct ModelVerdicts {
    docs: Arc<DocumentService>,
    judge: Arc<dyn ModelJudge>,
    wake: Arc<tokio::sync::Notify>,
    /// One pass at a time.
    pass: tokio::sync::Mutex<()>,
    /// Told when the model raised an alert (the host re-reads the alerts).
    on_alert: std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

/// How often the worker looks at the queue without being woken (a model
/// built after the last sync).
const MODEL_RETRY: std::time::Duration = std::time::Duration::from_secs(60);

impl ModelVerdicts {
    /// The worker, without its task: [`run_pending`](Self::run_pending) is
    /// called by whoever drives it.
    pub fn new(docs: Arc<DocumentService>, judge: Arc<dyn ModelJudge>) -> Arc<Self> {
        Arc::new(Self {
            docs,
            judge,
            wake: Arc::new(tokio::sync::Notify::new()),
            pass: tokio::sync::Mutex::new(()),
            on_alert: std::sync::Mutex::new(None),
        })
    }

    /// The worker and its background task on the current runtime: a first
    /// pass at once (the changes a quit interrupted), then a pass on every
    /// wake and every minute.
    pub fn spawn(docs: Arc<DocumentService>, judge: Arc<dyn ModelJudge>) -> Arc<Self> {
        let me = Self::new(docs, judge);
        // The task holds the worker weakly: it ends when the host drops it
        // (a persona switch, a quit).
        let worker = Arc::downgrade(&me);
        let wake = me.wake.clone();
        tokio::spawn(async move {
            loop {
                let Some(me) = worker.upgrade() else { return };
                let pass = me.run_pending().await;
                if pass.judged + pass.superseded > 0 || pass.waiting > 0 {
                    tracing::debug!("model verdicts: {pass:?}");
                }
                drop(me);
                let _ = tokio::time::timeout(MODEL_RETRY, wake.notified()).await;
            }
        });
        me
    }

    /// Called after each alert the model raises (never while a lock is held).
    pub fn set_on_alert(&self, f: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut g) = self.on_alert.lock() {
            *g = Some(f);
        }
    }

    /// Asks the worker for a pass. Never waits.
    pub fn wake(&self) {
        self.wake.notify_one();
    }

    /// One pass over the queue. Stops at the first change the model cannot
    /// judge now (no model): the rest wait for the next pass.
    pub async fn run_pending(&self) -> ModelPass {
        let _one = self.pass.lock().await;
        let mut pass = ModelPass::default();
        let checks = match self.docs.pending_model_checks().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("model verdicts: the queue could not be read: {e}");
                return pass;
            }
        };
        let total = checks.len();
        for (i, check) in checks.into_iter().enumerate() {
            let texts = match self.docs.model_check_texts(&check).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("model verdicts: {} could not be read: {e}", check.id);
                    continue;
                }
            };
            let Some((prior, incoming)) = texts else {
                tracing::info!("{}: not analysed by the model (replaced before the analysis ran)", check.id);
                let _ = self.docs.finish_model_check(&check).await;
                pass.superseded += 1;
                continue;
            };
            let Some(verdict) = self.judge.judge(&prior, &incoming).await else {
                pass.waiting = total - i;
                break;
            };
            pass.judged += 1;
            if let Some(finding) = verdict {
                // One alert per document: an open alert (the deterministic
                // checks', or an earlier one) is kept as it is.
                if !self.docs.has_alert(&check.id).await.unwrap_or(true) {
                    let alert = IncomingAlert {
                        id: check.id.clone(),
                        device: check.device.clone(),
                        finding,
                        previous_version: check.previous_version.clone(),
                    };
                    match self.docs.raise_alert(alert).await {
                        Ok(()) => {
                            pass.alerts += 1;
                            let f = self.on_alert.lock().ok().and_then(|g| g.clone());
                            if let Some(f) = f {
                                f();
                            }
                        }
                        Err(e) => tracing::warn!("could not record the alert on {}: {e}", check.id),
                    }
                }
            }
            if let Err(e) = self.docs.finish_model_check(&check).await {
                tracing::warn!("model verdicts: {} stays queued: {e}", check.id);
            }
        }
        pass
    }
}

/// The wire state sealed beside a document snapshot (C-WIRE *What travels*).
#[derive(Serialize, Deserialize, Default)]
struct SealedWire {
    #[serde(default)]
    trust: Option<String>,
    #[serde(default)]
    decision: Option<SealedDecision>,
    #[serde(default)]
    provenance: Option<String>,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    stamp: Option<WireStamp>,
    #[serde(default)]
    prev_stamp: Option<WireStamp>,
    #[serde(default)]
    history: Vec<WireStamp>,
    #[serde(default)]
    author_device: Option<String>,
    #[serde(default)]
    decision_stamp: Option<WireStamp>,
    /// A concurrent state the sender lost and keeps as a version (C-WIRE 7):
    /// stored as a version, never made current.
    #[serde(default)]
    kept: bool,
}

#[derive(Serialize, Deserialize)]
struct SealedDecision {
    trust: String,
    device: String,
    decided_at: chrono::DateTime<chrono::Utc>,
}

impl SealedDecision {
    fn to_decision(&self) -> Option<TrustDecision> {
        Some(TrustDecision { trust: trust_of(&self.trust)?, device: self.device.clone(), decided_at: self.decided_at })
    }
}

impl SealedWire {
    fn of(st: &WireState, kept: bool) -> Self {
        let d = &st.document;
        SealedWire {
            trust: d.trust.map(|t| trust_name(t).to_string()),
            decision: d.trust_decision.as_ref().map(|t| SealedDecision {
                trust: trust_name(t.trust).to_string(),
                device: t.device.clone(),
                decided_at: t.decided_at,
            }),
            provenance: d.provenance.as_ref().map(sovereign_docs::encode_provenance),
            created_at: st.meta.created_at,
            stamp: st.meta.stamp.as_ref().map(wire_stamp),
            prev_stamp: st.meta.prev_stamp.as_ref().map(wire_stamp),
            history: st.meta.history.iter().map(wire_stamp).collect(),
            author_device: st.meta.author_device.clone(),
            decision_stamp: st.meta.decision_stamp.as_ref().map(wire_stamp),
            kept,
        }
    }
}

fn open_wire(ec: &EncryptedCommit, key: &[u8; 32]) -> P2pResult<SealedWire> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let ct = b64.decode(&ec.sealed_wire).map_err(|e| P2pError::SyncError(format!("wire base64: {e}")))?;
    let nonce_v = b64.decode(&ec.wire_nonce).map_err(|e| P2pError::SyncError(format!("wire nonce base64: {e}")))?;
    let nonce: [u8; 24] =
        nonce_v.try_into().map_err(|_| P2pError::SyncError("wire nonce has the wrong length".into()))?;
    let plain = sovereign_crypto::aead::decrypt(&ct, &nonce, key).map_err(|e| P2pError::SyncError(format!("wire unseal: {e}")))?;
    serde_json::from_slice(&plain).map_err(|e| P2pError::SyncError(format!("wire decode: {e}")))
}

fn trust_name(t: Trust) -> &'static str {
    match t {
        Trust::Owned => "owned",
        Trust::External => "external",
        Trust::Undetermined => "undetermined",
    }
}

fn trust_of(s: &str) -> Option<Trust> {
    match s {
        "owned" => Some(Trust::Owned),
        "external" => Some(Trust::External),
        "undetermined" => Some(Trust::Undetermined),
        _ => None,
    }
}

fn wire_stamp(s: &Stamp) -> WireStamp {
    WireStamp { wall_ms: s.wall_ms, counter: s.counter, device: s.device.clone() }
}

fn stamp_of(s: &WireStamp) -> Stamp {
    Stamp { wall_ms: s.wall_ms, counter: s.counter, device: s.device.clone() }
}

/// What the incoming-change analysis reads of a document state
/// ([`sovereign_docs::analysis_text`]).
fn analysis_text(view: &sovereign_docs::DocumentView) -> String {
    sovereign_docs::analysis_text(&view.title, view.doc.as_ref())
}

fn doc_err(e: sovereign_docs::DocError) -> P2pError {
    P2pError::SyncError(format!("document service: {e}"))
}

/// The document part of a sync round, seen from the local side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentPlan {
    /// Documents to fetch from the remote.
    pub fetch: Vec<String>,
    /// Documents to send to the remote.
    pub push: Vec<String>,
    /// Documents edited on both sides since the last state they shared:
    /// exchanged in both directions, both versions kept (C-WIRE guarantee 7).
    pub concurrent: Vec<String>,
    /// Purge markers to send to the remote.
    pub push_purges: Vec<String>,
    /// The remote runs an older build: no document is exchanged with it
    /// (C-WIRE guarantee 12; rows are planned as before).
    pub blocked_old_peer: bool,
}

/// Plans the document exchange of a round (C-WIRE guarantees 3, 4, 7, 10, 12):
/// compares normalised hashes and stamps, never stored bytes or wall clocks.
///
/// - Equal hashes, or equal stamps: nothing to do.
/// - One side's stamp is an ancestor of the other's (its previous stamp or
///   in its history): the later side's state travels.
/// - A side whose state was never stamped (written before step 4) follows
///   the other; two never-stamped states are ordered by their derived stamps
///   (modification time, then device) — C-WIRE weakness 25.
/// - Otherwise the edits were concurrent: exchanged both ways.
/// - A document only one side holds travels to the other (the receiver's
///   purge marker, if any, decides by fingerprint).
pub fn plan_documents(local: &SyncManifest, remote: &SyncManifest) -> DocumentPlan {
    let mut plan = DocumentPlan::default();
    if remote.wire_version.is_none() {
        plan.blocked_old_peer = true;
        return plan;
    }
    let l: HashMap<&str, &DocumentWireEntry> = local.document_wire.iter().map(|e| (e.doc_id.as_str(), e)).collect();
    let r: HashMap<&str, &DocumentWireEntry> = remote.document_wire.iter().map(|e| (e.doc_id.as_str(), e)).collect();
    let ancestor = |a: &WireStamp, of: &DocumentWireEntry| of.prev_stamp.as_ref() == Some(a) || of.history.contains(a);
    for re in &remote.document_wire {
        match l.get(re.doc_id.as_str()) {
            None => plan.fetch.push(re.doc_id.clone()),
            Some(le) => {
                if le.normalized_hash == re.normalized_hash || le.stamp == re.stamp {
                    continue;
                }
                let lt = (le.stamp.wall_ms, le.stamp.counter, le.stamp.device.as_str());
                let rt = (re.stamp.wall_ms, re.stamp.counter, re.stamp.device.as_str());
                if ancestor(&le.stamp, re) || (!le.stamped && re.stamped) {
                    plan.fetch.push(re.doc_id.clone());
                } else if ancestor(&re.stamp, le) || (le.stamped && !re.stamped) {
                    plan.push.push(re.doc_id.clone());
                } else if !le.stamped && !re.stamped {
                    if rt > lt {
                        plan.fetch.push(re.doc_id.clone());
                    } else {
                        plan.push.push(re.doc_id.clone());
                    }
                } else {
                    plan.concurrent.push(re.doc_id.clone());
                }
            }
        }
    }
    for le in &local.document_wire {
        if !r.contains_key(le.doc_id.as_str()) {
            plan.push.push(le.doc_id.clone());
        }
    }
    for m in &local.purge_markers {
        if r.contains_key(m.doc_id.as_str()) {
            plan.push_purges.push(m.doc_id.clone());
        }
    }
    plan
}

/// The row tables a sync overwrite can be restored from — one list, used by
/// sync and by both *previous values* lists (C-WIRE guarantee 15).
pub fn restorable_tables() -> &'static [SyncTable] {
    &[
        SyncTable::Thread,
        SyncTable::Entity,
        SyncTable::PiiRecord,
        SyncTable::Contact,
        SyncTable::Message,
        SyncTable::Conversation,
        SyncTable::SuggestedLink,
    ]
}

/// Restores a guardian backup snapshot through the document service (C-WIRE
/// guarantee 17): refused on a workspace that holds any document; rows are
/// restored as before on an empty one.
pub async fn restore_backup(
    documents: &DocumentService,
    db: &dyn GraphDB,
    snapshot: &BackupSnapshot,
) -> Result<BackupReport, BackupError> {
    let docs = snapshot
        .documents
        .iter()
        .map(|row| sovereign_docs::backup_document_from_row(row, &snapshot.device_id))
        .collect();
    let markers = snapshot.purge_markers.iter().filter_map(sovereign_docs::purge_marker_wire).collect();
    // The documents first: a refusal (a workspace that holds documents)
    // happens before anything is written.
    let report = documents.restore_backup(docs, markers).await?;
    // The threads through the one thread-write path, with the stamps they
    // were backed up with (THREAD-sync S1); an id already here stays as it is.
    for t in &snapshot.threads {
        documents.thread_restore_backup(t.clone()).await.map_err(BackupError::Doc)?;
    }
    crate::backup::restore_snapshot(db, snapshot)
        .await
        .map_err(|e| BackupError::Doc(sovereign_docs::DocError::Store(e.to_string())))?;
    Ok(report)
}
