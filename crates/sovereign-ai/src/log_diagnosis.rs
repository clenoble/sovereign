//! The diagnosis of a quarantined conversation log (contract C-LOG, spec ch. 08, « The diagnosis
//! (the sandbox) », DG1-DG6).
//!
//! Types and signatures by the C-LOG test author, so the tests can name them; every body is the
//! implementer's. Two values, kept apart on purpose:
//! - [`Diagnosis`], what the check found and when, and the choices: the only part the assistant
//!   receives, as [`Diagnosis::for_the_model`] (DG1, DG2, DG4);
//! - [`Readable`], the entries that still verify, for the person's read-only window only (DG3).
//!   Nothing here turns it into text for the model.
//!
//! Both read the persona's own quarantine and its own record only (DG6), never the altered log
//! for the model: what reaches the model is made of numbers and sentences from the fixed list
//! below, never a string taken from the disk.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use sovereign_core::security::ActionLevel;

use crate::log_format::{check, LogFiles, ANCHOR_FILE, LOG_FILE};
use crate::orchestrator::OrchestratorStatePaths;
use crate::session_log::SessionEntry;

/// How her question reaches the diagnosis: the tool the assistant's agent loop offers the model
/// while the logged-in persona has a quarantine. Its result, the tool's turn the model reads, is
/// [`Diagnosis::for_the_model`] of [`crate::Orchestrator::log_diagnosis`]; it takes no argument
/// and carries out no choice.
pub const DIAGNOSIS_TOOL: &str = "what_happened_to_my_log";

/// How [`DIAGNOSIS_TOOL`] is described to the model.
pub const DIAGNOSIS_TOOL_DESCRIPTION: &str = "Say what the check found about her conversation log \
     that was quarantined at a login, when, and what she can do with it. Use it when she asks what \
     happened to her log.";

/// The tool's turn when the persona has no quarantine.
pub const NO_QUARANTINE: &str = "No conversation log of hers was quarantined.";

/// DG5: whether `text` is the question the quarantine notice tells her to ask, « what happened
/// to my log? », whatever its case, spaces, quotes and final punctuation.
pub fn asks_what_happened(text: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    words == ["what", "happened", "to", "my", "log"]
}

/// DG2: the words the files' times are said with (the contract's own words).
pub const ACCORDING_TO_THE_FILES: &str = "according to the files";

/// DG1: what the check found. Closed: the contract's list (spec v2.0.126), one variant per item;
/// each failure the check can make has its variant, and a log can have more than one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    /// The chain breaks at entry `at` of `of`. Entries are numbered from 1, as the person counts
    /// them; `of` is the number of entries in the quarantined log.
    ChainBreaks { at: u64, of: u64 },
    /// `count` entries are missing at the end (the log is shorter than its record, LG2).
    MissingAtEnd { count: u64 },
    /// The anchor's signature is invalid.
    AnchorSignatureInvalid,
    /// The anchor was deleted while entries existed.
    AnchorDeletedWhileEntriesExisted,
    /// The log was wiped.
    LogWiped,
    /// Entries do not decrypt with this persona's key.
    EntriesDoNotDecrypt,
    /// The log holds plaintext lines, or was written only in plaintext, from before encryption
    /// (LG3).
    HoldsPlaintext,
    /// A line or the file is above its cap, or the text is not valid UTF-8 (LG4).
    AboveCapOrNotText,
}

/// DG1, several quarantines: one before the most recent, named by its number and date. Numbers
/// count the persona's quarantines from 1, the oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlierQuarantine {
    pub number: u32,
    pub date: DateTime<Utc>,
}

/// DG2: when.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct When {
    /// The last good verification, from the sealed record (LG2). `None` when the record holds
    /// none.
    pub last_good_verification: Option<DateTime<Utc>>,
    /// The latest time the quarantined files were changed, read from the files themselves and
    /// said « according to the files ». `None` when there is no file to read it from.
    pub files_last_changed: Option<DateTime<Utc>>,
}

/// DG4: what the person can do with the quarantined log. The assistant offers these and never
/// chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Keep the quarantined log, as it is: the default.
    Keep,
    /// Export it for an expert: a Transmit action.
    ExportForAnExpert,
    /// Delete it, with a confirmation.
    Delete,
}

