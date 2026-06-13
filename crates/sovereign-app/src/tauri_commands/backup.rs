//! Tauri commands for the P4 encrypted backup.
//!
//! `backup_now` runs P4.1 + P4.2 in one shot for the current paired
//! fleet: snapshot → seal under a fresh key → Shamir-split the key →
//! distribute fragments round-robin across paired peers and one key
//! shard per paired peer (paired devices act as both hosts and
//! guardians until a dedicated guardian-enrollment UX exists). The
//! manifest is persisted locally; placement acks arrive async as
//! `sync-status` events ("backup placed ...").
//!
//! `approve_shard_release` / `deny_shard_release` are the guardian-side
//! controls for incoming recovery requests (the 72h delay is enforced
//! in the host store regardless). `backup_status` powers the Settings
//! display: last manifest + what we host for others + pending releases.
//!
//! The recovery (restore) flow runs pre-onboarding like pairing and
//! gets its own UX pass; the network + assembly primitives live in
//! `sovereign_p2p::backup_client`.

use serde::Serialize;
use tauri::State;

use crate::err::ToStringErr;
use crate::tauri_state::AppState;

#[derive(Serialize)]
pub struct BackupNowResult {
    pub snapshot_id: String,
    pub epoch: u32,
    pub fragment_count: u8,
    pub hosts: u32,
    pub guardians: u32,
}

/// Snapshot + seal + split + distribute to the paired fleet.
#[tauri::command]
pub async fn backup_now(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BackupNowResult, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        use base64::Engine;

        let account_key = state
            .account_key()
            .await
            .ok_or_else(|| "backup unavailable: account key not loaded".to_string())?;
        let cmd_tx = state
            .p2p_command_tx()
            .await
            .ok_or_else(|| "backup requires the P2P node — enable sync first".to_string())?;

        // Paired devices act as hosts AND guardians (one key shard each).
        let peer_ids: Vec<String> = {
            let guard = state.pairing_manager.read().await;
            match guard.as_ref() {
                Some(m) => m.list_devices().iter().map(|d| d.peer_id.clone()).collect(),
                None => Vec::new(),
            }
        };
        if peer_ids.len() < 2 {
            return Err(
                "backup needs at least 2 paired devices (they hold the key shards — \
                 a 2-of-2 split is the minimum)"
                    .to_string(),
            );
        }
        // Majority threshold, at least 2: 2 devices → 2-of-2, 3 → 2-of-3,
        // 5 → 3-of-5.
        let threshold = ((peer_ids.len() / 2 + 1).max(2)).min(peer_ids.len()) as u8;

        let device_id =
            crate::setup::load_or_create_device_id().map_err(|e| format!("device id: {e}"))?;
        let owner_tag = account_key.derive_backup_tag();
        let salt = std::fs::read(crate::setup::crypto_dir().join("salt"))
            .map_err(|e| format!("read salt: {e}"))?;

        // Epoch: previous manifest + 1 (hosts keep only the newest).
        let manifest_path = crate::setup::crypto_dir().join("backup_manifest.json");
        let epoch = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|json| sovereign_p2p::backup::BackupManifest::from_json(&json).ok())
            .map(|m| m.epoch + 1)
            .unwrap_or(1);

        let db: std::sync::Arc<dyn sovereign_db::GraphDB> = state.db.clone();
        let prepared = sovereign_p2p::backup::prepare_backup(
            db.as_ref(),
            &device_id,
            &owner_tag,
            &salt,
            epoch,
            &peer_ids,
            threshold,
            sovereign_p2p::backup::DEFAULT_DATA_FRAGMENTS,
            sovereign_p2p::backup::DEFAULT_PARITY_FRAGMENTS,
        )
        .await
        .str_err()?;

        // Persist the manifest before anything leaves this device.
        sovereign_crypto::fs_private::write_private(
            &manifest_path,
            prepared.manifest.to_json().str_err()?,
        )
        .map_err(|e| format!("persist manifest: {e}"))?;

        // Round-robin the fragments across the paired hosts and deliver
        // one key shard per peer.
        let manifest_json = prepared.manifest.to_json().str_err()?;
        let salt_b64 = base64::engine::general_purpose::STANDARD.encode(&salt);
        let fragment_count = prepared.fragments.len() as u8;
        for (host_idx, peer_id) in peer_ids.iter().enumerate() {
            let requests: Vec<sovereign_p2p::protocol::SovereignRequest> = prepared
                .fragments
                .iter()
                .filter(|f| (f.index as usize) % peer_ids.len() == host_idx)
                .map(|f| sovereign_p2p::protocol::SovereignRequest::StoreBackupFragment {
                    owner_tag: owner_tag.clone(),
                    snapshot_id: prepared.manifest.snapshot_id.clone(),
                    epoch,
                    manifest_json: manifest_json.clone(),
                    salt_b64: salt_b64.clone(),
                    index: f.index,
                    fragment_b64: f.data_b64.clone(),
                    digest: f.digest.clone(),
                })
                .collect();
            if !requests.is_empty() {
                cmd_tx
                    .send(sovereign_p2p::P2pCommand::PlaceBackup {
                        peer_id: peer_id.clone(),
                        requests,
                    })
                    .await
                    .map_err(|e| format!("queue placement: {e}"))?;
            }
            // One backup-key shard per paired peer (guardian role).
            let (gid, payload_b64) = &prepared.guardian_payloads[host_idx];
            cmd_tx
                .send(sovereign_p2p::P2pCommand::DistributeShard {
                    peer_id: peer_id.clone(),
                    shard_data: payload_b64.clone(),
                    shard_id: format!("{}-{gid}", prepared.manifest.snapshot_id),
                    for_user: owner_tag.clone(),
                    epoch,
                })
                .await
                .map_err(|e| format!("queue shard delivery: {e}"))?;
        }

        return Ok(BackupNowResult {
            snapshot_id: prepared.manifest.snapshot_id.clone(),
            epoch,
            fragment_count,
            hosts: peer_ids.len() as u32,
            guardians: peer_ids.len() as u32,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Err("backup requires a build with the p2p feature".to_string())
    }
}

