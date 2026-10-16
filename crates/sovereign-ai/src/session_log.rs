use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Max log size before rotation (10 MB).
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;
/// Number of rotated files to keep.
const MAX_ROTATED: usize = 3;

/// Append-only session log in JSONL format.
///
/// Records user inputs and orchestrator actions for auditing and context.
/// Supports optional per-entry encryption with tamper-proof hash chaining.
pub struct SessionLog {
    writer: BufWriter<std::fs::File>,
    path: PathBuf,
    #[cfg(feature = "encrypted-log")]
    encryption_key: Option<[u8; 32]>,
    #[cfg(feature = "encrypted-log")]
    prev_hash: String,
    /// SESSIONLOG-003: sidecar holding the MAC'd `(count, head)` high-water mark.
    #[cfg(feature = "encrypted-log")]
    anchor_path: PathBuf,
    /// Running line count of the current file, used to update the anchor.
    #[cfg(feature = "encrypted-log")]
    chain_count: u64,
}

impl SessionLog {
    /// Open or create the session log file (plaintext mode).
    /// Creates parent directories if needed.
    ///
    /// With the `encrypted-log` feature this open NEVER rotates (lot 0.6):
    /// the file may hold an encrypted, anchored chain, and only
    /// [`open_encrypted`](Self::open_encrypted) can verify it and reset the
    /// anchor with the rotation. Rotating here emptied the file under a
    /// still-valid anchor, so the next encrypted open read "0 lines < N
    /// anchored" as tampering and the log fell back to plaintext for good.
    /// Without the feature there is no anchor, and an oversize log rotates.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("session_log.jsonl");