impl Choice {
    /// The action gate's level of carrying out this choice, if it is an action at all.
    pub fn action_level(self) -> Option<ActionLevel> {
        match self {
            Choice::Keep => None,
            Choice::ExportForAnExpert => Some(ActionLevel::Transmit),
            Choice::Delete => Some(ActionLevel::Destruct),
        }
    }

    /// Whether carrying out this choice asks the person to confirm first.
    pub fn needs_confirmation(self) -> bool {
        match self {
            Choice::Keep => false,
            // A Transmit action asks before anything leaves; deleting asks to confirm.
            Choice::ExportForAnExpert | Choice::Delete => true,
        }
    }
}

/// The diagnosis of the persona's most recent quarantine (DG1, DG2, DG4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnosis {
    /// What the check found (DG1), at least one finding.
    pub found: Vec<Finding>,
    /// When (DG2).
    pub when: When,
    /// The choices offered (DG4).
    pub choices: Vec<Choice>,
    /// The choice that holds when the person makes none (DG4: keep).
    pub default_choice: Choice,
    /// The persona's quarantines before the most recent, oldest first (DG1, several quarantines).
    pub earlier: Vec<EarlierQuarantine>,
}

impl Diagnosis {
    /// What the assistant receives for DG1 and DG2: sentences from the fixed list
    /// ([`finding_sentence`], [`ACCORDING_TO_THE_FILES`] and the implementer's others, worded in
    /// this module) and numbers, never a string taken from the disk: no file or directory name,
    /// no parser message, no entry's text. It offers the choices, each by its
    /// [`choice_sentence`].
    pub fn for_the_model(&self) -> String {
        let mut out = String::from(
            "Her assistant's conversation log was quarantined at a login, because its check failed. \
             Nothing of its content is given here. What the check found:\n",
        );
        for f in &self.found {
            out.push_str("- ");
            out.push_str(&finding_sentence(*f));
            out.push('\n');
        }
        match self.when.last_good_verification {
            Some(t) => out.push_str(&format!("The log was last recorded intact on {}.\n", day_and_time(t))),
            None => out.push_str("There is no record of the log being intact.\n"),
        }
        if let Some(t) = self.when.files_last_changed {
            out.push_str(&format!(
                "The files were last changed on {}, {ACCORDING_TO_THE_FILES}.\n",
                day_and_time(t)
            ));
        }
        for e in &self.earlier {
            out.push_str(&earlier_quarantine_sentence(e));
            out.push('\n');
        }
        out.push_str(
            "What she can do with the quarantined log; offer these, and never choose for her:\n",
        );
        for c in &self.choices {
            out.push_str("- ");
            out.push_str(&choice_sentence(*c));
            out.push('\n');
        }
        out
    }
}

/// DG1: the plain words for one finding, with its numbers.
pub fn finding_sentence(finding: Finding) -> String {
    match finding {
        Finding::ChainBreaks { at, of } => format!(
            "The chain of entries breaks at entry {at} of {of}: from there on, an entry is not the one written in that place."
        ),
        Finding::MissingAtEnd { count: 1 } => {
            "1 entry is missing at the end: the log is shorter than at its last check.".to_string()
        }
        Finding::MissingAtEnd { count } => {
            format!("{count} entries are missing at the end: the log is shorter than at its last check.")
        }
        Finding::AnchorSignatureInvalid => "The signature of the log's integrity record is invalid.".to_string(),
        Finding::AnchorDeletedWhileEntriesExisted => {
            "The log's integrity record was deleted while the log still had entries.".to_string()
        }
        Finding::LogWiped => "The log was wiped: its entries are gone, though Sovereign had recorded that it held some.".to_string(),
        Finding::EntriesDoNotDecrypt => "The entries do not decrypt with this persona's key.".to_string(),
        Finding::HoldsPlaintext => {
            "The log holds lines that are not sealed, or was written only in plaintext, from before encryption.".to_string()
        }
        Finding::AboveCapOrNotText => {
            "A line or the whole log is above its size limit, or the text is not valid UTF-8.".to_string()
        }
    }
}

