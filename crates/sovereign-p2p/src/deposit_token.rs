//! RELAY-002 inc-2c — **issue** a deposit capability (mailbox-owner side).
//!
//! Counterpart to the relay's `verify_deposit_token`. The mailbox owner signs,
//! with its libp2p identity key, a token authorizing one specific `sender` to
//! deposit into its mailbox. Because the token type and its canonical signing
//! bytes live in `sovereign-core`, issue (here) and verify (in the relay) agree
//! by construction — neither can drift the message format without the other.
//!
//! Where this gets called: when the mailbox store-and-forward path is activated
//! (currently dormant — nothing sends `P2pCommand::MailboxPut` yet), the
//! guardian-enrollment handshake issues the mutual pair — the owner authorizes
//! each guardian to deposit shares/heartbeats to the owner, and each guardian
//! authorizes the owner to deposit recovery requests to the guardian. Both
//! parties hold the libp2p keypair their PeerId (their mailbox address) derives
//! from, so both can sign at handshake time. Until that wiring lands with mailbox
//! activation, this primitive is the ready building block and the relay's
//! `--require-deposit-token` is the enforcement switch.

use libp2p::{identity::Keypair, PeerId};
use sovereign_core::mailbox::DepositToken;

use crate::error::{P2pError, P2pResult};

/// Sign a deposit token: `recipient` (this mailbox's identity keypair) authorizes
/// `sender` to deposit until `expires_at` (unix seconds).
///
/// The relay accepts the result because `PeerId(recipient.public())` equals the
/// mailbox address `to`, the signature checks under that same key, and the
/// embedded `sender` matches the authenticated depositor — see
/// `sovereign-relay`'s `verify_deposit_token`.
pub fn issue(recipient: &Keypair, sender: &PeerId, expires_at: u64) -> P2pResult<DepositToken> {
    let recipient_pub = recipient.public().encode_protobuf();
    let sender_bytes = sender.to_bytes();
    let msg = DepositToken::signing_bytes(&recipient_pub, &sender_bytes, expires_at);
    let sig = recipient
        .sign(&msg)
        .map_err(|e| P2pError::Identity(format!("deposit token sign: {e}")))?;
    Ok(DepositToken { recipient_pub, sender: sender_bytes, expires_at, sig })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::PublicKey;

    #[test]
    fn issued_token_is_well_formed_and_verifiable() {
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&recipient, &sender, 1_000).unwrap();

        // recipient_pub decodes to the recipient's key, and its PeerId is the
        // mailbox address `to` the relay will match against.
        let pk = PublicKey::try_decode_protobuf(&tok.recipient_pub).unwrap();
        assert_eq!(
            PeerId::from_public_key(&pk),
            recipient.public().to_peer_id(),
            "recipient_pub is the mailbox owner's key"
        );
        // The signature verifies under that key over the canonical message —
        // exactly what the relay checks.
        assert!(
            pk.verify(&tok.signed_message(), &tok.sig),
            "signature verifies under recipient_pub"
        );
        // Bound to the authorized depositor and the expiry.
        assert_eq!(tok.sender, sender.to_bytes(), "sender bound");
        assert_eq!(tok.expires_at, 1_000, "expiry carried");
    }

    #[test]
    fn distinct_recipients_produce_distinct_tokens() {
        let sender = PeerId::random();
        let a = issue(&Keypair::generate_ed25519(), &sender, 1).unwrap();
        let b = issue(&Keypair::generate_ed25519(), &sender, 1).unwrap();
        assert_ne!(a.recipient_pub, b.recipient_pub, "different owners → different keys");
        assert_ne!(a.sig, b.sig);
    }

    #[test]
    fn tampering_after_issue_breaks_verification() {
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let mut tok = issue(&recipient, &sender, 1_000).unwrap();
        let pk = PublicKey::try_decode_protobuf(&tok.recipient_pub).unwrap();
        // Move the expiry out — the signature (which covers it) no longer checks.
        tok.expires_at = 9_999;
        assert!(!pk.verify(&tok.signed_message(), &tok.sig));
    }
}
