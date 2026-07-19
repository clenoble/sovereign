//! Store-and-forward **mailbox** wire types (M1.5), shared by the relay
//! server (`sovereign-relay`) and the client (`sovereign-p2p`).
//!
//! The mailbox lets guardian heartbeats, recovery requests, and approvals
//! cross the 72h social-recovery window without both ends being online at
//! once — the M0 spike proved this is the realistic case. Blobs are opaque
//! to the mailbox: end-to-end sealing is the sender's job (sovereign-crypto),
//! so the relay learns only routing metadata (recipient peer-id, size,
//! timing), never plaintext. Plain serde here — no libp2p — so the
//! lightweight `sovereign-core` can host it and the relay stays lean.

use serde::{Deserialize, Serialize};

/// The mailbox request/response protocol id (libp2p `StreamProtocol`).
pub const MAILBOX_PROTOCOL: &str = "/sovereign/mailbox/1";

/// Default retention before an undelivered item is GC'd. The 72h recovery
/// window fits inside this with wide margin; senders re-PUT idempotently,
/// so a GC'd heartbeat self-heals on the next beat.
pub const DEFAULT_TTL_SECS: u64 = 14 * 24 * 60 * 60;

/// Per-recipient caps (oldest-first eviction on overflow).
pub const DEFAULT_MAX_ITEMS_PER_RECIPIENT: usize = 256;
pub const DEFAULT_MAX_BYTES_PER_RECIPIENT: u64 = 8 * 1024 * 1024;

/// Per-blob ceiling — a mailbox item is a control message, not a payload
/// (shard transfers ride the post-punch direct/relayed data path).
pub const MAX_BLOB_BYTES: usize = 128 * 1024;

/// A capability authorizing ONE specific sender to deposit into ONE specific
/// recipient's mailbox (RELAY-002 inc-2, the deposit capability).
///
/// Issued by the **recipient**, signed with the recipient's libp2p identity key
/// — the same key their PeerId (the mailbox address `to`) is derived from. That
/// makes the token self-verifying at a dumb public relay, with no knowledge of
/// who-knows-whom: `PeerId(recipient_pub) == to`, the signature checks under
/// `recipient_pub`, and the authenticated depositor equals `sender`. The
/// per-sender caps bound how much *one* identity can deposit; this bounds *which*
/// identities may deposit at all — so a rotating-keypair Sybil (fresh identity =
/// fresh cap budget) can no longer flood a victim's queue, because it cannot mint
/// a token only the recipient can sign. That closes the residual the caps leave
/// open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositToken {
    /// The recipient's libp2p public key, protobuf-encoded
    /// (`PublicKey::encode_protobuf`). `PeerId::from_public_key(this)` must equal
    /// the `to` this token authorizes.
    pub recipient_pub: Vec<u8>,
    /// The authorized depositor's PeerId, raw bytes (`PeerId::to_bytes`). Must
    /// match the relay's authenticated sender.
    pub sender: Vec<u8>,
    /// Unix seconds after which the relay rejects the token. Bounds a leaked
    /// token's window; the recipient re-issues cheaply (e.g. on key rotation).
    pub expires_at: u64,
    /// The recipient's signature over [`DepositToken::signed_message`].
    pub sig: Vec<u8>,
}

impl DepositToken {
    /// Domain-separated, length-prefixed canonical bytes the recipient signs and
    /// the relay verifies. Length prefixes make field boundaries unambiguous, so
    /// no two distinct `(recipient_pub, sender, expires_at)` triples can produce
    /// the same message (no splice/extension confusion).
    pub fn signing_bytes(recipient_pub: &[u8], sender: &[u8], expires_at: u64) -> Vec<u8> {
        const DOMAIN: &[u8] = b"sovereign/mailbox/deposit-token/v1";
        let mut m = Vec::with_capacity(DOMAIN.len() + recipient_pub.len() + sender.len() + 24);
        m.extend_from_slice(DOMAIN);
        m.extend_from_slice(&(recipient_pub.len() as u64).to_be_bytes());
        m.extend_from_slice(recipient_pub);
        m.extend_from_slice(&(sender.len() as u64).to_be_bytes());
        m.extend_from_slice(sender);
        m.extend_from_slice(&expires_at.to_be_bytes());
        m
    }

