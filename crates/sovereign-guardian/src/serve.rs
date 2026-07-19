//! G3 — guardian serving mode.
//!
//! Runs a long-lived node that reserves a relay circuit (so a recovering
//! device can reach this guardian across NATs, per the M0 findings) and
//! answers `RequestShard` for the owners this guardian holds shares for —
//! gated by the 72h release delay + this guardian's approval.
//!
//! Design (approved 2026-07-11): **reuse the owner-app [`BackupHost`]** — its
//! `store_guardian_shard` / `request_shard_release` / approve-deny / 72h gate
//! is audited serving code. The guardian only adds the run loop and loads its
//! custody shards into the host. `RequestShard`'s `shard_data` is opaque at the
//! protocol level, so it carries a Recovery-Key share fine.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use sovereign_p2p::backup_host::BackupHost;
use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService, VersionStore};
use tokio::sync::mpsc;

use crate::custody::CustodyStore;
use crate::error::GuardianResult;
use crate::state::GuardianIdentity;

/// 64 MiB is plenty for key shares (they are ~33 bytes each).
const HOST_QUOTA_BYTES: u64 = 64 * 1024 * 1024;

/// Serve held shards until the process is stopped.
///
/// `auto_approve` releases requested shards immediately (still subject to the
/// release-delay window) — a **dev/e2e** convenience. In the shipping mobile
/// guardian app the human approves after verifying the requester out of band
/// (spec: Identity proof). Set `SOVEREIGN_RELEASE_DELAY_SECS` to shorten the
/// 72h window for testing.
pub async fn run(
    data_dir: &Path,
    identity: &GuardianIdentity,
    seed_relays: Vec<String>,
    auto_approve: bool,
) -> GuardianResult<()> {
    let custody = CustodyStore::open(data_dir)?;
    let keypair = identity.keypair()?;
    let peer_id = keypair.public().to_peer_id();

    // Serving store: reuse the audited BackupHost. Load each held shard in.
    let backup_host = Arc::new(BackupHost::open(data_dir.join("backup_host"), HOST_QUOTA_BYTES));
    for duty in custody.duties() {
        let shard_bytes = custody.shard_bytes(&duty.duty_id)?;
        let shard_b64 = base64::engine::general_purpose::STANDARD.encode(&shard_bytes);
        backup_host.store_guardian_shard(&duty.shard_id, &duty.owner_tag, duty.epoch, &shard_b64)?;
    }
    tracing::info!(
        "loaded {} shard(s) into the serving host",
        custody.duties().len()
    );

    // Minimal SyncService — the guardian never syncs; an empty in-memory DB
    // satisfies the node's type requirement (documented fattening).
    let db: Arc<dyn sovereign_db::GraphDB> = Arc::new(sovereign_db::mock::MockGraphDB::new());
    let sync = Arc::new(SyncService::new(
        db,
        peer_id.to_string(),
        [0u8; 32], // no sync ⇒ transport key unused
        keypair.clone(),
        VersionStore::ephemeral(),
    ));

    // The node's own config (distinct from the app's). The BackupHost is
    // passed directly below, so there is no `backup_host_enabled` flag here.
    let cfg = P2pConfig {
        enabled: true,
        device_name: "guardian".into(),
        enable_mdns: true,
        seed_relays,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::channel::<P2pEvent>(64);
    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);
    let mut node = SovereignNode::new(
        &cfg,
        keypair,
        event_tx,
        cmd_rx,
        sync,
        Some(backup_host.clone()),
    )?;
    node.listen(&cfg)?;
    tokio::spawn(async move { node.run().await });

    tracing::info!(%peer_id, "guardian serving — Ctrl-C to stop");

    // Surface recovery requests; approve on the dev flag. The channel closes
    // only when the node shuts down.
    while let Some(event) = event_rx.recv().await {
        match event {
            P2pEvent::ShardRequested {
                request_id,
                for_user,
                epoch,
            } => {
                tracing::warn!(
                    "RECOVERY REQUEST for {for_user} (epoch {epoch}, req {request_id}) — \
                     verify the requester OUT OF BAND before approving (spec: human proof)."
                );
                if auto_approve {
                    match backup_host.approve_shard_release(&for_user, epoch) {
                        Ok(true) => tracing::info!(
                            "auto-approved (dev) — the shard releases after the delay window"
                        ),
                        Ok(false) => tracing::warn!("no held shard for {for_user}"),
                        Err(e) => tracing::error!("approve failed: {e}"),
                    }
                }
            }
            P2pEvent::ListenAddr { address } => tracing::info!("listening on {address}"),
            other => tracing::debug!("event: {other:?}"),
        }
    }

    let _ = cmd_tx.send(P2pCommand::Shutdown).await;
    Ok(())
}
