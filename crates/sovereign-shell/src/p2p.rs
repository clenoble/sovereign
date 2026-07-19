//! P2P multi-device sync for the native shell — Batch 6c.
//!
//! A focused port of `sovereign-app`'s `sync_startup.rs`. The shell runs the
//! identical `sovereign-p2p` stack the Tauri app + the mobile app use, so the
//! pairing protocol, the per-pair sealing keys, and the on-wire envelope format
//! are byte-compatible across all three UIs. The only difference is how the
//! pairing OFFER is transferred (Phase 2): the shell can render the same QR the
//! mobile app scans, and accepts a pasted offer code + PIN.
//!
//! What this module does on login (when P2P is enabled):
//! 1. Derive the libp2p keypair from the per-device identity key (the DeviceKey)
//!    — the same keypair that signs every outgoing row envelope (P1.3), so a
//!    receiver can verify authorship against our PeerId (P2P-001).
//! 2. Load (or create) the encrypted `PairingManager`
//!    (`crypto/paired_devices.json`), populate any missing per-pair sealing keys
//!    (P1.4, deterministic from the shared AccountKey), and persist.
//! 3. Build a [`SyncService`] over the live DB, sealed under the AccountKey's
//!    transport key (P2P-002).
//! 4. Spawn the [`SovereignNode`] swarm (mDNS + QUIC) + an event translator that
//!    auto-triggers sync on discovery, collects listen addresses, persists
//!    pairing completions, and forwards every `P2pEvent` to the orchestrator
//!    event channel the shell already drains.
//!
//! Backup hosting (Phase 3) is not wired here yet — the node is started with
//! `backup_host: None`.

use std::sync::Arc;

use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_db::traits::GraphDB;
use sovereign_p2p::pairing::{PairedDevice, PairingManager};
use sovereign_p2p::pairing_offer::derive_handshake_key;
use sovereign_p2p::{
    ActiveGuardianOffer, ActivePairingOffer, P2pCommand, P2pConfig, P2pEvent, PairKeyMap,
    PairingOffer, SovereignNode, SyncService,
};
use tokio::sync::mpsc;

/// Pairing-offer validity window. Deliberately longer than the app's 120 s
/// default: the real flow (scan a QR or Copy code → switch device/window →
/// paste → type the code → set a password) easily runs past 2 minutes,
/// especially over RDP. Safe to extend — the offer carries no secret (the PIN
/// is proven online and attempt-capped), so the window only bounds the
/// handshake's availability, not the secret's exposure.
const OFFER_TTL_SECS: i64 = 600;

/// A freshly-armed pairing offer to show on the existing device: the encoded
/// offer payload (rendered as a QR) + the out-of-band PIN. The offer expires
/// after OFFER_TTL_SECONDS (surfaced as a static note in the modal).
pub(crate) struct PairingOfferOut {
    /// base64url `PairingOffer` — identical to the app's `qr_payload_b64`, so a
    /// mobile device scanning the QR (or a peer decoding the code) interops.
    pub(crate) code: String,
    /// Single-use pairing code (10-char, grouped XXXXX-XXXXX), proven online
    /// during the handshake (CRYPTO-003 pt2).
    pub(crate) pin: String,
}

/// Channel buffer for P2P commands (mDNS bursts queue many StartSync requests).
const COMMAND_BUFFER: usize = 64;
/// Channel buffer for P2P events flowing into the translator.
const EVENT_BUFFER: usize = 256;

/// Live handle to the running P2P node, held by the App. Dropping it does NOT
/// stop the node (the swarm + translator are detached tasks on the runtime);
/// it's the App's control surface for the Devices & Sync window.
pub(crate) struct P2pHandle {
    pub(crate) command_tx: mpsc::Sender<P2pCommand>,
    /// Our verifiable libp2p PeerId (sync authorship identity, P2P-001).
    pub(crate) local_peer_id: String,
    /// Concrete swarm listen addresses (dial hints for a pairing offer),
    /// seeded with the first `listen()` addr and grown by the translator.
    pub(crate) listen_addrs: Arc<std::sync::RwLock<Vec<String>>>,
    /// The encrypted paired-device store, shared with the translator so a
    /// pairing completion persists without a restart.
    pub(crate) pairing_manager: Arc<tokio::sync::RwLock<PairingManager>>,
    /// AccountKey — sealed into the armed offer (`arm_pairing_offer`) so the new
    /// device receives it over the live handshake (the AccountKey never travels
    /// in the QR, CRYPTO-003 pt2).
    pub(crate) account_key: Arc<AccountKey>,
    /// DeviceKey-derived key that encrypts the paired-device store at rest.
    pub(crate) store_key: [u8; 32],
    /// This device's human-readable name (shown to peers, and in the offer).
    pub(crate) device_name: String,
}

