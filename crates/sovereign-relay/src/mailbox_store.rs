//! Persistent store-and-forward mailbox (M1.5, implementing the spike's
//! FINDINGS.md design note).
//!
//! Design decisions carried from the M0 spike:
//! - **Persistence:** blobs live on disk (one JSON index + the queue), so a
//!   relay restart doesn't drop a guardian's in-flight heartbeat.
//! - **Retention/GC:** per-item TTL, per-recipient item+byte quota with
//!   oldest-first eviction, all swept on a **timeout** — never on a
//!   reservation-drop event (the spike proved drops aren't promptly
//!   observable).
//! - **Idempotent re-PUT:** an identical blob (same recipient + content
//!   hash) is de-duplicated, so a re-sent heartbeat self-heals rather than
//!   piling up.
//! - **Abuse:** the *recipient* quota bounds any one mailbox; a **per-sender
//!   deposit rate** (the "missing half" the spike flagged) bounds how fast
//!   one peer can fill others' mailboxes.
//!
//! The store is pure (no network) so all of this is unit-testable; the
//! relay server drives it from the request handler.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use sovereign_core::mailbox::{
    DEFAULT_MAX_BYTES_PER_RECIPIENT, DEFAULT_MAX_ITEMS_PER_RECIPIENT, DEFAULT_TTL_SECS,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    blob: Vec<u8>,
    /// Content hash (hex) for idempotent-PUT de-dup.
    hash: String,
    /// Unix seconds at deposit; TTL is measured from here.
    deposited_at: u64,
    /// Authenticated depositor peer-id. Bounds any one sender's share of a
    /// recipient queue (RELAY-002): a full queue can't be filled by one sender,
    /// and eviction is never used to make room (reject-over-evict). `#[serde(
    /// default)]` so pre-hardening `mailbox.json` still loads (legacy items get
    /// an empty sender and count toward no sub-quota).
    #[serde(default)]
    sender: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MailboxState {
    /// recipient peer-id (hex of raw bytes) → queue, oldest first.
    queues: HashMap<String, Vec<Item>>,
}

/// Configurable knobs (defaults from `sovereign_core::mailbox`).
#[derive(Debug, Clone)]
pub struct MailboxConfig {
    pub ttl_secs: u64,
    pub max_items: usize,
    pub max_bytes: u64,
    /// Max deposits one sender may make per `sender_window_secs`.
    pub sender_rate: u32,
    pub sender_window_secs: u64,
    /// RELAY-001: hard ceiling on total bytes across ALL recipient queues.
    /// Without it, unbounded distinct recipients OOM the node. A deposit that
    /// would cross this is rejected (never evicts another recipient's mail).
    pub max_total_bytes: u64,
    /// RELAY-001: hard ceiling on the number of distinct recipient queues, so a
    /// Sybil picking fresh `to` keys can't spawn unbounded queues.
    pub max_queues: usize,
    /// RELAY-002: max items one authenticated sender may hold in any single
    /// recipient's queue. Bounds one sender's share so it can't crowd out the
    /// recipient's genuine mail; combined with reject-over-evict, a single
    /// identity cannot censor. (A rotating-keypair Sybil still needs the
    /// deposit capability — the next increment — to be fully stopped.)
    pub max_items_per_sender: usize,
    /// RELAY-001: reject a `to` longer than a real peer-id multihash; garbage
    /// `to` keys are queue-spawning noise.
    pub max_recipient_key_len: usize,
    /// RELAY-002 inc-2: when true, a `Put` must carry a valid recipient-issued
    /// deposit token (verified in the relay's request handler, which has libp2p)
    /// or it is denied. This is what stops a rotating-keypair Sybil that the
    /// per-sender caps alone cannot. Default false until token issuance is wired
    /// into enrollment (inc-2c); flip on once senders actually carry tokens.
    pub require_deposit_token: bool,
}

impl Default for MailboxConfig {
    fn default() -> Self {
        Self {
            ttl_secs: DEFAULT_TTL_SECS,
            max_items: DEFAULT_MAX_ITEMS_PER_RECIPIENT,
            max_bytes: DEFAULT_MAX_BYTES_PER_RECIPIENT,
            sender_rate: 120,
            sender_window_secs: 60,
            // 256 MiB total store, 10k recipients: bounds the public seed node's
            // disk+RAM regardless of how many distinct queues are opened.
            max_total_bytes: 256 * 1024 * 1024,
            max_queues: 10_000,
            // A recipient has ≤5 guardians depositing; 32 per sender leaves ample
            // headroom under max_items while stopping a one-sender fill.
            max_items_per_sender: 32,
            // A peer-id is a bounded multihash (~38 bytes); 64 is generous.
            max_recipient_key_len: 64,
            // Off until inc-2c wires token issuance into enrollment; the deployed
            // seed node flips it on via `--require-deposit-token`.
            require_deposit_token: false,
        }
    }
}

