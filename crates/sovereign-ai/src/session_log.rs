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
    /// Creates parent directories if needed. Rotates if the log exceeds 10 MB.
    pub fn open(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("session_log.jsonl");

        // Rotate if the current log exceeds the size limit
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
    pub fn open_encrypted(dir: &Path, key: [u8; 32]) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("session_log.jsonl");
        let anchor_path = dir.join("session_log.anchor");

        // Rotate if needed. Rotation legitimately empties the current file, so
        // the anchor must be reset to match — otherwise the next load would see
        // "0 lines on disk < N anchored" and fail closed on a benign rotation
        // (this is the SESSIONLOG-001-reopen hazard the audit flagged).
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
        // anchor bookkeeping.
        let (chain_count, prev_hash) = Self::read_tail(&path);

        // SESSIONLOG-003: if a valid anchor says the file should have MORE lines
        // than it does (or the anchored line is gone), it was truncated/rolled
        // back while we were closed. Surface it loudly; the read path
        // (load_recent_encrypted) is the hard gate that refuses to feed such a
        // log to the model — here we just record the tamper before appending.
        match crate::encrypted_log::read_chain_anchor(&anchor_path, &key) {
            crate::encrypted_log::AnchorStatus::Valid { count, head } => {
                if let Err(e) = crate::encrypted_log::check_no_truncation(
                    &Self::read_all_lines(&path),
                    count,
                    &head,
                ) {
                    tracing::error!("SESSIONLOG-003: session log integrity anchor mismatch on open ({e})");
                }
            }
            crate::encrypted_log::AnchorStatus::Forged => {
                tracing::error!("SESSIONLOG-003: session log anchor MAC invalid on open (forged/corrupt)");
            }
            crate::encrypted_log::AnchorStatus::Missing => {
                // SESSIONLOG-001: missing anchor + existing encrypted lines = a
                // likely deleted anchor. Surface it before the first append mints
                // a fresh anchor over the (possibly truncated) state. The read
                // path (load_recent_encrypted) is the hard gate that refuses such
                // a log; this is the loud warning at the write side.
                if Self::read_all_lines(&path)
                    .iter()
                    .any(|l| crate::encrypted_log::is_encrypted_line(l))
                {
                    tracing::error!(
                        "SESSIONLOG-001: session log anchor MISSING on open but encrypted entries \
                         exist — possible anchor deletion / rollback"
                    );
                }
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
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
        fn encrypted_write_and_read_roundtrip() {
            let dir =
                std::env::temp_dir().join(format!("session-log-enc-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
                log.log_user_input("chat", "first session", "chat");
            }
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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

            let _ = fs::remove_dir_all(&dir);
        }

        #[test]
        fn forged_anchor_fails_closed() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-forge-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
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
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
                log.log_user_input("chat", "one", "chat");
            }
            {
                let mut log = SessionLog::open_encrypted(&dir, TEST_KEY).unwrap();
                log.log_user_input("chat", "two", "chat");
                log.log_user_input("chat", "three", "chat");
            }
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert_eq!(entries.len(), 3);
            assert_eq!(entries[2].content.as_deref(), Some("three"));

            let _ = fs::remove_dir_all(&dir);
        }
    }
}
