//! MODELTRUST-002: load-time integrity verification of model files.
//!
//! A poisoned or swapped model is the assistant's brain replaced — a deniable
//! compromise that does harm slowly over many sessions. Before any GGUF /
//! whisper model is loaded we hash it and check it against two tiers:
//!
//!  1. An **embedded pinned manifest** (`config/models.lock`, baked into the
//!     binary via `include_str!`) — the trust anchor for known models, usable
//!     even pre-login. A listed model whose bytes don't match its pinned hash
//!     is **refused**. (The binary is the anchor: changing a pinned hash means
//!     replacing the binary, a far higher bar than overwriting a model file —
//!     and exactly what code-signing/INSTALLER-002 will protect.)
//!  2. **Trust-on-first-use (TOFU)** for unlisted (custom / hot-swapped)
//!     models: the hash is recorded on first load and the model is **refused**
//!     if its bytes later change. The store is MAC'd under the session key so a
//!     disk-write attacker can't forge it, and is only consulted once a session
//!     is unlocked (the key is installed post-login). An unlisted model loaded
//!     before unlock is loaded with a warning — there's no key to anchor TOFU
//!     yet — and gets recorded on the next post-unlock load.
//!
//! Verification cost is one streamed SHA-256 over the file (~1–3 s for a multi-
//! GB GGUF), paid once per load/swap, amortized against the model load itself.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain separator for the TOFU-store MAC (see `sovereign_crypto::mac`).
const TOFU_MAC_DOMAIN: &[u8] = b"sovereign-model-tofu:v1";

/// The pinned manifest, baked into the binary at build time.
const PINNED_MANIFEST: &str = include_str!("../../../config/models.lock");

#[derive(Debug, Clone, Deserialize)]
struct PinnedManifest {
    #[serde(default)]
    models: HashMap<String, PinnedEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct PinnedEntry {
    sha256: String,
    /// Optional pinned prompt format, so a listed model's format isn't inferred
    /// from an attacker-controllable filename (the MODELTRUST format-confusion
    /// residual). `None` = fall back to filename detection.
    #[serde(default)]
    format: Option<String>,
}

/// On-disk TOFU store: filename -> recorded sha256 (hex). `BTreeMap` so the
/// JSON the MAC covers is deterministic (HashMap ordering would break the MAC).
#[derive(Debug, Default, Serialize, Deserialize)]
struct TofuFile {
    /// base64 MAC over the canonical JSON of `models`.
    #[serde(default)]
    mac: String,
    #[serde(default)]
    models: BTreeMap<String, String>,
}

/// Outcome of a successful verification.
#[derive(Debug, PartialEq, Eq)]
pub enum Verified {
    /// Matched a pinned hash. Carries the optional pinned prompt format.
    Pinned { format: Option<String> },
    /// Matched (or first-recorded) in the TOFU store.
    Tofu { first_use: bool },
    /// Unlisted model loaded before the session was unlocked — not anchored yet.
    UnverifiedPreUnlock,
}

/// Verifier state: the pinned manifest plus (post-unlock) the TOFU key + path.
pub struct ModelVerifier {
    pinned: HashMap<String, PinnedEntry>,
    key: Option<[u8; 32]>,
    tofu_path: Option<PathBuf>,
}

impl ModelVerifier {
    fn from_embedded() -> Self {
        let pinned = serde_json::from_str::<PinnedManifest>(PINNED_MANIFEST)
            .map(|m| m.models)
            .unwrap_or_else(|e| {
                tracing::error!("model integrity manifest parse failed (treating as empty): {e}");
                HashMap::new()
            });
        Self { pinned, key: None, tofu_path: None }
    }