        #[cfg(not(feature = "encrypted-log"))]
        if path.exists() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > MAX_LOG_SIZE {
                    Self::rotate(&path);
                }
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        tracing::info!("Session log: {}", path.display());
        Ok(Self {
            writer: BufWriter::new(file),
            #[cfg(feature = "encrypted-log")]
            anchor_path: dir.join("session_log.anchor"),
            path,
            #[cfg(feature = "encrypted-log")]
            encryption_key: None,
            #[cfg(feature = "encrypted-log")]
            prev_hash: crate::encrypted_log::GENESIS_HASH.to_string(),
            #[cfg(feature = "encrypted-log")]
            chain_count: 0,
        })
    }

    /// Open or create the session log with per-entry encryption.
    ///
    /// Each entry is encrypted with XChaCha20-Poly1305 and hash-chained to the
    /// previous entry for tamper detection. The hash of the last existing line
    /// is read from the file to maintain chain continuity.
    #[cfg(feature = "encrypted-log")]
    /// `beacon` is a path OUTSIDE the log dir (the crypto dir, alongside
    /// `auth.store`) that records "this account's encrypted session log has had
    /// content". It is created here once the log is non-empty and consulted to
    /// catch the genesis-wipe (sessionlog-genesis-wipe): deleting BOTH the log
    /// and its anchor reverts the dir to a pristine genesis state the in-dir
    /// checks can't distinguish from a fresh account — but the beacon, living
    /// elsewhere, survives that delete and proves the wipe. `None` disables the
    /// beacon (tests).
    pub fn open_encrypted(dir: &Path, key: [u8; 32], beacon: Option<&Path>) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("session_log.jsonl");
        let anchor_path = dir.join("session_log.anchor");

        // SESSIONLOG-001/003 + H-log1: verify the log BEFORE any rotation.
        // Rotation legitimately empties the file and re-mints a genesis
        // anchor — running it first let a disk-write attacker LAUNDER
        // tampering: truncate/rewrite the log, pad it past MAX_LOG_SIZE, and
        // the next open rotated the evidence away and re-anchored clean,
        // defeating the entire anti-rollback machinery. Only a VERIFIED log
        // may rotate. Checks: a valid anchor claiming MORE lines than the
        // file has (truncation/rollback), a forged anchor MAC, a deleted
        // anchor with encrypted entries remaining, or a genesis wipe caught
        // by the out-of-dir beacon.
        let mut tamper: Option<String> = None;
        match crate::encrypted_log::read_chain_anchor(&anchor_path, &key) {
            crate::encrypted_log::AnchorStatus::Valid { count, head } => {
                if let Err(e) = crate::encrypted_log::check_no_truncation(
                    &Self::read_all_lines(&path),
                    count,
                    &head,
                ) {
                    tamper = Some(format!("anchor/file mismatch: {e}"));
                }
            }
            crate::encrypted_log::AnchorStatus::Forged => {
                tamper = Some("anchor MAC invalid (forged/corrupt)".into());
            }
            crate::encrypted_log::AnchorStatus::Missing => {
                if Self::read_all_lines(&path)
                    .iter()
                    .any(|l| crate::encrypted_log::is_encrypted_line(l))
                {
                    tamper = Some("anchor missing but encrypted entries exist (deleted anchor)".into());
                } else if beacon.map(|b| b.exists()).unwrap_or(false) {
                    // sessionlog-genesis-wipe: the log AND its anchor are both
                    // gone (a pristine genesis state the in-dir checks would
                    // wave through as a fresh account), yet the out-of-dir
                    // beacon says this account's log previously had content.
                    // Both files were wiped together — fail closed.
                    tamper = Some("session log + anchor wiped (genesis beacon present)".into());
                }
            }
        }

        // SESSIONLOG-001: FAIL CLOSED. Opening the file for append would let the
        // first write re-mint a fully-valid anchor over the truncated/forged
        // prefix — permanently laundering the rollback so even a cold reload
        // can't detect it. Refuse instead: never lower the anchor's high-water,
        // never re-anchor a tampered file. The caller degrades to no encrypted
        // logging for the session rather than trusting laundered history.
        if let Some(reason) = tamper {
            tracing::error!("SESSIONLOG-001: session log tamper on open ({reason}); refusing to append");
            return Err(anyhow::anyhow!(
                "session log tamper detected ({reason}); refusing to append (SESSIONLOG-001)"
            ));
        }

        // Rotate if needed — the log verified clean above. Rotation
        // legitimately empties the current file, so the anchor is reset to
        // match; otherwise the next load would see "0 lines on disk < N
        // anchored" and fail closed on a benign rotation (the
        // SESSIONLOG-001-reopen hazard).
        if path.exists() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > MAX_LOG_SIZE {
                    Self::rotate(&path);
                    let _ = crate::encrypted_log::write_chain_anchor(
                        &anchor_path,
                        &key,
                        0,
                        crate::encrypted_log::GENESIS_HASH,
                    );
                }
            }
        }

        // Read the line count + hash of the last line for chain continuity and
        // anchor bookkeeping (post-rotation state).
        let (chain_count, prev_hash) = Self::read_tail(&path);

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // sessionlog-genesis-wipe: once the log has real content, drop the
        // out-of-dir beacon (created at open-time; the residual window is a wipe
        // between a log's first content and the next open). Best-effort — a
        // failure here only weakens future wipe detection, never blocks logging.
        if chain_count > 0 {
            if let Some(b) = beacon {
                if !b.exists() {
                    if let Some(parent) = b.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(b, b"1");
                }
            }
        }

        tracing::info!("Session log (encrypted): {}", path.display());
        Ok(Self {
            writer: BufWriter::new(file),
            path,
            encryption_key: Some(key),
            prev_hash,
            anchor_path,
            chain_count,
        })
    }

    /// All non-empty lines of the log file (empty if missing).
    #[cfg(feature = "encrypted-log")]
    fn read_all_lines(path: &Path) -> Vec<String> {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        BufReader::new(file)
            .lines()
            .map_while(|l| l.ok())
            .filter(|l| !l.trim().is_empty())
            .collect()
    }

    /// Line count + SHA-256 of the last line, for chain continuity + the anchor
    /// high-water mark. Returns `(0, GENESIS_HASH)` for a missing/empty file.
    #[cfg(feature = "encrypted-log")]
    fn read_tail(path: &Path) -> (u64, String) {
        use crate::encrypted_log;
        let lines = Self::read_all_lines(path);
        match lines.last() {
            Some(line) => (lines.len() as u64, encrypted_log::sha256_hex(line.as_bytes())),
            None => (0, encrypted_log::GENESIS_HASH.to_string()),
        }
    }

    /// Rotate log files: .jsonl -> .1.jsonl -> .2.jsonl -> .3.jsonl (oldest deleted).
    fn rotate(path: &Path) {
        let stem = path.with_extension("");
        // Delete oldest
        let oldest = format!("{}.{MAX_ROTATED}.jsonl", stem.display());
        let _ = fs::remove_file(&oldest);
        // Shift existing rotated files
        for i in (1..MAX_ROTATED).rev() {
            let from = format!("{}.{i}.jsonl", stem.display());
            let to = format!("{}.{}.jsonl", stem.display(), i + 1);
            let _ = fs::rename(&from, &to);
        }
        // Move current to .1
        let first = format!("{}.1.jsonl", stem.display());
        let _ = fs::rename(path, &first);
    }

    /// Log a user input event.
    pub fn log_user_input(&mut self, mode: &str, content: &str, intent: &str) {
        let entry = serde_json::json!({
            "ts": Utc::now().to_rfc3339(),
            "type": "user_input",
            "mode": mode,
            "content": content,
            "intent": intent,
        });
        self.write_line(&entry);
    }

    /// Log an orchestrator action.
    pub fn log_action(&mut self, action: &str, details: &str) {
        let entry = serde_json::json!({
            "ts": Utc::now().to_rfc3339(),
            "type": "orchestrator_action",
            "action": action,
            "details": details,
        });
        self.write_line(&entry);
    }

    /// Log an AI chat response (for conversation history persistence).
    pub fn log_chat_response(&mut self, response: &str) {
        let entry = serde_json::json!({
            "ts": Utc::now().to_rfc3339(),
            "type": "chat_response",
            "content": response,
        });
        self.write_line(&entry);
    }

    /// Get the path to the log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write_line(&mut self, entry: &serde_json::Value) {
        #[cfg(feature = "encrypted-log")]
        if let Some(ref key) = self.encryption_key {
            let json = entry.to_string();
            match crate::encrypted_log::encrypt_entry(&json, key, &self.prev_hash) {
                Ok((encrypted_line, new_hash)) => {
                    if let Err(e) = writeln!(self.writer, "{encrypted_line}") {
                        tracing::error!("Failed to write encrypted session log: {e}");
                        return;
                    }
                    let _ = self.writer.flush();
                    self.prev_hash = new_hash;
                    // SESSIONLOG-003: advance the MAC'd high-water anchor so a
                    // later tail-truncation/rollback is detectable. Written after
                    // the line is flushed, so a crash in between just leaves the
                    // anchor lagging by one (tolerated as a lower bound on read).
                    self.chain_count += 1;
                    if let Err(e) = crate::encrypted_log::write_chain_anchor(
                        &self.anchor_path,
                        key,
                        self.chain_count,
                        &self.prev_hash,
                    ) {
                        tracing::warn!("Failed to update session log integrity anchor: {e}");
                    }
                    return;
                }
                Err(e) => {
                    // ATREST-003 / SESSIONLOG-002: fail CLOSED. When encryption
                    // is active, NEVER fall back to a plaintext write — a
                    // plaintext line both leaks the entry to disk AND resets the
                    // hash chain (the keyless-forgery surface, SESSIONLOG-002).
                    // Drop the entry instead; losing one log line is preferable
                    // to silently persisting it unencrypted.
                    tracing::error!(
                        "Session log encryption failed; dropping entry (fail closed): {e}"
                    );
                    return;
                }
            }
        }

        // Reached only when encryption is NOT active (no key installed yet, or
        // the `encrypted-log` feature is off) — a legitimate plaintext write.
        if let Err(e) = writeln!(self.writer, "{}", entry) {
            tracing::error!("Failed to write session log: {e}");
        }
        // Flush after each entry for durability
        let _ = self.writer.flush();
    }

    /// Load the N most recent entries from the session log file (plaintext mode).
    /// Reads the full file and returns the last `max_entries` parsed entries.
    /// Returns an empty vec on any IO or parse error.
    pub fn load_recent(dir: &Path, max_entries: usize) -> Vec<SessionEntry> {
        let path = dir.join("session_log.jsonl");
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let reader = BufReader::new(file);
        let mut entries: Vec<SessionEntry> = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<SessionEntry>(&line) {
                entries.push(entry);
            }
        }

        // Keep only the most recent entries
        if entries.len() > max_entries {
            entries.drain(..entries.len() - max_entries);
        }
        entries
    }

    /// Load the N most recent entries, decrypting encrypted lines with the given key.
    ///
    /// Handles mixed files (plaintext seed data + encrypted entries). Plaintext lines
    /// are parsed directly; encrypted lines are decrypted then parsed. Chain integrity
    /// is verified and warnings logged for any breaks.
    #[cfg(feature = "encrypted-log")]
    pub fn load_recent_encrypted(
        dir: &Path,
        max_entries: usize,
        key: &[u8; 32],
    ) -> Vec<SessionEntry> {
        use crate::encrypted_log;

        let path = dir.join("session_log.jsonl");
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let reader = BufReader::new(file);
        let mut entries: Vec<SessionEntry> = Vec::new();
        let mut raw_lines: Vec<String> = Vec::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            raw_lines.push(line.clone());

            let json = if encrypted_log::is_encrypted_line(&line) {
                match encrypted_log::decrypt_entry(&line, key) {
                    Ok(plaintext) => plaintext,
                    Err(e) => {
                        tracing::warn!("Failed to decrypt session log entry: {e}");
                        continue;
                    }
                }
            } else {
                line
            };

            if let Ok(entry) = serde_json::from_str::<SessionEntry>(&json) {
                entries.push(entry);
            }
        }

        // SESSIONLOG-001: fail CLOSED on a broken/forged chain. These entries are
        // fed back to the model as authentic prior conversation turns, so a
        // tampered or keyless-forged log (see SESSIONLOG-002) is a
        // context-poisoning / prompt-injection vector. Never return untrusted
        // history — discard everything rather than hand the LLM planted turns.
        if let Err(e) = encrypted_log::verify_chain(&raw_lines) {
            tracing::error!(
                "Session log chain integrity check FAILED ({e}); discarding {} entries to \
                 avoid feeding tampered history to the model",
                entries.len()
            );
            return Vec::new();
        }

        // SESSIONLOG-003: verify_chain only proves the lines present link
        // together — it can't see a truncated tail or a rolled-back older copy.
        // Cross-check the on-disk chain against the MAC'd high-water anchor.
        let anchor_path = dir.join("session_log.anchor");
        match encrypted_log::read_chain_anchor(&anchor_path, key) {
            encrypted_log::AnchorStatus::Valid { count, head } => {
                if let Err(e) = encrypted_log::check_no_truncation(&raw_lines, count, &head) {
                    tracing::error!(
                        "Session log truncation/rollback detected ({e}); discarding {} entries \
                         (fail closed) — refusing to feed rolled-back history to the model",
                        entries.len()
                    );
                    return Vec::new();
                }
            }
            encrypted_log::AnchorStatus::Forged => {
                tracing::error!(
                    "Session log integrity anchor MAC invalid (forged/corrupt); discarding {} \
                     entries (fail closed)",
                    entries.len()
                );
                return Vec::new();
            }
            encrypted_log::AnchorStatus::Missing => {
                // SESSIONLOG-001: a missing anchor is only legitimate for a log
                // that has never been encrypted (pre-encryption seed data / first
                // run). Once ANY encrypted line exists, the anchor must exist too
                // — its absence means it was deleted, which is exactly the
                // rollback-gate downgrade (delete the anchor + truncate to a valid
                // prefix). Fail closed rather than feed unverifiable history to the
                // model. Only a log with no encrypted lines tolerates a missing
                // anchor (genuine pre-encryption state).
                if raw_lines.iter().any(|l| encrypted_log::is_encrypted_line(l)) {
                    tracing::error!(
                        "Session log integrity anchor MISSING while encrypted entries exist — \
                         treating as tampering (deleted anchor); discarding {} entries (fail closed)",
                        entries.len()
                    );
                    return Vec::new();
                }
                tracing::warn!(
                    "Session log integrity anchor absent and no encrypted entries yet — tolerated \
                     (legacy/first-run)"
                );
            }
        }

        // Keep only the most recent entries
        if entries.len() > max_entries {
            entries.drain(..entries.len() - max_entries);
        }
        entries
    }
}

