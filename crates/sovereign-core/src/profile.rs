//! User profile — tracks interaction patterns, skill preferences, and
//! suggestion feedback to make the orchestrator progressively smarter.

use std::collections::HashMap;
use std::path::Path;

use rand::prelude::*;
use serde::{Deserialize, Serialize};

const PROFILE_FILENAME: &str = "user_profile.json";

// ── Bubble style ────────────────────────────────────────────────────────

/// Visual style for the AI orchestrator bubble avatar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleStyle {
    Icon,
    Wave,
    Spin,
    Pulse,
    Blink,
    Rings,
    Matrix,
    Orbit,
    Morph,
}

impl Default for BubbleStyle {
    fn default() -> Self {
        Self::Icon
    }
}

impl BubbleStyle {
    /// All available bubble styles, in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Icon,
            Self::Wave,
            Self::Spin,
            Self::Pulse,
            Self::Blink,
            Self::Rings,
            Self::Matrix,
            Self::Orbit,
            Self::Morph,
        ]
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Icon => "Icon",
            Self::Wave => "Wave",
            Self::Spin => "Spin",
            Self::Pulse => "Pulse",
            Self::Blink => "Blink",
            Self::Rings => "Rings",
            Self::Matrix => "Matrix",
            Self::Orbit => "Orbit",
            Self::Morph => "Morph",
        }
    }
}

// ── Designation generation ──────────────────────────────────────────────

/// Alphanumeric pool (no ambiguous chars: no I, L, O, 0, 1).
const LATIN_POOL: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Curated non-Latin characters spanning multiple scripts.
const NON_LATIN_POOL: &[char] = &[
    'Ω', 'Δ', 'Σ', 'Λ', 'Π', 'θ', 'φ', // Greek
    'Ж', 'Я', 'Щ', // Cyrillic
    'त', 'क', 'द', // Devanagari
    '山', '龍', // CJK
    'ש', // Hebrew
    'Þ', 'ð', // Icelandic
];

/// Generate a unique orchestrator designation: `Ikshal-XXXX-Y`.
///
/// - 4 Latin/numeric chars from [`LATIN_POOL`] (30^4 = 810,000 combos)
/// - 1 non-Latin char from [`NON_LATIN_POOL`] (20 chars)
/// - Total: ~16.2M combinations — no collision check needed for a personal OS.
pub fn generate_designation() -> String {
    let mut rng = rand::rng();
    let latin: String = (0..4)
        .map(|_| *LATIN_POOL.choose(&mut rng).unwrap() as char)
        .collect();
    let suffix = *NON_LATIN_POOL.choose(&mut rng).unwrap();
    format!("Ikshal-{latin}-{suffix}")
}