impl P2pHandle {
    /// Snapshot of paired devices for the Devices window: (peer_id, name).
    pub(crate) fn paired_devices(&self, rt: &tokio::runtime::Runtime) -> Vec<(String, String)> {
        rt.block_on(async {
            self.pairing_manager
                .read()
                .await
                .list_devices()
                .iter()
                .map(|d| (d.peer_id.clone(), d.device_name.clone()))
                .collect()
        })
    }

    /// Current listen addresses (for display / pairing-offer dial hints).
    pub(crate) fn listen_addrs(&self) -> Vec<String> {
        self.listen_addrs.read().map(|a| a.clone()).unwrap_or_default()
    }

    /// Right after pairing, direct-dial the just-paired peer at the offer's
    /// address hints and kick the first sync — independent of mDNS discovery
    /// (the initial sync previously waited for an mDNS `PeerDiscovered`, which
    /// never fires when mDNS is unavailable / on the wrong interface). The
    /// `StartSync` is delayed + retried so the QUIC/Noise connection is up
    /// before the manifest request goes out; a too-early try just fails and the
    /// node removes its session (see `decrement_pending`), so the retry isn't
    /// deduped away.
    pub(crate) fn pair_sync(&self, rt: &tokio::runtime::Runtime, peer_id: String, addrs: Vec<String>) {
        let command_tx = self.command_tx.clone();
        rt.spawn(async move {
            // Dial each hint, appending /p2p/<peer> so the Noise handshake
            // authenticates the responder against the paired identity.
            for a in &addrs {
                let full = if a.contains("/p2p/") { a.clone() } else { format!("{a}/p2p/{peer_id}") };
                let _ = command_tx.send(P2pCommand::Dial { address: full }).await;
            }
            // Two attempts (≈3s, ≈10s) to cover a slow handshake. The node
            // dedupes an in-flight session, so a redundant second trigger is a
            // no-op once the first is running.
            for gap in [3u64, 7] {
                tokio::time::sleep(std::time::Duration::from_secs(gap)).await;
                if command_tx.send(P2pCommand::StartSync { peer_id: peer_id.clone() }).await.is_err() {
                    break; // node shut down
                }
            }
        });
    }

    /// Ask the node to restore a peer-overwritten row to its pre-sync value —
    /// the node holds the key to unseal the `RowRecovery` (peer-review panel).
    /// Returns false if the command channel is full/closed.
    pub(crate) fn restore_row(&self, recovery_id: &str) -> bool {
        self.command_tx
            .try_send(P2pCommand::RestoreRowRecovery {
                recovery_id: recovery_id.to_string(),
            })
            .is_ok()
    }

    /// Fire `StartSync` for every paired peer. Returns how many were queued.
    pub(crate) fn sync_now(&self, rt: &tokio::runtime::Runtime) -> u32 {
        rt.block_on(async {
            let guard = self.pairing_manager.read().await;
            let mut fired = 0u32;
            for device in guard.list_devices() {
                if self
                    .command_tx
                    .try_send(P2pCommand::StartSync {
                        peer_id: device.peer_id.clone(),
                    })
                    .is_ok()
                {
                    fired += 1;
                }
            }
            fired
        })
    }

