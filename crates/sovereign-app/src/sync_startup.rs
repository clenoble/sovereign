//! Post-login P2P startup (v0.0.5 Phase 3c).
//!
//! `install_session` calls [`start_p2p_node`] after key installation when
//! the `p2p` feature is on and the config has it enabled. This module:
//!
//! 1. Loads (or creates) the local `PairingManager` from
//!    `~/.sovereign/crypto/paired_devices.json`.
//! 2. Builds a [`SyncService`] over the live DB.
//! 3. Derives the libp2p keypair from the per-device identity key,
//!    spawns a [`SovereignNode`] with mDNS+QUIC, and wires its event
//!    loop.
//! 4. Spawns a translator task that consumes `P2pEvent`s, queues
//!    `StartSync` for every newly discovered peer (LAN-only trust
//!    boundary in v0.0.5; pair-key envelope encryption in v0.0.5.x will
//!    drop unpaired peers automatically), and forwards every event to
//!    the orchestrator's `OrchestratorEvent` channel so the existing
//!    `tauri_events` bridge can emit them to the frontend.
//! 5. Stores the `P2pCommand` sender on `AppState` and on the
//!    orchestrator so Tauri commands and intent handlers can fire
//!    `StartSync` / `PairDevice`.
//!
//! The startup is idempotent — calling twice in the same process (e.g.
//! a user logging out and back in) is a no-op on the second pass.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_p2p::pairing::PairingManager;
use sovereign_p2p::{ConnectivityState, P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService};
use tokio::sync::mpsc;

use crate::tauri_state::AppState;

/// Build a `sovereign_p2p::P2pConfig` from the app config struct. The
/// two have identical fields but live in different crates to keep
/// `sovereign-core` free of libp2p deps.
fn p2p_config_from_app(app_p2p: &sovereign_core::config::P2pConfig) -> P2pConfig {
    P2pConfig {
        enabled: app_p2p.enabled,
        listen_port: app_p2p.listen_port,
        rendezvous_server: app_p2p.rendezvous_server.clone(),
        device_name: app_p2p.device_name.clone(),
        enable_mdns: app_p2p.enable_mdns,
        wifi_only: app_p2p.wifi_only,
    }
}

/// Decode a u8 written by `tauri_state::connectivity_to_u8` back into
/// the `ConnectivityState` enum. Kept inline so the translator + the
/// periodic poll task don't need to import the helper from `tauri_state`.
fn connectivity_from_u8(byte: u8) -> ConnectivityState {
    match byte {
        1 => ConnectivityState::Wifi,
        2 => ConnectivityState::Cellular,
        3 => ConnectivityState::Offline,
        _ => ConnectivityState::Unknown,
    }
}

/// Channel buffer for P2P commands. Generous because mDNS bursts can
/// queue many StartSync requests in quick succession on first launch.
const COMMAND_BUFFER: usize = 64;
/// Channel buffer for P2P events flowing into the translator.
const EVENT_BUFFER: usize = 256;