/// Top-level persistent user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    /// AI orchestrator serial ID, e.g. `Ikshal-B4T9-Ω`.
    #[serde(default)]
    pub designation: String,
    /// What the user calls the AI for short (e.g. "Ike", "T-Nine").
    #[serde(default)]
    pub nickname: Option<String>,
    /// Visual style for the AI bubble avatar.
    #[serde(default)]
    pub bubble_style: BubbleStyle,
    /// The user's own display name.
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
    /// Create a fresh profile with a new UUID and auto-generated designation.
    pub fn default_new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            user_id: uuid::Uuid::new_v4().to_string(),
            designation: generate_designation(),
            nickname: None,
            bubble_style: BubbleStyle::default(),
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
    /// Backfills designation for old profiles that lack one.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let path = dir.join(PROFILE_FILENAME);
        if !path.exists() {
            return Ok(Self::default_new());
        }
        let data = std::fs::read_to_string(&path)?;
        let mut profile: Self = serde_json::from_str(&data)?;
        // Backfill designation for profiles created before this feature
        if profile.designation.is_empty() {
            profile.designation = generate_designation();
            profile.save(dir)?;
        }
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
    fn designation_format_valid() {
        let d = generate_designation();
        // Format: Ikshal-XXXX-Y
        assert!(d.starts_with("Ikshal-"), "designation must start with Ikshal-: {d}");
        let parts: Vec<&str> = d.split('-').collect();
        assert_eq!(parts.len(), 3, "designation must have 3 parts: {d}");
        assert_eq!(parts[1].len(), 4, "latin part must be 4 chars: {d}");
        assert_eq!(
            parts[2].chars().count(),
            1,
            "suffix must be 1 char: {d}"
        );
        // Latin part should only contain LATIN_POOL chars
        for ch in parts[1].chars() {
            assert!(
                LATIN_POOL.contains(&(ch as u8)),
                "unexpected latin char '{ch}' in {d}"
            );
        }
        // Suffix should be in NON_LATIN_POOL
        let suffix = parts[2].chars().next().unwrap();
        assert!(
            NON_LATIN_POOL.contains(&suffix),
            "unexpected suffix char '{suffix}' in {d}"
        );
    }

    #[test]
    fn designation_uniqueness() {
        let designations: Vec<String> = (0..50).map(|_| generate_designation()).collect();
        // With 16.2M combinations, 50 draws should all be unique
        for (i, a) in designations.iter().enumerate() {
            for b in &designations[i + 1..] {
                assert_ne!(a, b, "collision: {a}");
            }
        }
    }

    #[test]
    fn default_new_has_designation_and_bubble() {
        let p = UserProfile::default_new();
        assert!(p.designation.starts_with("Ikshal-"));
        assert_eq!(p.bubble_style, BubbleStyle::Icon);
        assert!(p.nickname.is_none());
    }

    #[test]
    fn bubble_style_serde_roundtrip() {
        for &style in BubbleStyle::all() {
            let json = serde_json::to_string(&style).unwrap();
            let back: BubbleStyle = serde_json::from_str(&json).unwrap();
            assert_eq!(back, style);
        }
    }

    #[test]
    fn bubble_style_default_is_icon() {
        assert_eq!(BubbleStyle::default(), BubbleStyle::Icon);
    }

    #[test]
    fn bubble_style_all_has_nine_variants() {
        assert_eq!(BubbleStyle::all().len(), 9);
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

    #[test]
    fn load_backfills_empty_designation() {
        let dir = test_dir("backfill_desig");
        // Write a profile with empty designation (simulates old format)
        let json = r#"{
            "user_id": "test-123",
            "designation": "",
            "created": "2026-01-01T00:00:00Z",
            "last_updated": "2026-01-01T00:00:00Z",
            "interaction_patterns": {"suggestion_receptiveness": 0.5, "command_verbosity": "detailed"},
            "skill_preferences": {},
            "suggestion_feedback": {}
        }"#;
        std::fs::write(dir.join("user_profile.json"), json).unwrap();

        let p = UserProfile::load(&dir).unwrap();
        assert!(
            p.designation.starts_with("Ikshal-"),
            "empty designation should be backfilled: {}",
            p.designation,
        );
        // Should also have been saved back
        let reloaded = UserProfile::load(&dir).unwrap();
        assert_eq!(reloaded.designation, p.designation);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_preserves_missing_fields_with_defaults() {
        let dir = test_dir("missing_fields");
        // Write a minimal profile (no nickname, no bubble_style, no designation)
        let json = r#"{
            "user_id": "test-456",
            "created": "2026-01-01T00:00:00Z",
            "last_updated": "2026-01-01T00:00:00Z",
            "interaction_patterns": {"suggestion_receptiveness": 0.5, "command_verbosity": "terse"},
            "skill_preferences": {},
            "suggestion_feedback": {}
        }"#;
        std::fs::write(dir.join("user_profile.json"), json).unwrap();

        let p = UserProfile::load(&dir).unwrap();
        assert!(p.nickname.is_none());
        assert_eq!(p.bubble_style, BubbleStyle::Icon);
        assert!(p.display_name.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
