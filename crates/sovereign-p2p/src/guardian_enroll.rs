//! Guardian enrollment handshake (G2) — the in-person QR flow that hands
//! one Shamir shard to a friend's guardian app.
//!
//! Deliberately modeled on the P3.1 pairing handshake (`pairing_offer` /
//! `pairing_client`) but a **distinct role** with its own QR type and its
//! own MAC domains: a pairing artifact can never be replayed into an
//! enrollment or vice versa, and what is released is a **shard only** —
//! no AccountKey, no MasterKey salt, no sync registration. The guardian
//! is *not* a paired device.
//!
//! Differences from pairing, and why:
//! - The guardian dials with its **persistent identity** (its app already
//!   has one), so there is no step-3 identity rebind. Instead the final
//!   leg is a **custody receipt**: the guardian confirms — bound to the
//!   handshake key — that it unsealed and persisted the shard. The owner
//!   marks the guardian enrolled only on that receipt, so a dropped
//!   `GuardianGranted` response can't leave the roster claiming a shard
//!   the guardian never stored.
//! - The sealed payload (`GuardianGrant`) carries the shard plus the
//!   metadata custody needs (owner tag/label, epoch, threshold/total).
//!
//! Handshake (request/response pairs on `/sovereign/sync/1`):
//!   1. G→O `GuardianHello { offer_id, guardian_label }`
//!      O→G `GuardianChallenge { nonce }`
//!   2. G→O `GuardianProof { offer_id, proof = MAC(K, nonce ‖ dialer) }`
//!      O→G `GuardianGranted { sealed GuardianGrant under K }`
//!   3. G→O `GuardianComplete { offer_id, receipt = MAC(K, "held" ‖ nonce ‖ shard_id ‖ dialer) }`
//!      O→G `GuardianDone`
//!
//! Like the pairing QR, the offer is plaintext and secret-free; the
//! short code (spoken in person) is what gates the shard, stretched with
//! Argon2id into the handshake key `K`.

use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use libp2p::futures::StreamExt;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{P2pError, P2pResult};
use crate::protocol::{SovereignRequest, SovereignResponse};

/// Schema version of the guardian-enroll offer. Distinct payload shape
/// from `PairingOffer` (v2) and the legacy encrypted payload (v1); the
/// `kind` field below makes the mismatch explicit rather than accidental.
const GUARDIAN_OFFER_VERSION: u8 = 1;
const GUARDIAN_OFFER_KIND: &str = "guardian-enroll";

/// Offer lifetime — same reasoning as pairing: the 3-attempt proof cap
/// bounds brute force, the TTL only bounds staleness. In-person flow,
/// 10 minutes.
pub const GUARDIAN_OFFER_TTL_SECONDS: i64 = 600;

/// Wrong proofs tolerated before the offer self-destructs.
pub const MAX_ENROLL_PROOF_ATTEMPTS: u8 = 3;

const ENROLL_PROOF_CONTEXT: &str = "sovereign-guardian-proof:v1";
const ENROLL_RECEIPT_CONTEXT: &str = "sovereign-guardian-receipt:v1";

/// Plaintext enrollment offer carried by the owner's QR. No secrets —
/// everything here is visible to anyone in the room anyway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianEnrollOffer {
    pub kind: String,
    pub schema_version: u8,
    /// Random 16-byte id, base64url. Doubles as the Argon2id salt for the
    /// handshake-key derivation (same construction as pairing).
    pub offer_id: String,
    /// The owner device's libp2p PeerId — the guardian dials it with the
    /// `/p2p/<peer>` suffix so the QUIC/Noise handshake authenticates the
    /// owner against the scanned offer.
    pub owner_peer_id: String,
    /// Dial hints (multiaddrs). In-person means same LAN; mDNS fallback
    /// covers stale hints.
    #[serde(default)]
    pub addrs: Vec<String>,
    /// Human-readable owner name shown in the guardian app's consent
    /// screen ("be ‹label›'s guardian?").
    pub owner_label: String,
    /// Unix milliseconds.
    pub issued_at: i64,
    pub expires_at: i64,
}

