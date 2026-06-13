//! New-device side of the P3.1 pairing handshake.
//!
//! Runs BEFORE onboarding completes, so there is no persistent identity
//! and no running [`crate::SovereignNode`]: this module spins up a
//! short-lived swarm under an **ephemeral** keypair, dials the offer's
//! source device, proves the user-typed pairing code, receives the
//! sealed AccountKey + salt, derives the device's *final* identity (via
//! the `finalize_identity` callback — only possible once the salt is
//! known), confirms it to the source, and tears the swarm down.
//!
//! The source device's identity is authenticated by construction: every
//! dial carries the `/p2p/<source_peer_id>` suffix from the scanned
//! offer, so the QUIC handshake fails against an impostor.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use tracing::{debug, info, warn};

use crate::error::{P2pError, P2pResult};
use crate::pairing_offer::{self, PairingOffer, PairSecrets};
use crate::protocol::{SovereignRequest, SovereignResponse};

/// What the handshake yields: everything the old QR used to carry, plus
/// the proof that the source now knows OUR final identity.
#[derive(Debug)]
pub struct PairingOutcome {
    pub secrets: PairSecrets,
    pub source_peer_id: String,
    /// The final peer id the `finalize_identity` callback produced (the
    /// one the source registered as paired).
    pub final_peer_id: String,
}

#[derive(libp2p::swarm::NetworkBehaviour)]
struct PairingClientBehaviour {
    mdns: libp2p::swarm::behaviour::toggle::Toggle<libp2p::mdns::tokio::Behaviour>,
    request_response:
        libp2p::request_response::cbor::Behaviour<SovereignRequest, SovereignResponse>,
}

/// Internal handshake progress.
enum Phase {
    AwaitChallenge,
    AwaitGrant { nonce: Vec<u8> },
    AwaitDone,
}

