use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// State machine for key recovery via Guardian protocol.
///
/// Flow: WaitingPeriod(72h) → AwaitingShards → Reconstructing → Complete | Aborted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub request_id: String,
    pub state: RecoveryState,
    /// ISO-8601 timestamp when recovery was initiated.
    pub initiated_at: String,
    /// ISO-8601 timestamp when the waiting period ends.
    pub waiting_period_ends: String,
    /// Minimum number of shards needed.
    pub threshold: u8,
    /// Per-guardian response tracking.
    pub guardian_responses: HashMap<String, GuardianResponse>,
    /// Collected shard data (guardian_id → base64 shard bytes).
    pub collected_shards: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryState {
    WaitingPeriod,
    AwaitingShards,
    Reconstructing,
    Complete,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianResponse {
    Notified,
    Approved,
    Rejected,
    TimedOut,
}

impl RecoveryRequest {
    /// Create a new recovery request with a 72-hour waiting period.
    pub fn new(
        request_id: String,
        threshold: u8,
        guardian_ids: &[String],
    ) -> Self {
        let now = chrono::Utc::now();
        let waiting_ends = now + chrono::Duration::hours(72);

        let guardian_responses = guardian_ids
            .iter()
            .map(|id| (id.clone(), GuardianResponse::Notified))
            .collect();

        Self {
            request_id,
            state: RecoveryState::WaitingPeriod,
            initiated_at: now.to_rfc3339(),
            waiting_period_ends: waiting_ends.to_rfc3339(),
            threshold,
            guardian_responses,
            collected_shards: HashMap::new(),
        }
    }

    /// Check if the waiting period has elapsed.
    pub fn waiting_period_elapsed(&self) -> bool {
        if let Ok(ends) = chrono::DateTime::parse_from_rfc3339(&self.waiting_period_ends) {
            chrono::Utc::now() >= ends
        } else {
            false
        }
    }

    /// Advance from WaitingPeriod to AwaitingShards (if waiting period is over).
    pub fn advance_past_waiting(&mut self) -> bool {
        if self.state == RecoveryState::WaitingPeriod && self.waiting_period_elapsed() {
            self.state = RecoveryState::AwaitingShards;
            true
        } else {
            false
        }
    }

    /// Force advance past waiting period (for testing / admin override).
    pub fn force_advance_past_waiting(&mut self) {
        if self.state == RecoveryState::WaitingPeriod {
            self.state = RecoveryState::AwaitingShards;
        }
    }

    /// Record a guardian's approval and their shard data.
    pub fn record_approval(&mut self, guardian_id: &str, shard_data: String) {
        if let Some(resp) = self.guardian_responses.get_mut(guardian_id) {
            *resp = GuardianResponse::Approved;
        }
        self.collected_shards
            .insert(guardian_id.to_string(), shard_data);
    }

    /// Record a guardian's rejection.
    pub fn record_rejection(&mut self, guardian_id: &str) {
        if let Some(resp) = self.guardian_responses.get_mut(guardian_id) {
            *resp = GuardianResponse::Rejected;
        }
    }

    /// Record a guardian timing out.
    pub fn record_timeout(&mut self, guardian_id: &str) {
        if let Some(resp) = self.guardian_responses.get_mut(guardian_id) {
            *resp = GuardianResponse::TimedOut;
        }
    }

    /// Check if enough shards have been collected for reconstruction.
    pub fn can_reconstruct(&self) -> bool {
        self.collected_shards.len() >= self.threshold as usize
    }

    /// Transition to Reconstructing state if enough shards collected.
    pub fn begin_reconstruction(&mut self) -> bool {
        if self.state == RecoveryState::AwaitingShards && self.can_reconstruct() {
            self.state = RecoveryState::Reconstructing;
            true
        } else {
            false
        }
    }

    /// Mark recovery as complete.
    pub fn complete(&mut self) {
        self.state = RecoveryState::Complete;
    }

    /// Abort the recovery request.
    pub fn abort(&mut self, _reason: &str) {
        self.state = RecoveryState::Aborted;
    }

    /// Number of approvals received so far.
    pub fn approval_count(&self) -> usize {
        self.guardian_responses
            .values()
            .filter(|r| **r == GuardianResponse::Approved)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guardian_ids() -> Vec<String> {
        (1..=5).map(|i| format!("guardian-{}", i)).collect()
    }

    #[test]
    fn new_recovery_starts_in_waiting() {
        let req = RecoveryRequest::new("req-1".into(), 3, &guardian_ids());
        assert_eq!(req.state, RecoveryState::WaitingPeriod);
        assert_eq!(req.guardian_responses.len(), 5);
        assert!(req
            .guardian_responses
            .values()
            .all(|r| *r == GuardianResponse::Notified));
    }

    #[test]
    fn waiting_period_not_elapsed_immediately() {
        let req = RecoveryRequest::new("req-1".into(), 3, &guardian_ids());
        assert!(!req.waiting_period_elapsed());
    }

    #[test]
    fn force_advance_and_collect_shards() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);

        req.force_advance_past_waiting();
        assert_eq!(req.state, RecoveryState::AwaitingShards);

        // Collect 3 approvals
        req.record_approval(&ids[0], "shard-data-0".into());
        req.record_approval(&ids[1], "shard-data-1".into());
        assert!(!req.can_reconstruct());

        req.record_approval(&ids[2], "shard-data-2".into());
        assert!(req.can_reconstruct());
        assert_eq!(req.approval_count(), 3);

        assert!(req.begin_reconstruction());
        assert_eq!(req.state, RecoveryState::Reconstructing);

        req.complete();
        assert_eq!(req.state, RecoveryState::Complete);
    }

    #[test]
    fn abort_flow() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();
        req.abort("user cancelled");
        assert_eq!(req.state, RecoveryState::Aborted);
    }

    #[test]
    fn rejection_and_timeout() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();

        req.record_rejection(&ids[0]);
        req.record_timeout(&ids[1]);
        req.record_approval(&ids[2], "data".into());
        req.record_approval(&ids[3], "data".into());

        assert_eq!(
            req.guardian_responses[&ids[0]],
            GuardianResponse::Rejected
        );
        assert_eq!(
            req.guardian_responses[&ids[1]],
            GuardianResponse::TimedOut
        );
        assert_eq!(req.approval_count(), 2);
        assert!(!req.can_reconstruct());
    }

    #[test]
    fn cannot_reconstruct_from_waiting() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        // Even with enough shards, can't begin from WaitingPeriod state
        req.collected_shards.insert(ids[0].clone(), "d".into());
        req.collected_shards.insert(ids[1].clone(), "d".into());
        req.collected_shards.insert(ids[2].clone(), "d".into());
        assert!(!req.begin_reconstruction());
    }

    #[test]
    fn serde_roundtrip() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();
        req.record_approval(&ids[0], "data".into());

        let json = serde_json::to_string(&req).unwrap();
        let back: RecoveryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.state, RecoveryState::AwaitingShards);
        assert_eq!(back.approval_count(), 1);
    }

    #[test]
    fn double_approval_same_guardian_does_not_double_count() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();

        req.record_approval(&ids[0], "shard-a".into());
        req.record_approval(&ids[0], "shard-b".into());

        // Only one shard stored (overwritten), only one Approved response
        assert_eq!(req.approval_count(), 1);
        assert_eq!(req.collected_shards.len(), 1);
        assert_eq!(req.collected_shards[&ids[0]], "shard-b");
    }

    #[test]
    fn threshold_boundary_exactly_at_threshold() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();

        req.record_approval(&ids[0], "s0".into());
        req.record_approval(&ids[1], "s1".into());
        assert!(!req.can_reconstruct());

        // Exactly at threshold
        req.record_approval(&ids[2], "s2".into());
        assert!(req.can_reconstruct());
        assert!(req.begin_reconstruction());
    }

    #[test]
    fn cannot_begin_reconstruction_when_already_reconstructing() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();
        req.record_approval(&ids[0], "s0".into());
        req.record_approval(&ids[1], "s1".into());
        req.record_approval(&ids[2], "s2".into());

        assert!(req.begin_reconstruction());
        assert_eq!(req.state, RecoveryState::Reconstructing);

        // Second begin_reconstruction must fail — wrong state
        assert!(!req.begin_reconstruction());
    }

    #[test]
    fn cannot_advance_past_waiting_when_not_in_waiting() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();
        assert_eq!(req.state, RecoveryState::AwaitingShards);

        // Already past waiting — advance should be no-op
        assert!(!req.advance_past_waiting());
        // force_advance also no-op
        req.force_advance_past_waiting();
        assert_eq!(req.state, RecoveryState::AwaitingShards);
    }

    #[test]
    fn complete_then_abort_is_still_aborted() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();
        req.record_approval(&ids[0], "s0".into());
        req.record_approval(&ids[1], "s1".into());
        req.record_approval(&ids[2], "s2".into());
        req.begin_reconstruction();
        req.complete();
        assert_eq!(req.state, RecoveryState::Complete);

        // Abort after completion — state overwritten (abort is unconditional)
        req.abort("admin override");
        assert_eq!(req.state, RecoveryState::Aborted);
    }

    #[test]
    fn unknown_guardian_approval_collected_but_not_counted() {
        let ids = guardian_ids();
        let mut req = RecoveryRequest::new("req-1".into(), 3, &ids);
        req.force_advance_past_waiting();

        // Unknown guardian — not in guardian_responses
        req.record_approval("unknown-guardian", "shard-x".into());

        // Shard is stored (collected_shards is a plain HashMap)
        assert_eq!(req.collected_shards.len(), 1);
        // But approval_count only counts known guardians with Approved status
        assert_eq!(req.approval_count(), 0);
    }
}
