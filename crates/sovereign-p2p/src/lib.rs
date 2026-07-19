pub mod backup;
pub mod backup_client;
pub mod backup_host;
pub mod behaviour;
pub mod config;
pub mod error;
pub mod deposit_token;
pub mod guardian_enroll;
pub mod identity;
pub mod node;
pub mod pairing;
pub mod pairing_client;
pub mod pairing_offer;
pub mod recovery;
pub mod access_recovery;
pub mod protocol;
pub mod sync_engine;
pub mod sync_service;
pub mod version_store;

pub use backup_host::BackupHost;
pub use config::{ConnectivityState, P2pConfig};
pub use error::{P2pError, P2pResult};
pub use guardian_enroll::{GuardianEnrollOffer, GuardianGrant};
pub use node::{
    ActiveGuardianOffer, ActivePairingOffer, P2pCommand, P2pEvent, PairKeyMap, SovereignNode,
};
pub use pairing_offer::{is_routable_listen_addr, PairingOffer};

// Re-exported so dependents that only build on top of this crate (the
// guardian app) can name libp2p types (Keypair, PeerId) without pinning
// their own — one libp2p version for the whole workspace.
pub use libp2p;
pub use sync_service::SyncService;
pub use version_store::VersionStore;