    /// The canonical bytes this token's `sig` covers.
    pub fn signed_message(&self) -> Vec<u8> {
        Self::signing_bytes(&self.recipient_pub, &self.sender, self.expires_at)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailboxRequest {
    /// Deposit a sealed, signed blob for `to` (raw recipient PeerId bytes).
    /// The relay authenticates the *sender* from the libp2p connection, so
    /// `Put` cannot be spoofed as another sender. `token` is the recipient-issued
    /// deposit capability (RELAY-002 inc-2); a relay in `require_deposit_token`
    /// mode rejects a `Put` whose token is missing or invalid. `#[serde(default)]`
    /// keeps a pre-inc-2 `Put` (no token field) decodable as `token: None`.
    Put {
        to: Vec<u8>,
        blob: Vec<u8>,
        #[serde(default)]
        token: Option<DepositToken>,
    },
    /// Drain every item addressed to the caller. The caller is identified
    /// by the authenticated connection — there is no way to pull another
    /// peer's mail.
    Pull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailboxResponse {
    /// Accepted; `dedup` is true when an identical blob was already queued
    /// (idempotent re-PUT — the heartbeat self-heal path).
    PutAck { dedup: bool },
    /// Drained items, newest last. Empty when the mailbox is empty.
    Items(Vec<Vec<u8>>),
    /// Refused, with a short reason (quota, oversize, rate).
    Denied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_bytes_domain_separated_and_deterministic() {
        let a = DepositToken::signing_bytes(b"recip", b"sender", 100);
        let b = DepositToken::signing_bytes(b"recip", b"sender", 100);
        assert_eq!(a, b, "deterministic");
        assert!(
            a.starts_with(b"sovereign/mailbox/deposit-token/v1"),
            "domain-separated"
        );
    }

    #[test]
    fn signing_bytes_field_boundaries_unambiguous() {
        // Length-prefixing prevents (recip="ab", sender="c") from producing the
        // same message as (recip="a", sender="bc") — no splice confusion.
        let x = DepositToken::signing_bytes(b"ab", b"c", 1);
        let y = DepositToken::signing_bytes(b"a", b"bc", 1);
        assert_ne!(x, y, "length-prefixing disambiguates the field split");
    }

    #[test]
    fn signing_bytes_binds_expiry() {
        let a = DepositToken::signing_bytes(b"r", b"s", 100);
        let b = DepositToken::signing_bytes(b"r", b"s", 101);
        assert_ne!(a, b, "expiry is covered by the signature");
    }

    #[test]
    fn signed_message_matches_constructor() {
        let t = DepositToken {
            recipient_pub: b"r".to_vec(),
            sender: b"s".to_vec(),
            expires_at: 7,
            sig: vec![],
        };
        assert_eq!(t.signed_message(), DepositToken::signing_bytes(b"r", b"s", 7));
    }

    #[test]
    fn put_without_token_field_decodes_as_none() {
        // Backward-compat: a pre-inc-2 Put (no token field) still decodes.
        let legacy = serde_json::json!({ "Put": { "to": [1, 2, 3], "blob": [9] } });
        let req: MailboxRequest = serde_json::from_value(legacy).unwrap();
        match req {
            MailboxRequest::Put { to, blob, token } => {
                assert_eq!(to, vec![1, 2, 3]);
                assert_eq!(blob, vec![9]);
                assert!(token.is_none(), "missing token field -> None");
            }
            _ => panic!("expected Put"),
        }
    }

    #[test]
    fn put_with_token_roundtrips() {
        let t = DepositToken {
            recipient_pub: vec![1],
            sender: vec![2],
            expires_at: 5,
            sig: vec![3],
        };
        let req = MailboxRequest::Put { to: vec![7], blob: vec![8], token: Some(t) };
        let j = serde_json::to_string(&req).unwrap();
        let back: MailboxRequest = serde_json::from_str(&j).unwrap();
        match back {
            MailboxRequest::Put { token: Some(tok), .. } => {
                assert_eq!(tok.expires_at, 5);
                assert_eq!(tok.sig, vec![3]);
            }
            _ => panic!("expected Put with token"),
        }
    }
}
