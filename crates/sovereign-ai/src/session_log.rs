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
/// Plaintext for MVP; encryption deferred to post-MVP per spec.
pub struct SessionLog {
    writer: BufWriter<std::fs::File>,
    path: PathBuf,
}

impl SessionLog {
    /// Open or create the session log file.
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
        })
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
        if let Err(e) = writeln!(self.writer, "{}", entry) {
            tracing::error!("Failed to write session log: {e}");
        }
        // Flush after each entry for durability
        let _ = self.writer.flush();
    }

    /// Load the N most recent entries from the session log file.
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
}