impl GuardianEnrollOffer {
    pub fn new(
        owner_peer_id: String,
        owner_label: String,
        addrs: Vec<String>,
        ttl_seconds: i64,
    ) -> Self {
        let mut id_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut id_bytes);
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            kind: GUARDIAN_OFFER_KIND.into(),
            schema_version: GUARDIAN_OFFER_VERSION,
            offer_id: B64URL.encode(id_bytes),
            owner_peer_id,
            owner_label,
            addrs,
            issued_at: now,
            expires_at: now + ttl_seconds * 1000,
        }
    }

    /// Encode for QR display (base64url of JSON).
    pub fn encode(&self) -> P2pResult<String> {
        let json = serde_json::to_vec(self)
            .map_err(|e| P2pError::GuardianEnroll(format!("offer encode: {e}")))?;
        Ok(B64URL.encode(&json))
    }

    /// Decode a scanned offer. Rejects wrong kinds/versions and expired
    /// offers — a pairing QR scanned into the guardian app fails here.
    pub fn decode(offer_b64: &str) -> P2pResult<Self> {
        let bytes = B64URL
            .decode(offer_b64)
            .map_err(|e| P2pError::GuardianEnroll(format!("offer base64: {e}")))?;
        let offer: Self = serde_json::from_slice(&bytes)
            .map_err(|e| P2pError::GuardianEnroll(format!("offer decode: {e}")))?;
        if offer.kind != GUARDIAN_OFFER_KIND {
            return Err(P2pError::GuardianEnroll(format!(
                "not a guardian-enroll offer (kind {})",
                offer.kind
            )));
        }
        if offer.schema_version != GUARDIAN_OFFER_VERSION {
            return Err(P2pError::GuardianEnroll(format!(
                "unsupported offer version {}",
                offer.schema_version
            )));
        }
        if offer.expired() {
            return Err(P2pError::GuardianEnroll("enrollment offer expired".into()));
        }
        Ok(offer)
    }

    pub fn expired(&self) -> bool {
        chrono::Utc::now().timestamp_millis() > self.expires_at
    }

    /// Raw offer-id bytes — the Argon2id salt for [`derive_enroll_key`].
    pub fn kdf_salt(&self) -> P2pResult<Vec<u8>> {
        B64URL
            .decode(&self.offer_id)
            .map_err(|e| P2pError::GuardianEnroll(format!("offer id base64: {e}")))
    }
}

/// Stretch the spoken short code into the enrollment handshake key `K`.
pub fn derive_enroll_key(code: &str, offer: &GuardianEnrollOffer) -> P2pResult<[u8; 32]> {
    let salt = offer.kdf_salt()?;
    sovereign_crypto::pair_payload::derive_code_key(code, &salt)
        .map_err(|e| P2pError::GuardianEnroll(format!("handshake key: {e}")))
}

/// MAC proving code knowledge, bound to nonce + offer + dialing peer.
/// Domain-separated from the pairing proof.
pub fn proof_mac(key: &[u8; 32], offer_id: &str, nonce: &[u8], dialer_peer_id: &str) -> Vec<u8> {
    sovereign_crypto::pair_payload::handshake_mac(
        key,
        ENROLL_PROOF_CONTEXT,
        &[offer_id.as_bytes(), nonce, dialer_peer_id.as_bytes()],
    )
}

pub fn verify_proof_mac(
    key: &[u8; 32],
    offer_id: &str,
    nonce: &[u8],
    dialer_peer_id: &str,
    tag: &[u8],
) -> bool {
    sovereign_crypto::pair_payload::verify_handshake_mac(
        key,
        ENROLL_PROOF_CONTEXT,
        &[offer_id.as_bytes(), nonce, dialer_peer_id.as_bytes()],
        tag,
    )
}

/// Custody receipt: the guardian confirms it unsealed AND persisted the
/// shard. Bound to the session nonce, the shard id, and the guardian's
/// (persistent) peer id.
pub fn receipt_mac(key: &[u8; 32], offer_id: &str, nonce: &[u8], shard_id: &str, guardian_peer_id: &str) -> Vec<u8> {
    sovereign_crypto::pair_payload::handshake_mac(
        key,
        ENROLL_RECEIPT_CONTEXT,
        &[offer_id.as_bytes(), nonce, shard_id.as_bytes(), guardian_peer_id.as_bytes()],
    )
}

