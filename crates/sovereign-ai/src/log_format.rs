//! The conversation log's format from v0.0.13, and the one check of a log (contract C-LOG, spec
//! ch. 08, LG1-LG4 and LG6).
//!
//! - **LG1:** each entry is sealed with associated data that hold the file's id, its sequence
//!   number (from 1) and the previous line's SHA-256, so an entry replayed, moved or removed no
//!   longer opens where it now stands. The line keeps the v0.0.12 envelope (`v`, `prev`, `nonce`,
//!   `ct`), with the file id `f` and the number `n` added in clear: the hash chain over the lines
//!   is the same, and [`crate::encrypted_log::verify_chain`] still reads it.
//! - **LG2:** a sealed record of the log's length and head, and of the time the log was last
//!   known good in that state, kept in the persona's beacon file, outside the log's directory. The
//!   beacon already said « this log had content » there (sessionlog-genesis-wipe); it now says
//!   how much, sealed. It is written with each entry, and by a check that finds the log in a state
//!   it does not hold yet: a login that finds the log as recorded changes no file (v0.0.12 item
//!   10, L5). The anchor in the log's directory is kept, bound to the file id.
//! - **LG3:** a line that is not sealed is a finding, wherever it is.
//! - **LG4:** the log is read as a stream, line by line, each line and the file under a cap; a
//!   line over its cap or not valid UTF-8 is a finding, and nothing panics.
//! - **LG6:** a log in the v0.0.12 format is checked under its own rules (no associated data, the
//!   v1 anchor); if it verifies, the caller carries it into this format once.
//!
//! [`check`] reads and never writes. It is what the login's check, the read of the history and
//! the diagnosis of a quarantine run, so they agree on what a failure is. Its findings are the
//! closed list of DG1 ([`Finding`]); no message here carries bytes of the file.

use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sovereign_crypto::aead;

use crate::encrypted_log::{self, sha256_hex, GENESIS_HASH};
use crate::log_diagnosis::Finding;
use crate::session_log::{MAX_FILE_BYTES, MAX_LINE_BYTES};

/// The log's file, its anchor, and the rotated files, by name inside the log's directory.
pub const LOG_FILE: &str = "session_log.jsonl";
pub const ANCHOR_FILE: &str = "session_log.anchor";


// ── the entry (LG1) ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct BoundEnvelope {
    v: u8,
    f: String,
    n: u64,
    prev: String,
    nonce: String,
    ct: String,
}

fn entry_aad(file_id: &str, n: u64, prev: &str) -> Vec<u8> {
    format!("sovereign-sessionlog-entry:v2\n{file_id}\n{n}\n{prev}").into_bytes()
}

