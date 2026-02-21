//! User profile — tracks interaction patterns, skill preferences, and
//! suggestion feedback to make the orchestrator progressively smarter.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

const PROFILE_FILENAME: &str = "user_profile.json";

/// Top-level persistent user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub created: String,
    pub last_updated: String,
    pub interaction_patterns: InteractionPatterns,
    pub skill_preferences: HashMap<String, String>,
    pub suggestion_feedback: HashMap<String, SuggestionFeedback>,
}

/// Observed interaction patterns (computed from accumulated data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionPatterns {
    /// Overall acceptance rate across all suggestion types.
    pub suggestion_receptiveness: f32,
    /// "terse" | "detailed" | "conversational"
    pub command_verbosity: String,
}

/// Per-action suggestion acceptance/dismissal counters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionFeedback {
    pub shown: u32,
    pub accepted: u32,
    pub dismissed: u32,
}

impl SuggestionFeedback {
    pub fn new() -> Self {
        Self {
            shown: 0,
            accepted: 0,
            dismissed: 0,
        }
    }

    pub fn acceptance_rate(&self) -> f32 {
        let total = self.accepted + self.dismissed;
        if total == 0 {
            return 0.5; // neutral default
        }
        self.accepted as f32 / total as f32
    }

    pub fn record_shown(&mut self) {
        self.shown += 1;
    }

    pub fn record_accepted(&mut self) {
        self.accepted += 1;
    }

    pub fn record_dismissed(&mut self) {
        self.dismissed += 1;
    }
}

impl Default for SuggestionFeedback {
    fn default() -> Self {
        Self::new()
    }
}

/// Adaptive parameters computed from acceptance rate.
/// Controls how aggressively the orchestrator shows suggestions.
#[derive(Debug, Clone)]
pub struct AdaptiveParams {
    /// Minimum acceptance rate required before showing this suggestion type.
    pub suggestion_threshold: f32,
    /// Multiplier on suggestion frequency (higher = more suggestions).
    pub frequency_multiplier: f32,
}

impl AdaptiveParams {
    /// Compute adaptive parameters from a suggestion's acceptance rate.
    ///
    /// - High acceptance (>= 0.7): low threshold, high frequency
    /// - Medium acceptance (>= 0.4): medium threshold, normal frequency
    /// - Low acceptance (< 0.4): high threshold, low frequency
    pub fn from_acceptance_rate(rate: f32) -> Self {
        if rate >= 0.7 {
            Self {
                suggestion_threshold: 0.5,
                frequency_multiplier: 1.5,
            }
        } else if rate >= 0.4 {
            Self {
                suggestion_threshold: 0.7,
                frequency_multiplier: 1.0,
            }
        } else {
            Self {
                suggestion_threshold: 0.9,
                frequency_multiplier: 0.5,
            }
        }
    }
}

impl UserProfile {
    /// Create a fresh profile with a new UUID.
    pub fn default_new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            user_id: uuid::Uuid::new_v4().to_string(),
            display_name: None,
            created: now.clone(),
            last_updated: now,
            interaction_patterns: InteractionPatterns {
                suggestion_receptiveness: 0.5,
                command_verbosity: "detailed".into(),
            },
            skill_preferences: HashMap::new(),
            suggestion_feedback: HashMap::new(),
        }
    }

    /// Load a profile from `dir/user_profile.json`.
    /// Returns a fresh default if the file doesn't exist.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(PROFILE_FILENAME);
        if !path.exists() {
            return Ok(Self::default_new());
        }
        let data = std::fs::read_to_string(&path)?;
        let profile: Self = serde_json::from_str(&data)?;
        Ok(profile)
    }

    /// Save the profile to `dir/user_profile.json`.
    pub fn save(&mut self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        self.last_updated = chrono::Utc::now().to_rfc3339();

        // Recompute overall receptiveness
        let (total_accepted, total_responded) = self
            .suggestion_feedback
            .values()
            .fold((0u32, 0u32), |(a, t), fb| {
                (a + fb.accepted, t + fb.accepted + fb.dismissed)
            });
        if total_responded > 0 {
            self.interaction_patterns.suggestion_receptiveness =
                total_accepted as f32 / total_responded as f32;
        }

        let path = dir.join(PROFILE_FILENAME);
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sovereign_profile_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn default_new_creates_valid_profile() {
        let p = UserProfile::default_new();
        assert!(!p.user_id.is_empty());
        assert!(!p.created.is_empty());
        assert_eq!(p.interaction_patterns.command_verbosity, "detailed");
        assert!(p.suggestion_feedback.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = test_dir("roundtrip");
        let mut p = UserProfile::default_new();
        p.skill_preferences
            .insert("text_editing".into(), "markdown-editor".into());
        p.save(&dir).unwrap();

        let loaded = UserProfile::load(&dir).unwrap();
        assert_eq!(loaded.user_id, p.user_id);
        assert_eq!(
            loaded.skill_preferences.get("text_editing").unwrap(),
            "markdown-editor"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = test_dir("missing");
        let p = UserProfile::load(&dir).unwrap();
        assert!(!p.user_id.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn suggestion_feedback_rate_empty() {
        let fb = SuggestionFeedback::new();
        assert!((fb.acceptance_rate() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn suggestion_feedback_rate_math() {
        let mut fb = SuggestionFeedback::new();
        fb.accepted = 8;
        fb.dismissed = 2;
        assert!((fb.acceptance_rate() - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn adaptive_params_high_acceptance() {
        let params = AdaptiveParams::from_acceptance_rate(0.8);
        assert!((params.suggestion_threshold - 0.5).abs() < f32::EPSILON);
        assert!((params.frequency_multiplier - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn adaptive_params_medium_acceptance() {
        let params = AdaptiveParams::from_acceptance_rate(0.5);
        assert!((params.suggestion_threshold - 0.7).abs() < f32::EPSILON);
        assert!((params.frequency_multiplier - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn adaptive_params_low_acceptance() {
        let params = AdaptiveParams::from_acceptance_rate(0.2);
        assert!((params.suggestion_threshold - 0.9).abs() < f32::EPSILON);
        assert!((params.frequency_multiplier - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn save_recomputes_receptiveness() {
        let dir = test_dir("receptiveness");
        let mut p = UserProfile::default_new();
        let mut fb = SuggestionFeedback::new();
        fb.accepted = 7;
        fb.dismissed = 3;
        p.suggestion_feedback.insert("adopt".into(), fb);
        p.save(&dir).unwrap();

        assert!((p.interaction_patterns.suggestion_receptiveness - 0.7).abs() < f32::EPSILON);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
