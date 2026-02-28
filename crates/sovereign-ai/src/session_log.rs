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
            path,
            #[cfg(feature = "encrypted-log")]
            encryption_key: None,
            #[cfg(feature = "encrypted-log")]
            prev_hash: crate::encrypted_log::GENESIS_HASH.to_string(),
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

        // Rotate if needed
        if path.exists() {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > MAX_LOG_SIZE {
                    Self::rotate(&path);
                }
            }
        }

        // Read the hash of the last line for chain continuity
        let prev_hash = Self::read_last_line_hash(&path);

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
        })
    }

    /// Read the last line of the log file and return its SHA-256 hash.
    /// Returns genesis hash if file doesn't exist or is empty.
    #[cfg(feature = "encrypted-log")]
    fn read_last_line_hash(path: &Path) -> String {
        use crate::encrypted_log;

        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return encrypted_log::GENESIS_HASH.to_string(),
        };

        let reader = BufReader::new(file);
        let mut last_line = None;
        for line in reader.lines() {
            if let Ok(l) = line {
                if !l.trim().is_empty() {
                    last_line = Some(l);
                }
            }
        }

        match last_line {
            Some(line) => encrypted_log::sha256_hex(line.as_bytes()),
            None => encrypted_log::GENESIS_HASH.to_string(),
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
                    return;
                }
                Err(e) => {
                    tracing::error!("Session log encryption failed: {e}");
                    // Fall through to plaintext write as last resort
                }
            }
        }

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

        // Verify chain integrity (non-fatal — just log warnings)
        if let Err(e) = encrypted_log::verify_chain(&raw_lines) {
            tracing::warn!("Session log chain integrity check failed: {e}");
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
    }
}
