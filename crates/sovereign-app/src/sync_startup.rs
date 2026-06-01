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

    // Build SyncService over the live DB. device_id is the local UUID
    // (load_or_create_device_id) — the same one the libp2p PeerId is
    // ultimately derived from.
    let device_id = crate::setup::load_or_create_device_id()
        .map_err(|e| format!("device id: {e}"))?;
    let db_dyn: Arc<dyn sovereign_db::GraphDB> = state.db.clone();
    // P2P-002: every sync envelope is AEAD-sealed under a transport key
    // derived from the shared AccountKey (all paired devices derive the
    // same one). Without the account key we can't sync securely, so refuse
    // to start P2P rather than fall back to plaintext.
    let transport_key = state
        .account_key()
        .await
        .ok_or_else(|| "p2p start requires the account key (sync transport key)".to_string())?
        .derive_transport_key();
    let sync_service = Arc::new(SyncService::new(db_dyn, device_id, transport_key));

    // Derive libp2p keypair from the per-device identity key.
    let keypair = sovereign_p2p::identity::derive_keypair(&p2p_identity_key)
        .map_err(|e| format!("derive_keypair: {e}"))?;

    // Channels.
    let (command_tx, command_rx) = mpsc::channel::<P2pCommand>(COMMAND_BUFFER);
    let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(EVENT_BUFFER);

    // Construct + listen.
    let p2p_cfg = p2p_config_from_app(&state.config.p2p);
    let mut node = SovereignNode::new(
        &p2p_cfg,
        keypair,
        event_tx,
        command_rx,
        sync_service,
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
    // forward as OrchestratorEvent). Holds clones of command_tx +
    // orch_tx + connectivity gate; all outlive the task naturally
    // because their other ends live for the process lifetime.
    let orch_tx = state.orch_tx.clone();
    let cmd_for_autosync = command_tx.clone();
    let connectivity = state.connectivity.clone();
    let wifi_only = state.config.p2p.wifi_only;
    tauri::async_runtime::spawn(async move {
        spawn_event_translator(event_rx, cmd_for_autosync, orch_tx, connectivity, wifi_only).await;
    });

    // Install the command sender on AppState + orchestrator.
    state.set_p2p_command_tx(command_tx.clone()).await;
    let cmd_for_pairing = command_tx.clone();
    if let Some(ref orch) = state.orchestrator {
        orch.set_p2p_command_tx(command_tx);
    }

    // Load paired devices (best-effort — empty manager on missing or
    // invalid file). v0.0.4 stale records are wiped per Risk 7 in the
    // v0.0.5 plan: any deserialization failure resets the list.
    let paired_path = crate::setup::crypto_dir().join("paired_devices.json");
    let manager = if paired_path.exists() {
        PairingManager::load(&paired_path).unwrap_or_else(|e| {
            tracing::warn!("paired_devices.json invalid ({e}); starting fresh");
            PairingManager::new(paired_path)
        })
    } else {
        PairingManager::new(paired_path)
    };
    // P2P-001: seed the node's paired-peer allow-list from the persisted
    // pairing list. Until this arrives the node's allow-list is empty, so
    // it fails CLOSED — no peer is served sync data before we've told it
    // which devices are actually paired.
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

/// Re-push the current paired-peer allow-list to the running P2P node.
/// Call this whenever the pairing list changes at runtime (a device is
/// paired or forgotten) so the node's [`P2P-001`] gate stays in sync
/// without requiring an app restart. No-op if P2P isn't running.
pub async fn refresh_paired_peers(state: &AppState) {
    let cmd_tx = match state.p2p_command_tx().await {
        Some(tx) => tx,
        None => return,
    };
    let peer_ids: Vec<String> = {
        let guard = state.pairing_manager.read().await;
        match guard.as_ref() {
            Some(m) => m.list_devices().iter().map(|d| d.peer_id.clone()).collect(),
            None => Vec::new(),
        }
    };
    let _ = cmd_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids })
        .await;
}

/// Translate `P2pEvent`s into `OrchestratorEvent`s for the UI bridge,
/// and auto-trigger `StartSync` for any peer mDNS surfaces (LAN trust
/// boundary in v0.0.5; pair-key encryption in v0.0.5.x will drop
/// unpaired peers via decrypt failure).
///
/// The connectivity gate (Phase 4.2) suppresses the auto-trigger when
/// the device reports cellular/offline and `wifi_only` is set. mDNS
/// itself doesn't work over cellular (no multicast on most carriers),
/// so this is mostly about not waking the QUIC transport for
/// unreachable peers and not racking up metered data on Android.
async fn spawn_event_translator(
    mut event_rx: mpsc::Receiver<P2pEvent>,
    command_tx: mpsc::Sender<P2pCommand>,
    orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    connectivity: Arc<AtomicU8>,
    wifi_only: bool,
) {
    while let Some(event) = event_rx.recv().await {
        // Auto-trigger sync on peer discovery. The node's StartSync
        // handler already dedupes against an in-flight session for the
        // same peer, so an mDNS burst doesn't kick off duplicate syncs.
        if let P2pEvent::PeerDiscovered { ref peer_id, .. } = event {
            let state = connectivity_from_u8(connectivity.load(Ordering::Relaxed));
            if state.allows_auto_sync(wifi_only) {
                let _ = command_tx
                    .try_send(P2pCommand::StartSync {
                        peer_id: peer_id.clone(),
                    });
            } else {
                tracing::debug!(
                    "Skipping auto-sync for {peer_id}: connectivity={state:?}, wifi_only={wifi_only}"
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
            P2pEvent::PairingCompleted { peer_id, .. } => {
                Some(OrchestratorEvent::DevicePaired { device_id: peer_id })
            }
            P2pEvent::ShardReceived { shard_id, .. } => {
                tracing::info!("Shard received: {shard_id}");
                None
            }
            P2pEvent::PairingRequested { peer_id, device_name } => {
                tracing::info!("Pairing requested from {peer_id} ({device_name})");
                None
            }
        };
        if let Some(e) = orch_event {
            let _ = orch_tx.send(e);
        }
    }
    tracing::info!("P2P event translator exited (channel closed)");
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
