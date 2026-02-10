use libp2p::swarm::NetworkBehaviour;

use crate::protocol::{SovereignRequest, SovereignResponse};

/// Composite network behaviour for Sovereign OS.
#[derive(NetworkBehaviour)]
pub struct SovereignBehaviour {
    pub mdns: libp2p::mdns::tokio::Behaviour,
    pub rendezvous: libp2p::rendezvous::client::Behaviour,
    pub request_response: libp2p::request_response::cbor::Behaviour<SovereignRequest, SovereignResponse>,
    pub identify: libp2p::identify::Behaviour,
}