pub fn verify_receipt_mac(
    key: &[u8; 32],
    offer_id: &str,
    nonce: &[u8],
    shard_id: &str,
    guardian_peer_id: &str,
    tag: &[u8],
) -> bool {
    sovereign_crypto::pair_payload::verify_handshake_mac(
        key,
        ENROLL_RECEIPT_CONTEXT,
        &[offer_id.as_bytes(), nonce, shard_id.as_bytes(), guardian_peer_id.as_bytes()],
        tag,
    )
}

/// What the owner releases on a valid proof: the shard plus the metadata
/// the guardian's custody store files it under. Sealed under `K` for
/// transport (the connection is already Noise-encrypted to the offer's
/// peer; sealing binds release to code knowledge, as in pairing).
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct GuardianGrant {
    /// The Shamir share, base64 (33 bytes for a 32-byte secret).
    pub shard_b64: String,
    pub shard_id: String,
    /// The owner tag recovery requests will quote.
    pub owner_tag: String,
    /// Human-readable owner name for the guardian app's duty list.
    pub owner_label: String,
    /// Key-rotation epoch this shard belongs to.
    pub epoch: u32,
    /// Shamir parameters, for display ("you are 1 of 5; 3 recover").
    pub threshold: u8,
    pub total: u8,
}

impl std::fmt::Debug for GuardianGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardianGrant")
            .field("shard_b64", &"[REDACTED]")
            .field("shard_id", &self.shard_id)
            .field("owner_tag", &self.owner_tag)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

impl GuardianGrant {
    /// AEAD-seal under the handshake key. Returns (ciphertext_b64, nonce_b64).
    pub fn seal(&self, key: &[u8; 32]) -> P2pResult<(String, String)> {
        let json = serde_json::to_vec(self)
            .map_err(|e| P2pError::GuardianEnroll(format!("grant encode: {e}")))?;
        let (ct, nonce) = sovereign_crypto::aead::encrypt(&json, key)
            .map_err(|e| P2pError::GuardianEnroll(format!("grant seal: {e}")))?;
        Ok((
            base64::engine::general_purpose::STANDARD.encode(&ct),
            base64::engine::general_purpose::STANDARD.encode(nonce),
        ))
    }

    pub fn unseal(ciphertext_b64: &str, nonce_b64: &str, key: &[u8; 32]) -> P2pResult<Self> {
        let ct = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| P2pError::GuardianEnroll(format!("grant b64: {e}")))?;
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(nonce_b64)
            .map_err(|e| P2pError::GuardianEnroll(format!("grant b64: {e}")))?;
        if nonce_bytes.len() != 24 {
            return Err(P2pError::GuardianEnroll("grant nonce length".into()));
        }
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_bytes);
        let plaintext = sovereign_crypto::aead::decrypt(&ct, &nonce, key)
            .map_err(|_| P2pError::GuardianEnroll("grant unseal failed".into()))?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| P2pError::GuardianEnroll(format!("grant decode: {e}")))
    }
}

/// What a successful enrollment yields on the guardian side.
#[derive(Debug)]
pub struct GuardianEnrollOutcome {
    pub grant: GuardianGrant,
    pub owner_peer_id: String,
}

#[derive(libp2p::swarm::NetworkBehaviour)]
struct EnrollClientBehaviour {
    mdns: libp2p::swarm::behaviour::toggle::Toggle<libp2p::mdns::tokio::Behaviour>,
    request_response:
        libp2p::request_response::cbor::Behaviour<SovereignRequest, SovereignResponse>,
}

enum Phase {
    AwaitChallenge,
    AwaitGrant { nonce: Vec<u8> },
    AwaitDone,
}