    fn file_name(path: &Path) -> String {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Verify a model file. Returns `Ok(Verified)` if it may load, or `Err`
    /// (the caller REFUSES the load) on a pinned/TOFU mismatch.
    pub fn verify(&self, path: &Path) -> anyhow::Result<Verified> {
        let name = Self::file_name(path);
        let hash = sha256_file(path)?;

        // Tier 1: pinned manifest — the anchor. A listed model that changed is
        // a tamper/replacement; refuse outright.
        if let Some(entry) = self.pinned.get(&name) {
            if entry.sha256.eq_ignore_ascii_case(&hash) {
                return Ok(Verified::Pinned { format: entry.format.clone() });
            }
            anyhow::bail!(
                "model integrity check FAILED for pinned model '{name}': expected {}, got {hash} \
                 — refusing to load a tampered/replaced model (MODELTRUST-002)",
                entry.sha256
            );
        }

        // Tier 2: TOFU for unlisted models — needs the post-login unlock key.
        match (self.key.as_ref(), self.tofu_path.as_ref()) {
            (Some(key), Some(store_path)) => {
                let mut store = load_tofu(store_path, key);
                match store.models.get(&name) {
                    Some(recorded) if recorded.eq_ignore_ascii_case(&hash) => {
                        Ok(Verified::Tofu { first_use: false })
                    }
                    Some(recorded) => anyhow::bail!(
                        "model integrity check FAILED for '{name}': recorded {recorded}, got {hash} \
                         — the model changed since first use; refusing (MODELTRUST-002)"
                    ),
                    None => {
                        store.models.insert(name.clone(), hash.clone());
                        save_tofu(store_path, key, &store);
                        tracing::info!("model integrity: recorded first-use hash for '{name}' ({hash})");
                        Ok(Verified::Tofu { first_use: true })
                    }
                }
            }
            _ => {
                tracing::warn!(
                    "model integrity: '{name}' is unlisted and the session is locked — loading \
                     WITHOUT a TOFU anchor (it will be recorded on the next load after unlock)"
                );
                Ok(Verified::UnverifiedPreUnlock)
            }
        }
    }
}

/// Streamed SHA-256 of a file, lowercase hex.
fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = std::fs::File::open(path)
        .map_err(|e| anyhow::anyhow!("open model '{}': {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16]; // 64 KiB
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn load_tofu(path: &Path, key: &[u8; 32]) -> TofuFile {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return TofuFile::default(),
    };
    let file: TofuFile = match serde_json::from_slice(&data) {
        Ok(f) => f,
        Err(_) => return TofuFile::default(),
    };
    let body = serde_json::to_vec(&file.models).unwrap_or_default();
    if !sovereign_crypto::mac::verify_keyed_mac(key, TOFU_MAC_DOMAIN, &body, &file.mac) {
        tracing::error!(
            "model TOFU store MAC invalid — discarding (possible tampering of {})",
            path.display()
        );
        return TofuFile::default();
    }
    file
}

fn save_tofu(path: &Path, key: &[u8; 32], store: &TofuFile) {
    let body = serde_json::to_vec(&store.models).unwrap_or_default();
    let mac = sovereign_crypto::mac::keyed_mac(key, TOFU_MAC_DOMAIN, &body);
    let out = TofuFile { mac, models: store.models.clone() };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_vec_pretty(&out) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                tracing::error!("model TOFU store save failed: {e}");
            }
        }
        Err(e) => tracing::error!("model TOFU store serialize failed: {e}"),
    }
}

// ---- Process-global verifier (so load paths don't have to thread it) ----

static VERIFIER: RwLock<Option<ModelVerifier>> = RwLock::new(None);

fn ensure_init() {
    {
        if VERIFIER.read().unwrap().is_some() {
            return;
        }
    }
    let mut g = VERIFIER.write().unwrap();
    if g.is_none() {
        *g = Some(ModelVerifier::from_embedded());
    }
}

/// Install the session unlock key + TOFU store path (call once post-login).
/// Enables TOFU verification of unlisted models for the rest of the session.
pub fn set_unlock_key(key: [u8; 32], tofu_path: PathBuf) {
    ensure_init();
    if let Some(v) = VERIFIER.write().unwrap().as_mut() {
        v.key = Some(key);
        v.tofu_path = Some(tofu_path);
    }
}

/// Verify a model file before loading it. `Err` means REFUSE the load.
/// Returns the optional pinned prompt format on success (for listed models).
pub fn verify_path(path: &str) -> anyhow::Result<Option<String>> {
    ensure_init();
    let g = VERIFIER.read().unwrap();
    let v = g.as_ref().expect("model verifier initialized");
    match v.verify(Path::new(path))? {
        Verified::Pinned { format } => Ok(format),
        Verified::Tofu { .. } | Verified::UnverifiedPreUnlock => Ok(None),
    }
}

