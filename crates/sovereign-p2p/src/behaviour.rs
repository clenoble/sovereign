use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::NetworkBehaviour;

use crate::protocol::{SovereignRequest, SovereignResponse};

/// Composite network behaviour for Sovereign GE.
#[derive(NetworkBehaviour)]
pub struct SovereignBehaviour {
    /// mDNS LAN discovery. Wrapped in `Toggle` so it can be disabled via
    /// `P2pConfig::enable_mdns` (P2P-006) — when off, no multicast traffic
    /// and no automatic peer discovery; pairing must use another path.
    pub mdns: Toggle<libp2p::mdns::tokio::Behaviour>,
    pub rendezvous: libp2p::rendezvous::client::Behaviour,
    pub request_response: libp2p::request_response::cbor::Behaviour<SovereignRequest, SovereignResponse>,
    pub identify: libp2p::identify::Behaviour,
}