/// A new file's id: 24 random bytes, as hex.
pub fn new_file_id() -> String {
    aead::random_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

/// Seals one entry as entry `n` of the file `file_id`, after the line whose SHA-256 is `prev`.
/// Returns the line and its SHA-256.
pub fn seal_entry(plaintext_json: &str, key: &[u8; 32], file_id: &str, n: u64, prev: &str) -> Result<(String, String)> {
    let (ct, nonce) = aead::encrypt_with_aad(plaintext_json.as_bytes(), &entry_aad(file_id, n, prev), key)
        .map_err(|e| anyhow!("session log encrypt: {e}"))?;
    let line = serde_json::to_string(&BoundEnvelope {
        v: 1,
        f: file_id.to_string(),
        n,
        prev: prev.to_string(),
        nonce: B64.encode(nonce),
        ct: B64.encode(ct),
    })?;
    let hash = sha256_hex(line.as_bytes());
    Ok((line, hash))
}

fn open_bound(env: &BoundEnvelope, key: &[u8; 32]) -> Option<String> {
    let nonce: [u8; aead::NONCE_SIZE] = B64.decode(&env.nonce).ok()?.try_into().ok()?;
    let ct = B64.decode(&env.ct).ok()?;
    let plain = aead::decrypt_with_aad(&ct, &entry_aad(&env.f, env.n, &env.prev), &nonce, key).ok()?;
    String::from_utf8(plain).ok()
}

// ── the anchor, bound to the file ───────────────────────────────────────────

const ANCHOR_DOMAIN: &[u8] = b"sovereign-sessionlog-anchor:v2";

#[derive(Serialize, Deserialize)]
struct BoundAnchor {
    f: String,
    count: u64,
    head: String,
    mac: String,
}

fn anchor_body(file_id: &str, count: u64, head: &str) -> String {
    format!("{file_id}:{count}:{head}")
}

pub fn write_anchor(path: &Path, key: &[u8; 32], file_id: &str, count: u64, head: &str) -> Result<()> {
    let mac = sovereign_crypto::mac::keyed_mac(key, ANCHOR_DOMAIN, anchor_body(file_id, count, head).as_bytes());
    let json = serde_json::to_vec(&BoundAnchor { f: file_id.to_string(), count, head: head.to_string(), mac })?;
    write_whole(path, &json)
}

/// Whether the anchor at `path` already holds this state (so it need not be written again).
pub fn anchor_holds(path: &Path, key: &[u8; 32], file_id: &str, count: u64, head: &str) -> bool {
    matches!(read_anchor(path, key), AnchorRead::Bound { file_id: f, count: c, head: h }
        if f == file_id && c == count && h == head)
}

enum AnchorRead {
    Missing,
    Forged,
    /// A v0.0.12 anchor whose MAC verifies.
    Old { count: u64, head: String },
    Bound { file_id: String, count: u64, head: String },
}

fn read_anchor(path: &Path, key: &[u8; 32]) -> AnchorRead {
    let Ok(data) = read_small(path) else { return AnchorRead::Missing };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) else { return AnchorRead::Forged };
    if value.get("f").is_some() {
        let Ok(a) = serde_json::from_value::<BoundAnchor>(value) else { return AnchorRead::Forged };
        if sovereign_crypto::mac::verify_keyed_mac(key, ANCHOR_DOMAIN, anchor_body(&a.f, a.count, &a.head).as_bytes(), &a.mac) {
            return AnchorRead::Bound { file_id: a.f, count: a.count, head: a.head };
        }
        return AnchorRead::Forged;
    }
    match encrypted_log::read_chain_anchor(path, key) {
        encrypted_log::AnchorStatus::Valid { count, head } => AnchorRead::Old { count, head },
        encrypted_log::AnchorStatus::Forged => AnchorRead::Forged,
        encrypted_log::AnchorStatus::Missing => AnchorRead::Missing,
    }
}

// ── the record (LG2) ────────────────────────────────────────────────────────

const RECORD_AAD: &[u8] = b"sovereign-sessionlog-record:v2";

#[derive(Serialize, Deserialize, Clone)]
pub struct Record {
    pub f: String,
    pub count: u64,
    pub head: String,
    /// When the log was last known good in this state: the entry that made it, or the check
    /// that found it.
    pub verified_at: DateTime<Utc>,
}

impl Record {
    /// Whether the record holds this state of the log.
    pub fn holds(&self, file_id: &str, count: u64, head: &str) -> bool {
        self.f == file_id && self.count == count && self.head == head
    }
}

#[derive(Serialize, Deserialize)]
struct SealedRecord {
    nonce: String,
    ct: String,
}

pub fn write_record(path: &Path, key: &[u8; 32], record: &Record) -> Result<()> {
    let body = serde_json::to_vec(record)?;
    let (ct, nonce) = aead::encrypt_with_aad(&body, RECORD_AAD, key).map_err(|e| anyhow!("record encrypt: {e}"))?;
    let json = serde_json::to_vec(&SealedRecord { nonce: B64.encode(nonce), ct: B64.encode(ct) })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_whole(path, &json)
}

pub enum RecordRead {
    Missing,
    /// A beacon of the v0.0.12 format: it says the log had content, nothing more.
    Legacy,
    Forged,
    Valid(Record),
}

pub fn read_record(path: &Path, key: &[u8; 32]) -> RecordRead {
    let Ok(data) = read_small(path) else { return RecordRead::Missing };
    let Ok(sealed) = serde_json::from_slice::<SealedRecord>(&data) else { return RecordRead::Legacy };
    let open = || -> Option<Record> {
        let nonce: [u8; aead::NONCE_SIZE] = B64.decode(&sealed.nonce).ok()?.try_into().ok()?;
        let ct = B64.decode(&sealed.ct).ok()?;
        let body = aead::decrypt_with_aad(&ct, RECORD_AAD, &nonce, key).ok()?;
        serde_json::from_slice(&body).ok()
    };
    match open() {
        Some(r) => RecordRead::Valid(r),
        None => RecordRead::Forged,
    }
}

// ── the check ───────────────────────────────────────────────────────────────