/// The pinned prompt format for a model filename, if it is listed in the
/// embedded manifest with one. Lets the loader override filename-based format
/// detection for KNOWN models (MODELTRUST format-confusion) — a rename can't
/// change a pinned model's format, and the pin is trustworthy because the
/// pinned hash had to match for the load to succeed. `None` for unlisted models.
pub fn pinned_format(filename: &str) -> Option<String> {
    ensure_init();
    let g = VERIFIER.read().unwrap();
    g.as_ref()
        .and_then(|v| v.pinned.get(filename))
        .and_then(|e| e.format.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifier(pinned: &[(&str, &str)], key: Option<[u8; 32]>, tofu: Option<PathBuf>) -> ModelVerifier {
        let pinned = pinned
            .iter()
            .map(|(n, h)| (n.to_string(), PinnedEntry { sha256: h.to_string(), format: None }))
            .collect();
        ModelVerifier { pinned, key, tofu_path: tofu }
    }

    fn write_model(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn pinned_match_and_mismatch() {
        let dir = std::env::temp_dir().join("sov_modeltrust_pinned");
        let _ = std::fs::create_dir_all(&dir);
        let p = write_model(&dir, "router.gguf", b"good model bytes");
        let good = sha256_file(&p).unwrap();

        let v = verifier(&[("router.gguf", &good)], None, None);
        assert!(matches!(v.verify(&p).unwrap(), Verified::Pinned { .. }));

        // Tamper the bytes — pinned hash no longer matches → refused.
        std::fs::write(&p, b"POISONED model bytes").unwrap();
        assert!(v.verify(&p).is_err(), "pinned mismatch must be refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tofu_records_then_detects_change() {
        let dir = std::env::temp_dir().join("sov_modeltrust_tofu");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("model_tofu.json");
        let key = [9u8; 32];
        let v = verifier(&[], Some(key), Some(store.clone()));

        let p = write_model(&dir, "custom.gguf", b"custom v1");
        // First use → recorded.
        assert_eq!(v.verify(&p).unwrap(), Verified::Tofu { first_use: true });
        // Same bytes → verified.
        assert_eq!(v.verify(&p).unwrap(), Verified::Tofu { first_use: false });
        // Changed bytes → refused.
        std::fs::write(&p, b"custom v2 (swapped!)").unwrap();
        assert!(v.verify(&p).is_err(), "a TOFU model that changed must be refused");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tofu_store_mac_tamper_is_discarded() {
        let dir = std::env::temp_dir().join("sov_modeltrust_mactamper");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = dir.join("model_tofu.json");
        let key = [9u8; 32];
        let v = verifier(&[], Some(key), Some(store.clone()));
        let p = write_model(&dir, "custom.gguf", b"custom v1");
        assert_eq!(v.verify(&p).unwrap(), Verified::Tofu { first_use: true });

        // Forge the store: claim a different hash but leave the MAC stale.
        let forged = r#"{"mac":"AAAA","models":{"custom.gguf":"deadbeef"}}"#;
        std::fs::write(&store, forged).unwrap();
        // The bad MAC → store discarded → treated as first-use again (records
        // the REAL current hash), not the forged one.
        assert_eq!(v.verify(&p).unwrap(), Verified::Tofu { first_use: true });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unlisted_pre_unlock_loads_with_warning() {
        let dir = std::env::temp_dir().join("sov_modeltrust_prelogin");
        let _ = std::fs::create_dir_all(&dir);
        let p = write_model(&dir, "custom.gguf", b"x");
        let v = verifier(&[], None, None); // no key = locked
        assert_eq!(v.verify(&p).unwrap(), Verified::UnverifiedPreUnlock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_parses_optional_pinned_format() {
        // The `format` field (used to override filename detection) round-trips.
        let json = r#"{"models":{"m.gguf":{"sha256":"abc","format":"chatml-qwen3"}}}"#;
        let m: PinnedManifest = serde_json::from_str(json).unwrap();
        let e = m.models.get("m.gguf").unwrap();
        assert_eq!(e.sha256, "abc");
        assert_eq!(e.format.as_deref(), Some("chatml-qwen3"));
        // A sha256-only entry parses with format = None (falls back to filename).
        let json2 = r#"{"models":{"n.gguf":{"sha256":"def"}}}"#;
        let m2: PinnedManifest = serde_json::from_str(json2).unwrap();
        assert_eq!(m2.models.get("n.gguf").unwrap().format, None);
    }
}
