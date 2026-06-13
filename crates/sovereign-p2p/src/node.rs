use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
use rand::Rng as _;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::behaviour::{SovereignBehaviour, SovereignBehaviourEvent};
use crate::config::P2pConfig;
use crate::error::{P2pError, P2pResult};
use crate::protocol::manifest::{EncryptedManifest, SyncManifest};
use crate::protocol::sync::SyncTable;
use crate::protocol::{SovereignRequest, SovereignResponse};
use crate::sync_engine;
use crate::sync_service::SyncService;

pub(crate) const PROTOCOL_NAME: &str = "/sovereign/sync/1";

/// Events emitted by the P2P node to the rest of the application.
#[derive(Debug, Clone)]
pub enum P2pEvent {
    PeerDiscovered { peer_id: String, device_name: Option<String> },
    PeerLost { peer_id: String },
    SyncStarted { peer_id: String },
    SyncCompleted { peer_id: String, docs_synced: u32 },
    SyncConflict { doc_id: String, description: String },
    ShardReceived { shard_id: String, from_peer: String },
    PairingRequested { peer_id: String, device_name: String },
    PairingCompleted { peer_id: String, device_name: String },
    /// A backup placement job for one peer finished (P4.2).
    BackupPlaced { peer_id: String, accepted: u32, rejected: u32 },
    /// A recovery request for a guardian shard we hold arrived and is
    /// pending this user's approval (P4.3). Surfaced to the UI.
    ShardRequested { request_id: String, for_user: String, epoch: u32 },
    /// A pairing handshake step failed (bad code, expired offer, ...).
    /// `offer_dead` is true when the offer self-destructed (expired or
    /// attempts exhausted) and the UI should regenerate the QR.
    PairingFailed { reason: String, offer_dead: bool },
    /// The swarm is reachable on a new (interface-expanded) address.
    /// The app collects these so pairing offers can carry real dial
    /// hints instead of relying on mDNS discovery (P3.1).
    ListenAddr { address: String },
}

/// Per-pair sealing keys (P1.4 / P2P-005) wrapped so the Debug impl on
/// `P2pCommand` can never leak key bytes into logs.
pub struct PairKeyMap(pub HashMap<String, [u8; 32]>);

impl std::fmt::Debug for PairKeyMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PairKeyMap({} key(s), [REDACTED])", self.0.len())
    }
}

/// Responder-side state for one active pairing offer (P3.1). Created by
/// the app when it renders the pairing QR and handed to the node via
/// [`P2pCommand::SetPairingOffer`]; the node then runs the handshake
/// autonomously. Single-use: consumed on success, destroyed on expiry or
/// after [`crate::pairing_offer::MAX_PROOF_ATTEMPTS`] bad proofs.
pub struct ActivePairingOffer {
    pub offer_id: String,
    /// Argon2id-stretched handshake key (the pairing code never reaches
    /// the node).
    pub handshake_key: [u8; 32],
    /// Unix milliseconds.
    pub expires_at_ms: i64,
    /// MasterKey salt released to the new device on a valid proof.
    pub salt: Vec<u8>,
    /// AccountKey released to the new device on a valid proof. Also used
    /// to derive the per-pair sealing key once the final identity is
    /// confirmed.
    pub account_key: [u8; 32],
    /// This (existing) device's human-readable name.
    pub device_name: String,
    /// Wrong proofs left before the offer self-destructs.
    attempts_left: u8,
    /// In-flight session: (dialer, challenge nonce, proof passed).
    session: Option<PairingSession>,
}

struct PairingSession {
    dialer: PeerId,
    nonce: [u8; 32],
    proven: bool,
}

impl ActivePairingOffer {
    pub fn new(
        offer_id: String,
        handshake_key: [u8; 32],
        expires_at_ms: i64,
        salt: Vec<u8>,
        account_key: [u8; 32],
        device_name: String,
    ) -> Self {
        Self {
            offer_id,
            handshake_key,
            expires_at_ms,
            salt,
            account_key,
            device_name,
            attempts_left: crate::pairing_offer::MAX_PROOF_ATTEMPTS,
            session: None,
        }
    }

    fn expired(&self) -> bool {
        chrono::Utc::now().timestamp_millis() > self.expires_at_ms
    }
}

impl Drop for ActivePairingOffer {
    fn drop(&mut self) {
        // SIDECHANNEL-002: scrub the released secret material (handshake key,
        // AccountKey, MasterKey salt) when the offer is consumed or expires, so
        // it doesn't linger in freed memory.
        use zeroize::Zeroize;
        self.handshake_key.zeroize();
        self.account_key.zeroize();
        self.salt.zeroize();
    }
}

impl Drop for PairingSession {
    fn drop(&mut self) {
        // SIDECHANNEL-002: scrub the challenge nonce on drop.
        use zeroize::Zeroize;
        self.nonce.zeroize();
    }
}