/// The files one check reads. `beacon`, which holds the record, is `None` where the caller has
/// none (the read of the history; tests that open a log without a beacon).
pub struct LogFiles {
    pub log: PathBuf,
    pub anchor: PathBuf,
    pub beacon: Option<PathBuf>,
}

impl LogFiles {
    /// The log in `dir`, with the persona's beacon when one is given.
    pub fn in_dir(dir: &Path, beacon: Option<&Path>) -> Self {
        Self { log: dir.join(LOG_FILE), anchor: dir.join(ANCHOR_FILE), beacon: beacon.map(Path::to_path_buf) }
    }
}

/// What one check found.
pub struct Checked {
    /// DG1's findings; empty when the log verifies.
    pub found: Vec<Finding>,
    /// The log is in the v0.0.12 format (LG6).
    pub old_format: bool,
    /// The number of entries (non-empty lines) read.
    pub entries: u64,
    /// The entries before the first break that open under the key, as their JSON text, oldest
    /// first; at most the number asked for, the latest kept.
    pub readable: VecDeque<String>,
    /// How many entries open before the first break (all of them when the log verifies).
    pub readable_count: u64,
    /// The file id of a log in this format: its lines', else its record's or its anchor's.
    pub file_id: Option<String>,
    /// SHA-256 of the last line, or the genesis hash.
    pub head: String,
    /// The log file's size in bytes.
    pub bytes: u64,
    /// The record, when one was read and opened.
    pub record: Option<Record>,
}

impl Checked {
    pub fn passes(&self) -> bool {
        self.found.is_empty()
    }
}

fn add(found: &mut Vec<Finding>, f: Finding) {
    if !found.contains(&f) {
        found.push(f);
    }
}

enum Line {
    Bound(BoundEnvelope),
    Old(encrypted_log::OldEnvelope),
    /// Shaped like a sealed entry, but not one this code can read.
    Malformed,
    Plain,
}

fn classify(line: &str) -> Line {
    if !encrypted_log::is_encrypted_line(line) {
        return Line::Plain;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { return Line::Malformed };
    if value.get("f").is_some() || value.get("n").is_some() {
        return match serde_json::from_value::<BoundEnvelope>(value) {
            Ok(b) if b.v == 1 => Line::Bound(b),
            _ => Line::Malformed,
        };
    }
    match serde_json::from_value::<encrypted_log::OldEnvelope>(value) {
        Ok(o) if o.v == 1 => Line::Old(o),
        _ => Line::Malformed,
    }
}

/// Why the first entry that is not readable is not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Break {
    /// Not sealed (LG3): its own finding.
    Plain,
    /// Over its cap, or not text (LG4): its own finding.
    Cap,
    /// Sealed, but not in its place, or not one this code can read, or not opening there.
    Chain,
}

