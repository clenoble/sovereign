//! P4.3 recovery-side client.
//!
//! Like the pairing client, recovery runs on a device that has nothing
//! yet — no identity, no running node — so this module drives a
//! short-lived swarm under an ephemeral keypair against a known host
//! address (`/ip4/.../p2p/<peer>`). Flow on the recovering device:
//!
//!   1. passphrase typed by the user;
//!   2. guardian devices: `RequestShard` (repeated polls — the release
//!      is gated by the guardian's approval + the 72h delay) until
//!      `threshold` [`BackupGuardianPayload`]s arrive. Any single
//!      payload already carries the SALT + MANIFEST, unblocking step 3;
//!   3. salt + passphrase → MasterKey → AccountKey → verify
//!      `derive_backup_tag()` matches the manifest's owner tag;
//!   4. fragment hosts: `ListBackups` / `FetchBackupFragment` until
//!      `data_fragments` valid fragments arrive;
//!   5. offline: reassemble → digest check → reconstruct the backup key
//!      from the shards → unseal → [`crate::backup::restore_snapshot`].
//!
//! This module provides the network primitives (2) and (4) plus the
//! offline assembly (5); the app orchestrates the loop and the UX.

use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, StreamProtocol};
use tracing::{debug, warn};

use crate::backup::{
    reassemble_backup, sha256_hex, unseal_backup, BackupFragment, BackupGuardianPayload,
    BackupManifest, BackupSnapshot,
};
use crate::error::{P2pError, P2pResult};
use crate::protocol::{HostedBackupInfo, SovereignRequest, SovereignResponse};

#[derive(libp2p::swarm::NetworkBehaviour)]
struct BackupClientBehaviour {
    request_response:
        libp2p::request_response::cbor::Behaviour<SovereignRequest, SovereignResponse>,
}

/// Connect to `addr` (must carry the `/p2p/<peer>` suffix, which
/// authenticates the host) and execute `requests` sequentially,
/// returning the responses in order.
pub async fn backup_requests(
    addr: &str,
    requests: Vec<SovereignRequest>,
    timeout: Duration,
) -> P2pResult<Vec<SovereignResponse>> {
    let ma: Multiaddr = addr
        .parse()
        .map_err(|e: libp2p::multiaddr::Error| P2pError::Transport(e.to_string()))?;
    let Some(libp2p::multiaddr::Protocol::P2p(target)) = ma.iter().last() else {
        return Err(P2pError::Transport(format!(
            "address must end in /p2p/<peer>: {addr}"
        )));
    };

    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|_key| {
            Ok(BackupClientBehaviour {
                request_response: libp2p::request_response::cbor::Behaviour::new(
                    [(
                        StreamProtocol::new(crate::node::PROTOCOL_NAME),
                        ProtocolSupport::Full,
                    )],
                    request_response::Config::default(),
                ),
            })
        })
        .map_err(|e| P2pError::Transport(e.to_string()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    swarm
        .dial(ma)
        .map_err(|e| P2pError::Transport(format!("dial: {e}")))?;

    tokio::time::timeout(timeout, drive_requests(&mut swarm, target, requests))
        .await
        .map_err(|_| P2pError::SyncError("backup request timed out".into()))?
}

async fn drive_requests(
    swarm: &mut libp2p::Swarm<BackupClientBehaviour>,
    target: PeerId,
    requests: Vec<SovereignRequest>,
) -> P2pResult<Vec<SovereignResponse>> {
    let mut queue = requests.into_iter();
    let mut responses = Vec::new();
    let mut connected = false;

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == target => {
                if !connected {
                    connected = true;
                    match queue.next() {
                        Some(req) => {
                            swarm
                                .behaviour_mut()
                                .request_response
                                .send_request(&target, req);
                        }
                        None => return Ok(responses),
                    }
                }
            }
            SwarmEvent::Behaviour(BackupClientBehaviourEvent::RequestResponse(
                request_response::Event::Message {
                    peer,
                    message: request_response::Message::Response { response, .. },
                    ..
                },
            )) if peer == target => {
                responses.push(response);
                match queue.next() {
                    Some(req) => {
                        swarm
                            .behaviour_mut()
                            .request_response
                            .send_request(&target, req);
                    }
                    None => return Ok(responses),
                }
            }
            SwarmEvent::OutgoingConnectionError { error, .. } => {
                return Err(P2pError::Transport(format!("backup dial failed: {error}")));
            }
            other => debug!("backup client swarm event: {other:?}"),
        }
    }
}

/// List the backups a host holds (optionally filtered by owner tag).
pub async fn list_backups(
    addr: &str,
    owner_tag: Option<String>,
    timeout: Duration,
) -> P2pResult<Vec<HostedBackupInfo>> {
    let mut responses = backup_requests(
        addr,
        vec![SovereignRequest::ListBackups { owner_tag }],
        timeout,
    )
    .await?;
    match responses.pop() {
        Some(SovereignResponse::BackupList { backups }) => Ok(backups),
        other => Err(P2pError::SyncError(format!(
            "unexpected ListBackups response: {:?}",
            other.map(|o| std::mem::discriminant(&o))
        ))),
    }
}