impl std::fmt::Debug for ActivePairingOffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivePairingOffer")
            .field("offer_id", &self.offer_id)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("attempts_left", &self.attempts_left)
            .field("secrets", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Commands sent to the P2P node from the application.
#[derive(Debug)]
pub enum P2pCommand {
    StartSync { peer_id: String },
    PairDevice { peer_id: String },
    /// Replace the node's paired-peer allow-list. The app sends this at
    /// P2P startup (from the persisted PairingManager) and whenever a
    /// device is paired/unpaired. Only peers in this set may be served
    /// sync data or be sync-initiated against — see P2P-001.
    UpdatePairedPeers { peer_ids: Vec<String> },
    /// Place backup fragments on one host peer (P4.2): sends one
    /// `StoreBackupFragment` per entry and emits `BackupPlaced` when
    /// every ack (or failure) has arrived.
    PlaceBackup {
        peer_id: String,
        requests: Vec<SovereignRequest>,
    },
    /// Replace the SyncService's per-pair sealing keys (P1.4 / P2P-005).
    /// Sent alongside `UpdatePairedPeers` whenever pairing changes, so an
    /// unpaired device loses its sealing key in the same breath as its
    /// allow-list entry.
    UpdatePairKeys { keys: PairKeyMap },
    /// Arm the node with a pairing offer (P3.1). The node answers the
    /// PairHello/PairProof/PairComplete handshake against it and emits
    /// `PairingCompleted` when a new device finishes. Replaces any
    /// previously armed offer.
    SetPairingOffer { offer: Box<ActivePairingOffer> },
    /// Disarm the current pairing offer (user closed the pairing screen).
    ClearPairingOffer,
    DistributeShard {
        peer_id: String,
        shard_data: String,
        shard_id: String,
        /// Owner tag the shard belongs to (P4: the guardian files it
        /// under this and a recovery request quotes it).
        for_user: String,
        epoch: u32,
    },
    SendRequest { peer_id: PeerId, request: SovereignRequest },
    /// Dial a peer's multiaddr directly (bypassing mDNS discovery).
    /// Used for tests and for explicit "connect to address" UI flows.
    /// `address` should be a full Multiaddr including the `/p2p/<peer_id>`
    /// suffix.
    Dial { address: String },
    Shutdown,
}

/// Per-peer sync session bookkeeping. A session begins when
/// `P2pCommand::StartSync` issues the initial `GetManifest`, accumulates
/// pending follow-up requests as the manifest is processed, and closes
/// (emitting `SyncCompleted`) when every in-flight request for that peer
/// has been acked or failed.
#[derive(Default, Debug)]
struct PeerSyncState {
    /// Outbound requests we've sent in this session that haven't yet
    /// resolved (Response or OutboundFailure). When this reaches 0,
    /// `SyncCompleted` is emitted and the session is removed.
    pending_responses: u32,
    /// Sum of `apply_commits` results across the session.
    docs_synced: u32,
    /// Sum of `apply_rows` written counts across the session.
    rows_synced: u32,
    /// Number of document conflicts surfaced during this session
    /// (each is also emitted as a `P2pEvent::SyncConflict`).
    conflicts: u32,
}

/// What kind of in-flight request an `OutboundRequestId` corresponds
/// to, so the response handler can route follow-ups correctly.
#[derive(Debug, Clone, Copy)]
enum InflightKind {
    /// Initial `GetManifest` — response triggers diff + follow-ups.
    Manifest,
    /// `GetCommits` — response is `Commits { commits }`, apply locally.
    Commits,
    /// `PushCommits` — expecting an `Ok` ack.
    PushCommitsAck,
    /// `GetRows` — response is `Rows { table, rows }`, apply locally.
    Rows(SyncTable),
    /// `PushRows` — expecting a `PushAck` with per-row counts.
    PushRowsAck,
    /// `StoreBackupFragment` — expecting a `BackupStored` ack (P4.2).
    BackupStoreAck,
}

/// Per-peer bookkeeping for an in-flight backup placement job.
#[derive(Default)]
struct BackupJob {
    pending: u32,
    accepted: u32,
    rejected: u32,
}

/// The Sovereign P2P node. Runs a libp2p Swarm in an async event loop.
pub struct SovereignNode {
    swarm: Swarm<SovereignBehaviour>,
    event_tx: mpsc::Sender<P2pEvent>,
    command_rx: mpsc::Receiver<P2pCommand>,
    sync_service: Arc<SyncService>,
    /// Outbound request id → (peer, kind). Lets the response handler
    /// route follow-ups and finalize the session.
    inflight: HashMap<OutboundRequestId, (PeerId, InflightKind)>,
    /// Per-peer aggregate counters for the active sync session.
    sessions: HashMap<PeerId, PeerSyncState>,
    /// Allow-list of paired peer-id strings. A code-level HARD BARRIER
    /// (P2P-001): inbound sync requests from peers NOT in this set are
    /// rejected, and `StartSync` against an unpaired peer is dropped — so
    /// a random LAN peer can neither read nor poison the DB even though
    /// mDNS discovers it and the app may auto-trigger sync. Mutated only
    /// from the single-threaded event loop via `UpdatePairedPeers`, so a
    /// plain `HashSet` (no lock) is sufficient.
    paired_peers: HashSet<String>,
    /// Per-peer high-water mark of manifest `generated_at` timestamps
    /// (P2P-003 replay guard) — see `check_manifest_freshness`.
    manifest_seen: HashMap<PeerId, chrono::DateTime<chrono::Utc>>,
    /// The active pairing offer, if the user has the pairing screen open
    /// (P3.1). Mutated only from the event loop.
    pairing_offer: Option<ActivePairingOffer>,
    /// Opt-in backup host store (P4.2). None = this device doesn't host
    /// other users' fragments or guardian shards.
    backup_host: Option<Arc<crate::backup_host::BackupHost>>,
    /// In-flight backup placement jobs per peer (P4.2).
    backup_jobs: HashMap<PeerId, BackupJob>,
}

impl SovereignNode {
    /// Create a new P2P node.
    pub fn new(
        config: &P2pConfig,
        keypair: libp2p::identity::Keypair,
        event_tx: mpsc::Sender<P2pEvent>,
        command_rx: mpsc::Receiver<P2pCommand>,
        sync_service: Arc<SyncService>,
        backup_host: Option<Arc<crate::backup_host::BackupHost>>,
    ) -> P2pResult<Self> {
        let peer_id = keypair.public().to_peer_id();
        let enable_mdns = config.enable_mdns;
        info!(
            "P2P node starting with PeerId: {} (mdns: {})",
            peer_id, enable_mdns
        );

        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| {
                // mDNS is toggleable (P2P-006). When disabled, the node does
                // no LAN multicast / auto-discovery at all.
                let mdns = if enable_mdns {
                    let m = libp2p::mdns::tokio::Behaviour::new(
                        libp2p::mdns::Config::default(),
                        peer_id,
                    )
                    .map_err(|e| P2pError::Transport(e.to_string()))?;
                    libp2p::swarm::behaviour::toggle::Toggle::from(Some(m))
                } else {
                    libp2p::swarm::behaviour::toggle::Toggle::from(None)
                };

                let rendezvous = libp2p::rendezvous::client::Behaviour::new(key.clone());

                let request_response = libp2p::request_response::cbor::Behaviour::<
                    SovereignRequest,
                    SovereignResponse,
                >::new(
                    [(
                        StreamProtocol::new(PROTOCOL_NAME),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                );

                let identify = libp2p::identify::Behaviour::new(
                    libp2p::identify::Config::new(
                        "/sovereign/id/1".to_string(),
                        key.public(),
                    )
                    // P2P-003: advertise a bare agent string with NO build
                    // version. The exact `sovereign/<x.y.z>` was a free
                    // version-fingerprint to anyone on an untrusted LAN; the
                    // protocol id "/sovereign/id/1" already carries the wire
                    // version we actually negotiate on.
                    .with_agent_version("sovereign".to_string()),
                );

                Ok(SovereignBehaviour {
                    mdns,
                    rendezvous,
                    request_response,
                    identify,
                })
            })
            .map_err(|e| P2pError::Transport(e.to_string()))?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(60))
            })
            .build();

        Ok(Self {
            swarm,
            event_tx,
            command_rx,
            sync_service,
            inflight: HashMap::new(),
            sessions: HashMap::new(),
            paired_peers: HashSet::new(),
            manifest_seen: HashMap::new(),
            pairing_offer: None,
            backup_host,
            backup_jobs: HashMap::new(),
        })
    }

    /// Start listening on the configured port.
    pub fn listen(&mut self, config: &P2pConfig) -> P2pResult<Multiaddr> {
        let addr: Multiaddr = format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| P2pError::Transport(e.to_string()))?;

        let listen_id = self
            .swarm
            .listen_on(addr.clone())
            .map_err(|e| P2pError::Transport(e.to_string()))?;

        info!("Listening with id {:?}", listen_id);

        Ok(addr)
    }

    /// Run the event loop. Blocks until shutdown.
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                swarm_event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(swarm_event).await;
                }
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(P2pCommand::Shutdown) | None => {
                            info!("P2P node shutting down");
                            return;
                        }
                        Some(cmd) => self.handle_command(cmd).await,
                    }
                }
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<SovereignBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                self.handle_behaviour_event(behaviour_event).await;
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {}", address);
                let _ = self
                    .event_tx
                    .send(P2pEvent::ListenAddr {
                        address: address.to_string(),
                    })
                    .await;
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connected to {}", peer_id);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Disconnected from {}", peer_id);
            }
            _ => {}
        }
    }

    async fn handle_behaviour_event(&mut self, event: SovereignBehaviourEvent) {
        match event {
            SovereignBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(peers)) => {
                for (peer_id, addr) in peers {
                    info!("mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm.add_peer_address(peer_id, addr);
                    let _ = self.event_tx.send(P2pEvent::PeerDiscovered {
                        peer_id: peer_id.to_string(),
                        device_name: None,
                    }).await;
                }
            }
            SovereignBehaviourEvent::Mdns(libp2p::mdns::Event::Expired(peers)) => {
                for (peer_id, _) in peers {
                    info!("mDNS peer expired: {}", peer_id);
                    let _ = self.event_tx.send(P2pEvent::PeerLost {
                        peer_id: peer_id.to_string(),
                    }).await;
                }
            }
            SovereignBehaviourEvent::RequestResponse(event) => {
                self.handle_request_response(event).await;
            }
            SovereignBehaviourEvent::Identify(event) => {
                if let libp2p::identify::Event::Received { peer_id, info, .. } = event {
                    info!("Identified peer {}: agent={}", peer_id, info.agent_version);
                    for addr in info.listen_addrs {
                        self.swarm.add_peer_address(peer_id, addr);
                    }
                }
            }
            SovereignBehaviourEvent::Rendezvous(event) => {
                tracing::debug!("Rendezvous event: {:?}", event);
            }
        }
    }

    async fn handle_request_response(
        &mut self,
        event: libp2p::request_response::Event<SovereignRequest, SovereignResponse>,
    ) {
        use libp2p::request_response::Event;
        match event {
            Event::Message { peer, message, .. } => {
                use libp2p::request_response::Message;
                match message {
                    Message::Request { request, channel, .. } => {
                        info!("Request from {}: {:?}", peer, std::mem::discriminant(&request));
                        // P3.1: the pairing handshake is handled by the node
                        // itself (it owns the offer state) and is allowed from
                        // unpaired peers by design — that's the whole point.
                        let response = if let Some(resp) =
                            self.handle_pairing_request(peer, &request).await
                        {
                            resp
                        } else if self.is_sync_request(&request)
                            && !self.paired_peers.contains(&peer.to_string())
                        {
                            // P2P-001 hard barrier: only serve sync data to
                            // PAIRED peers. An unpaired peer (any random device
                            // on the LAN that mDNS surfaced) is refused before
                            // process_request ever touches the DB — no
                            // manifest, commits, rows, or PII leave this node,
                            // and no PushCommits/PushRows can poison it.
                            warn!(
                                "Rejecting {:?} from UNPAIRED peer {} (P2P-001)",
                                std::mem::discriminant(&request),
                                peer
                            );
                            SovereignResponse::Error {
                                message: "peer not paired".into(),
                            }
                        } else {
                            process_request(
                                request,
                                peer,
                                &self.event_tx,
                                &self.sync_service,
                                self.backup_host.as_deref(),
                            )
                            .await
                        };
                        if self.swarm.behaviour_mut().request_response.send_response(channel, response).is_err() {
                            warn!("Failed to send response to {}", peer);
                        }
                    }
                    Message::Response { request_id, response } => {
                        match self.inflight.remove(&request_id) {
                            Some((peer_id, kind)) => {
                                self.handle_sync_response(peer_id, kind, response).await;
                            }
                            None => {
                                info!(
                                    "Untracked response from {}: {:?}",
                                    peer,
                                    std::mem::discriminant(&response)
                                );
                            }
                        }
                    }
                }
            }
            Event::OutboundFailure { peer, request_id, error, .. } => {
                warn!("Outbound request {request_id} to {peer} failed: {error:?}");
                if let Some((peer_id, kind)) = self.inflight.remove(&request_id) {
                    if matches!(kind, InflightKind::BackupStoreAck) {
                        self.note_backup_ack(&peer_id, false).await;
                    } else {
                        self.decrement_pending(&peer_id).await;
                    }
                }
            }
            Event::InboundFailure { peer, error, .. } => {
                warn!("Inbound request from {} failed: {:?}", peer, error);
            }
            Event::ResponseSent { peer, .. } => {
                tracing::debug!("Response sent to {}", peer);
            }
        }
    }

    /// Route a response from a peer back into the active sync session.
    async fn handle_sync_response(
        &mut self,
        peer_id: PeerId,
        kind: InflightKind,
        response: SovereignResponse,
    ) {
        match (kind, response) {
            (InflightKind::Manifest, SovereignResponse::Manifest(em)) => {
                self.handle_manifest_response(peer_id, em).await;
            }
            (InflightKind::Commits, SovereignResponse::Commits { commits }) => {
                match self.sync_service.apply_commits(commits, &peer_id).await {
                    Ok(n) => {
                        if let Some(s) = self.sessions.get_mut(&peer_id) {
                            s.docs_synced += n;
                        }
                    }
                    Err(e) => warn!("apply_commits from {peer_id} failed: {e}"),
                }
            }
            (InflightKind::PushCommitsAck, SovereignResponse::Ok) => {
                // Peer accepted; their apply_commits result isn't reported back.
            }
            (InflightKind::Rows(table), SovereignResponse::Rows { table: t, rows }) => {
                if t == table {
                    // P1.3: rows must be signed by the peer we requested
                    // them from — pass the sender for verification.
                    match self.sync_service.apply_rows(table, rows, &peer_id).await {
                        Ok((written, _skipped)) => {
                            if let Some(s) = self.sessions.get_mut(&peer_id) {
                                s.rows_synced += written;
                            }
                        }
                        Err(e) => {
                            warn!("apply_rows({:?}) from {peer_id} failed: {e}", table);
                        }
                    }
                } else {
                    warn!(
                        "Rows table mismatch from {peer_id}: requested {:?}, got {:?}",
                        table, t
                    );
                }
            }
            (InflightKind::PushRowsAck, SovereignResponse::PushAck { written, skipped: _ }) => {
                if let Some(s) = self.sessions.get_mut(&peer_id) {
                    s.rows_synced += written;
                }
            }
            (InflightKind::BackupStoreAck, SovereignResponse::BackupStored { accepted }) => {
                self.note_backup_ack(&peer_id, accepted).await;
                return; // not part of a sync session
            }
            (InflightKind::BackupStoreAck, _) => {
                // Error or unexpected shape — count as rejected.
                self.note_backup_ack(&peer_id, false).await;
                return;
            }
            (_, SovereignResponse::Error { message }) => {
                warn!("Peer {peer_id} returned error: {message}");
            }
            (kind, response) => {
                tracing::debug!(
                    "Unexpected response shape (kind={:?}, response={:?}) from {peer_id}",
                    kind,
                    std::mem::discriminant(&response),
                );
            }
        }
        self.decrement_pending(&peer_id).await;
    }

    /// Validate a peer manifest's `generated_at` against the replay rules
    /// and bump the per-peer high-water mark on success. This bounds the
    /// replay window rather than eliminating it — the deep fix (per-device
    /// signed monotonic counters) is tracked for the sync rework.
    fn check_manifest_freshness(
        &mut self,
        peer_id: &PeerId,
        generated_at: &str,
    ) -> Result<(), String> {
        let ts = validate_manifest_timestamp(
            generated_at,
            chrono::Utc::now(),
            self.manifest_seen.get(peer_id).copied(),
        )?;
        self.manifest_seen.insert(*peer_id, ts);
        Ok(())
    }

    /// Decode a manifest response, diff against local, and dispatch the
    /// follow-up `GetCommits` / `PushCommits` / `GetRows` / `PushRows`
    /// requests. Each follow-up bumps `pending_responses` so the session
    /// only finalizes once everything has been acked.
    async fn handle_manifest_response(
        &mut self,
        peer_id: PeerId,
        encrypted: EncryptedManifest,
    ) {
        // P2P-002: manifests are AEAD-sealed under the per-account transport
        // key. A peer that can't produce a manifest under our key (or sends
        // the old plaintext shape) fails to decrypt and is dropped.
        let remote = match SyncManifest::decrypt(&encrypted, self.sync_service.transport_key()) {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to decrypt manifest from {peer_id}: {e}");
                return;
            }
        };
        // P2P-003 (replay guard): AEAD seals the manifest but nothing else
        // binds it to *this* exchange, so a captured ciphertext could be
        // replayed to roll sync state back. Bind it to freshness instead.
        if let Err(reason) = self.check_manifest_freshness(&peer_id, &remote.generated_at) {
            warn!("Rejecting manifest from {peer_id}: {reason}");
            return;
        }
        let local = match self.sync_service.build_manifest().await {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to build local manifest: {e}");
                return;
            }
        };

        // ----- Documents (commit-chain track) -----
        let doc_diff = sync_engine::compute_diff(&local, &remote);

        // Fetch commits we need from the remote. For each doc we don't
        // have or that the remote is ahead on, ask for the head_commit;
        // the chain will be replayed in order by `apply_commits`.
        if !doc_diff.need_from_remote.is_empty() {
            let commit_ids: Vec<String> = doc_diff
                .need_from_remote
                .iter()
                .filter_map(|doc_id| {
                    remote
                        .documents
                        .iter()
                        .find(|e| &e.doc_id == doc_id)
                        .and_then(|e| e.head_commit.clone())
                })
                .collect();
            if !commit_ids.is_empty() {
                self.send_session_request(
                    peer_id,
                    InflightKind::Commits,
                    SovereignRequest::GetCommits { commit_ids },
                );
            }
        }

        // Push commits the remote needs from us.
        if !doc_diff.push_to_remote.is_empty() {
            let mut commits = Vec::new();
            for doc_id in &doc_diff.push_to_remote {
                let since = remote
                    .documents
                    .iter()
                    .find(|e| &e.doc_id == doc_id)
                    .and_then(|e| e.head_commit.clone());
                if let Ok(cs) = self
                    .sync_service
                    .get_commits_since(doc_id, since.as_deref(), &peer_id)
                    .await
                {
                    commits.extend(cs);
                }
            }
            if !commits.is_empty() {
                self.send_session_request(
                    peer_id,
                    InflightKind::PushCommitsAck,
                    SovereignRequest::PushCommits { commits },
                );
            }
        }

        // Surface document conflicts to the UI; LWW tables resolve silently.
        for conflict in doc_diff.conflicts {
            let _ = self
                .event_tx
                .send(P2pEvent::SyncConflict {
                    doc_id: conflict.doc_id.clone(),
                    description: format!(
                        "diverged: local commits={}, remote commits={}",
                        conflict.local_commit_count, conflict.remote_commit_count
                    ),
                })
                .await;
            if let Some(s) = self.sessions.get_mut(&peer_id) {
                s.conflicts += 1;
            }
        }

        // ----- Row-level tables (LWW) -----
        let row_diffs = sync_engine::compute_all_row_diffs(&local, &remote);
        for (table, rd) in row_diffs {
            if !rd.need_from_remote.is_empty() {
                self.send_session_request(
                    peer_id,
                    InflightKind::Rows(table),
                    SovereignRequest::GetRows {
                        table,
                        ids: rd.need_from_remote,
                    },
                );
            }
            if !rd.push_to_remote.is_empty() {
                if let Ok(rows) = self
                    .sync_service
                    .get_rows(table, &rd.push_to_remote, &peer_id)
                    .await
                {
                    if !rows.is_empty() {
                        self.send_session_request(
                            peer_id,
                            InflightKind::PushRowsAck,
                            SovereignRequest::PushRows { table, rows },
                        );
                    }
                }
            }
        }
    }

    /// Issue an outbound request inside an existing sync session,
    /// recording the inflight bookkeeping so the response handler can
    /// route the eventual reply.
    fn send_session_request(
        &mut self,
        peer_id: PeerId,
        kind: InflightKind,
        request: SovereignRequest,
    ) {
        let req_id = self
            .swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer_id, request);
        self.inflight.insert(req_id, (peer_id, kind));
        if let Some(s) = self.sessions.get_mut(&peer_id) {
            s.pending_responses += 1;
        }
    }

    /// Record one ack (or failure) for a backup placement job and emit
    /// `BackupPlaced` when the job drains (P4.2).
    async fn note_backup_ack(&mut self, peer_id: &PeerId, accepted: bool) {
        let done = if let Some(job) = self.backup_jobs.get_mut(peer_id) {
            if accepted {
                job.accepted += 1;
            } else {
                job.rejected += 1;
            }
            job.pending = job.pending.saturating_sub(1);
            job.pending == 0
        } else {
            false
        };
        if done {
            if let Some(job) = self.backup_jobs.remove(peer_id) {
                let _ = self
                    .event_tx
                    .send(P2pEvent::BackupPlaced {
                        peer_id: peer_id.to_string(),
                        accepted: job.accepted,
                        rejected: job.rejected,
                    })
                    .await;
            }
        }
    }

    /// Decrement the pending-response counter for a peer's session and,
    /// if it hits zero, emit `SyncCompleted` and remove the session.
    async fn decrement_pending(&mut self, peer_id: &PeerId) {
        let mut completed = None;
        if let Some(state) = self.sessions.get_mut(peer_id) {
            if state.pending_responses > 0 {
                state.pending_responses -= 1;
            }
            if state.pending_responses == 0 {
                completed = Some((state.docs_synced, state.rows_synced));
            }
        }
        if let Some((docs, rows)) = completed {
            self.sessions.remove(peer_id);
            // Surface a single combined "items synced" count. Document
            // commits and LWW rows are both content the user cares about.
            let _ = self
                .event_tx
                .send(P2pEvent::SyncCompleted {
                    peer_id: peer_id.to_string(),
                    docs_synced: docs + rows,
                })
                .await;
        }
    }

    async fn handle_command(&mut self, cmd: P2pCommand) {
        match cmd {
            P2pCommand::SendRequest { peer_id, request } => {
                self.swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer_id, request);
            }
            P2pCommand::DistributeShard { peer_id, shard_data, shard_id, for_user, epoch } => {
                if let Ok(pid) = peer_id.parse::<PeerId>() {
                    let req = SovereignRequest::DeliverShard(
                        crate::protocol::guardian::ShardDeliveryRequest {
                            shard_data,
                            shard_id,
                            for_user,
                            epoch,
                        },
                    );
                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&pid, req);
                } else {
                    warn!("Invalid peer ID: {}", peer_id);
                }
            }
            P2pCommand::UpdatePairedPeers { peer_ids } => {
                let n = peer_ids.len();
                self.paired_peers = peer_ids.into_iter().collect();
                info!("Paired-peer allow-list updated: {n} peer(s)");
            }
            P2pCommand::UpdatePairKeys { keys } => {
                let n = keys.0.len();
                self.sync_service.set_pair_keys(keys.0);
                info!("Per-pair sealing keys updated: {n} key(s)");
            }
            P2pCommand::PlaceBackup { peer_id, requests } => {
                let pid = match peer_id.parse::<PeerId>() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("Invalid peer ID for PlaceBackup: {peer_id}");
                        return;
                    }
                };
                let n = requests.len() as u32;
                if n == 0 {
                    return;
                }
                self.backup_jobs.insert(
                    pid,
                    BackupJob {
                        pending: n,
                        ..Default::default()
                    },
                );
                for request in requests {
                    let req_id = self
                        .swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&pid, request);
                    self.inflight.insert(req_id, (pid, InflightKind::BackupStoreAck));
                }
                info!("Backup placement started: {n} fragment(s) → {pid}");
            }
            P2pCommand::SetPairingOffer { offer } => {
                info!("Pairing offer armed: {}", offer.offer_id);
                self.pairing_offer = Some(*offer);
            }
            P2pCommand::ClearPairingOffer => {
                if self.pairing_offer.take().is_some() {
                    info!("Pairing offer disarmed");
                }
            }
            P2pCommand::StartSync { peer_id } => {
                // P2P-001: never initiate sync against an unpaired peer,
                // even if the app auto-triggered on mDNS discovery.
                if !self.paired_peers.contains(&peer_id) {
                    tracing::debug!("Skipping StartSync for unpaired peer {peer_id} (P2P-001)");
                    return;
                }
                let pid = match peer_id.parse::<PeerId>() {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("Invalid peer ID for StartSync: {peer_id}");
                        return;
                    }
                };
                if self.sessions.contains_key(&pid) {
                    tracing::debug!("Sync already in flight for {pid}, skipping duplicate trigger");
                    return;
                }
                let _ = self
                    .event_tx
                    .send(P2pEvent::SyncStarted {
                        peer_id: peer_id.clone(),
                    })
                    .await;
                self.sessions.insert(pid, PeerSyncState::default());
                self.send_session_request(pid, InflightKind::Manifest, SovereignRequest::GetManifest);
            }
            P2pCommand::PairDevice { peer_id } => {
                info!("Pairing with device: {}", peer_id);
            }
            P2pCommand::Dial { address } => {
                match address.parse::<Multiaddr>() {
                    Ok(addr) => {
                        if let Err(e) = self.swarm.dial(addr.clone()) {
                            warn!("Dial {address} failed: {e}");
                        } else {
                            info!("Dialing {addr}");
                        }
                    }
                    Err(e) => warn!("Invalid Multiaddr {address}: {e}"),
                }
            }
            P2pCommand::Shutdown => unreachable!("handled in run()"),
        }
    }

    /// Get the local peer ID.
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    /// Handle a P3.1 pairing-handshake request, or return `None` when the
    /// request isn't pairing-related. Runs entirely in the event loop —
    /// the offer state is single-threaded by construction.
    async fn handle_pairing_request(
        &mut self,
        peer: PeerId,
        request: &SovereignRequest,
    ) -> Option<SovereignResponse> {
        use crate::pairing_offer;

        // Only intercept the three handshake verbs.
        if !matches!(
            request,
            SovereignRequest::PairHello { .. }
                | SovereignRequest::PairProof { .. }
                | SovereignRequest::PairComplete { .. }
        ) {
            return None;
        }

        // Expiry sweep before anything else: a stale offer is dead even
        // if nobody ever dialed.
        if self.pairing_offer.as_ref().is_some_and(|o| o.expired()) {
            self.pairing_offer = None;
            let _ = self
                .event_tx
                .send(P2pEvent::PairingFailed {
                    reason: "pairing offer expired".into(),
                    offer_dead: true,
                })
                .await;
        }

        // Take the offer out for the duration of the step (avoids holding
        // a &mut borrow of self); it is put back below unless consumed.
        let Some(mut offer) = self.pairing_offer.take() else {
            return Some(SovereignResponse::PairRejected {
                reason: "no active pairing offer".into(),
            });
        };

        // Every verb must quote the active offer id.
        let quoted_offer_id = match request {
            SovereignRequest::PairHello { offer_id, .. }
            | SovereignRequest::PairProof { offer_id, .. }
            | SovereignRequest::PairComplete { offer_id, .. } => offer_id,
            _ => unreachable!("matched above"),
        };
        if quoted_offer_id != &offer.offer_id {
            self.pairing_offer = Some(offer);
            return Some(SovereignResponse::PairRejected {
                reason: "unknown pairing offer".into(),
            });
        }

        match request {
            SovereignRequest::PairHello { device_name, .. } => {
                // One session at a time. A second Hello from the SAME
                // dialer restarts its session (fresh nonce); a different
                // dialer is refused while a session is in flight so it
                // can't burn the legitimate device's attempts.
                if let Some(ref s) = offer.session {
                    if s.dialer != peer {
                        self.pairing_offer = Some(offer);
                        return Some(SovereignResponse::PairRejected {
                            reason: "pairing busy".into(),
                        });
                    }
                }
                let mut nonce = [0u8; 32];
                rand::rng().fill_bytes(&mut nonce);
                offer.session = Some(PairingSession {
                    dialer: peer,
                    nonce,
                    proven: false,
                });
                self.pairing_offer = Some(offer);
                let _ = self
                    .event_tx
                    .send(P2pEvent::PairingRequested {
                        peer_id: peer.to_string(),
                        device_name: device_name.clone(),
                    })
                    .await;
                Some(SovereignResponse::PairChallenge {
                    nonce: nonce.to_vec(),
                })
            }

            SovereignRequest::PairProof { proof, .. } => {
                let Some(ref session) = offer.session else {
                    self.pairing_offer = Some(offer);
                    return Some(SovereignResponse::PairRejected {
                        reason: "no challenge issued".into(),
                    });
                };
                if session.dialer != peer {
                    self.pairing_offer = Some(offer);
                    return Some(SovereignResponse::PairRejected {
                        reason: "challenge was issued to another peer".into(),
                    });
                }
                let ok = pairing_offer::verify_proof_mac(
                    &offer.handshake_key,
                    &offer.offer_id,
                    &session.nonce,
                    &peer.to_string(),
                    proof,
                );
                if !ok {
                    offer.attempts_left = offer.attempts_left.saturating_sub(1);
                    offer.session = None;
                    let dead = offer.attempts_left == 0;
                    warn!(
                        "pairing proof FAILED from {peer} ({} attempt(s) left)",
                        offer.attempts_left
                    );
                    if !dead {
                        self.pairing_offer = Some(offer);
                    }
                    // else: attempt cap reached — the offer stays consumed.
                    // The 50-bit code allows ~2^-48 success over 3 online
                    // guesses.
                    let _ = self
                        .event_tx
                        .send(P2pEvent::PairingFailed {
                            reason: "wrong pairing code".into(),
                            offer_dead: dead,
                        })
                        .await;
                    return Some(SovereignResponse::PairRejected {
                        reason: "invalid proof".into(),
                    });
                }
                // Proof valid: release the secrets sealed under the
                // handshake key.
                let secrets = pairing_offer::PairSecrets {
                    salt: offer.salt.clone(),
                    account_key_bytes: offer.account_key,
                    source_device_name: offer.device_name.clone(),
                };
                let sealed = match secrets.seal(&offer.handshake_key) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("failed to seal pair secrets: {e}");
                        self.pairing_offer = Some(offer);
                        return Some(SovereignResponse::PairRejected {
                            reason: "internal sealing error".into(),
                        });
                    }
                };
                if let Some(ref mut s) = offer.session {
                    s.proven = true;
                }
                info!("pairing proof accepted from {peer}; secrets released");
                self.pairing_offer = Some(offer);
                Some(SovereignResponse::PairGranted {
                    ciphertext: sealed.0,
                    nonce: sealed.1,
                })
            }

            SovereignRequest::PairComplete {
                final_peer_id,
                device_name,
                mac,
                ..
            } => {
                let Some(ref session) = offer.session else {
                    self.pairing_offer = Some(offer);
                    return Some(SovereignResponse::PairRejected {
                        reason: "no session".into(),
                    });
                };
                if session.dialer != peer || !session.proven {
                    self.pairing_offer = Some(offer);
                    return Some(SovereignResponse::PairRejected {
                        reason: "proof required before completion".into(),
                    });
                }
                let ok = pairing_offer::verify_confirm_mac(
                    &offer.handshake_key,
                    &offer.offer_id,
                    &session.nonce,
                    final_peer_id,
                    mac,
                );
                if !ok {
                    self.pairing_offer = Some(offer);
                    return Some(SovereignResponse::PairRejected {
                        reason: "invalid confirmation mac".into(),
                    });
                }
                // Register the new device's FINAL identity immediately:
                // allow-list (P2P-001) + per-pair sealing key (P1.4), so
                // the first sync works before the app even persists the
                // pairing record (it does so on the PairingCompleted
                // event).
                let local_peer = self.swarm.local_peer_id().to_string();
                let account = sovereign_crypto::account_key::AccountKey::from_bytes(
                    offer.account_key,
                );
                let pair_key = account.derive_pair_key(&local_peer, final_peer_id);
                self.sync_service
                    .add_pair_key(final_peer_id.clone(), pair_key);
                self.paired_peers.insert(final_peer_id.clone());
                info!(
                    "pairing complete: {final_peer_id} ({device_name}) registered as paired"
                );
                // Single-use: the offer is consumed (not put back).
                let _ = self
                    .event_tx
                    .send(P2pEvent::PairingCompleted {
                        peer_id: final_peer_id.clone(),
                        device_name: device_name.clone(),
                    })
                    .await;
                Some(SovereignResponse::PairDone)
            }

            _ => unreachable!("matched above"),
        }
    }

    /// Whether a request reads or writes synced DB state, and so must be
    /// gated behind pairing (P2P-001). Pairing-handshake and guardian
    /// shard requests are intentionally allowed pre-pairing so a new
    /// device can still pair / a guardian can deliver a recovery shard.
    fn is_sync_request(&self, request: &SovereignRequest) -> bool {
        matches!(
            request,
            SovereignRequest::GetManifest
                | SovereignRequest::GetCommits { .. }
                | SovereignRequest::PushCommits { .. }
                | SovereignRequest::GetRows { .. }
                | SovereignRequest::PushRows { .. }
                | SovereignRequest::PushManifest(_)
                // P4.2: accepting storage (fragments / guardian shards) is
                // a commitment made to KNOWN peers — gating it also kills
                // the disk-fill vector of strangers minting owner tags.
                // ListBackups / FetchBackupFragment / RequestShard stay
                // open: a recovering device is unpaired by definition, the
                // listed data is public-safe, and shard release is gated
                // by approval + delay on the host side.
                | SovereignRequest::StoreBackupFragment { .. }
                | SovereignRequest::DeliverShard(_)
        )
    }
}