    /// Forget a paired device: drop it from the store (losing its sealing key
    /// together with its allow-list entry), persist, and re-push the trimmed
    /// allow-list + key map to the node so it fails closed for that peer
    /// without a restart.
    pub(crate) fn forget(&self, rt: &tokio::runtime::Runtime, peer_id: &str) {
        rt.block_on(async {
            let (peer_ids, keys) = {
                let mut guard = self.pairing_manager.write().await;
                guard.remove_device(peer_id);
                if let Err(e) = guard.save(&self.store_key) {
                    eprintln!("failed to persist paired_devices.json after forget: {e}");
                }
                (
                    guard
                        .list_devices()
                        .iter()
                        .map(|d| d.peer_id.clone())
                        .collect::<Vec<String>>(),
                    guard.pair_key_map(),
                )
            };
            let _ = self
                .command_tx
                .send(P2pCommand::UpdatePairedPeers { peer_ids })
                .await;
            let _ = self
                .command_tx
                .send(P2pCommand::UpdatePairKeys {
                    keys: PairKeyMap(keys),
                })
                .await;
        });
    }

    /// Arm a fresh pairing offer (existing-device side). Builds a plaintext
    /// `PairingOffer` (this device's PeerId + name + listen addrs), generates a
    /// single-use code, derives the Argon2id handshake key, and arms the node via
    /// `SetPairingOffer` so it completes the handshake when a new device proves
    /// the PIN online — at which point the node hands over the salt + AccountKey
    /// (sealed under the handshake key, never in the QR) and emits
    /// `PairingCompleted` (persisted by the event translator). Returns the
    /// encoded offer + PIN to display.
    pub(crate) fn arm_pairing_offer(
        &self,
        rt: &tokio::runtime::Runtime,
    ) -> Result<PairingOfferOut, String> {
        rt.block_on(async {
            // The MasterKey salt is released to the new device over the
            // handshake (it used to travel in the QR).
            let salt = std::fs::read(crate::crypto::crypto_dir().join("salt"))
                .map_err(|e| format!("read salt: {e}"))?;
            // Filter loopback/unspecified dial hints — a remote joiner can't
            // reach them and a 127.0.0.1 hint makes it dial itself. Same one
            // predicate the app offer paths use (sovereign_p2p).
            let addrs = self
                .listen_addrs()
                .into_iter()
                .filter(|a| sovereign_p2p::is_routable_listen_addr(a))
                .collect::<Vec<_>>();
            let offer = PairingOffer::new(
                self.local_peer_id.clone(),
                self.device_name.clone(),
                addrs,
                OFFER_TTL_SECS,
            );
            // High-entropy single-use code (50 bits, grouped XXXXX-XXXXX). Both
            // ends derive the handshake key from it; it's proven over the live
            // connection and never travels in the QR (CRYPTO-003 pt2). Mobile must
            // be on this same protocol to interop.
            let pin = sovereign_crypto::pair_payload::generate_pairing_code();

            // Argon2id stretch (~0.5 s) — off the async worker.
            let offer_for_kdf = offer.clone();
            let pin_for_kdf = pin.clone();
            let handshake_key =
                tokio::task::spawn_blocking(move || derive_handshake_key(&pin_for_kdf, &offer_for_kdf))
                    .await
                    .map_err(|e| format!("kdf task: {e}"))?
                    .map_err(|e| format!("derive_handshake_key: {e}"))?;

            self.command_tx
                .send(P2pCommand::SetPairingOffer {
                    offer: Box::new(ActivePairingOffer::new(
                        offer.offer_id.clone(),
                        handshake_key,
                        offer.expires_at,
                        salt,
                        *self.account_key.as_bytes(),
                        self.device_name.clone(),
                    )),
                })
                .await
                .map_err(|e| format!("arm pairing offer: {e}"))?;

            Ok(PairingOfferOut {
                code: offer.encode().map_err(|e| format!("encode offer: {e}"))?,
                pin,
            })
        })
    }
}

/// A freshly-armed guardian-enrollment offer to show on the owner's device.
pub(crate) struct GuardianOfferOut {
    /// base64url `GuardianEnrollOffer` — rendered as the QR the guardian scans.
    pub(crate) qr_payload: String,
    /// Spoken code the owner reads aloud (XXXXX-XXXXX). Proven live; never in QR.
    pub(crate) code: String,
    /// Which of the 5 this offer is for (1-based), for the modal title.
    pub(crate) slot_ordinal: usize,
}