/// Bring up the P2P node, the event translator, and load the paired
/// devices list. Idempotent — returns early if already started.
pub async fn start_p2p_node(state: &AppState) -> Result<(), String> {
    // Idempotency: if a command sender is already installed, the node
    // is running.
    if state.p2p_command_tx().await.is_some() {
        return Ok(());
    }
    if !state.config.p2p.enabled {
        tracing::info!("P2P disabled in config; skipping node start");
        return Ok(());
    }

    let p2p_identity_key = state
        .p2p_identity_key()
        .await
        .ok_or_else(|| "p2p start requires per-device identity key".to_string())?;

    // P2P-001: the row/version authorship identity is the libp2p PeerId
    // (derived below) — a value the receiver can cryptographically verify
    // against each row's envelope signature — NOT the local device-id UUID,
    // which a paired peer could forge freely to win the (counter, device) LWW
    // race. We still ensure the device-id file exists (the PeerId is ultimately
    // derived from it via the DeviceKey), but it is not the sync identity.
    let _ = crate::setup::load_or_create_device_id()
        .map_err(|e| format!("device id: {e}"))?;
    let db_dyn: Arc<dyn sovereign_db::GraphDB> = state.db.clone();
    // P2P-002: the manifest envelope is AEAD-sealed under a transport key
    // derived from the shared AccountKey (all paired devices derive the
    // same one); rows/commits are sealed under per-pair keys (P1.4).
    // Without the account key we can't sync securely, so refuse to start
    // P2P rather than fall back to plaintext.
    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "p2p start requires the account key (sync transport key)".to_string())?;
    let transport_key = account_key.derive_transport_key();

    // Derive libp2p keypair from the per-device identity key. The same
    // keypair signs every outgoing row envelope (P1.3), so receivers can
    // verify against our PeerId.
    let keypair = sovereign_p2p::identity::derive_keypair(&p2p_identity_key)
        .map_err(|e| format!("derive_keypair: {e}"))?;
    let local_peer_id = keypair.public().to_peer_id().to_string();

    // Load paired devices BEFORE the SyncService so the per-pair sealing
    // keys (P1.4) are available from the first sync. The store is
    // encrypted under a DeviceKey-derived key; a legacy plaintext file
    // (pre-P1.4, carried no key material) loads transparently and is
    // re-saved encrypted below. Any other load failure resets the list
    // (Risk 7 in the v0.0.5 plan).
    let store_key = sovereign_p2p::pairing::derive_store_key(&p2p_identity_key);
    let paired_path = crate::setup::crypto_dir().join("paired_devices.json");
    let mut manager = if paired_path.exists() {
        PairingManager::load(&paired_path, &store_key).unwrap_or_else(|e| {
            tracing::warn!("paired_devices.json invalid ({e}); starting fresh");
            PairingManager::new(paired_path.clone())
        })
    } else {
        PairingManager::new(paired_path.clone())
    };
    // P1.4: populate any missing per-pair keys (deterministic derivation
    // from the shared AccountKey — both ends derive the same key), then
    // persist, which also migrates a legacy plaintext file to the
    // encrypted shape.
    if manager.ensure_pair_keys(&account_key, &local_peer_id) {
        if let Err(e) = manager.save(&store_key) {
            tracing::warn!("failed to persist pair keys: {e}");
        }
    }

    // P1.3: per-device Lamport version store, persisted next to the rest
    // of the crypto state.
    let version_store = sovereign_p2p::VersionStore::load_or_default(
        crate::setup::crypto_dir().join("sync_versions.json"),
    );

    let sync_service = Arc::new(SyncService::new(
        db_dyn,
        local_peer_id.clone(), // P2P-001: version identity = verifiable PeerId
        transport_key,
        keypair.clone(),
        version_store,
    ));
    sync_service.set_pair_keys(manager.pair_key_map());

    // Channels.
    let (command_tx, command_rx) = mpsc::channel::<P2pCommand>(COMMAND_BUFFER);
    let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(EVENT_BUFFER);

    // P4.2: opt-in backup hosting (fragments + guardian shards for
    // other users). The store lives under the crypto dir; the Arc is
    // shared with the node (serving) and AppState (approval commands +
    // accounting).
    let backup_host = if state.config.p2p.backup_host_enabled {
        let host = Arc::new(sovereign_p2p::BackupHost::open(
            crate::setup::crypto_dir().join("backup_host"),
            state.config.p2p.backup_quota_mb.max(1) * 1024 * 1024,
        ));
        *state.backup_host.write().await = Some(host.clone());
        tracing::info!(
            "Backup hosting enabled (quota {} MiB/owner)",
            state.config.p2p.backup_quota_mb.max(1)
        );
        Some(host)
    } else {
        None
    };

    // Construct + listen.
    let p2p_cfg = p2p_config_from_app(&state.config.p2p);
    let mut node = SovereignNode::new(
        &p2p_cfg,
        keypair,
        event_tx,
        command_rx,
        sync_service,
        backup_host,
    )
    .map_err(|e| format!("SovereignNode::new: {e}"))?;
    let listen_addr = node
        .listen(&p2p_cfg)
        .map_err(|e| format!("p2p listen: {e}"))?;
    tracing::info!("P2P node listening on {listen_addr}");

    // Spawn the swarm event loop.
    tauri::async_runtime::spawn(async move {
        node.run().await;
        tracing::info!("P2P node event loop exited");
    });

    // Spawn the event translator: P2pEvent → (auto-trigger StartSync,
    // persist P3.1 pairing completions, collect listen addrs, forward as
    // OrchestratorEvent). Holds clones/Arcs of everything it needs; all
    // outlive the task naturally because their other ends live for the
    // process lifetime.
    let ctx = TranslatorCtx {
        command_tx: command_tx.clone(),
        orch_tx: state.orch_tx.clone(),
        connectivity: state.connectivity.clone(),
        wifi_only: state.config.p2p.wifi_only,
        pairing_manager: state.pairing_manager.clone(),
        listen_addrs: state.p2p_listen_addrs.clone(),
        account_key: account_key.clone(),
        store_key,
        local_peer_id: local_peer_id.clone(),
    };
    tauri::async_runtime::spawn(async move {
        spawn_event_translator(event_rx, ctx).await;
    });

    // Install the command sender on AppState + orchestrator.
    state.set_p2p_command_tx(command_tx.clone()).await;
    let cmd_for_pairing = command_tx.clone();
    if let Some(ref orch) = state.orchestrator {
        orch.set_p2p_command_tx(command_tx);
    }

    // P2P-001: seed the node's paired-peer allow-list from the persisted
    // pairing list (loaded above, before the SyncService was built). Until
    // this arrives the node's allow-list is empty, so it fails CLOSED — no
    // peer is served sync data before we've told it which devices are
    // actually paired. (The per-pair sealing keys were installed on the
    // SyncService directly before the node took ownership of it.)
    let paired_ids: Vec<String> = manager
        .list_devices()
        .iter()
        .map(|d| d.peer_id.clone())
        .collect();
    let _ = cmd_for_pairing
        .send(P2pCommand::UpdatePairedPeers { peer_ids: paired_ids })
        .await;
    *state.pairing_manager.write().await = Some(manager);

    Ok(())
}

