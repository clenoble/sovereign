//! Trust calibration — tracks per-action approval patterns to allow
//! frequently-approved Level 3 actions to be auto-approved over time.
//!
//! Level 4-5 actions never auto-approve regardless of trust history.
//!
//! Trust is calibrated **per workflow** (UX Principle 5): counters are keyed
//! `workflow:action`, so approvals earned on the direct query path never
//! unlock auto-approval for the same action proposed by the chat agent loop,
//! and vice versa (audit GATING-003).

use std::path::Path;

use serde::{Deserialize, Serialize};
use sovereign_core::security::ActionLevel;

const TRUST_FILENAME: &str = "trust_state.json";

/// Earned auto-approval is not permanent (GATING-002). If the most recent
/// approval for an action is older than this, the action must be re-confirmed
/// — trust that isn't exercised decays, so a long-dormant grant can't be
/// silently reused (e.g. after content poisoning lands weeks later).
const AUTO_APPROVE_TTL_SECS: i64 = 30 * 24 * 60 * 60; // 30 days

/// Workflow scope for actions confirmed on the direct query/intent path.
pub const WORKFLOW_QUERY: &str = "query";
/// Workflow scope for actions proposed by the chat agent loop (tool calls).
pub const WORKFLOW_CHAT: &str = "chat";

fn scoped(workflow: &str, action: &str) -> String {
    format!("{workflow}:{action}")
}

