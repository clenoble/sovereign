use std::sync::Arc;
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol, Swarm};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::behaviour::{SovereignBehaviour, SovereignBehaviourEvent};
use crate::config::P2pConfig;
use crate::error::{P2pError, P2pResult};
use crate::protocol::{SovereignRequest, SovereignResponse};
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

/// The Sovereign P2P node. Runs a libp2p Swarm in an async event loop.
pub struct SovereignNode {
    swarm: Swarm<SovereignBehaviour>,
    event_tx: mpsc::Sender<P2pEvent>,
    command_rx: mpsc::Receiver<P2pCommand>,
    sync_service: Arc<SyncService>,
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
        })
    }

    /// Start listening on the configured port.
    pub fn listen(&mut self, config: &P2pConfig) -> P2pResult<Multiaddr> {
        let addr: Multiaddr = format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| P2pError::Transport(e.to_string()))?;

        let listen_id = self
            .swarm
            .listen_on(addr)
            .map_err(|e| P2pError::Transport(e.to_string()))?;

        info!("Listening with id {:?}", listen_id);

        Ok(format!("/ip4/0.0.0.0/udp/{}/quic-v1", config.listen_port)
            .parse()
            .unwrap())
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
                    Message::Response { response, .. } => {
                        info!("Response from {}: {:?}", peer, std::mem::discriminant(&response));
                    }
                }
            }
            Event::OutboundFailure { peer, error, .. } => {
                warn!("Outbound request to {} failed: {:?}", peer, error);
            }
            Event::InboundFailure { peer, error, .. } => {
                warn!("Inbound request from {} failed: {:?}", peer, error);
            }
            Event::ResponseSent { peer, .. } => {
                tracing::debug!("Response sent to {}", peer);
            }
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
                if let Ok(pid) = peer_id.parse::<PeerId>() {
                    let _ = self.event_tx.send(P2pEvent::SyncStarted {
                        peer_id: peer_id.clone(),
                    }).await;
                    self.swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&pid, SovereignRequest::GetManifest);
                }
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
        SovereignRequest::GetManifest => {
            match sync_service.build_manifest().await {
                Ok(manifest) => SovereignResponse::Manifest(manifest.to_plaintext()),
                Err(e) => {
                    warn!("Failed to build manifest: {e}");
                    SovereignResponse::Error {
                        message: format!("manifest build failed: {e}"),
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
        _ => SovereignResponse::Ok,
    }
}
