//! Guardian identity — one persistent libp2p identity per install.
//!
//! The identity root is 32 random bytes written once with `fs_private`;
//! the libp2p keypair (and so the PeerId the owner's roster records at
//! enrollment) is derived from it deterministically, the same way the
//! full app derives its device identity. Losing this file means the
//! owner sees the guardian go unreachable and rotates the shard to a
//! replacement — annoying, not fatal (that's what 3-of-5 is for).

use std::path::{Path, PathBuf};

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::master_key::MasterKey;

use crate::error::{GuardianError, GuardianResult};

const IDENTITY_FILE: &str = "identity.key";
/// Device-id string under which the keypair is derived from the root.
const IDENTITY_DERIVATION_ID: &str = "guardian";

pub struct GuardianIdentity {
    root: MasterKey,
    path: PathBuf,
}

impl GuardianIdentity {
    /// Load the identity from `dir`, creating a fresh one on first run.
    pub fn open(dir: &Path) -> GuardianResult<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(IDENTITY_FILE);
        let root = if path.exists() {
            let bytes = std::fs::read(&path)?;
            let arr: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| GuardianError::Identity("identity file corrupt".into()))?;
            MasterKey::from_bytes(arr)
        } else {
            let root = MasterKey::generate();
            sovereign_crypto::fs_private::write_private(&path, root.as_bytes())?;
            root
        };
        Ok(Self { root, path })
    }

    /// The persistent libp2p keypair for this install.
    pub fn keypair(&self) -> GuardianResult<sovereign_p2p::libp2p::identity::Keypair> {
        let dk = DeviceKey::derive(&self.root, IDENTITY_DERIVATION_ID)?;
        Ok(sovereign_p2p::identity::derive_keypair(&dk)?)
    }

    /// The PeerId the owner's roster will record.
    pub fn peer_id(&self) -> GuardianResult<String> {
        Ok(self.keypair()?.public().to_peer_id().to_string())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let id1 = GuardianIdentity::open(dir.path()).unwrap();
        let peer1 = id1.peer_id().unwrap();
        drop(id1);
        let id2 = GuardianIdentity::open(dir.path()).unwrap();
        assert_eq!(peer1, id2.peer_id().unwrap(), "same install, same PeerId");
    }

    #[test]
    fn fresh_dirs_get_distinct_identities() {
        let d1 = tempfile::tempdir().unwrap();
        let d2 = tempfile::tempdir().unwrap();
        let p1 = GuardianIdentity::open(d1.path()).unwrap().peer_id().unwrap();
        let p2 = GuardianIdentity::open(d2.path()).unwrap().peer_id().unwrap();
        assert_ne!(p1, p2);
    }

    #[test]
    fn corrupt_identity_file_is_an_error_not_a_silent_regen() {
        // A truncated root must NOT silently mint a new identity — that
        // would orphan every duty recorded under the old PeerId.
        let dir = tempfile::tempdir().unwrap();
        let _ = GuardianIdentity::open(dir.path()).unwrap();
        std::fs::write(dir.path().join(IDENTITY_FILE), b"short").unwrap();
        assert!(GuardianIdentity::open(dir.path()).is_err());
    }
}
