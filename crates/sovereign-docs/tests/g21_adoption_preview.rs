//! G21 — adoption gestures: the preview.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G21 (specified
//! 2026-10-15): "**An adoption gesture is previewed before anything changes.**
//! The preview lists exactly the documents the gesture would adopt now — those
//! its selector names (a list of documents, a thread, an import folder,
//! original creation or last modification before a date) that are neither in
//! the trash nor owned. **Thread, import-folder and date gestures take
//! undetermined documents only**; an explicit selection of documents takes
//! undetermined and external ones (decided by Céline, 2026-10-15: adopting a
//! thread must never silently move fenced web pages into the trusted plane).
//! Previewing changes nothing. An import folder names the documents whose
//! import path lies inside it, compared by path components. A date gesture's
//! preview says that a date is not proof of authorship."
//!
//! Ch. 04 (decided 2026-10-15): "A preview first — how many documents, and
//! which — before anything changes … A date is not proof of authorship."

mod common;

use common::adoption::*;
use common::*;
use sovereign_docs::{AdoptionSelector, PreviewWarning, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_a_thread_gesture_lists_its_undetermined_documents_only() {
    let c = corpus("g21-thread").await;
    let p = c.s.svc.preview_adoption(AdoptionSelector::Thread("thread:a".into())).await.unwrap();
    assert_eq!(ids(&p), sorted(vec![c.old_import.clone(), c.new_import.clone()]));
    assert!(!ids(&p).contains(&c.old_web), "an external web clip in the thread is not a candidate");
    assert!(!ids(&p).contains(&c.owned), "owned: nothing to adopt");
    assert!(!ids(&p).contains(&c.trashed), "the trash is not a candidate");
    for cand in &p.candidates {
        let v = c.s.svc.read(&cand.id).await.unwrap();
        assert_eq!(cand.trust, v.trust);
        assert_eq!((cand.created_at, cand.modified_at), (v.created_at, v.modified_at));
    }
    assert!(p.warnings.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_an_import_folder_names_the_documents_inside_it_by_path_components() {
    let c = corpus("g21-folder").await;
    let want = sorted(vec![c.old_import.clone(), c.new_import.clone()]);
    for folder in ["/in/corpus", "/in/corpus/", "\\in\\corpus"] {
        let p = c.s.svc.preview_adoption(AdoptionSelector::ImportFolder(folder.into())).await.unwrap();
        assert_eq!(ids(&p), want, "{folder}: corpusX is another folder, the trash is out");
    }
    let p = c.s.svc.preview_adoption(AdoptionSelector::ImportFolder("C:/in/win".into())).await.unwrap();
    assert_eq!(ids(&p), vec![c.windows_import.clone()], "Windows separators");
    let p = c.s.svc.preview_adoption(AdoptionSelector::ImportFolder("/in/corpus/sub".into())).await.unwrap();
    assert_eq!(ids(&p), vec![c.new_import.clone()], "a subfolder names only what is inside it");
    let p = c.s.svc.preview_adoption(AdoptionSelector::ImportFolder("/nowhere".into())).await.unwrap();
    assert!(p.candidates.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_a_folder_gesture_leaves_out_an_external_document_of_that_folder() {
    let c = corpus("g21-folder-external").await;
    let ext = make(&c.s, "ext", "thread:a", Trust::External, import("/in/corpus/shared.md"), 2014, 2014).await;
    let p = c.s.svc.preview_adoption(AdoptionSelector::ImportFolder("/in/corpus".into())).await.unwrap();
    assert!(!ids(&p).contains(&ext), "external, in the folder: not a candidate");
    assert_eq!(ids(&p), sorted(vec![c.old_import.clone(), c.new_import.clone()]));
    for cand in &p.candidates {
        assert_eq!(cand.trust, Trust::Undetermined);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_a_selection_lists_the_candidates_among_the_documents_named() {
    let c = corpus("g21-ids").await;
    let p = c
        .s
        .svc
        .preview_adoption(AdoptionSelector::Documents(vec![
            c.owned.clone(),
            c.old_import.clone(),
            c.old_web.clone(),
            c.trashed.clone(),
            "document:nosuch".into(),
        ]))
        .await
        .unwrap();
    assert_eq!(ids(&p), sorted(vec![c.old_import.clone(), c.old_web.clone()]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_a_date_gesture_takes_undetermined_documents_older_than_the_date() {
    let c = corpus("g21-date").await;
    let p = c.s.svc.preview_adoption(AdoptionSelector::CreatedBefore(at(2020))).await.unwrap();
    assert_eq!(ids(&p), sorted(vec![c.old_import.clone(), c.other_import.clone()]),
        "old external and old owned documents are not taken; the trash is out");
    assert_eq!(p.warnings, vec![PreviewWarning::DateIsNotAuthorship]);
    let p = c.s.svc.preview_adoption(AdoptionSelector::ModifiedBefore(at(2016) + chrono::Duration::days(1))).await.unwrap();
    assert_eq!(ids(&p), sorted(vec![c.old_import.clone(), c.other_import.clone()]));
    assert_eq!(p.warnings, vec![PreviewWarning::DateIsNotAuthorship]);
    // Strictly before: a document created exactly at the date is not older than it.
    let p = c.s.svc.preview_adoption(AdoptionSelector::CreatedBefore(at(2015))).await.unwrap();
    assert_eq!(ids(&p), vec![c.other_import.clone()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g21_previewing_changes_nothing() {
    let c = corpus("g21-pure").await;
    let s = &c.s;
    let all = s.edb.list_documents_including_deleted().await.unwrap();
    let mut before = vec![];
    for d in &all {
        let id = d.id_string().unwrap();
        let v = s.svc.read(&id).await.unwrap();
        before.push((s.stored_doc(&id).await.content, v.trust, v.trust_decision, s.commits(&id).await.len()));
    }
    s.events.take();
    for sel in [
        AdoptionSelector::Thread("thread:a".into()),
        AdoptionSelector::ImportFolder("/in".into()),
        AdoptionSelector::CreatedBefore(at(2030)),
        AdoptionSelector::ModifiedBefore(at(2030)),
        AdoptionSelector::Documents(all.iter().map(|d| d.id_string().unwrap()).collect()),
    ] {
        s.svc.preview_adoption(sel).await.unwrap();
    }
    for (d, b) in all.iter().zip(before) {
        let id = d.id_string().unwrap();
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!((s.stored_doc(&id).await.content, v.trust, v.trust_decision, s.commits(&id).await.len()), b);
    }
    assert!(s.events.peek().is_empty());
}