/// A time as the diagnosis says it: local, in History's form (`YYYY-MM-DD HH:MM`, no zone), as
/// every time said to her on both hosts.
fn day_and_time(t: DateTime<Utc>) -> String {
    crate::local_time::history_format(t)
}

/// DG4: the sentence that offers one choice, in what the model receives.
pub fn choice_sentence(choice: Choice) -> String {
    match choice {
        Choice::Keep => "Keep it as it is: the default, nothing changes.",
        Choice::ExportForAnExpert => "Export it for an expert: Sovereign asks her before anything leaves.",
        Choice::Delete => "Delete it: Sovereign asks her to confirm first.",
    }
    .to_string()
}

/// DG1, several quarantines: the fixed sentence that names an earlier quarantine by its number and
/// date.
pub fn earlier_quarantine_sentence(earlier: &EarlierQuarantine) -> String {
    format!(
        "An earlier log was also quarantined: number {}, on {}.",
        earlier.number,
        day_and_time(earlier.date)
    )
}

/// DG3: what remains readable, for the person's read-only window only. No `Debug`, no
/// `Display`: it is not meant to become text anywhere else.
pub struct Readable {
    /// The entries before the break that still verify (for a log in the v0.0.12 format: that
    /// still decrypt), in order, each as it is shown: plain text, with control and
    /// direction-changing characters replaced by visible marks.
    pub entries: Vec<String>,
    /// The entries from the break on, the breaking entry among them: counted, not shown. Shown
    /// and counted make the number of entries in the quarantined log.
    pub after_the_break: u64,
    /// True for a log in the v0.0.12 format: the window then says
    /// [`order_not_guaranteed_sentence`].
    pub order_not_guaranteed: bool,
}

/// DG3: what the window says for a log in the v0.0.12 format, that the entries' order and
/// completeness are not guaranteed.
pub fn order_not_guaranteed_sentence() -> &'static str {
    "These entries come from the older format: their order and completeness are not guaranteed."
}

/// DG4, Export for an expert (the Fable maille, by delegation of Céline, 2026-11-18): makes the
/// folder `to`, which must not exist yet (if it does, the export is refused and nothing is
/// written), and writes into it the files of the persona's most recent quarantine as they are on
/// disk, nothing decrypted,
/// and beside them one text file with what `diagnosis` says for DG1 and DG2 and the sentence
/// « Sealed entries can be read only with this persona's key, which Sovereign does not export. ».
/// The quarantine is left as it was. The host asks her before calling it (a Transmit action).
///
/// Signature by the test author of C-LOG's host lot; the body is the implementer's.
pub fn export_for_an_expert(paths: &OrchestratorStatePaths, diagnosis: &Diagnosis, to: &Path) -> anyhow::Result<()> {
    let Some((_, _, latest)) = quarantines(paths).pop() else { anyhow::bail!("there is no quarantined log to export") };
    // `create_dir`, not `create_dir_all`: a folder that exists is refused, and nothing is written.
    std::fs::create_dir(to).map_err(|e| anyhow::anyhow!("the export folder could not be made: {e}"))?;
    let written = copy_as_they_are(&latest, to).and_then(|()| {
        std::fs::write(to.join(EXPERT_TEXT_FILE), diagnosis.for_an_expert())
            .map_err(|e| anyhow::anyhow!("the diagnosis could not be written: {e}"))
    });
    if written.is_err() {
        // Half an export is worth less than none: what this call made, it removes.
        let _ = std::fs::remove_dir_all(to);
    }
    written
}

/// The name of the text file Export writes beside the quarantine's files. No file of a log has
/// this name.
pub const EXPERT_TEXT_FILE: &str = "What the check found.txt";

/// The sentence on sealed entries, in the text file Export writes (the Fable maille, by delegation
/// of Céline, 2026-11-18).
pub const SEALED_SENTENCE: &str = "Sealed entries can be read only with this persona's key, which Sovereign does not export.";

