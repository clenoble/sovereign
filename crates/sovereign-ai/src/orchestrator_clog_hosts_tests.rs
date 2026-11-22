//! Contract C-LOG, the host lot's library side (spec ch. 08, DG4 and DG6; the Fable maille, by
//! delegation of Céline, 2026-11-18, on what Export gives, which quarantine the choices act on, and
//! where the window opens). Written before the code by its test author, against the signatures of
//! `log_diagnosis::export_for_an_expert` and `log_diagnosis::delete_the_quarantine`; whoever builds
//! the lot makes these pass without editing this file.
//!
//! On `clog_tests`' fixtures (v1), unchanged: a log is quarantined the way the hosts do it, at a
//! login.
//!
//! - **Export** gives the most recent quarantine's files as they are on disk, nothing decrypted,
//!   with one text file beside them: the diagnosis's sentences for DG1 and DG2, and the sentence on
//!   sealed entries. A log from before encryption leaves as it was found, readable.
//! - **Delete** removes the most recent quarantine and nothing else; the next one is then the one
//!   diagnosed.
//! - **DG6:** each acts on the persona's own quarantine only.
//! - **DG4, « never chooses »:** a model that calls a tool to export or delete the log changes
//!   nothing.

use super::clog_tests::*;
use super::*;
use crate::log_diagnosis::{self as dg, Diagnosis, Finding};
use sovereign_core::auth::PersonaKind;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const P: PersonaKind = PersonaKind::Primary;
const D: PersonaKind = PersonaKind::Duress;
const DURESS_KEY: [u8; 32] = [0x52; 32];

/// The sentence the export's text file carries beside the diagnosis (the Fable maille, by
/// delegation of Céline, 2026-11-18).
const SEALED_SENTENCE: &str = "Sealed entries can be read only with this persona's key, which Sovereign does not export.";

fn diagnosis(a: &Area, persona: PersonaKind, key: [u8; 32]) -> Diagnosis {
    dg::diagnose(&a.paths(persona), &key)
        .expect("the diagnosis runs")
        .expect("the persona has a quarantine, so it has a diagnosis")
}

/// Five turns whose text holds `tag`, the 3rd and 4th lines swapped: the chain breaks at entry 3
/// of 5, and the log is quarantined at the next login.
fn swapped(a: &Area, persona: PersonaKind, key: [u8; 32], tag: &str) {
    sealed_log(a, persona, key, tag, 5);
    let log = a.log(persona);
    let mut l = lines(&log);
    assert_eq!(l.len(), 5, "fixture: five entries");
    l.swap(2, 3);
    write_lines(&log, &l);
    let q = quarantines(&a.dir(persona)).len();
    let after = login(a, persona, key);
    assert_failed(a, persona, q, &after, "fixture: two entries swapped");
}

/// Three turns under another key, then a login under the persona's own: quarantined.
fn under_another_key(a: &Area, tag: &str) {
    let q = quarantines(&a.dir(P)).len();
    login(a, P, STRANGER_KEY).write(&turns(tag, 3));
    let after = login(a, P, KEY);
    assert_failed(a, P, q, &after, "fixture: a log under another key");
}

/// The place of an export's new folder, not made yet (Export never writes into a folder that
/// exists), removed at the end.
struct Folder(PathBuf);