impl P2pHandle {
    /// Arm an in-person guardian-enrollment offer for the next un-enrolled slot.
    ///
    /// Mirrors the Tauri owner's `begin_guardian_enrollment` and this crate's
    /// own `arm_pairing_offer`: read the roster under the KEK (the handle does
    /// not hold it, so it is passed in), fold in any enrollments confirmed via
    /// p2p events, refuse if already armed at 5, take the next pending share,
    /// build + Argon2id-stretch the offer off the async worker, and hand it to
    /// the node via `SetGuardianOffer`. The share travels only over the live
    /// handshake, never in the QR.
    ///
    /// Listen addresses are filtered through
    /// `sovereign_p2p::is_routable_listen_addr` — the one shared copy all four
    /// offer paths now use (consolidated per coord from-windows/0059), so a
    /// loopback hint never reaches a remote guardian's QR.
    pub(crate) fn arm_guardian_offer(
        &self,
        rt: &tokio::runtime::Runtime,
        kek: &sovereign_crypto::kek::Kek,
        seed_relays: &[String],
    ) -> Result<GuardianOfferOut, String> {
        use sovereign_crypto::recovery_roster::{GUARDIAN_THRESHOLD, GUARDIAN_TOTAL};
        use sovereign_p2p::guardian_enroll::{
            derive_enroll_key, GuardianEnrollOffer, GUARDIAN_OFFER_TTL_SECONDS,
        };

        rt.block_on(async {
            // Roster read + reconcile need the KEK (owner-side, login-only).
            let store = crate::crypto::recovery_store();
            let mut setup = store.load_or_create(kek, &self.account_key)?;
            // Fold in guardians confirmed via GuardianEnrolled events (the
            // translator has no KEK, so it only queued them).
            let reconciled = store.reconcile_pending(&mut setup, kek)?;
            if reconciled.needs_attention() {
                eprintln!(
                    "guardian roster: {} unreadable pending enrollment(s) kept for review: {}",
                    reconciled.retained,
                    reconciled.skipped.join("; ")
                );
            }
            if setup.is_armed() {
                return Err("All 5 guardians are already enrolled — recovery is ready.".to_string());
            }
            let enrolled_before = setup.enrolled_count();
            let (shard_id, share_b64) = {
                let slot = setup
                    .next_pending_slot()
                    .ok_or_else(|| "no share left to hand out".to_string())?;
                (
                    slot.shard_id.clone(),
                    slot.pending_share_b64
                        .clone()
                        .ok_or_else(|| "slot has no pending share".to_string())?,
                )
            };

            // Refresh the pre-login recovery card so a recovering device can
            // reach the guardians we have so far (best-effort).
            let owner_tag = self.account_key.derive_backup_tag();
            if let Err(e) = store.write_recovery_card(&setup, &owner_tag, seed_relays) {
                eprintln!("recovery card refresh failed (continuing): {e}");
            }

            // Filter loopback/unspecified dial hints — same one predicate the
            // other three offer paths use (sovereign_p2p). Closes the four-way
            // scatter this fn's doc comment flagged (coord from-windows/0059).
            let addrs = self
                .listen_addrs()
                .into_iter()
                .filter(|a| sovereign_p2p::is_routable_listen_addr(a))
                .collect::<Vec<_>>();
            let offer = GuardianEnrollOffer::new(
                self.local_peer_id.clone(),
                self.device_name.clone(),
                addrs,
                GUARDIAN_OFFER_TTL_SECONDS,
            );
            let code = sovereign_crypto::pair_payload::generate_pairing_code();

            // Argon2id stretch (~0.5 s) — off the async worker.
            let offer_for_kdf = offer.clone();
            let code_for_kdf = code.clone();
            let handshake_key =
                tokio::task::spawn_blocking(move || derive_enroll_key(&code_for_kdf, &offer_for_kdf))
                    .await
                    .map_err(|e| format!("kdf task: {e}"))?
                    .map_err(|e| format!("derive_enroll_key: {e}"))?;

            self.command_tx
                .send(P2pCommand::SetGuardianOffer {
                    offer: Box::new(ActiveGuardianOffer::new(
                        offer.offer_id.clone(),
                        handshake_key,
                        offer.expires_at,
                        share_b64,
                        shard_id,
                        owner_tag,
                        self.device_name.clone(),
                        setup.epoch,
                        GUARDIAN_THRESHOLD,
                        GUARDIAN_TOTAL as u8,
                    )),
                })
                .await
                .map_err(|e| format!("arm guardian offer: {e}"))?;

            Ok(GuardianOfferOut {
                qr_payload: offer.encode().map_err(|e| format!("encode offer: {e}"))?,
                code,
                slot_ordinal: enrolled_before + 1,
            })
        })
    }
}