/// Checks the log in `files` under `key`. Reads only; keeps the last `keep` readable entries.
pub fn check(files: &LogFiles, key: &[u8; 32], keep: usize) -> Checked {
    let mut found = Vec::new();
    let anchor = read_anchor(&files.anchor, key);
    let record = files.beacon.as_ref().map(|p| read_record(p, key));
    let anchored_at = match &anchor {
        AnchorRead::Old { count, .. } | AnchorRead::Bound { count, .. } => *count,
        _ => 0,
    };
    let recorded_at = match &record {
        Some(RecordRead::Valid(r)) => r.count,
        _ => 0,
    };

    let log_exists = files.log.exists();
    let bytes = fs::metadata(&files.log).map(|m| m.len()).unwrap_or(0);
    let mut out = Checked {
        found: Vec::new(),
        old_format: false,
        entries: 0,
        readable: VecDeque::new(),
        readable_count: 0,
        file_id: None,
        head: GENESIS_HASH.to_string(),
        bytes,
        record: match &record {
            Some(RecordRead::Valid(r)) => Some(r.clone()),
            _ => None,
        },
    };

    // LG4: a file above its cap is not read at all.
    if bytes > MAX_FILE_BYTES {
        out.found = vec![Finding::AboveCapOrNotText];
        return out;
    }

    let mut expected_prev = GENESIS_HASH.to_string();
    let mut first_break: Option<(u64, Break)> = None;
    let mut opened_any = false;
    let mut sealed = 0u64;
    let mut file_id: Option<String> = None;
    let mut format_old: Option<bool> = None;
    // The hashes at the places the anchor and the record name.
    let mut hash_at: Vec<(u64, String)> = Vec::new();

    if let Ok(file) = fs::File::open(&files.log) {
        let mut reader = BufReader::new(file);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            let read = (&mut reader).take(MAX_LINE_BYTES + 1).read_until(b'\n', &mut buf);
            match read {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => {
                    out.entries += 1;
                    add(&mut found, Finding::AboveCapOrNotText);
                    first_break.get_or_insert((out.entries, Break::Cap));
                    break;
                }
            }
            let ended = buf.last() == Some(&b'\n');
            if ended {
                buf.pop();
            }
            if buf.len() as u64 > MAX_LINE_BYTES {
                out.entries += 1;
                add(&mut found, Finding::AboveCapOrNotText);
                first_break.get_or_insert((out.entries, Break::Cap));
                break;
            }
            let Ok(text) = std::str::from_utf8(&buf) else {
                out.entries += 1;
                add(&mut found, Finding::AboveCapOrNotText);
                first_break.get_or_insert((out.entries, Break::Cap));
                break;
            };
            if text.trim().is_empty() {
                continue;
            }
            out.entries += 1;
            let i = out.entries;
            let line_hash = sha256_hex(text.as_bytes());

            let (in_place, opened, plain) = match classify(text) {
                Line::Plain => {
                    add(&mut found, Finding::HoldsPlaintext);
                    (false, None, true)
                }
                Line::Malformed => {
                    sealed += 1;
                    (false, None, false)
                }
                Line::Bound(env) => {
                    sealed += 1;
                    let format_ok = !*format_old.get_or_insert(false);
                    let fid = file_id.get_or_insert_with(|| env.f.clone());
                    let place_ok = format_ok && env.f == *fid && env.n == i && env.prev == expected_prev;
                    (place_ok, open_bound(&env, key), false)
                }
                Line::Old(env) => {
                    sealed += 1;
                    let format_ok = *format_old.get_or_insert(true);
                    let place_ok = format_ok && env.prev == expected_prev;
                    (place_ok, encrypted_log::decrypt_entry(text, key).ok(), false)
                }
            };
            if opened.is_some() {
                opened_any = true;
            }
            match (first_break, opened) {
                (None, Some(json)) if in_place => {
                    out.readable_count += 1;
                    if keep > 0 {
                        if out.readable.len() == keep {
                            out.readable.pop_front();
                        }
                        out.readable.push_back(json);
                    }
                }
                (None, _) => {
                    first_break = Some((i, if plain { Break::Plain } else { Break::Chain }));
                }
                _ => {}
            }
            if i == anchored_at || i == recorded_at {
                hash_at.push((i, line_hash.clone()));
            }
            expected_prev = line_hash;
        }
    }
    out.head = expected_prev;
    out.old_format = format_old == Some(true);

    let n = out.entries;
    let no_entry_opens = sealed > 0 && !opened_any;
    if no_entry_opens {
        add(&mut found, Finding::EntriesDoNotDecrypt);
    } else if let Some((at, Break::Chain)) = first_break {
        add(&mut found, Finding::ChainBreaks { at, of: n });
    }

    // The anchor, the record and the beacon (LG2), unless no entry opens under this key: then
    // they are sealed under another key too, and the one finding says it.
    if !no_entry_opens {
        let head_at = |count: u64| hash_at.iter().find(|(i, _)| *i == count).map(|(_, h)| h.clone());
        match &anchor {
            AnchorRead::Missing => {
                if sealed > 0 {
                    add(&mut found, Finding::AnchorDeletedWhileEntriesExisted);
                }
            }
            AnchorRead::Forged => add(&mut found, Finding::AnchorSignatureInvalid),
            AnchorRead::Old { count, head } => {
                if sealed > 0 && !out.old_format {
                    // An anchor of the older format over a log of this one.
                    add(&mut found, Finding::AnchorSignatureInvalid);
                } else if *count == 0 && n > 0 {
                    // v0.0.12's rule: an anchor of count 0 records an empty log, so
                    // entries under it are not the log it recorded, from the first.
                    add(&mut found, Finding::ChainBreaks { at: 1, of: n });
                } else {
                    tail_against(&mut found, n, *count, head, head_at(*count));
                }
            }
            AnchorRead::Bound { file_id: af, count, head } => {
                if out.old_format {
                    add(&mut found, Finding::AnchorSignatureInvalid);
                } else if file_id.as_ref().is_some_and(|f| f != af) {
                    add(&mut found, Finding::ChainBreaks { at: 1, of: n });
                } else {
                    tail_against(&mut found, n, *count, head, head_at(*count));
                    file_id.get_or_insert_with(|| af.clone());
                }
            }
        }
        match &record {
            None => {}
            Some(RecordRead::Missing) => {
                if sealed > 0 && !out.old_format {
                    add(&mut found, Finding::AnchorDeletedWhileEntriesExisted);
                }
            }
            Some(RecordRead::Legacy) => {
                if sealed > 0 && !out.old_format {
                    // A beacon of the older format over a log of this one: put back.
                    add(&mut found, Finding::AnchorSignatureInvalid);
                }
            }
            Some(RecordRead::Forged) => add(&mut found, Finding::AnchorSignatureInvalid),
            Some(RecordRead::Valid(r)) => {
                if out.old_format {
                    // A record exists only after a check of this format: an older log was put
                    // back under it.
                    add(&mut found, Finding::ChainBreaks { at: 1, of: n });
                } else if file_id.as_ref().is_some_and(|f| f != &r.f) {
                    add(&mut found, Finding::ChainBreaks { at: 1, of: n });
                } else {
                    tail_against(&mut found, n, r.count, &r.head, head_at(r.count));
                    file_id.get_or_insert_with(|| r.f.clone());
                }
            }
        }
        // A wipe: no entry, the log or its anchor gone, while the beacon, the record or the
        // anchor says the log had content.
        if n == 0 {
            let beacon_says_content = matches!(record, Some(RecordRead::Legacy)) || recorded_at > 0;
            let said_content = beacon_says_content || anchored_at > 0;
            let anchor_gone = matches!(anchor, AnchorRead::Missing);
            if said_content && (anchor_gone || !log_exists) {
                found.retain(|f| !matches!(f, Finding::MissingAtEnd { .. }));
                add(&mut found, Finding::LogWiped);
            }
        }
    }

    out.file_id = file_id;
    out.found = found;
    out
}

