use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::request_response::{self, OutboundRequestId, ProtocolSupport};
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

const PROTOCOL_NAME: &str = "/sovereign/sync/1";

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
}

/// Commands sent to the P2P node from the application.
#[derive(Debug)]
pub enum P2pCommand {
    StartSync { peer_id: String },
    PairDevice { peer_id: String },
    DistributeShard { peer_id: String, shard_data: String, shard_id: String },
    SendRequest { peer_id: PeerId, request: SovereignRequest },
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
}

impl SovereignNode {
    /// Create a new P2P node.
    pub fn new(
        _config: &P2pConfig,
        keypair: libp2p::identity::Keypair,
        event_tx: mpsc::Sender<P2pEvent>,
        command_rx: mpsc::Receiver<P2pCommand>,
        sync_service: Arc<SyncService>,
    ) -> P2pResult<Self> {
        let peer_id = keypair.public().to_peer_id();
        info!("P2P node starting with PeerId: {}", peer_id);

        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| {
                let mdns = libp2p::mdns::tokio::Behaviour::new(
                    libp2p::mdns::Config::default(),
                    peer_id,
                )
                .map_err(|e| P2pError::Transport(e.to_string()))?;

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
                    .with_agent_version(format!("sovereign/{}", env!("CARGO_PKG_VERSION"))),
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
                        let response = process_request(request, &self.event_tx, &self.sync_service).await;
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
                if let Some((peer_id, _kind)) = self.inflight.remove(&request_id) {
                    self.decrement_pending(&peer_id).await;
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
                match self.sync_service.apply_commits(commits).await {
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
                    match self.sync_service.apply_rows(table, rows).await {
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

    /// Decode a manifest response, diff against local, and dispatch the
    /// follow-up `GetCommits` / `PushCommits` / `GetRows` / `PushRows`
    /// requests. Each follow-up bumps `pending_responses` so the session
    /// only finalizes once everything has been acked.
    async fn handle_manifest_response(
        &mut self,
        peer_id: PeerId,
        encrypted: EncryptedManifest,
    ) {
        // Phase 3 ships plaintext-marker manifests; pair-key encryption
        // arrives alongside the orchestrator's post-login p2p start in
        // v0.0.5.x.
        let remote = match SyncManifest::from_plaintext(&encrypted) {
            Some(m) => m,
            None => {
                warn!("Failed to decode manifest from {peer_id}");
                return;
            }
        };
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
                    .get_commits_since(doc_id, since.as_deref())
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
                if let Ok(rows) = self.sync_service.get_rows(table, &rd.push_to_remote).await {
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
            P2pCommand::DistributeShard { peer_id, shard_data, shard_id } => {
                if let Ok(pid) = peer_id.parse::<PeerId>() {
                    let req = SovereignRequest::DeliverShard(
                        crate::protocol::guardian::ShardDeliveryRequest {
                            shard_data,
                            shard_id,
                            for_user: "self".into(),
                            epoch: 1,
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
            P2pCommand::StartSync { peer_id } => {
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
            P2pCommand::Shutdown => unreachable!("handled in run()"),
        }
    }

    /// Get the local peer ID.
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }
}

/// Process a request without borrowing the node (avoids Send issues with Swarm).
async fn process_request(
    request: SovereignRequest,
    event_tx: &mpsc::Sender<P2pEvent>,
    sync_service: &SyncService,
) -> SovereignResponse {
    match request {
        SovereignRequest::GetManifest => match sync_service.build_manifest().await {
            Ok(manifest) => SovereignResponse::Manifest(manifest.to_plaintext()),
            Err(e) => {
                warn!("Failed to build manifest: {e}");
                SovereignResponse::Error {
                    message: format!("manifest build failed: {e}"),
                }
            }
        },
        SovereignRequest::GetCommits { commit_ids } => {
            match sync_service.get_commits(&commit_ids).await {
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
            match sync_service.apply_commits(commits).await {
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
            match sync_service.get_rows(table, &ids).await {
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
            match sync_service.apply_rows(table, rows).await {
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
            let _ = event_tx
                .send(P2pEvent::ShardReceived {
                    shard_id: delivery.shard_id.clone(),
                    from_peer: "unknown".into(),
                })
                .await;
            SovereignResponse::ShardAck { accepted: true }
        }
        SovereignRequest::RequestShard(_recovery_req) => {
            SovereignResponse::ShardData { shard_data: None }
        }
        SovereignRequest::PushManifest(_) => SovereignResponse::Ok,
        SovereignRequest::PairRequest { .. } | SovereignRequest::PairResponse { .. } => {
            // Pairing handshake lives in pairing.rs; the protocol surface
            // is reserved here but not wired in v0.0.5.
            SovereignResponse::Ok
        }
    }
}