/// Map the app-config P2P struct (`sovereign-core`, 8 fields incl. backup) onto
/// the `sovereign-p2p` runtime config (6 fields). Kept identical to the app's
/// `p2p_config_from_app`.
pub(crate) fn p2p_config_from_app(app_p2p: &sovereign_core::config::P2pConfig) -> P2pConfig {
    P2pConfig {
        enabled: app_p2p.enabled,
        listen_port: app_p2p.listen_port,
        rendezvous_server: app_p2p.rendezvous_server.clone(),
        device_name: app_p2p.device_name.clone(),
        enable_mdns: app_p2p.enable_mdns,
        wifi_only: app_p2p.wifi_only,
        seed_relays: app_p2p.seed_relays.clone(),
    }
}

/// Bring up the P2P node + event translator and load the paired-device store.
/// Synchronous: all setup runs inside `rt.block_on`, and the swarm event loop +
/// translator are detached onto the runtime (they outlive this call as long as
/// `rt` lives — it's owned by the App).
pub(crate) fn start_p2p_node(
    rt: &tokio::runtime::Runtime,
    db: Arc<dyn GraphDB>,
    device_key: Arc<DeviceKey>,
    account_key: Arc<AccountKey>,
    cfg: P2pConfig,
    orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
) -> Result<P2pHandle, String> {
    rt.block_on(async move {
        // Derive the libp2p keypair from the per-device identity key. The same
        // keypair signs every outgoing row envelope (P1.3), so receivers verify
        // against our PeerId.
        let keypair = sovereign_p2p::identity::derive_keypair(&device_key)
            .map_err(|e| format!("derive_keypair: {e}"))?;
        let local_peer_id = keypair.public().to_peer_id().to_string();

        // P2P-002: the sync manifest is AEAD-sealed under a transport key
        // derived from the shared AccountKey (all paired devices derive the
        // same one). Rows/commits seal under per-pair keys (P1.4).
        let transport_key = account_key.derive_transport_key();

        // Load the paired-device store BEFORE the SyncService so per-pair keys
        // are available from the first sync. The store is encrypted under a
        // DeviceKey-derived key; a corrupt file resets the list (Risk 7).
        let store_key = sovereign_p2p::pairing::derive_store_key(&device_key);
        let paired_path = crate::crypto::crypto_dir().join("paired_devices.json");
        let mut manager = if paired_path.exists() {
            PairingManager::load(&paired_path, &store_key).unwrap_or_else(|e| {
                eprintln!("paired_devices.json invalid ({e}); starting fresh");
                PairingManager::new(paired_path.clone())
            })
        } else {
            PairingManager::new(paired_path.clone())
        };
        // P1.4: populate any missing per-pair keys (deterministic from the
        // shared AccountKey), then persist (also migrates a legacy plaintext
        // file to the encrypted shape).
        if manager.ensure_pair_keys(&account_key, &local_peer_id) {
            if let Err(e) = manager.save(&store_key) {
                eprintln!("failed to persist pair keys: {e}");
            }
        }

        // P1.3: per-device Lamport version store, persisted next to crypto state.
        let version_store = sovereign_p2p::VersionStore::load_or_default(
            crate::crypto::crypto_dir().join("sync_versions.json"),
        );

        let sync_service = Arc::new(SyncService::new(
            db,
            local_peer_id.clone(), // P2P-001: version identity = verifiable PeerId
            transport_key,
            keypair.clone(),
            version_store,
        ));
        sync_service.set_pair_keys(manager.pair_key_map());

        // Channels.
        let (command_tx, command_rx) = mpsc::channel::<P2pCommand>(COMMAND_BUFFER);
        let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(EVENT_BUFFER);

        // Construct + listen (no backup host in Phase 1).
        let mut node = SovereignNode::new(&cfg, keypair, event_tx, command_rx, sync_service, None)
            .map_err(|e| format!("SovereignNode::new: {e}"))?;
        let listen_addr = node.listen(&cfg).map_err(|e| format!("p2p listen: {e}"))?;
        println!("P2P node listening on {listen_addr}");

        let listen_addrs = Arc::new(std::sync::RwLock::new(vec![listen_addr.to_string()]));
        let pairing_manager = Arc::new(tokio::sync::RwLock::new(manager));

        // Spawn the swarm event loop.
        tokio::spawn(async move {
            node.run().await;
            println!("P2P node event loop exited");
        });

        // Spawn the event translator.
        let ctx = TranslatorCtx {
            command_tx: command_tx.clone(),
            orch_tx,
            pairing_manager: pairing_manager.clone(),
            listen_addrs: listen_addrs.clone(),
            account_key: account_key.clone(),
            store_key,
            local_peer_id: local_peer_id.clone(),
        };
        tokio::spawn(async move {
            spawn_event_translator(event_rx, ctx).await;
        });

        // P2P-001: seed the node's paired-peer allow-list from the persisted
        // list. Until this arrives the allow-list is empty, so the node fails
        // CLOSED — no peer is served sync data before we've confirmed it's paired.
        let paired_ids: Vec<String> = pairing_manager
            .read()
            .await
            .list_devices()
            .iter()
            .map(|d| d.peer_id.clone())
            .collect();
        let _ = command_tx
            .send(P2pCommand::UpdatePairedPeers {
                peer_ids: paired_ids,
            })
            .await;

        Ok(P2pHandle {
            command_tx,
            local_peer_id,
            listen_addrs,
            pairing_manager,
            account_key,
            store_key,
            device_name: cfg.device_name,
        })
    })
}