/// The orchestrator's session log and its fail-closed state (lot 0.6).
///
/// Starts with a plaintext log (pre-login). Once a key has been offered with
/// [`install_key`](Self::install_key), the log is either encrypted (the open
/// verified the chain and its anchor) or OFF: on a failed encrypted open —
/// tamper, I/O error, a key that is not this log's — no line is ever written
/// again, in clear or otherwise, and no history is read back, until a later
/// `install_key` succeeds. The caller tells the user (decision K4: stop
/// logging and say so).
pub struct SessionLogSlot {
    dir: PathBuf,
    log: std::sync::Mutex<Option<SessionLog>>,
    #[cfg(feature = "encrypted-log")]
    key: std::sync::Mutex<Option<[u8; 32]>>,
    /// Set by a failed encrypted open; cleared by a successful one.
    disabled: std::sync::atomic::AtomicBool,
}

impl SessionLogSlot {
    /// Open the plaintext log in `dir`. An I/O error leaves the slot without
    /// a log (nothing is written), as before.
    pub fn open(dir: &Path) -> Self {
        let log = match SessionLog::open(dir) {
            Ok(log) => Some(log),
            Err(e) => {
                tracing::warn!("Session log unavailable: {e}");
                None
            }
        };
        Self {
            dir: dir.to_path_buf(),
            log: std::sync::Mutex::new(log),
            #[cfg(feature = "encrypted-log")]
            key: std::sync::Mutex::new(None),
            disabled: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The directory holding the log.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// True after a failed encrypted open: logging is off for this session.
    pub fn is_disabled(&self) -> bool {
        self.disabled.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Run `f` on the current log, if logging is on.
    pub fn with_log(&self, f: impl FnOnce(&mut SessionLog)) {
        if self.is_disabled() {
            return;
        }
        if let Ok(mut guard) = self.log.lock() {
            if let Some(log) = guard.as_mut() {
                f(log);
            }
        }
    }

    /// Switch to the encrypted log under `key`. On failure the slot turns
    /// logging OFF (the plaintext log is closed and never reopened) and the
    /// reason is returned for the caller to surface.
    #[cfg(feature = "encrypted-log")]
    pub fn install_key(&self, key: [u8; 32], beacon: Option<&Path>) -> Result<()> {
        // Hold the log lock across the open so no plaintext line can slip in
        // between the verification and the switch.
        let mut guard = self
            .log
            .lock()
            .map_err(|_| anyhow::anyhow!("session log lock poisoned"))?;
        match SessionLog::open_encrypted(&self.dir, key, beacon) {
            Ok(log) => {
                *guard = Some(log);
                if let Ok(mut k) = self.key.lock() {
                    *k = Some(key);
                }
                self.disabled.store(false, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.disabled.store(true, std::sync::atomic::Ordering::SeqCst);
                *guard = None;
                if let Ok(mut k) = self.key.lock() {
                    *k = None;
                }
                Err(e)
            }
        }
    }

    /// The most recent entries to feed back as conversation history: none
    /// while logging is off (the file failed verification, so its content
    /// is not trusted); decrypted and verified once a key is installed.
    pub fn load_recent(&self, max_entries: usize) -> Vec<SessionEntry> {
        if self.is_disabled() {
            return Vec::new();
        }
        #[cfg(feature = "encrypted-log")]
        {
            let key = self.key.lock().ok().and_then(|g| *g);
            if let Some(key) = key {
                return SessionLog::load_recent_encrypted(&self.dir, max_entries, &key);
            }
        }
        SessionLog::load_recent(&self.dir, max_entries)
    }
}

/// A parsed session log entry for context injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    /// ISO-8601 timestamp.
    pub ts: String,
    /// Entry type: "user_input", "orchestrator_action", "chat_response".
    #[serde(rename = "type")]
    pub entry_type: String,
    /// Content of the user message or chat response.
    #[serde(default)]
    pub content: Option<String>,
    /// Action name (for orchestrator_action entries).
    #[serde(default)]
    pub action: Option<String>,
    /// Action details (for orchestrator_action entries).
    #[serde(default)]
    pub details: Option<String>,
    /// Input mode: "text", "chat", "voice" (for user_input entries).
    #[serde(default)]
    pub mode: Option<String>,
    /// Classified intent (for user_input entries).
    #[serde(default)]
    pub intent: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H-log1: forced rotation ran BEFORE the tamper check, so a disk-write
    /// attacker could truncate/rewrite the log, pad it past MAX_LOG_SIZE,
    /// and the next open rotated the evidence away and re-minted a clean
    /// genesis anchor — laundering the rollback. Verification now runs
    /// first: an oversize tampered log FAILS CLOSED instead of rotating.
    #[cfg(feature = "encrypted-log")]
    #[test]
    fn oversize_padding_does_not_launder_tampering() {
        let dir = std::env::temp_dir()
            .join(format!("session-log-launder-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let key = [7u8; 32];
        {
            let mut log = SessionLog::open_encrypted(&dir, key, None).unwrap();
            log.log_user_input("keyboard", "sensitive action", "search");
            log.log_action("search", "found 1 document");
        }
        // Attacker: drop the last (anchored) line…
        let path = dir.join("session_log.jsonl");
        let content = fs::read_to_string(&path).unwrap();
        let first_line = content.lines().next().unwrap().to_string();
        // …and pad past MAX_LOG_SIZE so the next open wants to rotate.
        let mut padded = first_line;
        padded.push('\n');
        let filler = "x".repeat(1024 * 1024);
        while (padded.len() as u64) <= MAX_LOG_SIZE {
            padded.push_str(&filler);
            padded.push('\n');
        }
        fs::write(&path, padded).unwrap();

        let res = SessionLog::open_encrypted(&dir, key, None);
        assert!(
            res.is_err(),
            "padding a tampered log past the rotation threshold must fail closed, not rotate the evidence away (H-log1)"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The benign-rotation path still works: a clean oversize log rotates
    /// and re-opens without tripping the tamper check.
    #[cfg(feature = "encrypted-log")]
    #[test]
    fn clean_oversize_log_rotates_normally() {
        let dir = std::env::temp_dir()
            .join(format!("session-log-cleanrotate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let key = [8u8; 32];
        {
            let mut log = SessionLog::open_encrypted(&dir, key, None).unwrap();
            // Enough entries to exceed MAX_LOG_SIZE legitimately.
            let big = "y".repeat(1024 * 1024);
            for _ in 0..11 {
                log.log_action("bulk", &big);
            }
        }
        let log = SessionLog::open_encrypted(&dir, key, None);
        assert!(log.is_ok(), "a clean oversize log must rotate, not fail: {:?}", log.err());
        let rotated = dir.join("session_log.1.jsonl");
        assert!(rotated.exists(), "rotation must have produced the .1 file");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_log_file_and_writes() {
        let dir = std::env::temp_dir().join(format!("session-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let mut log = SessionLog::open(&dir).unwrap();
        log.log_user_input("keyboard", "search for notes", "search");
        log.log_action("search", "found 3 documents");

        let content = fs::read_to_string(log.path()).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("user_input"));
        assert!(lines[0].contains("search for notes"));
        assert!(lines[1].contains("orchestrator_action"));
        assert!(lines[1].contains("found 3 documents"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_mode_preserves_existing() {
        let dir = std::env::temp_dir().join(format!("session-log-test2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        {
            let mut log = SessionLog::open(&dir).unwrap();
            log.log_user_input("voice", "hello", "greeting");
        }
        {
            let mut log = SessionLog::open(&dir).unwrap();
            log.log_action("greet", "responded");
        }

        let content = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn log_chat_response_writes_correct_json() {
        let dir = std::env::temp_dir().join(format!("session-log-chat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let mut log = SessionLog::open(&dir).unwrap();
        log.log_chat_response("You have 4 threads: Research, Development, Design, Admin.");

        let content = fs::read_to_string(log.path()).unwrap();
        assert!(content.contains("chat_response"));
        assert!(content.contains("You have 4 threads"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_reads_entries() {
        let dir = std::env::temp_dir().join(format!("session-log-load-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        {
            let mut log = SessionLog::open(&dir).unwrap();
            log.log_user_input("chat", "hello", "chat");
            log.log_chat_response("Hi! How can I help?");
            log.log_user_input("chat", "list threads", "chat");
            log.log_chat_response("You have 4 threads.");
        }

        let entries = SessionLog::load_recent(&dir, 100);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].entry_type, "user_input");
        assert_eq!(entries[0].content.as_deref(), Some("hello"));
        assert_eq!(entries[1].entry_type, "chat_response");
        assert_eq!(entries[1].content.as_deref(), Some("Hi! How can I help?"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_caps_entries() {
        let dir = std::env::temp_dir().join(format!("session-log-cap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        {
            let mut log = SessionLog::open(&dir).unwrap();
            for i in 0..10 {
                log.log_user_input("chat", &format!("msg {i}"), "chat");
            }
        }

        let entries = SessionLog::load_recent(&dir, 3);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].content.as_deref(), Some("msg 7"));
        assert_eq!(entries[2].content.as_deref(), Some("msg 9"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_empty_dir_returns_empty() {
        let dir = std::env::temp_dir().join(format!("session-log-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let entries = SessionLog::load_recent(&dir, 50);
        assert!(entries.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(feature = "encrypted-log")]
    mod encrypted {
        use super::*;

        const TEST_KEY: [u8; 32] = [42u8; 32];

        #[test]
        fn genesis_wipe_with_beacon_fails_closed() {
            let pid = std::process::id();
            let dir = std::env::temp_dir().join(format!("session-log-wipe-{pid}"));
            let beacon = std::env::temp_dir().join(format!("session-log-wipe-beacon-{pid}"));
            let _ = fs::remove_dir_all(&dir);
            let _ = fs::remove_file(&beacon);

            // Session 1: write real content (log + anchor get written).
            {
                let mut log =
                    SessionLog::open_encrypted(&dir, TEST_KEY, Some(&beacon)).unwrap();
                log.log_user_input("chat", "real activity", "chat");
            }
            // Session 2: reopen — content present → beacon is created out-of-dir.
            {
                let _log =
                    SessionLog::open_encrypted(&dir, TEST_KEY, Some(&beacon)).unwrap();
            }
            assert!(beacon.exists(), "beacon must appear once the log has content");

            // Genesis wipe: delete BOTH log + anchor; the beacon (different dir)
            // survives.
            let _ = fs::remove_file(dir.join("session_log.jsonl"));
            let _ = fs::remove_file(dir.join("session_log.anchor"));

            // Reopen → the surviving beacon exposes the wipe → fail closed.
            assert!(
                SessionLog::open_encrypted(&dir, TEST_KEY, Some(&beacon)).is_err(),
                "a genesis wipe with a surviving beacon must fail closed"
            );

            // A genuinely fresh account (genesis, no beacon) still opens fine.
            let fresh = std::env::temp_dir().join(format!("session-log-fresh-{pid}"));
            let fresh_beacon =
                std::env::temp_dir().join(format!("session-log-fresh-beacon-{pid}"));
            let _ = fs::remove_dir_all(&fresh);
            let _ = fs::remove_file(&fresh_beacon);
            assert!(
                SessionLog::open_encrypted(&fresh, TEST_KEY, Some(&fresh_beacon)).is_ok(),
                "a fresh account at genesis (no beacon) must open normally"
            );

            let _ = fs::remove_dir_all(&dir);
            let _ = fs::remove_file(&beacon);
            let _ = fs::remove_dir_all(&fresh);
            let _ = fs::remove_file(&fresh_beacon);
        }

        #[test]
        fn encrypted_write_and_read_roundtrip() {
            let dir =
                std::env::temp_dir().join(format!("session-log-enc-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "secret message", "chat");
                log.log_chat_response("secret reply");
            }

            // Raw file should NOT contain plaintext
            let raw = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
            assert!(!raw.contains("secret message"));
            assert!(!raw.contains("secret reply"));

            // But decrypted load should recover them
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].content.as_deref(), Some("secret message"));
            assert_eq!(entries[1].content.as_deref(), Some("secret reply"));

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn chain_integrity_verified() {
            let dir =
                std::env::temp_dir().join(format!("session-log-chain-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                for i in 0..5 {
                    log.log_user_input("chat", &format!("msg {i}"), "chat");
                }
            }

            let raw = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
            let lines: Vec<String> = raw.lines().map(String::from).collect();
            assert!(crate::encrypted_log::verify_chain(&lines).is_ok());

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn tampered_entry_detected() {
            let dir =
                std::env::temp_dir().join(format!("session-log-tamper-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                for i in 0..3 {
                    log.log_user_input("chat", &format!("msg {i}"), "chat");
                }
            }

            // Tamper with a line
            let raw = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
            let mut lines: Vec<String> = raw.lines().map(String::from).collect();
            lines[1] = lines[1].replace("\"v\":1", "\"v\":1,\"x\":1");

            assert!(crate::encrypted_log::verify_chain(&lines).is_err());

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn chain_continues_across_reopen() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-reopen-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "first session", "chat");
            }
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "second session", "chat");
            }

            let raw = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
            let lines: Vec<String> = raw.lines().map(String::from).collect();
            assert_eq!(lines.len(), 2);
            assert!(crate::encrypted_log::verify_chain(&lines).is_ok());

            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].content.as_deref(), Some("first session"));
            assert_eq!(entries[1].content.as_deref(), Some("second session"));

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn mixed_plaintext_then_encrypted() {
            let dir =
                std::env::temp_dir().join(format!("session-log-mixed-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            // Write plaintext first (simulates seed data)
            {
                let mut log = SessionLog::open(&dir).unwrap();
                log.log_user_input("chat", "plaintext msg", "chat");
            }

            // Then write encrypted
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "encrypted msg", "chat");
            }

            // load_recent_encrypted handles both
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].content.as_deref(), Some("plaintext msg"));
            assert_eq!(entries[1].content.as_deref(), Some("encrypted msg"));

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn wrong_key_fails_decrypt() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-wrongkey-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "secret", "chat");
            }

            let wrong_key = [99u8; 32];
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &wrong_key);
            // Encrypted entries can't be decrypted with wrong key, so they're skipped
            assert!(entries.is_empty());

            let _ = fs::remove_dir_all(&dir);
        }

        // --- SESSIONLOG-003 ---

        #[test]
        fn tail_truncation_detected_on_load() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-trunc-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                for i in 0..4 {
                    log.log_user_input("chat", &format!("msg {i}"), "chat");
                }
            }
            // Baseline: all four load.
            assert_eq!(SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY).len(), 4);

            // Attacker lops off the last line. The remaining 3 still form a valid
            // chain (verify_chain passes), but the anchor says 4 → must fail closed.
            let path = dir.join("session_log.jsonl");
            let raw = fs::read_to_string(&path).unwrap();
            let mut lines: Vec<&str> = raw.lines().collect();
            lines.pop();
            fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
            assert!(
                crate::encrypted_log::verify_chain(
                    &fs::read_to_string(&path).unwrap().lines().map(String::from).collect::<Vec<_>>()
                )
                .is_ok(),
                "truncated prefix still chains internally (that's the gap)"
            );
            assert!(
                SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY).is_empty(),
                "tail truncation must be detected via the anchor and the log discarded"
            );

            // SESSIONLOG-001 (write side): reopening for append must ALSO refuse,
            // otherwise the next append re-mints a valid anchor over the truncated
            // prefix and the rollback is laundered for good (a later cold load
            // would see a self-consistent, fully-anchored — but rolled-back — log).
            assert!(
                SessionLog::open_encrypted(&dir, TEST_KEY, None).is_err(),
                "reopen over a truncated log must fail closed (never re-anchor)"
            );

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn forged_anchor_fails_closed() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-forge-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "m", "chat");
            }
            // Corrupt the anchor's MAC (still valid JSON, bad tag).
            let ap = dir.join("session_log.anchor");
            let forged = fs::read_to_string(&ap).unwrap().replace("\"mac\":\"", "\"mac\":\"Z");
            fs::write(&ap, forged).unwrap();

            assert!(
                SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY).is_empty(),
                "a forged/corrupt anchor must fail closed"
            );
            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn missing_anchor_with_encrypted_lines_fails_closed() {
            // SESSIONLOG-001: deleting the anchor must NOT downgrade the rollback
            // gate. A log with encrypted entries but no anchor = deleted anchor =
            // tampering → discard (fail closed), not silently tolerate.
            let dir = std::env::temp_dir()
                .join(format!("session-log-delanchor-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "secret entry", "chat");
            }
            // Attacker deletes the anchor sidecar (trivially easier than forging).
            let _ = fs::remove_file(dir.join("session_log.anchor"));

            assert!(
                SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY).is_empty(),
                "a missing anchor over an encrypted log must fail closed"
            );

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn plaintext_only_missing_anchor_is_tolerated() {
            // A log that has NEVER been encrypted (pre-encryption seed / first run)
            // legitimately has no anchor — it still loads.
            let dir = std::env::temp_dir()
                .join(format!("session-log-ptonly-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open(&dir).unwrap(); // plaintext writer
                log.log_user_input("chat", "seed entry", "chat");
            }
            assert!(!dir.join("session_log.anchor").exists());

            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].content.as_deref(), Some("seed entry"));

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn append_after_reopen_advances_anchor_and_loads() {
            // Regression: the anchor must not false-alarm on a normal reopen +
            // append (the SESSIONLOG-001-reopen hazard).
            let dir = std::env::temp_dir()
                .join(format!("session-log-readvance-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "one", "chat");
            }
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "two", "chat");
                log.log_user_input("chat", "three", "chat");
            }
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 3);
            assert_eq!(entries[2].content.as_deref(), Some("three"));

            let _ = fs::remove_dir_all(&dir);
        }

        // ── lot 0.6: fail closed ──────────────────────────────────────────

        fn file_bytes(dir: &Path) -> Vec<u8> {
            fs::read(dir.join("session_log.jsonl")).unwrap_or_default()
        }

        /// Write an encrypted log past MAX_LOG_SIZE under `key`.
        fn oversize_encrypted_log(dir: &Path, key: [u8; 32]) {
            let mut log = SessionLog::open_encrypted(dir, key, None).unwrap();
            let big = "z".repeat(1024 * 1024);
            for _ in 0..11 {
                log.log_action("bulk", &big);
            }
        }

        /// Invariant: a plaintext open never rotates — the file and its
        /// anchor are left exactly as they were, whatever their size.
        #[test]
        fn plaintext_open_never_rotates() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-plain-norotate-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            oversize_encrypted_log(&dir, TEST_KEY);
            let before = file_bytes(&dir);
            let anchor_before = fs::read(dir.join("session_log.anchor")).unwrap();
            assert!(before.len() as u64 > MAX_LOG_SIZE);

            drop(SessionLog::open(&dir).unwrap());
            drop(SessionLogSlot::open(&dir));

            assert_eq!(file_bytes(&dir), before, "plaintext open must not touch the log");
            assert_eq!(fs::read(dir.join("session_log.anchor")).unwrap(), anchor_before);
            assert!(!dir.join("session_log.1.jsonl").exists(), "plaintext open must not rotate");
            let _ = fs::remove_dir_all(&dir);
        }

        /// The review's path (orchestrator.rs:88 then set_session_log_key):
        /// an oversize, clean encrypted log opened plaintext first must still
        /// switch to encrypted mode — the rotation happens in the verified
        /// open, with its anchor reset — and history stays readable.
        #[test]
        fn oversize_log_survives_plaintext_open_then_key() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-slot-oversize-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            oversize_encrypted_log(&dir, TEST_KEY);

            let slot = SessionLogSlot::open(&dir);
            slot.install_key(TEST_KEY, None)
                .expect("a clean oversize log must open encrypted after a plaintext open");
            assert!(!slot.is_disabled());
            slot.with_log(|l| l.log_user_input("chat", "after rotation", "chat"));
            let entries = slot.load_recent(10);
            assert_eq!(entries.last().and_then(|e| e.content.as_deref()), Some("after rotation"));
            assert!(fs::read_to_string(dir.join("session_log.jsonl"))
                .unwrap()
                .lines()
                .all(crate::encrypted_log::is_encrypted_line));
            let _ = fs::remove_dir_all(&dir);
        }

        /// Invariant: after a failed encrypted open, no line is ever written
        /// (in clear or otherwise) and no history is read back.
        #[test]
        fn failed_encrypted_open_never_writes_plaintext() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-slot-wrongkey-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "primary persona", "chat");
            }

            let slot = SessionLogSlot::open(&dir);
            let other_key = [9u8; 32]; // e.g. another persona's key
            assert!(slot.install_key(other_key, None).is_err());
            assert!(slot.is_disabled());

            let before = file_bytes(&dir);
            slot.with_log(|l| l.log_user_input("chat", "SECRET duress chat", "chat"));
            slot.with_log(|l| l.log_chat_response("SECRET reply"));
            slot.with_log(|l| l.log_action("x", "SECRET detail"));
            assert_eq!(file_bytes(&dir), before, "nothing may be written once logging is off");
            assert!(!String::from_utf8_lossy(&file_bytes(&dir)).contains("SECRET"));
            assert!(slot.load_recent(50).is_empty(), "no history from a log that failed verification");
            let _ = fs::remove_dir_all(&dir);
        }

        /// Same invariant for tampering (forged anchor).
        #[test]
        fn tampered_log_turns_logging_off() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-slot-tamper-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "one", "chat");
            }
            fs::write(dir.join("session_log.anchor"), b"forged").unwrap();

            let slot = SessionLogSlot::open(&dir);
            assert!(slot.install_key(TEST_KEY, None).is_err());
            let before = file_bytes(&dir);
            slot.with_log(|l| l.log_user_input("chat", "after tamper", "chat"));
            assert_eq!(file_bytes(&dir), before);
            assert!(slot.load_recent(50).is_empty());
            let _ = fs::remove_dir_all(&dir);
        }

        /// A later successful key install turns logging back on, encrypted.
        #[test]
        fn successful_install_after_failure_logs_encrypted() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-slot-recover-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY, None).unwrap();
                log.log_user_input("chat", "one", "chat");
            }
            let slot = SessionLogSlot::open(&dir);
            assert!(slot.install_key([9u8; 32], None).is_err());
            slot.install_key(TEST_KEY, None).unwrap();
            assert!(!slot.is_disabled());
            slot.with_log(|l| l.log_user_input("chat", "two", "chat"));
            let text = fs::read_to_string(dir.join("session_log.jsonl")).unwrap();
            assert!(text.lines().all(crate::encrypted_log::is_encrypted_line));
            assert_eq!(slot.load_recent(10).len(), 2);
            let _ = fs::remove_dir_all(&dir);
        }
    }
}