/// Re-push the current paired-peer allow-list AND per-pair sealing keys
/// to the running P2P node. Call this whenever the pairing list changes
/// at runtime (a device is paired or forgotten) so the node's [`P2P-001`]
/// gate and the SyncService's pair-key map (P1.4 / P2P-005) stay in sync
/// without requiring an app restart — a forgotten device loses its
/// sealing key together with its allow-list entry. No-op if P2P isn't
/// running.
pub async fn refresh_paired_peers(state: &AppState) {
    let cmd_tx = match state.p2p_command_tx().await {
        Some(tx) => tx,
        None => return,
    };
    let (peer_ids, pair_keys) = {
        let guard = state.pairing_manager.read().await;
        match guard.as_ref() {
            Some(m) => (
                m.list_devices()
                    .iter()
                    .map(|d| d.peer_id.clone())
                    .collect::<Vec<String>>(),
                m.pair_key_map(),
            ),
            None => (Vec::new(), std::collections::HashMap::new()),
        }
    };
    let _ = cmd_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids })
        .await;
    let _ = cmd_tx
        .send(P2pCommand::UpdatePairKeys {
            keys: sovereign_p2p::PairKeyMap(pair_keys),
        })
        .await;
}

/// Everything the event translator needs to act on P2P events without
/// touching `AppState` (which it can't hold across the spawn).
struct TranslatorCtx {
    command_tx: mpsc::Sender<P2pCommand>,
    orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    connectivity: Arc<AtomicU8>,
    wifi_only: bool,
    pairing_manager:
        Arc<tokio::sync::RwLock<Option<sovereign_p2p::pairing::PairingManager>>>,
    listen_addrs: Arc<std::sync::RwLock<Vec<String>>>,
    account_key: Arc<sovereign_crypto::account_key::AccountKey>,
    store_key: [u8; 32],
    local_peer_id: String,
}