/// Outcome of a deposit attempt.
pub enum PutOutcome {
    Stored,
    /// Identical blob already queued — idempotent no-op.
    Duplicate,
    Denied(&'static str),
}

pub struct MailboxStore {
    dir: PathBuf,
    cfg: MailboxConfig,
    state: MailboxState,
    /// sender peer-id → (window_start_unix, count). In-memory: a restart
    /// resets rate windows, harmless at these rates.
    sender_windows: HashMap<String, (u64, u32)>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl MailboxStore {
    pub fn open(dir: PathBuf, cfg: MailboxConfig) -> Self {
        let state = std::fs::read_to_string(dir.join("mailbox.json"))
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        Self { dir, cfg, state, sender_windows: HashMap::new() }
    }

    fn persist(&self) {
        if let Err(e) = std::fs::create_dir_all(&self.dir) {
            tracing::warn!(%e, "mailbox dir create failed");
            return;
        }
        match serde_json::to_string(&self.state) {
            Ok(j) => {
                let tmp = self.dir.join("mailbox.json.tmp");
                if std::fs::write(&tmp, j).is_ok() {
                    let _ = std::fs::rename(&tmp, self.dir.join("mailbox.json"));
                }
            }
            Err(e) => tracing::warn!(%e, "mailbox encode failed"),
        }
    }

    /// Deposit `blob` for `to` (raw recipient peer-id bytes), attributed to
    /// authenticated `sender` (peer-id string). `now` is unix seconds
    /// (injected for testability).
    pub fn put(&mut self, sender: &str, to: &[u8], blob: Vec<u8>, now: u64) -> PutOutcome {
        if blob.len() > sovereign_core::mailbox::MAX_BLOB_BYTES {
            return PutOutcome::Denied("blob too large");
        }
        // RELAY-001: a recipient key is a bounded peer-id multihash. Reject
        // empty/oversized `to` — garbage keys are just queue-spawning noise.
        if to.is_empty() || to.len() > self.cfg.max_recipient_key_len {
            return PutOutcome::Denied("invalid recipient");
        }
        // Per-sender deposit rate (the spike's "missing half").
        let (start, count) = self
            .sender_windows
            .entry(sender.to_string())
            .or_insert((now, 0));
        if now.saturating_sub(*start) >= self.cfg.sender_window_secs {
            *start = now;
            *count = 0;
        }
        if *count >= self.cfg.sender_rate {
            return PutOutcome::Denied("sender rate exceeded");
        }
        *count += 1;

        let key = hex(to);
        let hash = hex(&Sha256::digest(&blob));
        let blob_len = blob.len() as u64;

        // RELAY-001: global store ceiling across ALL queues. Reject the deposit
        // rather than evict another recipient's mail to make room.
        let store_bytes: u64 = self
            .state
            .queues
            .values()
            .flat_map(|q| q.iter())
            .map(|it| it.blob.len() as u64)
            .sum();
        if store_bytes + blob_len > self.cfg.max_total_bytes {
            return PutOutcome::Denied("store full");
        }

        // RELAY-001: cap distinct recipient queues (a fresh `to` opens a queue).
        if !self.state.queues.contains_key(&key) && self.state.queues.len() >= self.cfg.max_queues
        {
            return PutOutcome::Denied("too many recipients");
        }

        let queue = self.state.queues.entry(key).or_default();

        // Idempotent re-PUT: identical live blob already queued.
        if queue.iter().any(|it| it.hash == hash) {
            return PutOutcome::Duplicate;
        }

        // RELAY-002: reject-over-evict. A recipient queue at its item- or
        // byte-cap rejects the NEW deposit; it NEVER evicts existing mail, so an
        // attacker cannot push out a victim's genuine guardian/recovery messages.
        let queue_bytes: u64 = queue.iter().map(|it| it.blob.len() as u64).sum();
        if queue.len() >= self.cfg.max_items || queue_bytes + blob_len > self.cfg.max_bytes {
            return PutOutcome::Denied("recipient queue full");
        }

        // RELAY-002: bound any one authenticated sender's share of this queue, so
        // a single identity can't fill it and starve the recipient's real mail.
        let from_this_sender = queue.iter().filter(|it| it.sender == sender).count();
        if from_this_sender >= self.cfg.max_items_per_sender {
            return PutOutcome::Denied("sender queue quota");
        }

        queue.push(Item {
            blob,
            hash,
            deposited_at: now,
            sender: sender.to_string(),
        });

        self.persist();
        PutOutcome::Stored
    }

    /// Drain (and remove) every live item for `recipient` (raw peer-id
    /// bytes of the authenticated caller). Expired items are dropped, not
    /// returned.
    pub fn pull(&mut self, recipient: &[u8], now: u64) -> Vec<Vec<u8>> {
        let key = hex(recipient);
        let items = self.state.queues.remove(&key).unwrap_or_default();
        self.persist();
        items
            .into_iter()
            .filter(|it| now.saturating_sub(it.deposited_at) < self.cfg.ttl_secs)
            .map(|it| it.blob)
            .collect()
    }

    /// Timeout-driven sweep: drop expired items and empty queues. Returns
    /// the number of items removed. Called on a timer by the relay.
    pub fn sweep(&mut self, now: u64) -> usize {
        let mut removed = 0;
        for queue in self.state.queues.values_mut() {
            let before = queue.len();
            queue.retain(|it| now.saturating_sub(it.deposited_at) < self.cfg.ttl_secs);
            removed += before - queue.len();
        }
        self.state.queues.retain(|_, q| !q.is_empty());
        if removed > 0 {
            self.persist();
        }
        removed
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// RELAY-002 inc-2: whether the relay requires a valid deposit token on
    /// `Put`. The token itself is verified in the request handler (which has
    /// libp2p); this store stays pure.
    pub fn require_deposit_token(&self) -> bool {
        self.cfg.require_deposit_token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(cfg: MailboxConfig) -> (MailboxStore, tempfile::TempDir) {
        let d = tempfile::tempdir().unwrap();
        (MailboxStore::open(d.path().to_path_buf(), cfg), d)
    }

    #[test]
    fn put_then_pull_delivers_once() {
        let (mut m, _d) = store(MailboxConfig::default());
        assert!(matches!(m.put("sndr", b"rcpt", b"hello".to_vec(), 100), PutOutcome::Stored));
        assert_eq!(m.pull(b"rcpt", 200), vec![b"hello".to_vec()]);
        assert!(m.pull(b"rcpt", 300).is_empty(), "drain-once");
    }

    #[test]
    fn cross_recipient_isolation() {
        let (mut m, _d) = store(MailboxConfig::default());
        m.put("sndr", b"alice", b"for-alice".to_vec(), 1);
        assert!(m.pull(b"bob", 2).is_empty(), "bob cannot pull alice's mail");
        assert_eq!(m.pull(b"alice", 3), vec![b"for-alice".to_vec()]);
    }

    #[test]
    fn idempotent_reput_dedups() {
        let (mut m, _d) = store(MailboxConfig::default());
        assert!(matches!(m.put("s", b"r", b"beat".to_vec(), 1), PutOutcome::Stored));
        assert!(matches!(m.put("s", b"r", b"beat".to_vec(), 2), PutOutcome::Duplicate));
        assert_eq!(m.pull(b"r", 3).len(), 1, "re-PUT did not pile up");
    }

    #[test]
    fn ttl_expiry_on_pull_and_sweep() {
        let cfg = MailboxConfig { ttl_secs: 10, ..Default::default() };
        let (mut m, _d) = store(cfg);
        m.put("s", b"r", b"old".to_vec(), 100);
        // Pull well after TTL → nothing.
        assert!(m.pull(b"r", 200).is_empty());
        // Sweep also reclaims.
        m.put("s", b"r", b"old2".to_vec(), 100);
        assert_eq!(m.sweep(200), 1);
    }

    #[test]
    fn recipient_item_cap_rejects_over_evict() {
        // RELAY-002: a full queue rejects NEW deposits and keeps the existing
        // (victim) mail — the opposite of the old evict-oldest behaviour that
        // let an attacker push a victim's messages out.
        let cfg = MailboxConfig { max_items: 3, ..Default::default() };
        let (mut m, _d) = store(cfg);
        for i in 0..3u8 {
            assert!(matches!(m.put("s", b"r", vec![i], 1), PutOutcome::Stored));
        }
        // Queue is full: further deposits are rejected, not accepted-by-eviction.
        assert!(matches!(m.put("s", b"r", vec![3], 1), PutOutcome::Denied(_)));
        assert!(matches!(m.put("s", b"r", vec![4], 1), PutOutcome::Denied(_)));
        let got = m.pull(b"r", 2);
        assert_eq!(got, vec![vec![0u8], vec![1], vec![2]], "earliest kept, not evicted");
    }

    #[test]
    fn one_sender_cannot_evict_a_victims_mail() {
        // The RELAY-002 attack, directly: a victim has one genuine guardian
        // message; an attacker floods. Reject-over-evict + per-sender quota mean
        // the genuine message survives and is still delivered.
        let cfg = MailboxConfig { max_items: 4, max_items_per_sender: 2, ..Default::default() };
        let (mut m, _d) = store(cfg);
        assert!(matches!(m.put("guardian", b"victim", b"heartbeat".to_vec(), 1), PutOutcome::Stored));
        // Attacker can deposit up to its sub-quota, then is denied — it never
        // reaches or evicts the guardian's message.
        assert!(matches!(m.put("attacker", b"victim", vec![1], 1), PutOutcome::Stored));
        assert!(matches!(m.put("attacker", b"victim", vec![2], 1), PutOutcome::Stored));
        assert!(matches!(m.put("attacker", b"victim", vec![3], 1), PutOutcome::Denied(_)), "sub-quota");
        let got = m.pull(b"victim", 2);
        assert!(got.contains(&b"heartbeat".to_vec()), "genuine guardian message survived");
    }

    #[test]
    fn global_store_cap_rejects_when_full() {
        // RELAY-001: total bytes across ALL queues is bounded; a fresh recipient
        // cannot push the store over the ceiling.
        let cfg = MailboxConfig { max_total_bytes: 10, ..Default::default() };
        let (mut m, _d) = store(cfg);
        assert!(matches!(m.put("s", b"a", vec![0u8; 6], 1), PutOutcome::Stored));
        assert!(matches!(m.put("s", b"b", vec![0u8; 6], 1), PutOutcome::Denied(_)), "over global cap");
    }

    #[test]
    fn queue_count_cap_bounds_distinct_recipients() {
        // RELAY-001: a Sybil picking fresh `to` keys cannot spawn unbounded queues.
        let cfg = MailboxConfig { max_queues: 2, ..Default::default() };
        let (mut m, _d) = store(cfg);
        assert!(matches!(m.put("s", b"r1", vec![1], 1), PutOutcome::Stored));
        assert!(matches!(m.put("s", b"r2", vec![1], 1), PutOutcome::Stored));
        assert!(matches!(m.put("s", b"r3", vec![1], 1), PutOutcome::Denied(_)), "queue-count cap");
        // An existing recipient still accepts (not a new queue).
        assert!(matches!(m.put("s", b"r1", vec![2], 1), PutOutcome::Stored));
    }

    #[test]
    fn invalid_recipient_key_denied() {
        // RELAY-001: empty / oversized `to` is queue-spawning noise.
        let (mut m, _d) = store(MailboxConfig::default());
        assert!(matches!(m.put("s", b"", vec![1], 1), PutOutcome::Denied(_)), "empty to");
        let huge = vec![0u8; 65];
        assert!(matches!(m.put("s", &huge, vec![1], 1), PutOutcome::Denied(_)), "oversized to");
    }

    #[test]
    fn sender_rate_limits_deposits() {
        let cfg = MailboxConfig { sender_rate: 2, sender_window_secs: 60, ..Default::default() };
        let (mut m, _d) = store(cfg);
        assert!(matches!(m.put("flood", b"r", vec![1], 1), PutOutcome::Stored));
        assert!(matches!(m.put("flood", b"r", vec![2], 1), PutOutcome::Stored));
        assert!(matches!(m.put("flood", b"r", vec![3], 1), PutOutcome::Denied(_)));
        // Window rolls over → allowed again.
        assert!(matches!(m.put("flood", b"r", vec![4], 61), PutOutcome::Stored));
    }

    #[test]
    fn oversize_blob_denied() {
        let (mut m, _d) = store(MailboxConfig::default());
        let big = vec![0u8; sovereign_core::mailbox::MAX_BLOB_BYTES + 1];
        assert!(matches!(m.put("s", b"r", big, 1), PutOutcome::Denied(_)));
    }

    #[test]
    fn persistence_survives_reopen() {
        let d = tempfile::tempdir().unwrap();
        {
            let mut m = MailboxStore::open(d.path().to_path_buf(), MailboxConfig::default());
            m.put("s", b"r", b"durable".to_vec(), 1);
        }
        let mut m2 = MailboxStore::open(d.path().to_path_buf(), MailboxConfig::default());
        assert_eq!(m2.pull(b"r", 2), vec![b"durable".to_vec()], "survived restart");
    }
}
