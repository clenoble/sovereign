//! Trust calibration — tracks per-action approval patterns to allow
//! frequently-approved Level 3 actions to be auto-approved over time.
//!
//! Level 4-5 actions never auto-approve regardless of trust history.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sovereign_core::security::ActionLevel;

const TRUST_FILENAME: &str = "trust_state.json";

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

    /// Check whether an action can be auto-approved based on trust history.
    /// Only Level 3 (Modify) actions can be auto-approved.
    /// Level 4 (Transmit) and Level 5 (Destruct) never auto-approve.
    pub fn should_auto_approve(&self, action: &str, level: ActionLevel) -> bool {
        // Only Level 3 can be auto-approved through trust
        if level != ActionLevel::Modify {
            return false;
        }

        if let Some(entry) = self.entries.get(action) {
            entry.consecutive_approvals >= self.auto_approve_threshold
        } else {
            false
        }
    }

    /// Record a user approval for an action pattern.
    pub fn record_approval(&mut self, action: &str) {
        let entry = self.entries.entry(action.to_string()).or_insert(TrustEntry {
            consecutive_approvals: 0,
            last_rejection: None,
        });
        entry.consecutive_approvals += 1;
    }

    /// Record a user rejection. Resets the consecutive approval counter.
    pub fn record_rejection(&mut self, action: &str) {
        let entry = self.entries.entry(action.to_string()).or_insert(TrustEntry {
            consecutive_approvals: 0,
            last_rejection: None,
        });
        entry.consecutive_approvals = 0;
        entry.last_rejection = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Get the current approval count for an action (for debugging/display).
    pub fn approval_count(&self, action: &str) -> u32 {
        self.entries
            .get(action)
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
        assert!(!tracker.should_auto_approve("create_thread", ActionLevel::Modify));
    }

    #[test]
    fn auto_approve_after_threshold() {
        let mut tracker = TrustTracker::with_threshold(3);
        for _ in 0..3 {
            tracker.record_approval("create_thread");
        }
        assert!(tracker.should_auto_approve("create_thread", ActionLevel::Modify));
    }

    #[test]
    fn rejection_resets_counter() {
        let mut tracker = TrustTracker::with_threshold(3);
        tracker.record_approval("create_thread");
        tracker.record_approval("create_thread");
        tracker.record_rejection("create_thread");
        // Counter reset — need 3 more approvals
        assert!(!tracker.should_auto_approve("create_thread", ActionLevel::Modify));
        assert_eq!(tracker.approval_count("create_thread"), 0);
    }

    #[test]
    fn level4_never_auto_approves() {
        let mut tracker = TrustTracker::with_threshold(1);
        for _ in 0..10 {
            tracker.record_approval("export");
        }
        assert!(!tracker.should_auto_approve("export", ActionLevel::Transmit));
    }

    #[test]
    fn level5_never_auto_approves() {
        let mut tracker = TrustTracker::with_threshold(1);
        for _ in 0..10 {
            tracker.record_approval("delete_thread");
        }
        assert!(!tracker.should_auto_approve("delete_thread", ActionLevel::Destruct));
    }

    #[test]
    fn different_actions_track_independently() {
        let mut tracker = TrustTracker::with_threshold(2);
        tracker.record_approval("create_thread");
        tracker.record_approval("create_thread");
        tracker.record_approval("rename_thread");
        assert!(tracker.should_auto_approve("create_thread", ActionLevel::Modify));
        assert!(!tracker.should_auto_approve("rename_thread", ActionLevel::Modify));
    }

    #[test]
    fn observe_level_not_auto_approved_via_trust() {
        let mut tracker = TrustTracker::with_threshold(1);
        tracker.record_approval("search");
        // Observe-level actions don't need trust — they're always auto-approved
        // via the gate, not via trust. Trust returns false for non-Modify.
        assert!(!tracker.should_auto_approve("search", ActionLevel::Observe));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = test_dir("roundtrip");
        let mut tracker = TrustTracker::with_threshold(3);
        tracker.record_approval("create_thread");
        tracker.record_approval("create_thread");
        tracker.save(&dir).unwrap();

        let loaded = TrustTracker::load(&dir).unwrap();
        assert_eq!(loaded.approval_count("create_thread"), 2);
        assert_eq!(loaded.auto_approve_threshold, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loaded_tracker_retains_approval_counts() {
        let dir = test_dir("retain");
        let mut tracker = TrustTracker::with_threshold(3);
        for _ in 0..3 {
            tracker.record_approval("rename_thread");
        }
        tracker.save(&dir).unwrap();

        let loaded = TrustTracker::load(&dir).unwrap();
        assert!(loaded.should_auto_approve("rename_thread", ActionLevel::Modify));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = test_dir("missing_trust");
        let tracker = TrustTracker::load(&dir).unwrap();
        assert_eq!(tracker.approval_count("anything"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejection_timestamp_persists() {
        let dir = test_dir("rejection_ts");
        let mut tracker = TrustTracker::new();
        tracker.record_rejection("delete_thread");
        tracker.save(&dir).unwrap();

        let data = std::fs::read_to_string(dir.join(TRUST_FILENAME)).unwrap();
        assert!(data.contains("last_rejection"));
        assert!(data.contains("20")); // starts with year 20xx

        let loaded = TrustTracker::load(&dir).unwrap();
        assert_eq!(loaded.approval_count("delete_thread"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
