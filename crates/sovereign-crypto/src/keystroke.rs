use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::error::{CryptoError, CryptoResult};

// ── Typing data ──────────────────────────────────────────────────────

/// A single keystroke timing sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystrokeSample {
    /// Key identifier (e.g. "a", "shift", "1").
    pub key: String,
    /// Timestamp of key press in milliseconds since epoch.
    pub press_ms: u64,
    /// Timestamp of key release (0 if unknown).
    pub release_ms: u64,
}

/// One complete password-typing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingProfile {
    pub samples: Vec<KeystrokeSample>,
}

// ── Reference profile ────────────────────────────────────────────────

/// Statistical reference built from multiple enrollment typing sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeystrokeReference {
    /// Digraph timings: "a->b" → (mean_ms, stddev_ms).
    pub digraph_timings: HashMap<String, (f64, f64)>,
    /// Per-key hold times: "a" → (mean_ms, stddev_ms).
    pub hold_times: HashMap<String, (f64, f64)>,
    /// Number of enrollment samples used.
    pub enrollment_count: u32,
    /// Acceptance threshold (calibrated from enrollment variance).
    pub threshold: f64,
}

/// Encrypted keystroke profile for disk storage.
#[derive(Serialize, Deserialize)]
pub struct EncryptedKeystrokeProfile {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
}

impl KeystrokeReference {
    /// Build a reference from multiple enrollment typing samples.
    /// Requires at least 3 samples for meaningful statistics.
    pub fn from_enrollments(profiles: &[TypingProfile]) -> Option<Self> {
        if profiles.len() < 3 {
            return None;
        }

        // Collect all digraph intervals across all samples.
        let mut digraph_values: HashMap<String, Vec<f64>> = HashMap::new();
        let mut hold_values: HashMap<String, Vec<f64>> = HashMap::new();

        for profile in profiles {
            for pair in profile.samples.windows(2) {
                let key_from = &pair[0].key;
                let key_to = &pair[1].key;
                let interval = pair[1].press_ms as f64 - pair[0].press_ms as f64;
                if interval > 0.0 && interval < 5000.0 {
                    let digraph = format!("{}->{}", key_from, key_to);
                    digraph_values.entry(digraph).or_default().push(interval);
                }
            }
            for sample in &profile.samples {
                if sample.release_ms > sample.press_ms {
                    let hold = sample.release_ms as f64 - sample.press_ms as f64;
                    if hold > 0.0 && hold < 2000.0 {
                        hold_values
                            .entry(sample.key.clone())
                            .or_default()
                            .push(hold);
                    }
                }
            }
        }

        let digraph_timings = compute_stats(&digraph_values);
        let hold_times = compute_stats(&hold_values);

        // Calibrate threshold: compute distance of each enrollment sample against
        // the reference built from all samples, take max * 1.5.
        let mut ref_candidate = Self {
            digraph_timings,
            hold_times,
            enrollment_count: profiles.len() as u32,
            threshold: f64::MAX, // temporary
        };

        let max_dist = profiles
            .iter()
            .map(|p| ref_candidate.compare(p))
            .fold(0.0_f64, f64::max);

        // Set threshold with headroom for natural variance.
        ref_candidate.threshold = if max_dist < 0.1 { 1.0 } else { max_dist * 1.5 };

        Some(ref_candidate)
    }

    /// Compare a typing sample against this reference.
    /// Returns a distance score (0.0 = identical, higher = more different).
    pub fn compare(&self, sample: &TypingProfile) -> f64 {
        let sample_digraphs = extract_digraphs(sample);
        let sample_holds = extract_holds(sample);

        let mut total_distance = 0.0;
        let mut count = 0;

        // Compare digraph timings.
        for (digraph, interval) in &sample_digraphs {
            if let Some(&(mean, stddev)) = self.digraph_timings.get(digraph) {
                let std = if stddev < 10.0 { 10.0 } else { stddev };
                total_distance += (interval - mean).abs() / std;
                count += 1;
            }
        }

        // Compare hold times.
        for (key, hold) in &sample_holds {
            if let Some(&(mean, stddev)) = self.hold_times.get(key) {
                let std = if stddev < 5.0 { 5.0 } else { stddev };
                total_distance += (hold - mean).abs() / std;
                count += 1;
            }
        }

        if count == 0 {
            return f64::MAX;
        }

        total_distance / count as f64
    }

    /// Returns true if the sample is within the acceptance threshold.
    pub fn matches(&self, sample: &TypingProfile) -> bool {
        self.compare(sample) <= self.threshold
    }

    /// Encrypt this reference for disk storage.
    pub fn encrypt(&self, key: &[u8; KEY_SIZE]) -> CryptoResult<EncryptedKeystrokeProfile> {
        let json =
            serde_json::to_vec(self).map_err(|e| CryptoError::Serialization(e.to_string()))?;
        let (ciphertext, nonce) = aead::encrypt(&json, key)?;
        Ok(EncryptedKeystrokeProfile { ciphertext, nonce })
    }