/// Translate `P2pEvent`s into `OrchestratorEvent`s for the UI bridge,
/// auto-trigger `StartSync` for any peer mDNS surfaces, collect the
/// swarm's concrete listen addrs (pairing-offer dial hints), and persist
/// P3.1 pairing completions.
///
/// The connectivity gate (Phase 4.2) suppresses the auto-trigger when
/// the device reports cellular/offline and `wifi_only` is set. mDNS
/// itself doesn't work over cellular (no multicast on most carriers),
/// so this is mostly about not waking the QUIC transport for
/// unreachable peers and not racking up metered data on Android.
async fn spawn_event_translator(mut event_rx: mpsc::Receiver<P2pEvent>, ctx: TranslatorCtx) {
    while let Some(event) = event_rx.recv().await {
        // Auto-trigger sync on peer discovery. The node's StartSync
        // handler already dedupes against an in-flight session for the
        // same peer, so an mDNS burst doesn't kick off duplicate syncs.
        if let P2pEvent::PeerDiscovered { ref peer_id, .. } = event {
            let state = connectivity_from_u8(ctx.connectivity.load(Ordering::Relaxed));
            if state.allows_auto_sync(ctx.wifi_only) {
                let _ = ctx.command_tx
                    .try_send(P2pCommand::StartSync {
                        peer_id: peer_id.clone(),
                    });
            } else {
                tracing::debug!(
                    "Skipping auto-sync for {peer_id}: connectivity={state:?}, wifi_only={}",
                    ctx.wifi_only
                );
            }
        }

        // Forward as OrchestratorEvent so tauri_events.rs can emit a
        // typed payload to the frontend.
        let orch_event = match event {
            P2pEvent::PeerDiscovered {
                peer_id,
                device_name,
            } => Some(OrchestratorEvent::DeviceDiscovered {
                device_id: peer_id,
                device_name: device_name.unwrap_or_else(|| "Unknown device".into()),
            }),
            P2pEvent::PeerLost { peer_id } => Some(OrchestratorEvent::SyncStatus {
                peer_id,
                status: "disconnected".into(),
            }),
            P2pEvent::SyncStarted { peer_id } => Some(OrchestratorEvent::SyncStatus {
                peer_id,
                status: "started".into(),
            }),
            P2pEvent::SyncCompleted {
                peer_id,
                docs_synced,
            } => Some(OrchestratorEvent::SyncStatus {
                peer_id,
                status: format!("completed ({docs_synced} items)"),
            }),
            P2pEvent::SyncConflict {
                doc_id,
                description,
            } => Some(OrchestratorEvent::SyncConflict { doc_id, description }),
            P2pEvent::PairingCompleted { peer_id, device_name } => {
                // P3.1: the node already registered the new device for
                // this session (allow-list + pair key); persist it so it
                // survives restarts, then re-push the full lists.
                persist_paired_device(&ctx, &peer_id, &device_name).await;
                Some(OrchestratorEvent::DevicePaired {
                    device_id: peer_id,
                    device_name,
                })
            }
            P2pEvent::PairingFailed { reason, offer_dead } => {
                tracing::warn!(
                    "Pairing attempt failed: {reason} (offer dead: {offer_dead})"
                );
                Some(OrchestratorEvent::PairingFailed { reason, offer_dead })
            }
            P2pEvent::ListenAddr { address } => {
                let mut addrs = ctx
                    .listen_addrs
                    .write()
                    .expect("listen addr lock poisoned");
                if !addrs.contains(&address) {
                    addrs.push(address);
                }
                None
            }
            P2pEvent::ShardReceived { shard_id, .. } => {
                tracing::info!("Shard received: {shard_id}");
                None
            }
            P2pEvent::BackupPlaced { peer_id, accepted, rejected } => {
                Some(OrchestratorEvent::SyncStatus {
                    peer_id,
                    status: format!("backup placed ({accepted} ok, {rejected} rejected)"),
                })
            }
            P2pEvent::ShardRequested { request_id, for_user, epoch } => {
                // P4.3: a recovery wants a guardian shard we hold. The
                // release stays locked until this device's user approves
                // (approve_shard_release command) + the 72h delay.
                // Dedicated UI lands with the recovery UX pass.
                tracing::warn!(
                    "RECOVERY REQUEST pending approval: request {request_id} for {for_user} (epoch {epoch})"
                );
                None
            }
            P2pEvent::PairingRequested { peer_id, device_name } => {
                tracing::info!("Pairing requested from {peer_id} ({device_name})");
                None
            }
        };
        if let Some(e) = orch_event {
            let _ = ctx.orch_tx.send(e);
        }
    }
    tracing::info!("P2P event translator exited (channel closed)");
}