/// Copies the files under `from` into `to`, byte for byte, keeping their names and any folder
/// among them; nothing is read as a log, nothing decrypted. One name changes: a beacon is
/// exported under the same name whichever persona's it is, so the export names no persona
/// (Céline, 2026-11-20). A file is never written over another: two files that would take one
/// name make the copy fail, and the caller removes what it made.
fn copy_as_they_are(from: &Path, to: &Path) -> anyhow::Result<()> {
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let target = to.join(exported_name(&e.file_name()));
        let kind = e.file_type()?;
        if kind.is_dir() {
            std::fs::create_dir(&target)?;
            copy_as_they_are(&e.path(), &target)?;
        } else if kind.is_file() {
            let mut source = std::fs::File::open(e.path())?;
            let mut copy = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|err| anyhow::anyhow!("{}: {err}", target.display()))?;
            std::io::copy(&mut source, &mut copy)?;
            copy.sync_all()?;
        }
    }
    Ok(())
}

/// The name a quarantined file takes in the export: a persona's beacon takes the one beacon name,
/// every other file keeps its own.
fn exported_name(name: &std::ffi::OsStr) -> std::ffi::OsString {
    use sovereign_core::auth::{session_log_beacon_path, PersonaKind};
    let beacon = |persona| session_log_beacon_path(Path::new(""), persona).file_name().map(|n| n.to_os_string());
    match (beacon(PersonaKind::Duress), beacon(PersonaKind::Primary)) {
        (Some(duress), Some(neutral)) if name == duress => neutral,
        _ => name.to_os_string(),
    }
}

impl Diagnosis {
    /// What Export's text file holds: DG1's and DG2's sentences, the earlier quarantines named,
    /// and [`SEALED_SENTENCE`]. Made from the same fixed sentences as [`Self::for_the_model`],
    /// never a string taken from the disk; it offers no choice.
    pub fn for_an_expert(&self) -> String {
        let mut out = String::from(
            "A Sovereign conversation log, quarantined at a login because its check failed. Its files are in this folder as they were on disk.\n\nWhat the check found:\n",
        );
        for f in &self.found {
            out.push_str("- ");
            out.push_str(&finding_sentence(*f));
            out.push('\n');
        }
        out.push('\n');
        match self.when.last_good_verification {
            Some(t) => out.push_str(&format!("The log was last recorded intact on {}.\n", day_and_time(t))),
            None => out.push_str("There is no record of the log being intact.\n"),
        }
        if let Some(t) = self.when.files_last_changed {
            out.push_str(&format!(
                "The files were last changed on {}, {ACCORDING_TO_THE_FILES}.\n",
                day_and_time(t)
            ));
        }
        for e in &self.earlier {
            out.push_str(&earlier_quarantine_sentence(e));
            out.push('\n');
        }
        out.push('\n');
        out.push_str(SEALED_SENTENCE);
        out.push('\n');
        out
    }
}

/// DG4, Delete (the Fable maille, by delegation of Céline, 2026-11-18): removes the persona's most
/// recent quarantine, the one the diagnosis is of, and nothing else: the live log, its record and
/// the earlier quarantines stay. The next most recent, if any, is then the one diagnosed. The host
/// asks her to confirm before calling it.
///
/// Signature by the test author of C-LOG's host lot; the body is the implementer's.
pub fn delete_the_quarantine(paths: &OrchestratorStatePaths) -> anyhow::Result<()> {
    let Some((_, _, latest)) = quarantines(paths).pop() else { anyhow::bail!("there is no quarantined log to delete") };
    std::fs::remove_dir_all(&latest).map_err(|e| anyhow::anyhow!("the quarantined log could not be deleted: {e}"))
}

/// The persona's quarantines in its log directory, oldest first, each with its time, read from
/// its name (`quarantine-` and the UTC time to the second, with `-1`, `-2`… after it when that
/// name existed). A directory whose name holds no time is left out.
fn quarantines(paths: &OrchestratorStatePaths) -> Vec<(DateTime<Utc>, u32, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(&paths.dir) else { return Vec::new() };
    let mut out: Vec<(DateTime<Utc>, u32, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            let rest = name.strip_prefix("quarantine-")?;
            let stamp = rest.get(..16)?;
            let date = NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%SZ").ok()?.and_utc();
            let suffix = match rest.get(16..) {
                None | Some("") => 0,
                Some(s) => s.strip_prefix('-').and_then(|n| n.parse::<u32>().ok()).unwrap_or(u32::MAX),
            };
            Some((date, suffix, e.path()))
        })
        .collect();
    out.sort();
    out
}

