//! The app's serving loop.
//!
//! Mirrors `sovereign_guardian::serve::run` (same audited pieces —
//! `SovereignNode` + `BackupHost`, relay-circuit reservation, the 72h gate),
//! but instead of the CLI's `--auto-approve`, it never approves on its own:
//! incoming recovery requests land in `BackupHost`'s pending list, and the
//! human decides in the UI (spec: Identity proof, verified out of band).
//!
//! The caller owns the `BackupHost` so the Tauri commands can read the
//! pending requests and approve/deny the same instance this loop is serving.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use sovereign_p2p::backup_host::BackupHost;
use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService, VersionStore};
use tokio::sync::mpsc;

use sovereign_guardian::custody::CustodyStore;
use sovereign_guardian::state::GuardianIdentity;

const HOST_QUOTA_BYTES: u64 = 64 * 1024 * 1024;

/// Build a `BackupHost` and load every held custody shard into it. The
/// commands and the serving loop share this one instance.
pub fn build_host(data_dir: &Path) -> anyhow::Result<Arc<BackupHost>> {
    let custody = CustodyStore::open(data_dir)?;
    let host = Arc::new(BackupHost::open(
        data_dir.join("backup_host"),
        HOST_QUOTA_BYTES,
    ));
    for duty in custody.duties() {
        let shard_bytes = custody.shard_bytes(&duty.duty_id)?;
        let shard_b64 = base64::engine::general_purpose::STANDARD.encode(&shard_bytes);
        host.store_guardian_shard(&duty.shard_id, &duty.owner_tag, duty.epoch, &shard_b64)?;
    }
    tracing::info!("loaded {} shard(s) into the serving host", custody.duties().len());
    Ok(host)
}

/// Run the guardian node until the process stops. Reserves a relay circuit on
/// each `seed_relay` so a recovering device can reach this guardian across
/// NATs. Requests are surfaced (logged + left pending in `host`); approval is
/// the human's, via the UI — never automatic here.
///
/// Opens its own `GuardianIdentity` from `data_dir` so nothing non-`Send` has
/// to cross the spawn boundary.
pub async fn run_serving(
    data_dir: &Path,
    host: Arc<BackupHost>,
    seed_relays: Vec<String>,
) -> anyhow::Result<()> {
    let identity = GuardianIdentity::open(data_dir)?;
    let keypair = identity.keypair()?;
    let peer_id = keypair.public().to_peer_id();

    // The guardian never syncs — an empty in-memory DB satisfies the node's
    // SyncService requirement (documented fattening, same as the CLI engine).
    let db: Arc<dyn sovereign_db::GraphDB> = Arc::new(sovereign_db::mock::MockGraphDB::new());
    let sync = Arc::new(SyncService::new(
        db,
        peer_id.to_string(),
        [0u8; 32], // no sync ⇒ transport key unused
        keypair.clone(),
        VersionStore::ephemeral(),
    ));

    let cfg = P2pConfig {
        enabled: true,
        device_name: "guardian".into(),
        enable_mdns: true,
        seed_relays,
        ..Default::default()
    };

    let (event_tx, mut event_rx) = mpsc::channel::<P2pEvent>(64);
    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);
    let mut node = SovereignNode::new(&cfg, keypair, event_tx, cmd_rx, sync, Some(host))?;
    node.listen(&cfg)?;
    tokio::spawn(async move { node.run().await });

    tracing::info!(%peer_id, "guardian app serving — approve/deny is the human's");

    // Drain events so the channel never fills. A recovery request is left
    // PENDING in the host for the UI; we only log it. Nothing auto-approves.
    while let Some(event) = event_rx.recv().await {
        match event {
            P2pEvent::ShardRequested { request_id, for_user, epoch } => {
                tracing::warn!(
                    "RECOVERY REQUEST for {for_user} (epoch {epoch}, req {request_id}) — \
                     surfaced to the UI; verify the requester OUT OF BAND before approving."
                );
            }
            P2pEvent::ListenAddr { address } => tracing::info!("listening on {address}"),
            other => tracing::debug!("event: {other:?}"),
        }
    }

    let _ = cmd_tx.send(P2pCommand::Shutdown).await;
    Ok(())
}