/// Process a request without borrowing the node (avoids Send issues with Swarm).
/// `peer` is the verified libp2p sender — `apply_rows` checks every row's
/// envelope signature against the key embedded in it (P1.3 / P2P-003).
/// `backup_host` is Some when this device opted into hosting (P4.2).
async fn process_request(
    request: SovereignRequest,
    peer: PeerId,
    event_tx: &mpsc::Sender<P2pEvent>,
    sync_service: &SyncService,
    backup_host: Option<&crate::backup_host::BackupHost>,
) -> SovereignResponse {
    match request {
        SovereignRequest::GetManifest => match sync_service.build_manifest().await {
            // P2P-002: seal the manifest under the per-account transport key.
            Ok(manifest) => match manifest.encrypt(sync_service.transport_key()) {
                Ok(em) => SovereignResponse::Manifest(em),
                Err(e) => {
                    warn!("Failed to encrypt manifest: {e}");
                    SovereignResponse::Error {
                        message: format!("manifest encrypt failed: {e}"),
                    }
                }
            },
            Err(e) => {
                warn!("Failed to build manifest: {e}");
                SovereignResponse::Error {
                    message: format!("manifest build failed: {e}"),
                }
            }
        },
        SovereignRequest::GetCommits { commit_ids } => {
            match sync_service.get_commits(&commit_ids, &peer).await {
                Ok(commits) => SovereignResponse::Commits { commits },
                Err(e) => {
                    warn!("Failed to fetch commits: {e}");
                    SovereignResponse::Error {
                        message: format!("get_commits failed: {e}"),
                    }
                }
            }
        }
        SovereignRequest::PushCommits { commits } => {
            match sync_service.apply_commits(commits, &peer).await {
                Ok(_n) => SovereignResponse::Ok,
                Err(e) => {
                    warn!("Failed to apply commits: {e}");
                    SovereignResponse::Error {
                        message: format!("apply_commits failed: {e}"),
                    }
                }
            }
        }
        SovereignRequest::GetRows { table, ids } => {
            match sync_service.get_rows(table, &ids, &peer).await {
                Ok(rows) => SovereignResponse::Rows { table, rows },
                Err(e) => {
                    warn!("Failed to fetch rows: {e}");
                    SovereignResponse::Error {
                        message: format!("get_rows failed: {e}"),
                    }
                }
            }
        }
        SovereignRequest::PushRows { table, rows } => {
            match sync_service.apply_rows(table, rows, &peer).await {
                Ok((written, skipped)) => SovereignResponse::PushAck { written, skipped },
                Err(e) => {
                    warn!("Failed to apply rows: {e}");
                    SovereignResponse::Error {
                        message: format!("apply_rows failed: {e}"),
                    }
                }
            }
        }
        SovereignRequest::DeliverShard(delivery) => {
            // P4: store the guardian shard for real (hosting opt-in).
            let accepted = match backup_host {
                Some(host) => host
                    .store_guardian_shard(
                        &delivery.shard_id,
                        &delivery.for_user,
                        delivery.epoch,
                        &delivery.shard_data,
                    )
                    .unwrap_or_else(|e| {
                        warn!("guardian shard store failed: {e}");
                        false
                    }),
                None => false,
            };
            if accepted {
                let _ = event_tx
                    .send(P2pEvent::ShardReceived {
                        shard_id: delivery.shard_id.clone(),
                        from_peer: peer.to_string(),
                    })
                    .await;
            }
            SovereignResponse::ShardAck { accepted }
        }
        SovereignRequest::RequestShard(recovery_req) => {
            // P4.3: release ONLY after this user's approval + the 72h
            // delay (both enforced inside the host store). Until then the
            // request is recorded and surfaced for approval.
            let shard_data = match backup_host {
                Some(host) => host
                    .request_shard_release(
                        &recovery_req.request_id,
                        &recovery_req.for_user,
                        recovery_req.epoch,
                    )
                    .unwrap_or_else(|e| {
                        warn!("shard release check failed: {e}");
                        None
                    }),
                None => None,
            };
            if shard_data.is_none() && backup_host.is_some() {
                let _ = event_tx
                    .send(P2pEvent::ShardRequested {
                        request_id: recovery_req.request_id.clone(),
                        for_user: recovery_req.for_user.clone(),
                        epoch: recovery_req.epoch,
                    })
                    .await;
            }
            SovereignResponse::ShardData { shard_data }
        }
        SovereignRequest::StoreBackupFragment {
            owner_tag,
            snapshot_id,
            epoch,
            manifest_json,
            salt_b64,
            index,
            fragment_b64,
            digest,
        } => {
            use base64::Engine;
            let accepted = match backup_host {
                Some(host) => {
                    match base64::engine::general_purpose::STANDARD.decode(&fragment_b64) {
                        Ok(bytes) => host
                            .store_fragment(
                                &owner_tag,
                                &snapshot_id,
                                epoch,
                                &manifest_json,
                                &salt_b64,
                                index,
                                &bytes,
                                &digest,
                            )
                            .unwrap_or_else(|e| {
                                warn!("backup fragment store failed: {e}");
                                false
                            }),
                        Err(_) => false,
                    }
                }
                None => false,
            };
            SovereignResponse::BackupStored { accepted }
        }
        SovereignRequest::ListBackups { owner_tag } => {
            let backups = match backup_host {
                Some(host) => host
                    .list_hosted(owner_tag.as_deref())
                    .into_iter()
                    .map(|(tag, s)| crate::protocol::HostedBackupInfo {
                        owner_tag: tag,
                        snapshot_id: s.snapshot_id,
                        epoch: s.epoch,
                        manifest_json: s.manifest_json,
                        salt_b64: s.salt_b64,
                        fragment_indices: s.fragments.iter().map(|f| f.index).collect(),
                    })
                    .collect(),
                None => Vec::new(),
            };
            SovereignResponse::BackupList { backups }
        }
        SovereignRequest::FetchBackupFragment {
            owner_tag,
            snapshot_id,
            index,
        } => {
            use base64::Engine;
            let fragment_b64 = match backup_host {
                Some(host) => host
                    .fetch_fragment(&owner_tag, &snapshot_id, index)
                    .unwrap_or_else(|e| {
                        warn!("backup fragment fetch failed: {e}");
                        None
                    })
                    .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
                None => None,
            };
            SovereignResponse::BackupFragmentData { fragment_b64 }
        }
        SovereignRequest::PushManifest(_) => SovereignResponse::Ok,
        SovereignRequest::PairHello { .. }
        | SovereignRequest::PairProof { .. }
        | SovereignRequest::PairComplete { .. } => {
            // Handled by SovereignNode::handle_pairing_request before this
            // function is ever reached; unreachable in practice but kept
            // total for safety.
            SovereignResponse::PairRejected {
                reason: "pairing not available".into(),
            }
        }
    }
}

