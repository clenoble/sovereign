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
    /// M1.5: relay-v2 client — reserve circuit slots on seed relays so
    /// NATed peers stay reachable (M0 verdict: relay mandatory).
    pub relay_client: libp2p::relay::client::Behaviour,
    /// M1.5: DCUtR hole-punch. Opportunistically upgrades a relayed
    /// connection to direct when the NAT pair allows (LAN, cone NATs);
    /// falls back to the relay otherwise.
    pub dcutr: libp2p::dcutr::Behaviour,
    /// M1.5: store-and-forward mailbox **client** — deposit/pull sealed
    /// items via a relay so guardian heartbeats/recovery survive a peer
    /// being offline (the 72h window). Server lives in `sovereign-relay`.
    pub mailbox: libp2p::request_response::cbor::Behaviour<
        sovereign_core::mailbox::MailboxRequest,
        sovereign_core::mailbox::MailboxResponse,
    >,
}