#[derive(Serialize)]
pub struct HostedForOtherDto {
    pub owner_tag: String,
    pub snapshot_id: String,
    pub epoch: u32,
    pub fragment_count: usize,
    pub total_bytes: u64,
}

#[derive(Serialize)]
pub struct PendingReleaseDto {
    pub for_user: String,
    pub epoch: u32,
    pub requested_at: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Serialize)]
pub struct BackupStatusDto {
    /// This device's last backup manifest (if any), as JSON.
    pub last_manifest_json: Option<String>,
    /// Whether this device hosts for others.
    pub hosting_enabled: bool,
    pub hosting: Vec<HostedForOtherDto>,
    pub pending_releases: Vec<PendingReleaseDto>,
}

/// Settings display: last backup + reciprocity accounting + pending
/// guardian-release approvals.
#[tauri::command]
pub async fn backup_status(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BackupStatusDto, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        let last_manifest_json =
            std::fs::read_to_string(crate::setup::crypto_dir().join("backup_manifest.json")).ok();
        let host = state.backup_host.read().await.clone();
        let (hosting, pending_releases) = match host.as_ref() {
            Some(h) => (
                h.accounting()
                    .into_iter()
                    .map(|a| HostedForOtherDto {
                        owner_tag: a.owner_tag,
                        snapshot_id: a.snapshot_id,
                        epoch: a.epoch,
                        fragment_count: a.fragment_count,
                        total_bytes: a.total_bytes,
                    })
                    .collect(),
                h.pending_release_requests()
                    .into_iter()
                    .map(|s| PendingReleaseDto {
                        for_user: s.for_user,
                        epoch: s.epoch,
                        requested_at: s.release_requested_at,
                        request_id: s.release_request_id,
                    })
                    .collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        return Ok(BackupStatusDto {
            last_manifest_json,
            hosting_enabled: host.is_some(),
            hosting,
            pending_releases,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Ok(BackupStatusDto {
            last_manifest_json: None,
            hosting_enabled: false,
            hosting: Vec::new(),
            pending_releases: Vec::new(),
        })
    }
}

/// Guardian-side: approve releasing the key shard we hold for `for_user`
/// to a pending recovery. The 72h delay still applies on top.
#[tauri::command]
pub async fn approve_shard_release(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    for_user: String,
    epoch: u32,
) -> Result<bool, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        let host = state.backup_host.read().await.clone();
        return match host {
            Some(h) => h.approve_shard_release(&for_user, epoch).str_err(),
            None => Err("backup hosting is not enabled on this device".to_string()),
        };
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &for_user, epoch);
        Err("backup requires a build with the p2p feature".to_string())
    }
}

/// Guardian-side: deny (and reset) a pending shard-release request.
#[tauri::command]
pub async fn deny_shard_release(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    for_user: String,
    epoch: u32,
) -> Result<bool, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        let host = state.backup_host.read().await.clone();
        return match host {
            Some(h) => h.deny_shard_release(&for_user, epoch).str_err(),
            None => Err("backup hosting is not enabled on this device".to_string()),
        };
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &for_user, epoch);
        Err("backup requires a build with the p2p feature".to_string())
    }
}