/// The log against a count and head it was known to hold: shorter is entries missing at the
/// end; the same length or longer must still hold that head at that place.
fn tail_against(found: &mut Vec<Finding>, n: u64, count: u64, head: &str, at_count: Option<String>) {
    if count == 0 {
        return;
    }
    if n < count {
        add(found, Finding::MissingAtEnd { count: count - n });
    } else if at_count.as_deref() != Some(head) {
        add(found, Finding::ChainBreaks { at: count, of: n });
    }
}

// ── writing ─────────────────────────────────────────────────────────────────

/// Carries a v0.0.12 log that verified into this format, once (LG6): each entry opened, sealed
/// again in its place, written to a new file that replaces the log. Returns the new file's id,
/// its count and head.
pub fn carry_old_log(files: &LogFiles, key: &[u8; 32]) -> Result<(String, u64, String)> {
    let file = fs::File::open(&files.log)?;
    let reader = BufReader::new(file);
    let file_id = new_file_id();
    let tmp = files.log.with_extension("jsonl.carry");
    let mut out = String::new();
    let mut prev = GENESIS_HASH.to_string();
    let mut n = 0u64;
    for line in reader.split(b'\n') {
        let line = line?;
        let text = std::str::from_utf8(&line).map_err(|_| anyhow!("carry: a line is not text"))?;
        if text.trim().is_empty() {
            continue;
        }
        let plain = encrypted_log::decrypt_entry(text, key)?;
        n += 1;
        let (sealed, hash) = seal_entry(&plain, key, &file_id, n, &prev)?;
        out.push_str(&sealed);
        out.push('\n');
        prev = hash;
    }
    write_whole(&tmp, out.as_bytes())?;
    fs::rename(&tmp, &files.log)?;
    Ok((file_id, n, prev))
}

/// Writes `bytes` to `path` through a file beside it, renamed over it.
fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, bytes).map_err(|e| anyhow!("write: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| anyhow!("rename: {e}"))?;
    Ok(())
}

/// Reads a small file (an anchor, a record), refusing one above 64 KiB.
fn read_small(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut data = Vec::new();
    (&mut f).take(64 * 1024 + 1).read_to_end(&mut data)?;
    if data.len() > 64 * 1024 {
        return Err(std::io::Error::other("too large"));
    }
    Ok(data)
}