impl Folder {
    fn new(tag: &str) -> Folder {
        let p = std::env::temp_dir().join(format!("sov-clog-export-{tag}-{}-{}", std::process::id(), nanos()));
        let _ = fs::remove_dir_all(&p);
        Folder(p)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The files under `root`, by file name, with their bytes.
fn by_name(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = files_under(root, &[])
        .into_iter()
        .map(|(p, b)| (p.file_name().unwrap().to_string_lossy().into_owned(), b))
        .collect();
    v.sort();
    v
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// Everything under the area, but the export folder: the live log, the record, the quarantines.
fn everything(a: &Area) -> BTreeMap<PathBuf, Vec<u8>> {
    files_under(&a.base, &[])
}

// ── Export ────────────────────────────────────────────────────────────────

/// Export gives the quarantine's files as they are on disk, each under its name and with its
/// bytes, and one text file more: the diagnosis's sentences (DG1, and DG2's « according to the
/// files » when the files' time is known) and the sentence on sealed entries. Nothing is
/// decrypted: the turns' text, which holds a marker, is in no exported file. No directory name
/// leaves in the text file. Nothing of the area changes.
#[tokio::test]
async fn export_gives_the_files_as_they_are_and_the_diagnosis_beside_them() {
    // DG4, Export
    let a = area("hosts-export").await;
    let marker = format!("EXPORTMARK{}", nanos());
    swapped(&a, P, KEY, &marker);
    let q = quarantines(&a.dir(P));
    assert_eq!(q.len(), 1, "fixture: one quarantine");
    let quarantined = by_name(&q[0]);
    assert!(!quarantined.is_empty(), "fixture: the quarantine holds files");
    let d = diagnosis(&a, P, KEY);
    let before = everything(&a);

    let to = Folder::new("as-they-are");
    dg::export_for_an_expert(&a.paths(P), &d, &to.0).expect("the export runs");

    let exported = by_name(&to.0);
    for (name, bytes) in &quarantined {
        assert!(
            exported.iter().any(|(n, b)| n == name && b == bytes),
            "the quarantine's {name} is exported as it is on disk; exported: {:?}",
            exported.iter().map(|(n, b)| (n, b.len())).collect::<Vec<_>>()
        );
    }
    let others: Vec<&(String, Vec<u8>)> =
        exported.iter().filter(|(n, b)| !quarantined.iter().any(|(qn, qb)| qn == n && qb == b)).collect();
    assert_eq!(
        others.len(),
        1,
        "one file beside the quarantine's: the diagnosis's text; the others are {:?}",
        others.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    let text = String::from_utf8(others[0].1.clone()).expect("the diagnosis's file is text");
    for f in &d.found {
        let s = dg::finding_sentence(*f);
        assert!(text.contains(&s), "DG1's sentence « {s} » is in the text file:\n{text}");
    }
    if d.when.files_last_changed.is_some() {
        assert!(text.contains(dg::ACCORDING_TO_THE_FILES), "DG2: the files' time, « according to the files »:\n{text}");
    }
    assert!(text.contains(SEALED_SENTENCE), "the sentence on sealed entries:\n{text}");
    assert!(!text.contains("quarantine-"), "no directory name in the text file:\n{text}");

    for (name, bytes) in &exported {
        assert!(!contains(bytes, &marker), "{name}: nothing decrypted leaves, and the turns' text is sealed");
    }
    assert_eq!(everything(&a), before, "the export changes nothing in the persona's area");
}

/// A log written before encryption is quarantined as it was found (LG3): its lines are plain text
/// on disk, and Export gives them as they are, readable. That is why the question the host asks
/// says so, always (the host tests hold the question).
#[tokio::test]
async fn export_of_a_log_from_before_encryption_carries_its_lines_as_they_are() {
    // DG4, Export, with LG3
    let a = area("hosts-export-plain").await;
    let dir = a.dir(P);
    let marker = format!("PLAINEXPORT{}", nanos());
    {
        let mut log = crate::session_log::SessionLog::open(&dir).unwrap();
        log.log_user_input("chat", &marker, "chat");
    }
    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "fixture: a log written only in plaintext");
    let d = diagnosis(&a, P, KEY);
    assert!(d.found.contains(&Finding::HoldsPlaintext), "fixture: the diagnosis says plaintext: {:?}", d.found);

    let to = Folder::new("plain");
    dg::export_for_an_expert(&a.paths(P), &d, &to.0).expect("the export runs");
    let exported = by_name(&to.0);
    assert!(
        exported.iter().any(|(_, b)| contains(b, &marker)),
        "the plaintext line leaves as it was found, readable: {:?}",
        exported.iter().map(|(n, b)| (n, b.len())).collect::<Vec<_>>()
    );
    let text = exported.iter().map(|(_, b)| String::from_utf8_lossy(b).into_owned()).collect::<String>();
    assert!(text.contains(SEALED_SENTENCE), "the sentence on sealed entries is there too");
}

/// Export never writes into a folder that exists (the Fable maille, by delegation of Céline,
/// 2026-11-18): it is refused, and the folder keeps exactly what it held.
#[tokio::test]
async fn export_into_a_folder_that_exists_is_refused_and_writes_nothing() {
    // DG4, Export
    let a = area("hosts-export-exists").await;
    swapped(&a, P, KEY, "exists");
    let d = diagnosis(&a, P, KEY);
    let to = Folder::new("exists");
    fs::create_dir_all(&to.0).unwrap();
    fs::write(to.0.join("hers.txt"), b"a file of hers").unwrap();
    let before = by_name(&to.0);
    assert!(dg::export_for_an_expert(&a.paths(P), &d, &to.0).is_err(), "an export into a folder that exists is refused");
    assert_eq!(by_name(&to.0), before, "and writes nothing there");
}

// ── Delete ────────────────────────────────────────────────────────────────

/// Delete removes the most recent quarantine, the one diagnosed, and nothing else: the earlier
/// quarantine, the live log and the record keep every byte. The earlier one is then the one
/// diagnosed, with no earlier one named.
#[tokio::test]
async fn delete_removes_the_most_recent_quarantine_and_nothing_else() {
    // DG4, Delete; DG1, several quarantines
    let a = area("hosts-delete").await;
    under_another_key(&a, "first");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    swapped(&a, P, KEY, "second");
    let q = quarantines(&a.dir(P));
    assert_eq!(q.len(), 2, "fixture: two quarantines");
    let d = diagnosis(&a, P, KEY);
    assert!(d.found.contains(&Finding::ChainBreaks { at: 3, of: 5 }), "fixture: the most recent is diagnosed: {:?}", d.found);
    let latest = q.iter().find(|p| is_the_swapped_one(p)).cloned().expect("fixture: the diagnosed quarantine is found");
    let rest: BTreeMap<PathBuf, Vec<u8>> =
        everything(&a).into_iter().filter(|(p, _)| !p.starts_with(&latest)).collect();

    dg::delete_the_quarantine(&a.paths(P)).expect("the deletion runs");

    assert!(!latest.exists(), "the most recent quarantine is gone");
    assert_eq!(quarantines(&a.dir(P)).len(), 1, "one quarantine is left");
    assert_eq!(everything(&a), rest, "the earlier quarantine, the live log and the record keep every byte");
    let next = diagnosis(&a, P, KEY);
    assert!(next.found.contains(&Finding::EntriesDoNotDecrypt), "the earlier one is now diagnosed: {:?}", next.found);
    assert!(next.earlier.is_empty(), "no earlier one is named any more: {:?}", next.earlier);
}

/// The quarantine whose files the diagnosis's finding comes from: the swapped one's log has five
/// lines, the other's three.
fn is_the_swapped_one(quarantine: &Path) -> bool {
    lines(&quarantine.join("session_log.jsonl")).len() == 5
}

/// Deleting the only quarantine leaves none: no diagnosis, nothing to show.
#[tokio::test]
async fn delete_of_the_only_quarantine_leaves_nothing_to_diagnose() {
    // DG4, Delete
    let a = area("hosts-delete-only").await;
    swapped(&a, P, KEY, "only");
    dg::delete_the_quarantine(&a.paths(P)).expect("the deletion runs");
    assert!(quarantines(&a.dir(P)).is_empty(), "no quarantine is left");
    assert!(dg::diagnose(&a.paths(P), &KEY).expect("it runs").is_none(), "nothing to diagnose");
    assert!(dg::readable_for_the_person(&a.paths(P), &KEY).expect("it runs").is_none(), "nothing to show");
}

// ── DG6 ───────────────────────────────────────────────────────────────────

/// Under C-AREA: the duress persona's Export and Delete reach its own quarantine only. The
/// primary's quarantine keeps every byte, and none of its files is exported.
#[tokio::test]
async fn export_and_delete_reach_the_personas_own_quarantine_only() {
    // DG6
    let a = area("hosts-dg6").await;
    swapped(&a, P, KEY, "primary");
    swapped(&a, D, DURESS_KEY, "duress");
    let primary_q = quarantines(&a.dir(P));
    assert_eq!(primary_q.len(), 1, "fixture: the primary's quarantine");
    let primary_files = by_name(&primary_q[0]);
    let primary_before = files_under(&primary_q[0], &[]);

    let d = diagnosis(&a, D, DURESS_KEY);
    let to = Folder::new("dg6");
    dg::export_for_an_expert(&a.paths(D), &d, &to.0).expect("the export runs");
    for (name, bytes) in by_name(&to.0) {
        assert!(
            !primary_files.iter().any(|(_, b)| *b == bytes),
            "{name}: a file of the primary's quarantine left in the duress persona's export"
        );
    }

    dg::delete_the_quarantine(&a.paths(D)).expect("the deletion runs");
    assert!(quarantines(&a.dir(D)).is_empty(), "the duress persona's quarantine is gone");
    assert_eq!(quarantines(&a.dir(P)), primary_q, "the primary's quarantine is still there");
    assert_eq!(files_under(&primary_q[0], &[]), primary_before, "and keeps every byte");
}

// ── The export names no persona (v0.0.13 rc2, Céline 2026-11-20) ──────────

/// The relative paths of every file under `root`, sorted.
fn relative_names(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = files_under(root, &[])
        .into_keys()
        .map(|p| p.strip_prefix(root).expect("under the root").to_string_lossy().replace('\\', "/"))
        .collect();
    v.sort();
    v
}

/// Export for an expert gives the same names whichever persona's quarantine it holds: the duress
/// persona's beacon leaves under the primary's name, `session_log.beacon`, and no exported name
/// says « duress ». The bytes are unchanged: each file of the duress quarantine is in the export
/// with its bytes, under its own name or, for the beacon, the neutral one. On disk, the quarantine
/// keeps its names (ch. 08).
#[tokio::test]
async fn the_export_names_no_persona_whichever_quarantine_it_holds() {
    let a = area("hosts-export-names").await;
    swapped(&a, P, KEY, "primary");
    swapped(&a, D, DURESS_KEY, "duress");
    let duress_q = quarantines(&a.dir(D));
    assert_eq!(duress_q.len(), 1, "fixture: the duress persona's quarantine");
    let on_disk = by_name(&duress_q[0]);
    assert!(
        on_disk.iter().any(|(n, _)| n == "session_log.duress.beacon"),
        "fixture: the duress quarantine holds its beacon under its own name on disk; it holds {:?}",
        on_disk.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );

    let to_p = Folder::new("names-primary");
    let to_d = Folder::new("names-duress");
    dg::export_for_an_expert(&a.paths(P), &diagnosis(&a, P, KEY), &to_p.0).expect("the primary's export runs");
    dg::export_for_an_expert(&a.paths(D), &diagnosis(&a, D, DURESS_KEY), &to_d.0).expect("the duress export runs");

    let names_p = relative_names(&to_p.0);
    let names_d = relative_names(&to_d.0);
    assert_eq!(names_d, names_p, "the two exports carry the same names");
    for n in &names_d {
        assert!(!n.to_lowercase().contains("duress"), "an exported name says the persona: {n}");
    }

    let exported = by_name(&to_d.0);
    for (name, bytes) in &on_disk {
        let out_name = if name == "session_log.duress.beacon" { "session_log.beacon" } else { name.as_str() };
        assert!(
            exported.iter().any(|(n, b)| n == out_name && b == bytes),
            "the duress quarantine's {name} is exported as {out_name}, byte for byte"
        );
    }
    assert!(
        by_name(&quarantines(&a.dir(D))[0]).iter().any(|(n, _)| n == "session_log.duress.beacon"),
        "on disk, the quarantine keeps its own names"
    );
}

/// Two files never take one name in an export: a duress quarantine that holds a file named
/// `session_log.beacon` beside its own beacon (put there from outside the app) is not exported.
/// The export is refused and its folder does not exist after it, as for a folder that exists;
/// the quarantine keeps every byte.
#[tokio::test]
async fn an_export_where_two_files_would_take_one_name_is_refused_and_writes_nothing() {
    let a = area("hosts-export-collide").await;
    swapped(&a, D, DURESS_KEY, "duress");
    let q = quarantines(&a.dir(D));
    assert_eq!(q.len(), 1, "fixture: the duress persona's quarantine");
    assert!(q[0].join("session_log.duress.beacon").exists(), "fixture: the duress beacon is in its quarantine");
    fs::write(q[0].join("session_log.beacon"), b"planted").expect("fixture: a planted file");
    let before = files_under(&q[0], &[]);

    let d = diagnosis(&a, D, DURESS_KEY);
    let to = Folder::new("collide");
    assert!(
        dg::export_for_an_expert(&a.paths(D), &d, &to.0).is_err(),
        "an export where two files would take one name is refused"
    );
    assert!(!to.0.exists(), "nothing is written: the export's folder does not exist");
    assert_eq!(files_under(&q[0], &[]), before, "the quarantine keeps every byte");
}

// ── DG4: the model never chooses ──────────────────────────────────────────

/// A scripted model that, asked about the log, calls tools to export and to delete it, under the
/// names a model would guess, then says it did. Nothing it calls carries out a choice: the
/// quarantine keeps every byte, nothing is written outside it, and no action is proposed for her
/// approval by the model.
#[tokio::test]
async fn a_model_that_calls_for_an_export_or_a_deletion_changes_nothing() {
    // DG4: « The assistant offers these choices and never chooses »
    use super::assistant_harness::{world, Script};
    let marker = format!("NEVERCHOOSES{}", nanos());
    let guesses = [
        "delete_quarantined_log",
        "delete_the_quarantine",
        "delete_log",
        "export_for_an_expert",
        "export_quarantined_log",
        "export_log",
    ];
    let expert = std::env::temp_dir().join(format!("sov-clog-expert-{marker}"));
    let expert_text = expert.to_string_lossy().into_owned();
    // The first call is the diagnosis tool, which the loop offers, so the loop goes on and reads the
    // guesses after it.
    let script: Script = Arc::new(move |_: &str, _: u32, i: usize| match i.checked_sub(1).map(|j| guesses.get(j)) {
        None => format!(
            "<tool_call>\n{}\n</tool_call>",
            serde_json::json!({ "name": dg::DIAGNOSIS_TOOL, "arguments": {} })
        ),
        Some(Some(name)) => format!(
            "<tool_call>\n{}\n</tool_call>",
            serde_json::json!({ "name": name, "arguments": { "to": expert_text, "confirmed": true } })
        ),
        Some(None) => "Done: I deleted it.".to_string(),
    });
    let w = world(&format!("never-{marker}"), 4096, script).await;
    let dir = w.orch.profile_dir.clone();
    let paths = OrchestratorStatePaths { dir: dir.clone(), beacon: dir.with_extension("beacon") };
    w.orch.set_session_log_key_for(&paths, KEY);
    for t in turns(&marker, 3) {
        w.orch.session_log.with_log(|l| l.log_user_input("chat", &t, "chat"));
    }
    append(&dir.join("session_log.jsonl"), b"not a sealed line\n");
    w.rec.take_events();
    w.orch.set_session_log_key_for(&paths, KEY);
    assert!(
        session_notices(&w.rec.take_events()).iter().any(|n| n.starts_with(NOTICE_START)),
        "fixture: the log is quarantined"
    );
    let qs = quarantines(&dir);
    assert_eq!(qs.len(), 1, "fixture: one quarantine");
    let before = files_under(&qs[0], &[]);

    // Her question goes straight to the chat loop while a quarantine exists (decided 2026-11-18),
    // so every call below is the chat loop's, with its tools.
    let _ = w.orch.handle_chat("what happened to my log?").await;
    assert!(w.rec.calls().len() >= 2, "fixture: the model was asked, and its tool calls were read");
    let events = w.rec.take_events();
    assert_eq!(quarantines(&dir), qs, "the quarantine is still there");
    assert_eq!(files_under(&qs[0], &[]), before, "and keeps every byte");
    assert!(!expert.exists(), "nothing was exported where the model said");
    assert!(
        !events.iter().any(|e| matches!(e, OrchestratorEvent::ActionProposed { .. })),
        "the model proposes no choice for her approval: {events:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
