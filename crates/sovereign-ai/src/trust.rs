//! Trust calibration — tracks per-action approval patterns to allow
//! frequently-approved Level 3 actions to be auto-approved over time.
//!
//! Level 4-5 actions never auto-approve regardless of trust history.

use std::collections::HashMap;
use std::time::Instant;

use sovereign_core::security::ActionLevel;

/// Tracks approval history for action patterns.
pub struct TrustTracker {
    entries: HashMap<String, TrustEntry>,
    auto_approve_threshold: u32,
}

/// Per-action trust accumulator.
struct TrustEntry {
    consecutive_approvals: u32,
    last_rejection: Option<Instant>,
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
        entry.last_rejection = Some(Instant::now());
    }

    /// Get the current approval count for an action (for debugging/display).
    pub fn approval_count(&self, action: &str) -> u32 {
        self.entries
            .get(action)
            .map(|e| e.consecutive_approvals)
            .unwrap_or(0)
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
}