/// Accept-side pairing (Phase 2b): a fresh device joins an existing account.
/// Ports the app's `complete_onboarding_paired` (minus profile/canary/seed — the
/// shell's onboarding is minimal). Self-contained: `pair_with_source` spins up
/// its own ephemeral swarm to dial the offerer, prove the PIN online, and
/// receive salt + AccountKey (sealed under the handshake key). On success this
/// writes salt + auth.store (AccountKey wrapped under the new local passphrase)
/// + the source as a paired device — after which a normal login unlocks the
/// imported account. Blocks up to `timeout`; run off the UI thread.
///
/// Refuses if an auth.store already exists (mirrors the app's IPC-001 guard) —
/// joining REPLACES identity, so it's an onboarding-time action only.
pub(crate) async fn accept_pairing(
    offer_code: String,
    pin: String,
    password: String,
    duress: String,
    device_name: String,
) -> Result<(), String> {
    use sovereign_crypto::account_key::AccountKey;
    use sovereign_crypto::auth::AuthStore;
    use sovereign_crypto::device_key::DeviceKey;
    use sovereign_crypto::master_key::{Kdf, MasterKey};

    if crate::crypto::auth_store_exists() {
        return Err("This device is already set up — log in instead.".into());
    }
    let offer = PairingOffer::decode(offer_code.trim())
        .map_err(|e| format!("That doesn't look like a valid pairing code: {e}"))?;
    let crypto_dir = crate::crypto::crypto_dir();
    std::fs::create_dir_all(&crypto_dir).map_err(|e| format!("crypto dir: {e}"))?;
    let device_id =
        crate::crypto::load_or_create_device_id().map_err(|e| format!("device id: {e}"))?;

    // Interactive handshake: dial the source, prove the PIN, receive the secrets,
    // then derive THIS device's final identity (only possible once the salt
    // arrives — hence the callback).
    let device_id_cb = device_id.clone();
    let password_cb = password.clone();
    let mut device_key_stash: Option<DeviceKey> = None;
    let outcome = sovereign_p2p::pairing_client::pair_with_source(
        &offer,
        &pin,
        &device_name,
        |secrets| {
            // Derive THIS device's identity with the SAME KDF login will use —
            // Argon2id (Kdf::current), matching create_with_imported_account_key +
            // AuthStore::authenticate below. The legacy HKDF `from_passphrase`
            // would make the handshake PeerId + paired-store key diverge from the
            // login-time ones: the paired store wouldn't decrypt after login AND
            // the source would register the wrong PeerId for us, so no sync.
            let master = MasterKey::derive(password_cb.as_bytes(), &secrets.salt, &Kdf::current())
                .map_err(|e| format!("master key: {e}"))?;
            let dk =
                DeviceKey::derive(&master, &device_id_cb).map_err(|e| format!("device key: {e}"))?;
            let kp = sovereign_p2p::identity::derive_keypair(&dk)
                .map_err(|e| format!("identity: {e}"))?;
            let peer_id = kp.public().to_peer_id().to_string();
            device_key_stash = Some(dk);
            Ok(peer_id)
        },
        std::time::Duration::from_secs(60),
    )
    .await
    .map_err(|e| format!("pairing failed: {e}"))?;

    let device_key =
        device_key_stash.ok_or_else(|| "handshake finished without an identity".to_string())?;
    let imported_account_key = AccountKey::from_bytes(outcome.secrets.account_key_bytes);

    // Persist salt + auth.store (imported AccountKey wrapped under the new local
    // passphrase). Duress is optional — fall back to a RANDOM, unreachable
    // decoy passphrase (as the onboarding wizard does), never the shared
    // literal "duress-fallback-unused": that literal is public in the source,
    // identical across installs, and would let a source-aware adversary unlock
    // the decoy and confirm no real duress persona exists (H-shell1 theme).
    std::fs::write(crypto_dir.join("salt"), &outcome.secrets.salt)
        .map_err(|e| format!("write salt: {e}"))?;
    let random_duress;
    let duress = if duress.is_empty() {
        random_duress = sovereign_crypto::random_hex_32();
        &random_duress
    } else {
        &duress
    };
    let auth_store = AuthStore::create_with_imported_account_key(
        password.as_bytes(),
        duress.as_bytes(),
        &outcome.secrets.salt,
        &device_id,
        &imported_account_key,
    )
    .map_err(|e| format!("auth store: {e}"))?;
    auth_store
        .save(&crate::crypto::auth_store_path())
        .map_err(|e| format!("save auth store: {e}"))?;

    // Persist the source as a paired device WITH its per-pair key (P1.4) —
    // encrypted from birth. The source registered OUR final identity during the
    // handshake's PairComplete.
    let store_key = sovereign_p2p::pairing::derive_store_key(&device_key);
    let pair_key =
        imported_account_key.derive_pair_key(&outcome.final_peer_id, &offer.source_peer_id);
    let mut manager = PairingManager::new(crypto_dir.join("paired_devices.json"));
    manager.add_device(PairedDevice::with_key(
        offer.source_peer_id.clone(),
        outcome.secrets.source_device_name.clone(),
        pair_key,
    ));
    if let Err(e) = manager.save(&store_key) {
        eprintln!("failed to persist paired_devices.json: {e}");
    }
    println!(
        "sovereign-shell: paired onboarding complete (source: {})",
        outcome.secrets.source_device_name
    );
    Ok(())
}

