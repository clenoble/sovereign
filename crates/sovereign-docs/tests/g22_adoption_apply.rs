//! G22 — adoption gestures: applying a preview.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G22 (specified
//! 2026-10-15): "**Applying a preview adopts exactly the documents it
//! listed** that are still candidates — never one that arrived or became a
//! candidate after the preview — and reports the others as skipped. Each
//! adopted document gets its own trust decision (owned, this device, the time
//! of the apply), which travels with sync (G12), and one `TrustChanged` event;
//! nothing else about it changes. Each document is adopted whole or not at all:
//! after a failure at any point, every previewed document is either adopted —
//! trust, decision and the fence flag together — or as it was, and applying
//! again completes the rest. A gesture is never a standing rule. Adoption only
//! raises trust; lowering stays a per-document `set_trust`. An external
//! document is adopted only when the user picks it (one document, or an
//! explicit selection): a thread or folder gesture leaves it fenced."
//! (Céline, 2026-10-15.)
//!
//! Ch. 04 (decided 2026-10-15): "the gesture applies to the documents selected
//! *now*; it never becomes a standing rule for a thread, a folder or a date
//! range".

mod common;

use common::adoption::*;
use common::*;
use sovereign_docs::{
    AdoptionSelector, AdoptionSkip, ChangeKind, Provenance, Trust, TrustDecision,
};