/// Parse an RFC-3339 timestamp into a UTC datetime, or `None` if unparseable.
fn parse_rfc3339_utc(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Domain separator for the trust-state MAC (ai-safety M1 — same scheme as
/// the model-TOFU store; see `sovereign_crypto::mac`).
const TRUST_MAC_DOMAIN: &[u8] = b"sovereign-trust-state:v1";

/// Tracks approval history for action patterns.
///
/// ai-safety M1: `trust_state.json` was plaintext-unauthenticated — a
/// disk-write attacker could write `consecutive_approvals: 99` and earn
/// silent auto-approval of Level-3 writes. The file now carries a keyed MAC
/// (the key is installed post-login via [`TrustTracker::arm_key`]); an
/// unverified tracker NEVER auto-approves, and a file whose MAC fails is
/// discarded (trust is cheap to re-earn; integrity is not).
#[derive(Serialize, Deserialize)]
pub struct TrustTracker {
    /// base64 keyed MAC over the canonical JSON of `(entries, threshold)`.
    /// Empty on legacy files (which then never auto-approve until re-earned
    /// and re-saved under a key).
    #[serde(default)]
    mac: String,
    /// `BTreeMap` so the JSON the MAC covers is deterministic.
    entries: std::collections::BTreeMap<String, TrustEntry>,
    auto_approve_threshold: u32,
    /// Session MAC key, installed post-login. Never serialized.
    #[serde(skip)]
    mac_key: Option<[u8; 32]>,
    /// True only when the loaded state was authenticated (or is fresh under
    /// an armed key). Gates auto-approval. Never serialized.
    #[serde(skip)]
    verified: bool,
}

/// Per-action trust accumulator.
#[derive(Clone, Serialize, Deserialize)]
struct TrustEntry {
    consecutive_approvals: u32,
    /// ISO-8601 timestamp of the last rejection (replaces `Instant` for serializability).
    last_rejection: Option<String>,
    /// ISO-8601 timestamp of the most recent approval. `None` for entries
    /// persisted before trust decay existed — they fail safe (no auto-approval
    /// until re-earned under the new scheme). (GATING-002)
    #[serde(default)]
    last_approval: Option<String>,
}

impl TrustTracker {
    /// Create a new tracker with default threshold (5 consecutive approvals).
    pub fn new() -> Self {
        Self::with_threshold(5)
    }

    /// Create a tracker with a custom threshold.
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            mac: String::new(),
            entries: std::collections::BTreeMap::new(),
            auto_approve_threshold: threshold,
            mac_key: None,
            // A fresh tracker holds only live-session approvals, which are
            // authentic by construction (they came through the decision
            // channel). Only DISK-loaded state is untrusted until its MAC
            // verifies — see `load` / `arm_key` (M1).
            verified: true,
        }
    }

    /// Canonical bytes the MAC covers: the JSON of `(entries, threshold)`.
    fn mac_body(&self) -> Vec<u8> {
        serde_json::to_vec(&(&self.entries, self.auto_approve_threshold)).unwrap_or_default()
    }

    /// Install the session MAC key (post-login) and re-authenticate the
    /// on-disk state (ai-safety M1). A valid MAC adopts the file's counts;
    /// a missing or invalid MAC (legacy file, or tampering) DISCARDS them —
    /// auto-approval is re-earned rather than granted on unauthenticated
    /// counts. The state is re-saved MAC'd either way.
    pub fn arm_key(&mut self, dir: &Path, key: [u8; 32]) {
        self.mac_key = Some(key);
        let path = dir.join(TRUST_FILENAME);
        if path.exists() {
            let reloaded = std::fs::read_to_string(&path)
                .ok()
                .and_then(|d| serde_json::from_str::<Self>(&d).ok());
            match reloaded {
                Some(t)
                    if sovereign_crypto::mac::verify_keyed_mac(
                        &key,
                        TRUST_MAC_DOMAIN,
                        &serde_json::to_vec(&(&t.entries, t.auto_approve_threshold))
                            .unwrap_or_default(),
                        &t.mac,
                    ) =>
                {
                    self.entries = t.entries;
                    self.auto_approve_threshold = t.auto_approve_threshold;
                }
                _ => {
                    tracing::warn!(
                        "trust_state.json is unauthenticated (legacy) or its MAC failed — \
                         discarding persisted trust; auto-approval must be re-earned (M1)"
                    );
                    self.entries.clear();
                }
            }
        }
        self.verified = true;
        if let Err(e) = self.save(dir) {
            tracing::warn!("failed to re-save MAC'd trust state: {e}");
        }
    }

    /// Check whether an action can be auto-approved based on trust history
    /// accumulated in the given workflow.
    /// Only Level 3 (Modify) actions can be auto-approved.
    /// Level 4 (Transmit) and Level 5 (Destruct) never auto-approve.
    pub fn should_auto_approve(&self, workflow: &str, action: &str, level: ActionLevel) -> bool {
        // Only Level 3 can be auto-approved through trust
        if level != ActionLevel::Modify {
            return false;
        }

        // ai-safety M1: auto-approval requires AUTHENTICATED trust state. An
        // unverified tracker (pre-login, legacy file, failed MAC) records
        // approvals normally but never grants unattended writes.
        if !self.verified {
            return false;
        }

        if let Some(entry) = self.entries.get(&scoped(workflow, action)) {
            if entry.consecutive_approvals < self.auto_approve_threshold {
                return false;
            }
            // GATING-002: the grant must also be FRESH. An approval older than
            // the TTL — or an entry from before decay existed, which carries no
            // approval timestamp — no longer auto-approves; the user re-confirms.
            match entry.last_approval.as_deref().and_then(parse_rfc3339_utc) {
                Some(ts) => (chrono::Utc::now() - ts).num_seconds() <= AUTO_APPROVE_TTL_SECS,
                None => false,
            }
        } else {
            false
        }
    }

    /// Record a user approval for an action pattern within a workflow.
    pub fn record_approval(&mut self, workflow: &str, action: &str) {
        let entry = self
            .entries
            .entry(scoped(workflow, action))
            .or_insert(TrustEntry {
                consecutive_approvals: 0,
                last_rejection: None,
                last_approval: None,
            });
        entry.consecutive_approvals += 1;
        // GATING-002: stamp the approval so auto-approval can decay if the
        // action then goes unused for longer than AUTO_APPROVE_TTL_SECS.
        entry.last_approval = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Record a user rejection. Resets the consecutive approval counter.
    pub fn record_rejection(&mut self, workflow: &str, action: &str) {
        let entry = self
            .entries
            .entry(scoped(workflow, action))
            .or_insert(TrustEntry {
                consecutive_approvals: 0,
                last_rejection: None,
                last_approval: None,
            });
        entry.consecutive_approvals = 0;
        entry.last_rejection = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Get the current approval count for an action (for debugging/display).
    pub fn approval_count(&self, workflow: &str, action: &str) -> u32 {
        self.entries
            .get(&scoped(workflow, action))
            .map(|e| e.consecutive_approvals)
            .unwrap_or(0)
    }

    /// Save trust state to `dir/trust_state.json`, MAC'd when a key is armed
    /// (ai-safety M1). A keyless save writes an empty MAC — such a file loads
    /// but never auto-approves.
    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(TRUST_FILENAME);
        let mac = match &self.mac_key {
            Some(key) => sovereign_crypto::mac::keyed_mac(key, TRUST_MAC_DOMAIN, &self.mac_body()),
            None => String::new(),
        };
        let out = Self {
            mac,
            entries: self.entries.clone(),
            auto_approve_threshold: self.auto_approve_threshold,
            mac_key: None,
            verified: false,
        };
        let json = serde_json::to_string_pretty(&out)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load trust state from `dir/trust_state.json`.
    /// Returns a fresh default if the file doesn't exist.
    ///
    /// M1: disk state loads UNVERIFIED (the `#[serde(skip)]` `verified`
    /// field defaults to false) — counts display and accumulate, but
    /// auto-approval stays off until [`Self::arm_key`] authenticates the
    /// file post-login.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(TRUST_FILENAME);
        if !path.exists() {
            return Ok(Self::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let tracker: Self = serde_json::from_str(&data)?;
        Ok(tracker)
    }

    /// Return all trust entries for dashboard display.
    pub fn all_entries(&self) -> Vec<TrustEntryView> {
        self.entries
            .iter()
            .map(|(action, entry)| TrustEntryView {
                action: action.clone(),
                approval_count: entry.consecutive_approvals,
                auto_approve: entry.consecutive_approvals >= self.auto_approve_threshold,
                last_rejected: entry.last_rejection.clone(),
            })
            .collect()
    }

    /// Reset trust for a specific action (removes its entry).
    pub fn reset_action(&mut self, action: &str) {
        self.entries.remove(action);
    }

    /// Reset all trust entries.
    pub fn reset_all(&mut self) {
        self.entries.clear();
    }
}

/// View of a single trust entry for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntryView {
    pub action: String,
    pub approval_count: u32,
    pub auto_approve: bool,
    pub last_rejected: Option<String>,
}

impl Default for TrustTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sovereign_trust_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_auto_approve_without_history() {
        let tracker = TrustTracker::new();
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
    }

    #[test]
    fn auto_approve_after_threshold() {
        let mut tracker = TrustTracker::with_threshold(3);
        for _ in 0..3 {
            tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        }
        assert!(tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
    }

    #[test]
    fn auto_approve_decays_after_ttl() {
        // GATING-002: a grant that meets the threshold but whose last approval
        // is older than the TTL must require re-confirmation.
        let mut tracker = TrustTracker::with_threshold(2);
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        assert!(tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));

        // Backdate the last approval beyond the TTL.
        let key = scoped(WORKFLOW_QUERY, "create_thread");
        let stale =
            (chrono::Utc::now() - chrono::Duration::seconds(AUTO_APPROVE_TTL_SECS + 1)).to_rfc3339();
        tracker.entries.get_mut(&key).unwrap().last_approval = Some(stale);
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
    }

    #[test]
    fn legacy_entry_without_last_approval_fails_safe() {
        // Entries persisted before trust decay (no `last_approval`) must not
        // auto-approve until re-earned under the new scheme.
        let dir = test_dir("legacy_no_last_approval");
        std::fs::write(
            dir.join(TRUST_FILENAME),
            r#"{"entries":{"query:create_thread":{"consecutive_approvals":99,"last_rejection":null}},"auto_approve_threshold":5}"#,
        )
        .unwrap();
        let loaded = TrustTracker::load(&dir).unwrap();
        assert!(!loaded.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejection_resets_counter() {
        let mut tracker = TrustTracker::with_threshold(3);
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_rejection(WORKFLOW_QUERY, "create_thread");
        // Counter reset — need 3 more approvals
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
        assert_eq!(tracker.approval_count(WORKFLOW_QUERY, "create_thread"), 0);
    }

    #[test]
    fn level4_never_auto_approves() {
        let mut tracker = TrustTracker::with_threshold(1);
        for _ in 0..10 {
            tracker.record_approval(WORKFLOW_QUERY, "export");
        }
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "export", ActionLevel::Transmit));
    }

    #[test]
    fn level5_never_auto_approves() {
        let mut tracker = TrustTracker::with_threshold(1);
        for _ in 0..10 {
            tracker.record_approval(WORKFLOW_QUERY, "delete_thread");
        }
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "delete_thread", ActionLevel::Destruct));
    }

    #[test]
    fn different_actions_track_independently() {
        let mut tracker = TrustTracker::with_threshold(2);
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "rename_thread");
        assert!(tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "rename_thread", ActionLevel::Modify));
    }

    #[test]
    fn workflows_track_independently() {
        // GATING-003: approvals on the query path must not unlock the same
        // action when proposed by the chat agent loop, and vice versa.
        let mut tracker = TrustTracker::with_threshold(2);
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        assert!(tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
        assert!(!tracker.should_auto_approve(WORKFLOW_CHAT, "create_thread", ActionLevel::Modify));
        assert_eq!(tracker.approval_count(WORKFLOW_CHAT, "create_thread"), 0);

        // A rejection in the chat loop must not reset query-path trust.
        tracker.record_rejection(WORKFLOW_CHAT, "create_thread");
        assert!(tracker.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
    }

    #[test]
    fn observe_level_not_auto_approved_via_trust() {
        let mut tracker = TrustTracker::with_threshold(1);
        tracker.record_approval(WORKFLOW_QUERY, "search");
        // Observe-level actions don't need trust — they're always auto-approved
        // via the gate, not via trust. Trust returns false for non-Modify.
        assert!(!tracker.should_auto_approve(WORKFLOW_QUERY, "search", ActionLevel::Observe));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = test_dir("roundtrip");
        let mut tracker = TrustTracker::with_threshold(3);
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.record_approval(WORKFLOW_QUERY, "create_thread");
        tracker.save(&dir).unwrap();

        let loaded = TrustTracker::load(&dir).unwrap();
        assert_eq!(loaded.approval_count(WORKFLOW_QUERY, "create_thread"), 2);
        assert_eq!(loaded.auto_approve_threshold, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn armed_tracker_retains_approval_counts_across_reload() {
        // M1: the full authenticated lifecycle — arm, earn, save (MAC'd),
        // reload, re-arm with the same key → counts survive and auto-approve.
        let dir = test_dir("retain");
        let key = [3u8; 32];
        let mut tracker = TrustTracker::with_threshold(3);
        tracker.arm_key(&dir, key);
        for _ in 0..3 {
            tracker.record_approval(WORKFLOW_QUERY, "rename_thread");
        }
        tracker.save(&dir).unwrap();

        let mut loaded = TrustTracker::load(&dir).unwrap();
        // Straight off disk: counts visible but NOT auto-approving (M1).
        assert_eq!(loaded.approval_count(WORKFLOW_QUERY, "rename_thread"), 3);
        assert!(!loaded.should_auto_approve(WORKFLOW_QUERY, "rename_thread", ActionLevel::Modify));
        // Authenticated: the MAC verifies, auto-approval resumes.
        loaded.arm_key(&dir, key);
        assert_eq!(loaded.approval_count(WORKFLOW_QUERY, "rename_thread"), 3);
        assert!(loaded.should_auto_approve(WORKFLOW_QUERY, "rename_thread", ActionLevel::Modify));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forged_trust_counts_are_discarded_on_arm() {
        // M1: the attack from the review — a disk-write attacker plants
        // consecutive_approvals: 99. No valid MAC → discarded at arm time,
        // and never auto-approves even before arming.
        let dir = test_dir("forged_trust");
        std::fs::create_dir_all(&dir).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        std::fs::write(
            dir.join(TRUST_FILENAME),
            format!(
                r#"{{"entries":{{"query:create_document":{{"consecutive_approvals":99,"last_rejection":null,"last_approval":"{now}"}}}},"auto_approve_threshold":5}}"#
            ),
        )
        .unwrap();

        let mut loaded = TrustTracker::load(&dir).unwrap();
        assert!(
            !loaded.should_auto_approve(WORKFLOW_QUERY, "create_document", ActionLevel::Modify),
            "unauthenticated disk counts must never auto-approve (M1)"
        );
        loaded.arm_key(&dir, [5u8; 32]);
        assert_eq!(
            loaded.approval_count(WORKFLOW_QUERY, "create_document"),
            0,
            "forged/legacy counts must be discarded when the key arms (M1)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_key_discards_trust_state() {
        // A MAC'd file authenticated under a different key (e.g. tampered
        // key material) must not be adopted.
        let dir = test_dir("wrong_key_trust");
        let mut tracker = TrustTracker::with_threshold(2);
        tracker.arm_key(&dir, [1u8; 32]);
        tracker.record_approval(WORKFLOW_QUERY, "move_document");
        tracker.record_approval(WORKFLOW_QUERY, "move_document");
        tracker.save(&dir).unwrap();

        let mut loaded = TrustTracker::load(&dir).unwrap();
        loaded.arm_key(&dir, [2u8; 32]);
        assert_eq!(loaded.approval_count(WORKFLOW_QUERY, "move_document"), 0);
        assert!(!loaded.should_auto_approve(WORKFLOW_QUERY, "move_document", ActionLevel::Modify));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = test_dir("missing_trust");
        let tracker = TrustTracker::load(&dir).unwrap();
        assert_eq!(tracker.approval_count(WORKFLOW_QUERY, "anything"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejection_timestamp_persists() {
        let dir = test_dir("rejection_ts");
        let mut tracker = TrustTracker::new();
        tracker.record_rejection(WORKFLOW_QUERY, "delete_thread");
        tracker.save(&dir).unwrap();

        let data = std::fs::read_to_string(dir.join(TRUST_FILENAME)).unwrap();
        assert!(data.contains("last_rejection"));
        assert!(data.contains("20")); // starts with year 20xx

        let loaded = TrustTracker::load(&dir).unwrap();
        assert_eq!(loaded.approval_count(WORKFLOW_QUERY, "delete_thread"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pre_scoping_entries_never_auto_approve() {
        // Entries persisted before per-workflow scoping (bare action keys)
        // must fail safe: no auto-approval until re-earned under a scope.
        let dir = test_dir("legacy_keys");
        std::fs::write(
            dir.join(TRUST_FILENAME),
            r#"{"entries":{"create_thread":{"consecutive_approvals":99,"last_rejection":null}},"auto_approve_threshold":5}"#,
        )
        .unwrap();
        let loaded = TrustTracker::load(&dir).unwrap();
        assert!(!loaded.should_auto_approve(WORKFLOW_QUERY, "create_thread", ActionLevel::Modify));
        assert!(!loaded.should_auto_approve(WORKFLOW_CHAT, "create_thread", ActionLevel::Modify));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