/// Decode-only preview of an offer code (for the join form to show the source
/// device name before the user commits, and to validate before the handshake).
/// No network, no secrets. Returns Ok(source_device_name) or Err(reason) — the
/// reason distinguishes an expired offer from a malformed one.
pub(crate) fn preview_offer(offer_code: &str) -> Result<String, String> {
    match PairingOffer::decode(offer_code.trim()) {
        Ok(o) => Ok(o.source_device_name),
        Err(e) => Err(e.to_string()),
    }
}

/// Everything the translator needs without holding App state across the spawn.
struct TranslatorCtx {
    command_tx: mpsc::Sender<P2pCommand>,
    orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    pairing_manager: Arc<tokio::sync::RwLock<PairingManager>>,
    listen_addrs: Arc<std::sync::RwLock<Vec<String>>>,
    account_key: Arc<AccountKey>,
    store_key: [u8; 32],
    local_peer_id: String,
}

/// Translate `P2pEvent`s into `OrchestratorEvent`s for the shell's event drain,
/// auto-trigger `StartSync` for discovered peers (the node drops unpaired ones),
/// collect listen addresses, and persist pairing completions. Desktop has no
/// connectivity gate (wired/Wi-Fi assumed), so auto-sync always fires.
async fn spawn_event_translator(mut event_rx: mpsc::Receiver<P2pEvent>, ctx: TranslatorCtx) {
    while let Some(event) = event_rx.recv().await {
        if let P2pEvent::PeerDiscovered { ref peer_id, .. } = event {
            // The node's StartSync handler dedupes in-flight sessions and drops
            // unpaired peers, so an mDNS burst can't kick off duplicate syncs.
            let _ = ctx.command_tx.try_send(P2pCommand::StartSync {
                peer_id: peer_id.clone(),
            });
        }

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
            P2pEvent::PairingCompleted {
                peer_id,
                device_name,
            } => {
                persist_paired_device(&ctx, &peer_id, &device_name).await;
                Some(OrchestratorEvent::DevicePaired {
                    device_id: peer_id,
                    device_name,
                })
            }
            P2pEvent::PairingFailed { reason, offer_dead } => {
                eprintln!("Pairing attempt failed: {reason} (offer dead: {offer_dead})");
                Some(OrchestratorEvent::PairingFailed { reason, offer_dead })
            }
            P2pEvent::RowsFlaggedForReview { peer_id, count } => {
                // C2: a peer overwrite was stashed — surface it instead of
                // leaving it invisible until the review panel is opened.
                Some(OrchestratorEvent::SyncStatus {
                    peer_id,
                    status: format!(
                        "{count} change(s) from this device stashed for review — press r"
                    ),
                })
            }
            P2pEvent::ListenAddr { address } => {
                if let Ok(mut addrs) = ctx.listen_addrs.write() {
                    if !addrs.contains(&address) {
                        addrs.push(address);
                    }
                }
                None
            }
            // M1.5: mailbox events are consumed by the backup/guardian
            // layer (M2), not the shell sync UI. Ignore here for now.
            P2pEvent::MailboxDeposited { .. } | P2pEvent::MailboxItems { .. } => None,
            P2pEvent::BackupPlaced {
                peer_id,
                accepted,
                rejected,
            } => Some(OrchestratorEvent::SyncStatus {
                peer_id,
                status: format!("backup placed ({accepted} ok, {rejected} rejected)"),
            }),
            P2pEvent::ShardReceived { shard_id, .. } => {
                println!("Shard received: {shard_id}");
                None
            }
            P2pEvent::ShardRequested {
                request_id,
                for_user,
                epoch,
            } => {
                eprintln!(
                    "RECOVERY REQUEST pending approval: request {request_id} for {for_user} (epoch {epoch})"
                );
                None
            }
            P2pEvent::PairingRequested {
                peer_id,
                device_name,
            } => {
                println!("Pairing requested from {peer_id} ({device_name})");
                None
            }
            // F1 guardian enrollment events drive the Tauri owner UI (roster,
            // enrollment QR). The native shell has no F1 owner surface yet —
            // that arrives with the shell port of Surfaces 1/2 — so ignore them
            // here rather than surfacing a half-wired status.
            P2pEvent::GuardianEnrollRequested { .. }
            | P2pEvent::GuardianEnrolled { .. }
            | P2pEvent::GuardianEnrollFailed { .. } => None,
        };
        if let Some(e) = orch_event {
            let _ = ctx.orch_tx.send(e);
        }
    }
    println!("P2P event translator exited (channel closed)");
}

/// Persist a pairing completion (P3.1): add the device with its derived per-pair
/// key to the encrypted store and re-push the allow-list + key map to the node.
async fn persist_paired_device(ctx: &TranslatorCtx, peer_id: &str, device_name: &str) {
    let pair_key = ctx.account_key.derive_pair_key(&ctx.local_peer_id, peer_id);
    let (peer_ids, keys) = {
        let mut guard = ctx.pairing_manager.write().await;
        guard.add_device(PairedDevice::with_key(
            peer_id.to_string(),
            device_name.to_string(),
            pair_key,
        ));
        if let Err(e) = guard.save(&ctx.store_key) {
            eprintln!("failed to persist paired_devices.json after pairing: {e}");
        }
        (
            guard
                .list_devices()
                .iter()
                .map(|d| d.peer_id.clone())
                .collect::<Vec<String>>(),
            guard.pair_key_map(),
        )
    };
    let _ = ctx
        .command_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids })
        .await;
    let _ = ctx
        .command_tx
        .send(P2pCommand::UpdatePairKeys {
            keys: PairKeyMap(keys),
        })
        .await;
    println!("Paired device persisted: {device_name} ({peer_id})");
}