async fn is_adopted(s: &Stack, id: &str) -> bool {
    let v = s.svc.read(id).await.unwrap();
    v.trust == Trust::Owned
        && matches!(&v.trust_decision, Some(TrustDecision { trust: Trust::Owned, device, .. }) if device == DEVICE)
        && s.stored_doc(id).await.is_owned
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_apply_adopts_exactly_the_previewed_documents_each_with_its_own_decision() {
    let c = corpus("g22-apply").await;
    let s = &c.s;
    let p = s.svc.preview_adoption(AdoptionSelector::Thread("thread:a".into())).await.unwrap();
    s.clock.advance(chrono::Duration::minutes(3));
    let report = s.svc.apply_adoption(&p).await.unwrap();
    assert_eq!(sorted(report.adopted.clone()), ids(&p));
    assert!(report.skipped.is_empty());
    for id in ids(&p) {
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!(v.trust, Trust::Owned);
        assert_eq!(
            v.trust_decision,
            Some(TrustDecision { trust: Trust::Owned, device: DEVICE.into(), decided_at: s.clock.get() }),
            "its own recorded decision"
        );
        assert!(s.stored_doc(&id).await.is_owned);
    }
    for id in [&c.other_import, &c.windows_import, &c.trashed] {
        assert_ne!(s.svc.read(id).await.unwrap().trust, Trust::Owned, "{id} untouched");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_a_document_arriving_after_the_preview_is_not_adopted() {
    let c = corpus("g22-late").await;
    let s = &c.s;
    let p = s.svc.preview_adoption(AdoptionSelector::Thread("thread:a".into())).await.unwrap();
    let p_folder = s.svc.preview_adoption(AdoptionSelector::ImportFolder("/in/corpus".into())).await.unwrap();
    let p_date = s.svc.preview_adoption(AdoptionSelector::CreatedBefore(at(2020))).await.unwrap();
    // Arrives after the previews: same thread, same folder, old date.
    let late = make(s, "late", "thread:a", Trust::Undetermined, import("/in/corpus/late.md"), 2001, 2001).await;
    for preview in [&p, &p_folder, &p_date] {
        let report = s.svc.apply_adoption(preview).await.unwrap();
        assert!(!report.adopted.contains(&late));
    }
    assert_eq!(s.svc.read(&late).await.unwrap().trust, Trust::Undetermined);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_a_gesture_is_never_a_standing_rule() {
    let c = corpus("g22-standing").await;
    let s = &c.s;
    for sel in [
        AdoptionSelector::Thread("thread:a".into()),
        AdoptionSelector::ImportFolder("/in/corpus".into()),
        AdoptionSelector::CreatedBefore(at(2020)),
        AdoptionSelector::ModifiedBefore(at(2020)),
    ] {
        let p = s.svc.preview_adoption(sel.clone()).await.unwrap();
        s.svc.apply_adoption(&p).await.unwrap();
    }
    // Later: new documents in the same thread, folder and date range.
    let later = make(s, "later", "thread:a", Trust::Undetermined, import("/in/corpus/later.md"), 2002, 2002).await;
    s.clock.advance(chrono::Duration::days(30));
    let v = s.svc.read(&later).await.unwrap();
    assert_eq!(v.trust, Trust::Undetermined, "lands undetermined, whatever was adopted before");
    assert_eq!(v.trust_decision, None);
    // A new gesture over the same thread lists it, and only it.
    let p = s.svc.preview_adoption(AdoptionSelector::Thread("thread:a".into())).await.unwrap();
    assert_eq!(ids(&p), vec![later]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_previewed_documents_that_stopped_being_candidates_are_skipped() {
    let c = corpus("g22-skipped").await;
    let s = &c.s;
    let p = s
        .svc
        .preview_adoption(AdoptionSelector::Documents(vec![
            c.old_web.clone(),
            c.new_import.clone(),
            c.old_import.clone(),
        ]))
        .await
        .unwrap();
    s.svc.set_trust(&c.old_web, Trust::Owned).await.unwrap(); // adopted meanwhile
    s.svc.trash(&c.new_import).await.unwrap();
    s.svc.trash(&c.old_import).await.unwrap();
    s.svc.purge_from_trash(&c.old_import).await.unwrap();
    let report = s.svc.apply_adoption(&p).await.unwrap();
    assert!(report.adopted.is_empty(), "{:?}", report.adopted);
    let skip = |id: &str| report.skipped.iter().find(|(d, _)| d == id).map(|(_, r)| r.clone());
    assert_eq!(skip(&c.old_web), Some(AdoptionSkip::AlreadyOwned));
    assert_eq!(skip(&c.new_import), Some(AdoptionSkip::InTrash));
    assert_eq!(skip(&c.old_import), Some(AdoptionSkip::Purged));
    assert_eq!(s.svc.read(&c.new_import).await.unwrap().trust, Trust::Undetermined, "not adopted in the trash");
}

/// Decided by Céline, 2026-10-15: adopting a thread or a folder never moves a
/// fenced external document into the trusted plane; only her own pick does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_thread_and_folder_gestures_leave_external_documents_fenced() {
    let c = corpus("g22-external").await;
    let s = &c.s;
    let ext_import = make(s, "shared", "thread:a", Trust::External, import("/in/corpus/shared.md"), 2014, 2014).await;
    for sel in [
        AdoptionSelector::Thread("thread:a".into()),
        AdoptionSelector::ImportFolder("/in/corpus".into()),
    ] {
        let p = s.svc.preview_adoption(sel.clone()).await.unwrap();
        let report = s.svc.apply_adoption(&p).await.unwrap();
        assert!(!report.adopted.contains(&c.old_web) && !report.adopted.contains(&ext_import), "{sel:?}");
    }
    for id in [&c.old_web, &ext_import] {
        let v = s.svc.read(id).await.unwrap();
        assert_eq!(v.trust, Trust::External, "{id} still fenced");
        assert_eq!(v.trust_decision, None, "no decision recorded on {id}");
        assert!(!s.stored_doc(id).await.is_owned);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_an_explicit_selection_including_an_external_document_adopts_it() {
    let c = corpus("g22-pick").await;
    let s = &c.s;
    let p = s
        .svc
        .preview_adoption(AdoptionSelector::Documents(vec![c.old_web.clone(), c.old_import.clone()]))
        .await
        .unwrap();
    assert_eq!(ids(&p), sorted(vec![c.old_web.clone(), c.old_import.clone()]));
    let report = s.svc.apply_adoption(&p).await.unwrap();
    assert_eq!(sorted(report.adopted), sorted(vec![c.old_web.clone(), c.old_import.clone()]));
    assert!(is_adopted(s, &c.old_web).await, "her own pick adopts an external document");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_one_trust_changed_event_per_adopted_document_and_nothing_else_changes() {
    let c = corpus("g22-events").await;
    let s = &c.s;
    let p = s.svc.preview_adoption(AdoptionSelector::ImportFolder("/in".into())).await.unwrap();
    let mut before = vec![];
    for id in ids(&p) {
        let d = s.stored_doc(&id).await;
        let v = s.svc.read(&id).await.unwrap();
        before.push((d.title, d.content, d.thread_id, d.created_at, d.modified_at, v.provenance));
    }
    s.events.take();
    let report = s.svc.apply_adoption(&p).await.unwrap();
    let mut ev: Vec<_> = s.events.take();
    assert!(ev.iter().all(|e| e.kind == ChangeKind::TrustChanged), "{ev:?}");
    let mut ev_ids: Vec<String> = ev.drain(..).map(|e| e.id).collect();
    ev_ids.sort();
    assert_eq!(ev_ids, sorted(report.adopted.clone()), "exactly one event per adopted document");
    for (id, b) in ids(&p).into_iter().zip(before) {
        let d = s.stored_doc(&id).await;
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!((d.title, d.content, d.thread_id, d.created_at, d.modified_at, v.provenance), b, "{id}");
    }
    // Applying again adopts nothing more and says nothing.
    let again = s.svc.apply_adoption(&p).await.unwrap();
    assert!(again.adopted.is_empty());
    assert!(again.skipped.iter().all(|(_, r)| *r == AdoptionSkip::AlreadyOwned));
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_owned_documents_are_never_touched_by_a_gesture() {
    let c = corpus("g22-owned").await;
    let s = &c.s;
    let before = s.svc.read(&c.owned).await.unwrap().trust_decision;
    let p = s.svc.preview_adoption(AdoptionSelector::Documents(vec![c.owned.clone()])).await.unwrap();
    assert!(p.candidates.is_empty());
    s.svc.apply_adoption(&p).await.unwrap();
    assert_eq!(s.svc.read(&c.owned).await.unwrap().trust_decision, before, "no decision recorded");
}

/// Crash at every point of an apply: each previewed document is adopted whole
/// or left as it was; an event is emitted only for a document wholly adopted;
/// applying again completes the rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_a_crash_anywhere_in_an_apply_leaves_each_document_adopted_or_untouched() {
    let mut completed = false;
    for k in 0..600 {
        let mut c = corpus(&format!("g22-crash-{k}")).await;
        let p = c.s.svc.preview_adoption(AdoptionSelector::Thread("thread:a".into())).await.unwrap();
        let mut before = vec![];
        for id in ids(&p) {
            let v = c.s.svc.read(&id).await.unwrap();
            before.push((id.clone(), v.trust, v.trust_decision, c.s.stored_doc(&id).await.is_owned));
        }
        c.s.events.take();
        c.s.faulty.crash_after(k);
        let _ = c.s.svc.apply_adoption(&p).await;
        let crashed = c.s.faulty.crashed();
        c.s.restart();
        for (id, trust, decision, is_owned) in &before {
            let v = c.s.svc.read(id).await.unwrap();
            let untouched = (v.trust, &v.trust_decision, c.s.stored_doc(id).await.is_owned) == (*trust, decision, *is_owned);
            let adopted = is_adopted(&c.s, id).await;
            assert!(untouched || adopted, "k={k}: {id} half-adopted");
        }
        for e in c.s.events.take() {
            assert_eq!(e.kind, ChangeKind::TrustChanged, "k={k}");
            assert!(is_adopted(&c.s, &e.id).await, "k={k}: event for {} not wholly adopted", e.id);
        }
        c.s.svc.apply_adoption(&p).await.expect("applying again");
        for id in ids(&p) {
            assert!(is_adopted(&c.s, &id).await, "k={k}: {id} not adopted after the second apply");
        }
        if !crashed {
            completed = true;
            break;
        }
    }
    assert!(completed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g22_an_adopted_document_keeps_its_provenance() {
    let c = corpus("g22-provenance").await;
    let s = &c.s;
    let p = s.svc.preview_adoption(AdoptionSelector::Documents(vec![c.old_web.clone()])).await.unwrap();
    s.svc.apply_adoption(&p).await.unwrap();
    assert_eq!(
        s.svc.read(&c.old_web).await.unwrap().provenance,
        Provenance::Web { url: "https://example.org/old".into() },
        "adopting changes whose judgment, not where it came from"
    );
}
