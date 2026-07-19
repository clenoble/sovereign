//! RELAY-002 inc-2 — verify a recipient-issued **deposit capability**.
//!
//! The per-sender caps (inc-1) bound how much any one identity can deposit, but
//! a rotating-keypair Sybil gets a fresh budget per identity and can still crowd
//! a victim's queue. The deposit token closes that: only the **recipient** — the
//! holder of the libp2p key their mailbox address `to` is derived from — can mint
//! a token, so an attacker cannot manufacture permission for a victim's mailbox
//! no matter how many identities it spins up.
//!
//! Verification is stand-alone (a public relay knows no social graph): the token
//! carries the recipient's public key, and the four checks below tie it to the
//! authenticated deposit attempt.

use libp2p::{identity::PublicKey, PeerId};
use sovereign_core::mailbox::DepositToken;

/// Verify `token` against an authenticated deposit of a blob addressed to `to`
/// by `sender`, at time `now` (unix seconds). `Ok(())` iff **all** hold:
///
/// 1. `now < token.expires_at` — not expired.
/// 2. `token.recipient_pub` decodes to a libp2p [`PublicKey`] whose [`PeerId`]
///    equals `to` — only the mailbox owner's key could yield this, so only they
///    could have signed a token targeting their own address.
/// 3. that key verifies `token.sig` over the token's canonical message — the
///    token is authentic and untampered (expiry and sender are both signed).
/// 4. `token.sender == sender` — a token issued for one depositor cannot be
///    replayed by another.
///
/// Returns a short, non-secret reason on failure (safe to log / return to the
/// caller). Order matters only for the message returned; every check must pass.
pub fn verify_deposit_token(
    token: &DepositToken,
    to: &[u8],
    sender: &PeerId,
    now: u64,
) -> Result<(), &'static str> {
    if now >= token.expires_at {
        return Err("deposit token expired");
    }
    let recipient_pub = PublicKey::try_decode_protobuf(&token.recipient_pub)
        .map_err(|_| "deposit token recipient key malformed")?;
    // Only the holder of the key whose PeerId == `to` (the mailbox owner) could
    // have produced a token that passes this check for this mailbox.
    if PeerId::from_public_key(&recipient_pub).to_bytes() != to {
        return Err("deposit token recipient mismatch");
    }
    // Authentic + untampered (covers expiry and the authorized sender).
    if !recipient_pub.verify(&token.signed_message(), &token.sig) {
        return Err("deposit token signature invalid");
    }
    // Bound to the authenticated depositor — no cross-sender replay.
    if token.sender != sender.to_bytes() {
        return Err("deposit token sender mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;

    /// Recipient issues a token authorizing `sender` until `expires_at`.
    fn issue(recipient: &Keypair, sender: &PeerId, expires_at: u64) -> DepositToken {
        let recipient_pub = recipient.public().encode_protobuf();
        let sender_b = sender.to_bytes();
        let msg = DepositToken::signing_bytes(&recipient_pub, &sender_b, expires_at);
        let sig = recipient.sign(&msg).expect("sign");
        DepositToken { recipient_pub, sender: sender_b, expires_at, sig }
    }

    fn to_addr(recipient: &Keypair) -> Vec<u8> {
        PeerId::from_public_key(&recipient.public()).to_bytes()
    }

    #[test]
    fn valid_token_accepted() {
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&recipient, &sender, 1000);
        assert!(verify_deposit_token(&tok, &to_addr(&recipient), &sender, 500).is_ok());
    }

    #[test]
    fn expired_rejected() {
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&recipient, &sender, 1000);
        assert_eq!(
            verify_deposit_token(&tok, &to_addr(&recipient), &sender, 1000).unwrap_err(),
            "deposit token expired",
            "now == expires_at is already expired"
        );
    }

    #[test]
    fn wrong_recipient_mailbox_rejected() {
        // Token issued by `recipient`, presented for a different mailbox `to`.
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&recipient, &sender, 1000);
        let other_to = PeerId::random().to_bytes();
        assert_eq!(
            verify_deposit_token(&tok, &other_to, &sender, 500).unwrap_err(),
            "deposit token recipient mismatch"
        );
    }

    #[test]
    fn wrong_sender_rejected() {
        // Token authorizes `sender`; a different depositor presents it.
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&recipient, &sender, 1000);
        let attacker = PeerId::random();
        assert_eq!(
            verify_deposit_token(&tok, &to_addr(&recipient), &attacker, 500).unwrap_err(),
            "deposit token sender mismatch"
        );
    }

    #[test]
    fn tampered_signed_field_breaks_signature() {
        let recipient = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let mut tok = issue(&recipient, &sender, 1000);
        // expires_at is covered by the signature; changing it invalidates the sig
        // (and would otherwise be caught as a mismatch vs the signed message).
        tok.expires_at = 9_999;
        assert_eq!(
            verify_deposit_token(&tok, &to_addr(&recipient), &sender, 500).unwrap_err(),
            "deposit token signature invalid"
        );
    }

    #[test]
    fn self_signed_forgery_cannot_target_a_victim() {
        // The core Sybil defense: an attacker crafts a token with their OWN key as
        // recipient_pub and signs it correctly — but then PeerId(their key) is
        // their own address, never the victim's `to`. So a forged token can only
        // authorize deposits to the forger's own mailbox, which is useless for
        // flooding someone else.
        let attacker = Keypair::generate_ed25519();
        let sender = PeerId::random();
        let tok = issue(&attacker, &sender, 1000); // valid sig, attacker is "recipient"
        let victim_to = PeerId::random().to_bytes();
        assert_eq!(
            verify_deposit_token(&tok, &victim_to, &sender, 500).unwrap_err(),
            "deposit token recipient mismatch"
        );
    }

    #[test]
    fn malformed_recipient_key_rejected_cleanly() {
        let sender = PeerId::random();
        let tok = DepositToken {
            recipient_pub: vec![0xff, 0x00, 0x13], // not a valid protobuf pubkey
            sender: sender.to_bytes(),
            expires_at: 1000,
            sig: vec![0u8; 64],
        };
        // `to` doesn't matter — decode fails first.
        assert_eq!(
            verify_deposit_token(&tok, &PeerId::random().to_bytes(), &sender, 500).unwrap_err(),
            "deposit token recipient key malformed"
        );
    }
}