/// Fetch the given fragment indices of one hosted snapshot. Fragments
/// the host doesn't hold (or that fail base64) are skipped — the caller
/// accumulates across hosts until `data_fragments` valid ones exist.
pub async fn fetch_fragments(
    addr: &str,
    owner_tag: &str,
    snapshot_id: &str,
    indices: &[u8],
    manifest: &BackupManifest,
    timeout: Duration,
) -> P2pResult<Vec<BackupFragment>> {
    use base64::Engine;
    let requests: Vec<SovereignRequest> = indices
        .iter()
        .map(|&index| SovereignRequest::FetchBackupFragment {
            owner_tag: owner_tag.to_string(),
            snapshot_id: snapshot_id.to_string(),
            index,
        })
        .collect();
    let responses = backup_requests(addr, requests, timeout).await?;

    let mut fragments = Vec::new();
    for (&index, response) in indices.iter().zip(responses) {
        let SovereignResponse::BackupFragmentData { fragment_b64: Some(b64) } = response else {
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&b64) else {
            warn!("fragment {index}: bad base64 from host, skipping");
            continue;
        };
        let digest = sha256_hex(&bytes);
        let expected = manifest.fragment_digests.get(index as usize);
        if expected != Some(&digest) {
            warn!("fragment {index}: digest mismatch from host, skipping");
            continue;
        }
        fragments.push(BackupFragment {
            index,
            data_b64: b64,
            digest,
        });
    }
    Ok(fragments)
}

/// Poll one guardian for our key shard. Returns the decoded payload
/// once the guardian has approved AND the release delay elapsed; None
/// while the request is pending.
pub async fn request_guardian_shard(
    addr: &str,
    request_id: &str,
    owner_tag: &str,
    epoch: u32,
    timeout: Duration,
) -> P2pResult<Option<BackupGuardianPayload>> {
    let mut responses = backup_requests(
        addr,
        vec![SovereignRequest::RequestShard(
            crate::protocol::guardian::ShardRecoveryRequest {
                request_id: request_id.to_string(),
                for_user: owner_tag.to_string(),
                epoch,
            },
        )],
        timeout,
    )
    .await?;
    match responses.pop() {
        Some(SovereignResponse::ShardData { shard_data: Some(data) }) => {
            Ok(Some(BackupGuardianPayload::decode(&data)?))
        }
        Some(SovereignResponse::ShardData { shard_data: None }) => Ok(None),
        other => Err(P2pError::SyncError(format!(
            "unexpected RequestShard response: {:?}",
            other.map(|o| std::mem::discriminant(&o))
        ))),
    }
}

/// Offline final assembly (P4.3 step 5): digest-checked reassembly of
/// the ciphertext, backup-key reconstruction from the guardian
/// payloads, and unsealing. Verifies the ciphertext digest against the
/// manifest before decrypting.
pub fn assemble_snapshot(
    manifest: &BackupManifest,
    fragments: &[BackupFragment],
    guardian_payloads: &[BackupGuardianPayload],
) -> P2pResult<BackupSnapshot> {
    use base64::Engine;

    if guardian_payloads.len() < manifest.key_threshold as usize {
        return Err(P2pError::SyncError(format!(
            "{} guardian shard(s) collected but threshold is {}",
            guardian_payloads.len(),
            manifest.key_threshold
        )));
    }

    let ciphertext = reassemble_backup(
        fragments,
        manifest.data_fragments as usize,
        manifest.parity_fragments as usize,
        manifest.ciphertext_len,
    )?;
    if sha256_hex(&ciphertext) != manifest.ciphertext_digest {
        return Err(P2pError::SyncError(
            "reassembled ciphertext digest does not match the manifest".into(),
        ));
    }

    let shares: Vec<_> = guardian_payloads
        .iter()
        .map(|p| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&p.key_share_b64)
                .map_err(|e| P2pError::SyncError(format!("key share base64: {e}")))?;
            sovereign_crypto::guardian::shamir::share_from_bytes(&bytes)
                .map_err(|e| P2pError::SyncError(format!("key share decode: {e}")))
        })
        .collect::<P2pResult<Vec<_>>>()?;
    let key = sovereign_crypto::guardian::shamir::reconstruct_secret(
        &shares,
        manifest.key_threshold,
    )
    .map_err(|e| P2pError::SyncError(format!("backup key reconstruction: {e}")))?;

    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&manifest.nonce_b64)
        .map_err(|e| P2pError::SyncError(format!("nonce base64: {e}")))?;
    if nonce_bytes.len() != 24 {
        return Err(P2pError::SyncError("manifest nonce length".into()));
    }
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&nonce_bytes);

    unseal_backup(&ciphertext, &nonce, &key)
}