/// The files of a quarantine, under the names the quarantine gave them.
fn files_of(quarantine: &Path, paths: &OrchestratorStatePaths) -> LogFiles {
    let beacon_name = paths.beacon.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "session_log.beacon".into());
    LogFiles {
        log: quarantine.join(LOG_FILE),
        anchor: quarantine.join(ANCHOR_FILE),
        beacon: Some(quarantine.join(beacon_name)),
    }
}

/// The latest time any of the log's files in the quarantine was changed.
fn files_last_changed(quarantine: &Path) -> Option<DateTime<Utc>> {
    std::fs::read_dir(quarantine)
        .ok()?
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
        .map(DateTime::<Utc>::from)
}

/// The diagnosis of the most recent quarantine in the persona's log directory (`paths.dir`),
/// under the persona's log key, the earlier ones named. `Ok(None)` when the persona has no quarantine. Reads the
/// persona's own quarantine and record only (DG6), and changes nothing on disk.
pub fn diagnose(paths: &OrchestratorStatePaths, key: &[u8; 32]) -> anyhow::Result<Option<Diagnosis>> {
    let mut all = quarantines(paths);
    let Some((_, _, latest)) = all.pop() else { return Ok(None) };
    let files = files_of(&latest, paths);
    let checked = check(&files, key, 0);
    let earlier = all
        .iter()
        .enumerate()
        .map(|(i, (date, _, _))| EarlierQuarantine { number: u32::try_from(i + 1).unwrap_or(u32::MAX), date: *date })
        .collect();
    Ok(Some(Diagnosis {
        found: checked.found,
        when: When {
            last_good_verification: checked.record.map(|r| r.verified_at),
            files_last_changed: files_last_changed(&latest),
        },
        choices: vec![Choice::Keep, Choice::ExportForAnExpert, Choice::Delete],
        default_choice: Choice::Keep,
        earlier,
    }))
}

/// What remains readable of the same quarantine, for the person only (DG3). `Ok(None)` when the
/// persona has no quarantine. Reads the persona's own quarantine and record only (DG6), and
/// changes nothing on disk.
pub fn readable_for_the_person(paths: &OrchestratorStatePaths, key: &[u8; 32]) -> anyhow::Result<Option<Readable>> {
    let Some((_, _, latest)) = quarantines(paths).pop() else { return Ok(None) };
    let checked = check(&files_of(&latest, paths), key, usize::MAX);
    let entries: Vec<String> = checked.readable.iter().map(|json| shown(json)).collect();
    Ok(Some(Readable {
        after_the_break: checked.entries.saturating_sub(entries.len() as u64),
        entries,
        order_not_guaranteed: checked.old_format,
    }))
}

/// One entry as the window shows it: its time, its kind and its words, plain text.
fn shown(json: &str) -> String {
    let text = match serde_json::from_str::<SessionEntry>(json) {
        Ok(e) => {
            let words = match (e.content, e.action, e.details) {
                (Some(c), _, _) => c,
                (None, Some(a), Some(d)) => format!("{a}: {d}"),
                (None, Some(a), None) => a,
                (None, None, Some(d)) => d,
                (None, None, None) => String::new(),
            };
            format!("{} {}: {}", e.ts, e.entry_type, words)
        }
        Err(_) => json.to_string(),
    };
    with_visible_marks(&text)
}

/// DG3: control characters (newline and tab aside) and direction-changing characters are
/// shown as visible marks, never applied; a tab is shown as a space, a newline as a line break.
fn with_visible_marks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            c if c.is_control() || is_direction_changing(c) => out.push_str(&format!("[U+{:04X}]", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The characters that change the direction of the text around them: the marks (ALM, LRM,
/// RLM), the embeddings and overrides, and the isolates.
fn is_direction_changing(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}
