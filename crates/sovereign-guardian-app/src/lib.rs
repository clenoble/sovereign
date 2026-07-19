//! Sovereign Guardian — the friend-side app (Surface 3).
//!
//! A guardian holds one Shamir share of a friend's Recovery Key. When that
//! friend forgets their password and starts recovery, each guardian gets a
//! request; the guardian **verifies the friend out of band** (the proof they
//! agreed at enrollment — a shared memory, an object, a private question) and
//! then Approves or Denies. Approving arms the release; a 72-hour window still
//! runs before the share actually leaves, so the real owner can stop a thief.
//!
//! This app wraps the audited `sovereign-guardian` engine + `BackupHost`. It
//! is deliberately minimal — no content, no sync, no AI. Desktop Tauri today;
//! the same frontend targets Android later.
//!
//! Env at launch:
//!   GUARDIAN_DATA_DIR   — identity + custody + sealed shards (default "guardian-data")
//!   GUARDIAN_RELAYS     — comma-separated seed-relay multiaddrs to reserve a
//!                         circuit on, so a recovering friend can reach you
//!   SOVEREIGN_RELEASE_DELAY_SECS — dev/e2e override of the 72h release window

mod serve;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use sovereign_guardian::custody::CustodyStore;
use sovereign_guardian::{enroll, state::GuardianIdentity};
use sovereign_p2p::backup_host::BackupHost;

/// Shared app state. The identity and custody are re-opened from `data_dir`
/// where needed (they're file-backed); `backup_host` is the single live
/// instance the serving loop and the commands both act on.
struct GuardianApp {
    data_dir: PathBuf,
    peer_id: String,
    host: Arc<BackupHost>,
}

// ---- DTOs ------------------------------------------------------------------

#[derive(Serialize)]
struct DutyDto {
    owner_label: String,
    owner_tag: String,
    shard_id: String,
    epoch: u32,
    threshold: u8,
    total: u8,
    enrolled_at: String,
}

#[derive(Serialize)]
struct StatusDto {
    peer_id: String,
    duties: Vec<DutyDto>,
    pending_count: usize,
}

#[derive(Serialize)]
struct RecoveryRequestDto {
    /// The friend recovering (owner tag) — display + the approve/deny key.
    for_user: String,
    epoch: u32,
    request_id: Option<String>,
    requested_at: Option<String>,
    /// Which of this guardian's held shards it concerns.
    shard_id: String,
}

fn duties_of(data_dir: &PathBuf) -> Result<Vec<DutyDto>, String> {
    let custody = CustodyStore::open(data_dir).map_err(|e| e.to_string())?;
    Ok(custody
        .duties()
        .iter()
        .map(|d| DutyDto {
            owner_label: d.owner_label.clone(),
            owner_tag: d.owner_tag.clone(),
            shard_id: d.shard_id.clone(),
            epoch: d.epoch,
            threshold: d.threshold,
            total: d.total,
            enrolled_at: d.enrolled_at.clone(),
        })
        .collect())
}

// ---- Commands --------------------------------------------------------------

#[tauri::command]
fn guardian_status(state: tauri::State<'_, GuardianApp>) -> Result<StatusDto, String> {
    Ok(StatusDto {
        peer_id: state.peer_id.clone(),
        duties: duties_of(&state.data_dir)?,
        pending_count: state.host.pending_release_requests().len(),
    })
}

/// Recovery requests awaiting THIS guardian's decision (a request arrived and
/// the guardian hasn't approved yet).
#[tauri::command]
fn list_recovery_requests(
    state: tauri::State<'_, GuardianApp>,
) -> Result<Vec<RecoveryRequestDto>, String> {
    Ok(state
        .host
        .pending_release_requests()
        .into_iter()
        .map(|s| RecoveryRequestDto {
            for_user: s.for_user,
            epoch: s.epoch,
            request_id: s.release_request_id,
            requested_at: s.release_requested_at,
            shard_id: s.shard_id,
        })
        .collect())
}

/// Approve the release — only after verifying the friend out of band. This
/// arms the share; the 72-hour window still runs before it actually leaves.
#[tauri::command]
fn approve_recovery(
    state: tauri::State<'_, GuardianApp>,
    for_user: String,
    epoch: u32,
) -> Result<bool, String> {
    state
        .host
        .approve_shard_release(&for_user, epoch)
        .map_err(|e| e.to_string())
}

/// Deny (and reset) the request — if the person can't prove they're the owner.
#[tauri::command]
fn deny_recovery(
    state: tauri::State<'_, GuardianApp>,
    for_user: String,
    epoch: u32,
) -> Result<bool, String> {
    state
        .host
        .deny_shard_release(&for_user, epoch)
        .map_err(|e| e.to_string())
}

/// Enroll as a guardian: run the in-person handshake against the owner's
/// offer, store the sealed share, and load it into the live serving host so
/// it's servable immediately.
#[tauri::command]
async fn enroll_guardian(
    state: tauri::State<'_, GuardianApp>,
    offer: String,
    code: String,
    label: String,
) -> Result<DutyDto, String> {
    // Snapshot what we need so no `tauri::State` is held across the await.
    let data_dir = state.data_dir.clone();
    let host = state.host.clone();
    let identity = GuardianIdentity::open(&data_dir).map_err(|e| e.to_string())?;
    let mut custody = CustodyStore::open(&data_dir).map_err(|e| e.to_string())?;
    let duty = enroll::enroll(
        &offer,
        &code,
        &label,
        &identity,
        &mut custody,
        enroll::ENROLL_TIMEOUT,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Make the freshly held share servable now, without a restart.
    if let Ok(bytes) = custody.shard_bytes(&duty.duty_id) {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let _ = host.store_guardian_shard(&duty.shard_id, &duty.owner_tag, duty.epoch, &b64);
    }

    Ok(DutyDto {
        owner_label: duty.owner_label,
        owner_tag: duty.owner_tag,
        shard_id: duty.shard_id,
        epoch: duty.epoch,
        threshold: duty.threshold,
        total: duty.total,
        enrolled_at: duty.enrolled_at,
    })
}

// ---- Entry point -----------------------------------------------------------

fn env_relays() -> Vec<String> {
    std::env::var("GUARDIAN_RELAYS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Desktop + (later) Android entry point.
pub fn run() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,libp2p_swarm=warn".into()),
        )
        .try_init();

    let data_dir = PathBuf::from(
        std::env::var("GUARDIAN_DATA_DIR").unwrap_or_else(|_| "guardian-data".into()),
    );
    let peer_id = GuardianIdentity::open(&data_dir)?.peer_id()?;
    let host = serve::build_host(&data_dir)?;

    let serve_dir = data_dir.clone();
    let serve_host = host.clone();

    tauri::Builder::default()
        .manage(GuardianApp {
            data_dir,
            peer_id,
            host,
        })
        .invoke_handler(tauri::generate_handler![
            guardian_status,
            list_recovery_requests,
            approve_recovery,
            deny_recovery,
            enroll_guardian
        ])
        .setup(move |_app| {
            // Start serving in the background: reserve the relay circuit and
            // answer recovery requests (approval stays the human's, in the UI).
            let relays = env_relays();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = serve::run_serving(&serve_dir, serve_host, relays).await {
                    tracing::error!("guardian serving stopped: {e}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|e| anyhow::anyhow!("tauri app error: {e}"))
}