/// Persist a P3.1 pairing completion: add the device (with its derived
/// per-pair key) to the encrypted paired store and re-push the
/// allow-list + key map to the node.
async fn persist_paired_device(ctx: &TranslatorCtx, peer_id: &str, device_name: &str) {
    let pair_key = ctx.account_key.derive_pair_key(&ctx.local_peer_id, peer_id);
    let (peer_ids, keys) = {
        let mut guard = ctx.pairing_manager.write().await;
        let Some(manager) = guard.as_mut() else {
            tracing::warn!("PairingCompleted before PairingManager was loaded; not persisted");
            return;
        };
        manager.add_device(sovereign_p2p::pairing::PairedDevice::with_key(
            peer_id.to_string(),
            device_name.to_string(),
            pair_key,
        ));
        if let Err(e) = manager.save(&ctx.store_key) {
            tracing::warn!("failed to persist paired_devices.json after pairing: {e}");
        }
        (
            manager
                .list_devices()
                .iter()
                .map(|d| d.peer_id.clone())
                .collect::<Vec<String>>(),
            manager.pair_key_map(),
        )
    };
    let _ = ctx.command_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids })
        .await;
    let _ = ctx.command_tx
        .send(P2pCommand::UpdatePairKeys {
            keys: sovereign_p2p::PairKeyMap(keys),
        })
        .await;
    tracing::info!("Paired device persisted: {device_name} ({peer_id})");
}

/// Fire `StartSync` for every paired peer. Used by the periodic 5-min
/// poll task and by the `trigger_sync_now` Tauri command. A no-op when
/// the P2P node hasn't started or when the connectivity gate
/// (Phase 4.2) blocks (e.g. cellular + wifi_only on Android).
pub async fn trigger_sync_for_all_paired(state: &AppState) -> u32 {
    let cmd_tx = match state.p2p_command_tx().await {
        Some(tx) => tx,
        None => return 0,
    };
    let connectivity = state.connectivity_state();
    if !connectivity.allows_auto_sync(state.config.p2p.wifi_only) {
        tracing::debug!(
            "Skipping paired-peer sync: connectivity={connectivity:?}, wifi_only={}",
            state.config.p2p.wifi_only
        );
        return 0;
    }
    let manager_guard = state.pairing_manager.read().await;
    let manager = match manager_guard.as_ref() {
        Some(m) => m,
        None => return 0,
    };
    let mut fired = 0u32;
    for device in manager.list_devices() {
        if cmd_tx
            .try_send(P2pCommand::StartSync {
                peer_id: device.peer_id.clone(),
            })
            .is_ok()
        {
            fired += 1;
        }
    }
    fired
}