/// Reject manifests that are stale (a captured ciphertext replayed later),
/// from the future beyond a small clock-skew allowance (forged timestamp),
/// or not strictly newer than the last manifest accepted from the same peer
/// (fast replay / rollback). Returns the parsed timestamp for the caller to
/// record as the new high-water mark.
fn validate_manifest_timestamp(
    generated_at: &str,
    now: chrono::DateTime<chrono::Utc>,
    previous: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<chrono::DateTime<chrono::Utc>, String> {
    const MAX_AGE_MINUTES: i64 = 10;
    const MAX_FUTURE_SKEW_MINUTES: i64 = 5;

    let ts = chrono::DateTime::parse_from_rfc3339(generated_at)
        .map_err(|e| format!("unparseable generated_at '{generated_at}': {e}"))?
        .with_timezone(&chrono::Utc);

    if ts < now - chrono::Duration::minutes(MAX_AGE_MINUTES) {
        return Err(format!("manifest too old (generated {ts}, possible replay)"));
    }
    if ts > now + chrono::Duration::minutes(MAX_FUTURE_SKEW_MINUTES) {
        return Err(format!("manifest from the future (generated {ts})"));
    }
    if let Some(prev) = previous {
        if ts <= prev {
            return Err(format!(
                "manifest not newer than the last accepted one ({ts} <= {prev}, possible replay)"
            ));
        }
    }
    Ok(ts)
}

#[cfg(test)]
mod freshness_tests {
    use super::validate_manifest_timestamp;
    use chrono::{Duration, Utc};

    #[test]
    fn fresh_manifest_accepted() {
        let now = Utc::now();
        let ts = (now - Duration::seconds(5)).to_rfc3339();
        assert!(validate_manifest_timestamp(&ts, now, None).is_ok());
    }

    #[test]
    fn stale_manifest_rejected_as_replay() {
        let now = Utc::now();
        let ts = (now - Duration::minutes(30)).to_rfc3339();
        let err = validate_manifest_timestamp(&ts, now, None).unwrap_err();
        assert!(err.contains("too old"), "{err}");
    }

    #[test]
    fn future_manifest_rejected() {
        let now = Utc::now();
        let ts = (now + Duration::hours(2)).to_rfc3339();
        let err = validate_manifest_timestamp(&ts, now, None).unwrap_err();
        assert!(err.contains("future"), "{err}");
    }

    #[test]
    fn replayed_manifest_not_newer_than_high_water_rejected() {
        let now = Utc::now();
        let first = now - Duration::seconds(30);
        let ts = (now - Duration::seconds(30)).to_rfc3339();
        // Exact same timestamp as the previously accepted manifest → replay.
        let err = validate_manifest_timestamp(&ts, now, Some(first)).unwrap_err();
        assert!(err.contains("not newer"), "{err}");
        // A strictly newer one passes.
        let newer = (now - Duration::seconds(10)).to_rfc3339();
        assert!(validate_manifest_timestamp(&newer, now, Some(first)).is_ok());
    }

    #[test]
    fn garbage_timestamp_rejected() {
        assert!(validate_manifest_timestamp("not-a-date", Utc::now(), None).is_err());
    }
}