/// Run the guardian-side enrollment handshake against
/// `offer.owner_peer_id`, under the guardian app's **persistent**
/// keypair (unlike pairing there is no identity rebind — the dialing
/// identity is the one the owner registers).
///
/// `persist_grant` runs once, when the sealed grant is unsealed; it must
/// durably store the shard and return `Ok(())` — the custody receipt
/// (which is what makes the owner mark this guardian enrolled) is only
/// sent after it succeeds.
pub async fn enroll_with_owner(
    offer: &GuardianEnrollOffer,
    code: &str,
    keypair: libp2p::identity::Keypair,
    guardian_label: &str,
    persist_grant: impl FnOnce(&GuardianGrant) -> Result<(), String>,
    timeout: Duration,
) -> P2pResult<GuardianEnrollOutcome> {
    if offer.expired() {
        return Err(P2pError::GuardianEnroll("enrollment offer expired".into()));
    }
    let owner_peer: PeerId = offer
        .owner_peer_id
        .parse()
        .map_err(|e| P2pError::GuardianEnroll(format!("bad owner peer id: {e}")))?;
    let handshake_key = derive_enroll_key(code, offer)?;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| {
            // mDNS in parallel with the offer's dial hints, so a stale
            // hint self-heals on the LAN (same reasoning as pairing).
            let mdns = match libp2p::mdns::tokio::Behaviour::new(
                libp2p::mdns::Config::default(),
                key.public().to_peer_id(),
            ) {
                Ok(m) => libp2p::swarm::behaviour::toggle::Toggle::from(Some(m)),
                Err(e) => {
                    warn!("enroll-client mDNS unavailable, relying on offer addrs: {e}");
                    libp2p::swarm::behaviour::toggle::Toggle::from(None)
                }
            };
            let request_response = libp2p::request_response::cbor::Behaviour::new(
                [(
                    StreamProtocol::new(crate::node::PROTOCOL_NAME),
                    ProtocolSupport::Full,
                )],
                request_response::Config::default(),
            );
            Ok(EnrollClientBehaviour {
                mdns,
                request_response,
            })
        })
        .map_err(|e| P2pError::Transport(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let listen: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1"
        .parse()
        .map_err(|e: libp2p::multiaddr::Error| P2pError::Transport(e.to_string()))?;
    let _ = swarm.listen_on(listen);

    for addr in &offer.addrs {
        let mut ma: Multiaddr = match addr.parse() {
            Ok(a) => a,
            Err(e) => {
                warn!("skipping bad offer addr {addr}: {e}");
                continue;
            }
        };
        if !matches!(ma.iter().last(), Some(libp2p::multiaddr::Protocol::P2p(_))) {
            ma.push(libp2p::multiaddr::Protocol::P2p(owner_peer));
        }
        if let Err(e) = swarm.dial(ma.clone()) {
            warn!("dial {ma} failed: {e}");
        }
    }

    tokio::time::timeout(
        timeout,
        drive_handshake(
            &mut swarm,
            offer,
            &handshake_key,
            owner_peer,
            guardian_label,
            persist_grant,
        ),
    )
    .await
    .map_err(|_| P2pError::GuardianEnroll("enrollment timed out".into()))?
}

async fn drive_handshake(
    swarm: &mut libp2p::Swarm<EnrollClientBehaviour>,
    offer: &GuardianEnrollOffer,
    handshake_key: &[u8; 32],
    owner_peer: PeerId,
    guardian_label: &str,
    persist_grant: impl FnOnce(&GuardianGrant) -> Result<(), String>,
) -> P2pResult<GuardianEnrollOutcome> {
    let local_peer = *swarm.local_peer_id();
    let mut phase = Phase::AwaitChallenge;
    let mut hello_sent = false;
    let mut persist = Some(persist_grant);
    let mut outcome: Option<GuardianGrant> = None;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == owner_peer => {
                if !hello_sent {
                    hello_sent = true;
                    info!("connected to owner {peer_id}; sending GuardianHello");
                    swarm.behaviour_mut().request_response.send_request(
                        &owner_peer,
                        SovereignRequest::GuardianHello {
                            offer_id: offer.offer_id.clone(),
                            guardian_label: guardian_label.to_string(),
                        },
                    );
                }
            }
            SwarmEvent::Behaviour(EnrollClientBehaviourEvent::Mdns(
                libp2p::mdns::Event::Discovered(peers),
            )) => {
                for (peer_id, addr) in peers {
                    if peer_id == owner_peer {
                        debug!("mDNS found owner at {addr}");
                        let mut ma = addr;
                        if !matches!(
                            ma.iter().last(),
                            Some(libp2p::multiaddr::Protocol::P2p(_))
                        ) {
                            ma.push(libp2p::multiaddr::Protocol::P2p(owner_peer));
                        }
                        let _ = swarm.dial(ma);
                    }
                }
            }
            SwarmEvent::Behaviour(EnrollClientBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message: request_response::Message::Response { response, .. },
                    ..
                },
            )) if peer == owner_peer => match (&phase, response) {
                (Phase::AwaitChallenge, SovereignResponse::GuardianChallenge { nonce }) => {
                    let proof = proof_mac(
                        handshake_key,
                        &offer.offer_id,
                        &nonce,
                        &local_peer.to_string(),
                    );
                    swarm.behaviour_mut().request_response.send_request(
                        &owner_peer,
                        SovereignRequest::GuardianProof {
                            offer_id: offer.offer_id.clone(),
                            proof,
                        },
                    );
                    phase = Phase::AwaitGrant { nonce };
                }
                (
                    Phase::AwaitGrant { nonce },
                    SovereignResponse::GuardianGranted { ciphertext, nonce: aead_nonce },
                ) => {
                    let grant = GuardianGrant::unseal(&ciphertext, &aead_nonce, handshake_key)?;
                    // Persist BEFORE the receipt: the receipt is the
                    // owner's proof of custody, so it must not precede
                    // durable storage.
                    let persist = persist
                        .take()
                        .ok_or_else(|| P2pError::GuardianEnroll("grant received twice".into()))?;
                    persist(&grant)
                        .map_err(|e| P2pError::GuardianEnroll(format!("persist: {e}")))?;
                    let receipt = receipt_mac(
                        handshake_key,
                        &offer.offer_id,
                        nonce,
                        &grant.shard_id,
                        &local_peer.to_string(),
                    );
                    swarm.behaviour_mut().request_response.send_request(
                        &owner_peer,
                        SovereignRequest::GuardianComplete {
                            offer_id: offer.offer_id.clone(),
                            receipt,
                        },
                    );
                    outcome = Some(grant);
                    phase = Phase::AwaitDone;
                }
                (Phase::AwaitDone, SovereignResponse::GuardianDone) => {
                    let grant = outcome.take().expect("set before AwaitDone");
                    info!("guardian enrollment complete (shard {})", grant.shard_id);
                    return Ok(GuardianEnrollOutcome {
                        grant,
                        owner_peer_id: offer.owner_peer_id.clone(),
                    });
                }
                (_, SovereignResponse::GuardianRejected { reason }) => {
                    return Err(P2pError::GuardianEnroll(format!(
                        "enrollment rejected by owner: {reason}"
                    )));
                }
                (_, other) => {
                    debug!(
                        "ignoring out-of-phase enrollment response: {:?}",
                        std::mem::discriminant(&other)
                    );
                }
            },
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                warn!("enrollment dial error (peer {peer_id:?}): {error}");
            }
            other => {
                debug!("enroll client swarm event: {other:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> GuardianEnrollOffer {
        GuardianEnrollOffer::new(
            "12D3KooWOwnerPeer".into(),
            "Céline".into(),
            vec!["/ip4/192.168.1.5/udp/4001/quic-v1".into()],
            GUARDIAN_OFFER_TTL_SECONDS,
        )
    }

    #[test]
    fn offer_encode_decode_roundtrip() {
        let o = offer();
        let b64 = o.encode().unwrap();
        let back = GuardianEnrollOffer::decode(&b64).unwrap();
        assert_eq!(back.offer_id, o.offer_id);
        assert_eq!(back.owner_peer_id, "12D3KooWOwnerPeer");
        assert_eq!(back.owner_label, "Céline");
    }

    #[test]
    fn expired_offer_rejected_on_decode() {
        let mut o = offer();
        o.expires_at = chrono::Utc::now().timestamp_millis() - 1_000;
        let b64 = o.encode().unwrap();
        assert!(GuardianEnrollOffer::decode(&b64).is_err());
    }

    #[test]
    fn pairing_offer_rejected_by_guardian_decode() {
        // Role separation at the QR layer: a pairing QR must not decode
        // as an enrollment offer.
        let pairing = crate::pairing_offer::PairingOffer::new(
            "12D3KooWSourcePeer".into(),
            "Laptop".into(),
            vec![],
            600,
        );
        let b64 = pairing.encode().unwrap();
        assert!(GuardianEnrollOffer::decode(&b64).is_err());
    }

    #[test]
    fn guardian_offer_rejected_by_pairing_decode() {
        let o = offer();
        let b64 = o.encode().unwrap();
        assert!(crate::pairing_offer::PairingOffer::decode(&b64).is_err());
    }

    #[test]
    fn offer_carries_no_secret_material() {
        let o = offer();
        let json = String::from_utf8(B64URL.decode(o.encode().unwrap()).unwrap()).unwrap();
        assert!(!json.contains("shard"), "no shard material in the QR");
        assert!(!json.contains("code"), "no short code in the QR");
    }

    #[test]
    fn enroll_key_is_offer_scoped_and_domain_separated_from_pairing() {
        let o1 = offer();
        let o2 = offer();
        let k1 = derive_enroll_key("ABCDE-FGHJK", &o1).unwrap();
        let k2 = derive_enroll_key("ABCDE-FGHJK", &o2).unwrap();
        assert_ne!(k1, k2, "different offers derive different keys");

        // Same key, same inputs — but pairing and enrollment proofs are
        // domain-separated, so one can never be replayed as the other.
        let nonce = [9u8; 32];
        let enroll_proof = proof_mac(&k1, &o1.offer_id, &nonce, "12D3KooWDialer");
        assert!(!crate::pairing_offer::verify_proof_mac(
            &k1,
            &o1.offer_id,
            &nonce,
            "12D3KooWDialer",
            &enroll_proof
        ));
    }

    #[test]
    fn proof_and_receipt_macs_are_bound() {
        let o = offer();
        let k = derive_enroll_key("ABCDE-FGHJK", &o).unwrap();
        let nonce = [9u8; 32];

        let proof = proof_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer");
        assert!(verify_proof_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer", &proof));
        assert!(!verify_proof_mac(&k, &o.offer_id, &nonce, "12D3KooWOther", &proof));
        assert!(!verify_proof_mac(&k, "other-offer", &nonce, "12D3KooWDialer", &proof));

        let receipt = receipt_mac(&k, &o.offer_id, &nonce, "shard-1", "12D3KooWDialer");
        assert!(verify_receipt_mac(&k, &o.offer_id, &nonce, "shard-1", "12D3KooWDialer", &receipt));
        assert!(!verify_receipt_mac(&k, &o.offer_id, &nonce, "shard-2", "12D3KooWDialer", &receipt));
        assert!(!verify_receipt_mac(&k, &o.offer_id, &nonce, "shard-1", "12D3KooWOther", &receipt));
        // Proof and receipt are domain-separated from each other.
        assert!(!verify_proof_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer", &receipt));
    }

    #[test]
    fn grant_seal_unseal_roundtrip() {
        let o = offer();
        let k = derive_enroll_key("ABCDE-FGHJK", &o).unwrap();
        let grant = GuardianGrant {
            shard_b64: base64::engine::general_purpose::STANDARD.encode([0xAB; 33]),
            shard_id: "shard-1".into(),
            owner_tag: "tag-1".into(),
            owner_label: "Céline".into(),
            epoch: 3,
            threshold: 3,
            total: 5,
        };
        let (ct, nonce) = grant.seal(&k).unwrap();
        let back = GuardianGrant::unseal(&ct, &nonce, &k).unwrap();
        assert_eq!(back.shard_id, "shard-1");
        assert_eq!(back.owner_tag, "tag-1");
        assert_eq!(back.epoch, 3);
        assert_eq!(back.threshold, 3);

        let wrong = derive_enroll_key("WRONG-CODEE", &o).unwrap();
        assert!(GuardianGrant::unseal(&ct, &nonce, &wrong).is_err());
    }
}
