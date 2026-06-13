//! Trust calibration — tracks per-action approval patterns to allow
//! frequently-approved Level 3 actions to be auto-approved over time.
//!
//! Level 4-5 actions never auto-approve regardless of trust history.
//!
//! Trust is calibrated **per workflow** (UX Principle 5): counters are keyed
//! `workflow:action`, so approvals earned on the direct query path never
//! unlock auto-approval for the same action proposed by the chat agent loop,
//! and vice versa (audit GATING-003).

use std::collections::HashMap;
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

/// Tracks approval history for action patterns.
#[derive(Serialize, Deserialize)]
pub struct TrustTracker {
    entries: HashMap<String, TrustEntry>,
    auto_approve_threshold: u32,
}

/// Per-action trust accumulator.
#[derive(Serialize, Deserialize)]
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
        Self {
            entries: HashMap::new(),
            auto_approve_threshold: 5,
        }
    }

    /// Create a tracker with a custom threshold.
    pub fn with_threshold(threshold: u32) -> Self {
        Self {
            entries: HashMap::new(),
            auto_approve_threshold: threshold,
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

    /// Save trust state to `dir/trust_state.json`.
    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(TRUST_FILENAME);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load trust state from `dir/trust_state.json`.
    /// Returns a fresh default if the file doesn't exist.
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
    fn loaded_tracker_retains_approval_counts() {
        let dir = test_dir("retain");
        let mut tracker = TrustTracker::with_threshold(3);
        for _ in 0..3 {
            tracker.record_approval(WORKFLOW_QUERY, "rename_thread");
        }
        tracker.save(&dir).unwrap();

        let loaded = TrustTracker::load(&dir).unwrap();
        assert!(loaded.should_auto_approve(WORKFLOW_QUERY, "rename_thread", ActionLevel::Modify));
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