/// Run the full pairing handshake against `offer.source_peer_id`.
///
/// `code` is the user-typed pairing code (Argon2id-stretched here — call
/// off the UI thread). `device_name` names THIS device on the source's
/// paired list. `finalize_identity` receives the MasterKey salt and must
/// return the device's final libp2p peer-id string (the app derives
/// device_id → DeviceKey → keypair from it); it runs once, after the
/// secrets arrive.
pub async fn pair_with_source(
    offer: &PairingOffer,
    code: &str,
    device_name: &str,
    finalize_identity: impl FnOnce(&PairSecrets) -> Result<String, String>,
    timeout: Duration,
) -> P2pResult<PairingOutcome> {
    if offer.expired() {
        return Err(P2pError::PairingError("pairing offer expired".into()));
    }
    let source_peer: PeerId = offer
        .source_peer_id
        .parse()
        .map_err(|e| P2pError::PairingError(format!("bad source peer id: {e}")))?;
    let handshake_key = pairing_offer::derive_handshake_key(code, offer)?;

    // Ephemeral identity — the final one isn't derivable until the salt
    // arrives (see PairComplete).
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| {
            // mDNS only as a discovery fallback when the offer carries no
            // dial hints.
            let mdns = if offer.addrs.is_empty() {
                let m = libp2p::mdns::tokio::Behaviour::new(
                    libp2p::mdns::Config::default(),
                    key.public().to_peer_id(),
                )
                .map_err(|e| P2pError::Transport(e.to_string()))?;
                libp2p::swarm::behaviour::toggle::Toggle::from(Some(m))
            } else {
                libp2p::swarm::behaviour::toggle::Toggle::from(None)
            };
            let request_response = libp2p::request_response::cbor::Behaviour::new(
                [(
                    StreamProtocol::new(crate::node::PROTOCOL_NAME),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default(),
            );
            Ok(PairingClientBehaviour {
                mdns,
                request_response,
            })
        })
        .map_err(|e| P2pError::Transport(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // mDNS needs a listener to join the multicast group; explicit-addr
    // dials work without one but listening is harmless either way.
    let listen: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1"
        .parse()
        .map_err(|e: libp2p::multiaddr::Error| P2pError::Transport(e.to_string()))?;
    let _ = swarm.listen_on(listen);

    // Dial the offer's address hints (authenticated by the /p2p suffix).
    for addr in &offer.addrs {
        let mut ma: Multiaddr = match addr.parse() {
            Ok(a) => a,
            Err(e) => {
                warn!("skipping bad offer addr {addr}: {e}");
                continue;
            }
        };
        if !matches!(ma.iter().last(), Some(libp2p::multiaddr::Protocol::P2p(_))) {
            ma.push(libp2p::multiaddr::Protocol::P2p(source_peer));
        }
        if let Err(e) = swarm.dial(ma.clone()) {
            warn!("dial {ma} failed: {e}");
        }
    }

    let result = tokio::time::timeout(
        timeout,
        drive_handshake(
            &mut swarm,
            offer,
            &handshake_key,
            source_peer,
            device_name,
            finalize_identity,
        ),
    )
    .await
    .map_err(|_| P2pError::PairingError("pairing timed out".into()))?;

    result
}

async fn drive_handshake(
    swarm: &mut libp2p::Swarm<PairingClientBehaviour>,
    offer: &PairingOffer,
    handshake_key: &[u8; 32],
    source_peer: PeerId,
    device_name: &str,
    finalize_identity: impl FnOnce(&PairSecrets) -> Result<String, String>,
) -> P2pResult<PairingOutcome> {
    let local_peer = *swarm.local_peer_id();
    let mut phase = Phase::AwaitChallenge;
    let mut hello_sent = false;
    let mut finalize = Some(finalize_identity);
    let mut outcome: Option<(PairSecrets, String)> = None;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == source_peer => {
                if !hello_sent {
                    hello_sent = true;
                    info!("connected to pairing source {peer_id}; sending PairHello");
                    swarm.behaviour_mut().request_response.send_request(
                        &source_peer,
                        SovereignRequest::PairHello {
                            offer_id: offer.offer_id.clone(),
                            device_name: device_name.to_string(),
                        },
                    );
                }
            }
            SwarmEvent::Behaviour(PairingClientBehaviourEvent::Mdns(
                libp2p::mdns::Event::Discovered(peers),
            )) => {
                for (peer_id, addr) in peers {
                    if peer_id == source_peer {
                        debug!("mDNS found pairing source at {addr}");
                        let mut ma = addr;
                        if !matches!(
                            ma.iter().last(),
                            Some(libp2p::multiaddr::Protocol::P2p(_))
                        ) {
                            ma.push(libp2p::multiaddr::Protocol::P2p(source_peer));
                        }
                        let _ = swarm.dial(ma);
                    }
                }
            }
            SwarmEvent::Behaviour(PairingClientBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message: request_response::Message::Response { response, .. },
                    ..
                },
            )) if peer == source_peer => match (&phase, response) {
                (Phase::AwaitChallenge, SovereignResponse::PairChallenge { nonce }) => {
                    let proof = pairing_offer::proof_mac(
                        handshake_key,
                        &offer.offer_id,
                        &nonce,
                        &local_peer.to_string(),
                    );
                    swarm.behaviour_mut().request_response.send_request(
                        &source_peer,
                        SovereignRequest::PairProof {
                            offer_id: offer.offer_id.clone(),
                            proof,
                        },
                    );
                    phase = Phase::AwaitGrant { nonce };
                }
                (Phase::AwaitGrant { nonce }, SovereignResponse::PairGranted { ciphertext, nonce: aead_nonce }) => {
                    let secrets = PairSecrets::unseal(&ciphertext, &aead_nonce, handshake_key)?;
                    // Derive the final identity now that the salt is known.
                    let finalize = finalize.take().ok_or_else(|| {
                        P2pError::PairingError("grant received twice".into())
                    })?;
                    let final_peer_id = finalize(&secrets)
                        .map_err(|e| P2pError::PairingError(format!("finalize: {e}")))?;
                    let mac = pairing_offer::confirm_mac(
                        handshake_key,
                        &offer.offer_id,
                        nonce,
                        &final_peer_id,
                    );
                    swarm.behaviour_mut().request_response.send_request(
                        &source_peer,
                        SovereignRequest::PairComplete {
                            offer_id: offer.offer_id.clone(),
                            final_peer_id: final_peer_id.clone(),
                            device_name: device_name.to_string(),
                            mac,
                        },
                    );
                    outcome = Some((secrets, final_peer_id));
                    phase = Phase::AwaitDone;
                }
                (Phase::AwaitDone, SovereignResponse::PairDone) => {
                    let (secrets, final_peer_id) = outcome.take().expect("set before AwaitDone");
                    info!("pairing handshake complete (final identity {final_peer_id})");
                    return Ok(PairingOutcome {
                        secrets,
                        source_peer_id: offer.source_peer_id.clone(),
                        final_peer_id,
                    });
                }
                (_, SovereignResponse::PairRejected { reason }) => {
                    return Err(P2pError::PairingError(format!(
                        "pairing rejected by source: {reason}"
                    )));
                }
                (_, other) => {
                    debug!(
                        "ignoring out-of-phase pairing response: {:?}",
                        std::mem::discriminant(&other)
                    );
                }
            },
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                // Only fatal when no path remains; with several addr hints
                // others may still connect, so log and keep driving.
                warn!("pairing dial error (peer {peer_id:?}): {error}");
            }
            other => {
                debug!("pairing client swarm event: {other:?}");
            }
        }
    }
}
