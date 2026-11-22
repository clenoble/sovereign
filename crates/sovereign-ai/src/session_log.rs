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

/// What the person is told when the log is quarantined: Céline's words (v0.0.12 item 10,
/// 2026-10-26), then the sentence C-LOG's DG5 gives back (spec ch. 08). Both hosts show it as the
/// session log's rejection; the native shell also says it at its login check.
pub const QUARANTINE_NOTICE: &str = "Your assistant's conversation log could not be verified: it \
     may have been changed while Sovereign was closed, or a write was cut short (a crash, a full \
     disk). The assistant starts without its earlier conversations. Nothing was deleted. Ask the \
     assistant 'what happened to my log?'";

/// C-LOG LG4: the largest line the check reads. A line the log would write above it is dropped
/// instead (the application never quarantines its own log for a line's size).
pub const MAX_LINE_BYTES: u64 = 4 * 1024 * 1024;

/// C-LOG LG4: the largest log file the check reads. Above what one session writes before the
/// next open rotates the file (at `MAX_LOG_SIZE`): the writer stops short of it, dropping an entry
/// rather than passing it, so the application never quarantines its own log for its size.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

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
    /// C-LOG LG1: the current file's id, bound into each entry.
    #[cfg(feature = "encrypted-log")]
    file_id: String,
    /// The current file's size, kept under `MAX_FILE_BYTES` (LG4).
    #[cfg(feature = "encrypted-log")]
    bytes: u64,
    /// The persona's beacon, which holds the sealed record of LG2.
    #[cfg(feature = "encrypted-log")]
    beacon: Option<PathBuf>,
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
            #[cfg(feature = "encrypted-log")]
            file_id: String::new(),
            #[cfg(feature = "encrypted-log")]
            bytes: 0,
            #[cfg(feature = "encrypted-log")]
            beacon: None,
        })
    }

    /// Open or create the session log with per-entry encryption, in the format of C-LOG (spec
    /// ch. 08): each entry sealed to its place (LG1), the anchor bound to the file, and the
    /// sealed record of the log's length and head kept in the beacon (LG2).
    ///
    /// `beacon` is a path OUTSIDE the log dir (the crypto dir, alongside `auth.store`) that
    /// records "this account's encrypted session log has had content". It is created here once
    /// the log is non-empty and consulted to catch the genesis-wipe (sessionlog-genesis-wipe):
    /// deleting BOTH the log and its anchor reverts the dir to a pristine genesis state the
    /// in-dir checks can't distinguish from a fresh account — but the beacon, living elsewhere,
    /// survives that delete and proves the wipe. It holds the sealed record of LG2, written with
    /// each entry. `None` disables both (tests).
    ///
    /// The log is verified BEFORE any rotation or carry (SESSIONLOG-001/003, H-log1): only a
    /// verified log may rotate, be carried, or have its anchor and record written. A log that
    /// does not verify is refused, never re-anchored.
    #[cfg(feature = "encrypted-log")]
    pub fn open_encrypted(dir: &Path, key: [u8; 32], beacon: Option<&Path>) -> Result<Self> {
        use crate::log_format::{self, LogFiles, Record};
        fs::create_dir_all(dir)?;
        let files = LogFiles::in_dir(dir, beacon);
        let checked = log_format::check(&files, &key, 0);
        if !checked.passes() {
            // No bytes of the file: the findings are names and numbers.
            let reason = format!("the log did not verify: {:?}", checked.found);
            tracing::error!("SESSIONLOG-001: session log tamper on open ({reason}); refusing to append");
            return Err(anyhow::anyhow!(
                "session log tamper detected ({reason}); refusing to append (SESSIONLOG-001)"
            ));
        }

        let (mut file_id, mut count, mut head) = (
            checked.file_id.clone().unwrap_or_else(log_format::new_file_id),
            checked.entries,
            checked.head.clone(),
        );
        // LG6: a log of the v0.0.12 format that verified is carried into this one, once.
        if checked.old_format && checked.entries > 0 {
            (file_id, count, head) = log_format::carry_old_log(&files, &key)?;
            tracing::info!("Session log carried into the new format ({count} entries)");
        }

        // Rotate if needed — the log verified clean above. The rotated file is no longer read;
        // the new one gets its own id, and the anchor and record restart with it.
        let size = fs::metadata(&files.log).map(|m| m.len()).unwrap_or(0);
        if size > MAX_LOG_SIZE {
            Self::rotate(&files.log);
            file_id = log_format::new_file_id();
            count = 0;
            head = crate::encrypted_log::GENESIS_HASH.to_string();
        }

        // The anchor and the record are written only when the state they hold changes: a
        // login that finds the log as recorded changes no file (v0.0.12 item 10, L5).
        if !log_format::anchor_holds(&files.anchor, &key, &file_id, count, &head) {
            log_format::write_anchor(&files.anchor, &key, &file_id, count, &head)?;
        }
        if let Some(beacon) = beacon {
            let record = checked.record.as_ref();
            let unchanged = record.is_some_and(|r| r.holds(&file_id, count, &head));
            // As the beacon before it, the record appears once the log has content.
            if !unchanged && (count > 0 || beacon.exists()) {
                log_format::write_record(
                    beacon,
                    &key,
                    &Record { f: file_id.clone(), count, head: head.clone(), verified_at: Utc::now() },
                )?;
            }
        }

        let file = OpenOptions::new().create(true).append(true).open(&files.log)?;
        let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);

        tracing::info!("Session log (encrypted): {}", files.log.display());
        Ok(Self {
            writer: BufWriter::new(file),
            path: files.log,
            encryption_key: Some(key),
            prev_hash: head,
            anchor_path: files.anchor,
            chain_count: count,
            file_id,
            bytes,
            beacon: beacon.map(Path::to_path_buf),
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
        #[cfg(feature = "encrypted-log")]
        if let Some(ref key) = self.encryption_key {
            let json = entry.to_string();
            let n = self.chain_count + 1;
            match crate::log_format::seal_entry(&json, key, &self.file_id, n, &self.prev_hash) {
                Ok((encrypted_line, new_hash)) => {
                    // C-LOG LG4: the writer stays under the caps the check reads, so the
                    // application never quarantines its own log for a size it wrote.
                    let len = encrypted_line.len() as u64;
                    if len > MAX_LINE_BYTES || self.bytes + len + 1 > MAX_FILE_BYTES {
                        tracing::warn!(
                            "Session log entry of {len} bytes not written: above the log's cap"
                        );
                        return;
                    }
                    if let Err(e) = writeln!(self.writer, "{encrypted_line}") {
                        tracing::error!("Failed to write encrypted session log: {e}");
                        return;
                    }
                    let _ = self.writer.flush();
                    self.bytes += len + 1;
                    self.prev_hash = new_hash;
                    // SESSIONLOG-003: advance the MAC'd high-water anchor so a
                    // later tail-truncation/rollback is detectable. Written after
                    // the line is flushed, so a crash in between just leaves the
                    // anchor lagging by one (tolerated as a lower bound on read).
                    self.chain_count = n;
                    if let Err(e) = crate::log_format::write_anchor(
                        &self.anchor_path,
                        key,
                        &self.file_id,
                        self.chain_count,
                        &self.prev_hash,
                    ) {
                        tracing::warn!("Failed to update session log integrity anchor: {e}");
                    }
                    // C-LOG LG2: the record outside the log's directory follows each entry,
                    // so a roll-back of that directory is seen.
                    if let Some(beacon) = &self.beacon {
                        let record = crate::log_format::Record {
                            f: self.file_id.clone(),
                            count: self.chain_count,
                            head: self.prev_hash.clone(),
                            verified_at: Utc::now(),
                        };
                        if let Err(e) = crate::log_format::write_record(beacon, key, &record) {
                            tracing::warn!("Failed to update the session log's record: {e}");
                        }
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

    /// Load the N most recent entries, decrypting them with the given key.
    ///
    /// The log is read through the one check of its format (C-LOG, [`crate::log_format::check`]):
    /// a stream of lines under their caps, each entry opened in its place, the anchor against
    /// the file. SESSIONLOG-001: these entries are fed back to the model as authentic prior
    /// turns, so a log that does not verify gives nothing back — no plaintext line (LG3), no
    /// entry out of place, nothing past a truncation, a forged or a deleted anchor.
    #[cfg(feature = "encrypted-log")]
    pub fn load_recent_encrypted(
        dir: &Path,
        max_entries: usize,
        key: &[u8; 32],
    ) -> Vec<SessionEntry> {
        let files = crate::log_format::LogFiles::in_dir(dir, None);
        let checked = crate::log_format::check(&files, key, max_entries);
        if !checked.passes() {
            tracing::error!(
                "Session log integrity check FAILED ({:?}); giving back no history (fail closed)",
                checked.found
            );
            return Vec::new();
        }
        checked
            .readable
            .iter()
            .filter_map(|json| serde_json::from_str::<SessionEntry>(json).ok())
            .collect()
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
    /// Where the log lives and the key it was opened under, read together so
    /// a read never pairs one persona's directory with another's key.
    at: std::sync::Mutex<SlotAt>,
    log: std::sync::Mutex<Option<SessionLog>>,
    /// Set by a failed encrypted open; cleared by a successful one.
    disabled: std::sync::atomic::AtomicBool,
    /// Bumped each time the slot is pointed at a directory
    /// ([`point_at`](Self::point_at)): a read that straddles the move returns
    /// nothing rather than one persona's history to the next.
    generation: std::sync::atomic::AtomicU64,
}

/// The slot's directory and key.
struct SlotAt {
    dir: PathBuf,
    #[cfg(feature = "encrypted-log")]
    key: Option<[u8; 32]>,
}

impl SessionLogSlot {
    /// Open the plaintext log in `dir`. An I/O error leaves the slot without
    /// a log (nothing is written), as before.
    ///
    /// With the encrypted log, nothing is opened until a key is installed: a
    /// plaintext line is never accepted (C-LOG LG3), so writing one before the
    /// login would get the persona's log quarantined at that login.
    pub fn open(dir: &Path) -> Self {
        #[cfg(feature = "encrypted-log")]
        let log: Option<SessionLog> = None;
        #[cfg(not(feature = "encrypted-log"))]
        let log = match SessionLog::open(dir) {
            Ok(log) => Some(log),
            Err(e) => {
                tracing::warn!("Session log unavailable: {e}");
                None
            }
        };
        Self {
            at: std::sync::Mutex::new(SlotAt {
                dir: dir.to_path_buf(),
                #[cfg(feature = "encrypted-log")]
                key: None,
            }),
            log: std::sync::Mutex::new(log),
            disabled: std::sync::atomic::AtomicBool::new(false),
            generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// The directory holding the log.
    pub fn dir(&self) -> PathBuf {
        match self.at.lock() {
            Ok(at) => at.dir.clone(),
            Err(poisoned) => poisoned.into_inner().dir.clone(),
        }
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

    /// Point the slot at the log in `dir` (v0.0.12 item 10, L6: a host whose
    /// one orchestrator serves each persona that logs in). The log open until
    /// now is closed, unwritten, and its key forgotten; logging is off until
    /// [`install_key`](Self::install_key) opens the log in `dir`. Nothing at
    /// the previous directory is touched, and no read returns its history
    /// from now on.
    #[cfg(feature = "encrypted-log")]
    pub fn point_at(&self, dir: &Path) {
        self.disabled.store(true, std::sync::atomic::Ordering::SeqCst);
        self.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // Lock order, everywhere: `log`, then `at`.
        let mut guard = match self.log.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard = None;
        let mut at = match self.at.lock() {
            Ok(a) => a,
            Err(poisoned) => poisoned.into_inner(),
        };
        at.dir = dir.to_path_buf();
        at.key = None;
    }

    /// Switch to the encrypted log under `key`, in the slot's directory. On
    /// failure the slot turns logging OFF (the plaintext log is closed and
    /// never reopened) and the reason is returned for the caller to surface.
    #[cfg(feature = "encrypted-log")]
    pub fn install_key(&self, key: [u8; 32], beacon: Option<&Path>) -> Result<()> {
        // Hold the log lock across the open so no plaintext line can slip in
        // between the verification and the switch.
        let mut guard = self
            .log
            .lock()
            .map_err(|_| anyhow::anyhow!("session log lock poisoned"))?;
        let mut at = self
            .at
            .lock()
            .map_err(|_| anyhow::anyhow!("session log lock poisoned"))?;
        match SessionLog::open_encrypted(&at.dir, key, beacon) {
            Ok(log) => {
                *guard = Some(log);
                at.key = Some(key);
                self.disabled.store(false, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                self.disabled.store(true, std::sync::atomic::Ordering::SeqCst);
                *guard = None;
                at.key = None;
                Err(e)
            }
        }
    }

    /// Turn logging OFF for this session without trying to open the log —
    /// the state a failed [`install_key`](Self::install_key) leaves: no line
    /// is written, in clear or otherwise, and no history is read back. Used
    /// when an unverifiable log could not be quarantined (v0.0.12 item 10).
    #[cfg(feature = "encrypted-log")]
    pub fn turn_off(&self) {
        self.disabled.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut guard) = self.log.lock() {
            *guard = None;
        }
        if let Ok(mut at) = self.at.lock() {
            at.key = None;
        }
    }

    /// The most recent entries to feed back as conversation history: none
    /// while logging is off (the file failed verification, so its content
    /// is not trusted); decrypted and verified once a key is installed.
    pub fn load_recent(&self, max_entries: usize) -> Vec<SessionEntry> {
        if self.is_disabled() {
            return Vec::new();
        }
        let generation = self.generation.load(std::sync::atomic::Ordering::SeqCst);
        let Ok(at) = self.at.lock() else {
            return Vec::new();
        };
        let dir = at.dir.clone();
        #[cfg(feature = "encrypted-log")]
        let key = at.key;
        drop(at);
        // C-LOG LG3: with the encrypted log, no key, no history (a plaintext
        // log is never read back).
        #[cfg(feature = "encrypted-log")]
        let entries = match key {
            Some(key) => SessionLog::load_recent_encrypted(&dir, max_entries, &key),
            None => Vec::new(),
        };
        #[cfg(not(feature = "encrypted-log"))]
        let entries = SessionLog::load_recent(&dir, max_entries);
        // The slot was turned off, or pointed elsewhere, while reading.
        if self.is_disabled()
            || self.generation.load(std::sync::atomic::Ordering::SeqCst) != generation
        {
            return Vec::new();
        }
        entries
    }
}

/// Quarantine of an unverifiable conversation log (v0.0.12 item 10; spec
/// ch. 08 *Session log*; C-LOG). Runs at every log-key install, before
/// [`SessionLogSlot::install_key`].
///
/// If the log in `dir` does not verify under `key` — the one check of its
/// format, [`crate::log_format::check`], which [`SessionLog::open_encrypted`]
/// runs too — its files (log, anchor, rotated logs) and the `beacon`, which
/// holds the record, each under its own name, are moved, unread and unchanged, into a new `quarantine-<UTC time>`
/// directory inside `dir`, which no read path ever opens. The diagnosis
/// ([`crate::log_diagnosis`]) reads them there. Nothing is deleted. Returns
/// that directory, or `None` when the log verifies or is missing.
///
/// The conversation log is a prompt-injection surface (SESSIONLOG-001 to
/// 003), so there is no other path: no marker, no one-time rule. If a move
/// fails, the error is returned; what remains in place still fails the same
/// check (the beacon is moved last, so while it stays an emptied directory
/// reads as a genesis wipe), so `install_key` fails closed, and the next
/// install tries again.
#[cfg(feature = "encrypted-log")]
pub fn quarantine_unverifiable(
    dir: &Path,
    key: [u8; 32],
    beacon: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let files = crate::log_format::LogFiles::in_dir(dir, beacon);
    let checked = crate::log_format::check(&files, &key, 0);
    if checked.passes() {
        return Ok(None);
    }

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut quarantine = dir.join(format!("quarantine-{stamp}"));
    let mut n = 1;
    while quarantine.exists() {
        quarantine = dir.join(format!("quarantine-{stamp}-{n}"));
        n += 1;
    }
    fs::create_dir_all(&quarantine)?;

    let mut names = vec!["session_log.jsonl".to_string(), "session_log.anchor".to_string()];
    names.extend((1..=MAX_ROTATED).map(|i| format!("session_log.{i}.jsonl")));
    for name in &names {
        let from = dir.join(name);
        if from.exists() {
            move_file(&from, &quarantine.join(name))?;
        }
    }
    // The beacon, and the record it holds, go with the log they vouched for:
    // left behind, it would make the fresh log read as a wipe.
    if let Some(b) = beacon {
        if b.exists() {
            let name = b
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_else(|| "session_log.beacon".into());
            move_file(b, &quarantine.join(name))?;
        }
    }

    tracing::warn!(
        "Session log did not verify ({:?}); quarantined unread in {}",
        checked.found,
        quarantine.display()
    );
    Ok(Some(quarantine))
}

/// Rename, or copy then remove when the two paths are on different volumes.
#[cfg(feature = "encrypted-log")]
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)?;
    Ok(())
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

        /// C-LOG LG3 (brought to the contract by its test author, 2026-11-18): a
        /// log holding a plaintext line is not opened for encrypted writing, and
        /// its plaintext turn is never read back. Until then this test held that
        /// plaintext seed data followed by encrypted entries opened and read back
        /// both.
        #[test]
        fn plaintext_then_encrypted_is_refused() {
            let dir =
                std::env::temp_dir().join(format!("session-log-mixed-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            // Write plaintext first (seed data from before encryption)
            {
                let mut log = SessionLog::open(&dir).unwrap();
                log.log_user_input("chat", "plaintext msg", "chat");
            }

            assert!(
                SessionLog::open_encrypted(&dir, TEST_KEY, None).is_err(),
                "a log holding a plaintext line is refused (LG3)"
            );
            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert!(
                !entries.iter().any(|e| e.content.as_deref() == Some("plaintext msg")),
                "the plaintext turn is never read back (LG3)"
            );

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

        /// C-LOG LG3 (brought to the contract by its test author, 2026-11-18): a
        /// log written only in plaintext, with no anchor, is never read back as
        /// history under a key. Until then this test held that it still loaded.
        #[test]
        fn plaintext_only_log_is_not_read_back() {
            let dir = std::env::temp_dir()
                .join(format!("session-log-ptonly-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);

            {
                let mut log = SessionLog::open(&dir).unwrap(); // plaintext writer
                log.log_user_input("chat", "seed entry", "chat");
            }
            assert!(!dir.join("session_log.anchor").exists());

            let entries = SessionLog::load_recent_encrypted(&dir, 100, &TEST_KEY);
            assert!(entries.is_empty(), "a plaintext-only log is not read back (LG3): {entries:?}");

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