    /// Decrypt a stored profile.
    pub fn decrypt(
        enc: &EncryptedKeystrokeProfile,
        key: &[u8; KEY_SIZE],
    ) -> CryptoResult<Self> {
        let json = aead::decrypt(&enc.ciphertext, &enc.nonce, key)?;
        serde_json::from_slice(&json).map_err(|e| CryptoError::Serialization(e.to_string()))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn compute_stats(values: &HashMap<String, Vec<f64>>) -> HashMap<String, (f64, f64)> {
    values
        .iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|(key, vals)| {
            let n = vals.len() as f64;
            let mean = vals.iter().sum::<f64>() / n;
            let variance = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
            let stddev = variance.sqrt();
            (key.clone(), (mean, stddev))
        })
        .collect()
}

fn extract_digraphs(profile: &TypingProfile) -> Vec<(String, f64)> {
    profile
        .samples
        .windows(2)
        .filter_map(|pair| {
            let interval = pair[1].press_ms as f64 - pair[0].press_ms as f64;
            if interval > 0.0 && interval < 5000.0 {
                Some((format!("{}->{}", pair[0].key, pair[1].key), interval))
            } else {
                None
            }
        })
        .collect()
}

fn extract_holds(profile: &TypingProfile) -> Vec<(String, f64)> {
    profile
        .samples
        .iter()
        .filter_map(|s| {
            if s.release_ms > s.press_ms {
                let hold = s.release_ms as f64 - s.press_ms as f64;
                if hold > 0.0 && hold < 2000.0 {
                    return Some((s.key.clone(), hold));
                }
            }
            None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(timings: &[(u64, u64)]) -> TypingProfile {
        let keys = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'];
        TypingProfile {
            samples: timings
                .iter()
                .enumerate()
                .map(|(i, &(press, release))| KeystrokeSample {
                    key: keys[i % keys.len()].to_string(),
                    press_ms: press,
                    release_ms: release,
                })
                .collect(),
        }
    }

    fn enrollment_profiles() -> Vec<TypingProfile> {
        // 5 similar typing sessions with slight timing variations.
        (0..5)
            .map(|offset| {
                let base = 1000u64;
                let shift = offset * 3; // small per-session jitter
                make_profile(&[
                    (base + shift, base + shift + 80),
                    (base + shift + 150, base + shift + 230),
                    (base + shift + 300, base + shift + 375),
                    (base + shift + 460, base + shift + 535),
                    (base + shift + 610, base + shift + 690),
                ])
            })
            .collect()
    }

    #[test]
    fn from_enrollments_requires_min_3() {
        let profiles = vec![make_profile(&[(100, 180), (250, 320)])];
        assert!(KeystrokeReference::from_enrollments(&profiles).is_none());
    }

    #[test]
    fn from_enrollments_builds_reference() {
        let profiles = enrollment_profiles();
        let reference = KeystrokeReference::from_enrollments(&profiles).unwrap();
        assert!(reference.enrollment_count == 5);
        assert!(!reference.digraph_timings.is_empty());
        assert!(reference.threshold > 0.0);
    }

    #[test]
    fn similar_profile_matches() {
        let profiles = enrollment_profiles();
        let reference = KeystrokeReference::from_enrollments(&profiles).unwrap();

        // A profile very similar to enrollment should match.
        let similar = make_profile(&[
            (1005, 1085),
            (1155, 1235),
            (1305, 1380),
            (1465, 1540),
            (1615, 1695),
        ]);
        assert!(reference.matches(&similar));
    }

    #[test]
    fn very_different_profile_does_not_match() {
        let profiles = enrollment_profiles();
        let reference = KeystrokeReference::from_enrollments(&profiles).unwrap();

        // A profile with wildly different timing should not match.
        let different = make_profile(&[
            (1000, 1200),
            (1800, 2000),
            (3000, 3200),
            (4500, 4700),
            (6000, 6200),
        ]);
        assert!(!reference.matches(&different));
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let profiles = enrollment_profiles();
        let reference = KeystrokeReference::from_enrollments(&profiles).unwrap();

        let key = [42u8; KEY_SIZE];
        let encrypted = reference.encrypt(&key).unwrap();
        let decrypted = KeystrokeReference::decrypt(&encrypted, &key).unwrap();

        assert_eq!(reference.enrollment_count, decrypted.enrollment_count);
        assert!((reference.threshold - decrypted.threshold).abs() < 0.001);
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let profiles = enrollment_profiles();
        let reference = KeystrokeReference::from_enrollments(&profiles).unwrap();

        let key = [42u8; KEY_SIZE];
        let wrong_key = [99u8; KEY_SIZE];
        let encrypted = reference.encrypt(&key).unwrap();
        assert!(KeystrokeReference::decrypt(&encrypted, &wrong_key).is_err());
    }
}
