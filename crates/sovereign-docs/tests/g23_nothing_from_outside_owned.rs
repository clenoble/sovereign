//! G23 — nothing from outside is ever created owned.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G23 (specified
//! 2026-10-15): "**Nothing from outside is created owned.** A document whose
//! provenance is the web or an import cannot be created owned; it lands
//! external or undetermined, and becomes owned only by a user decision (G12,
//! G22). Which of the allowed states a creator asks for — owned for what the
//! user writes from scratch, undetermined for imports and a skill's external
//! file, external for the web — is the host's choice of default, not the
//! service's."
//!
//! Ch. 04 (decided 2026-10-15): "Nothing from outside is ever created owned."

mod common;

use common::*;
use sovereign_docs::{DocError, Origin, Provenance, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g23_web_and_import_content_cannot_be_created_owned() {
    let s = Stack::new("g23-refused").await;
    for provenance in [
        Provenance::Web { url: "https://example.org/".into() },
        Provenance::Import { source_path: "D:/in/a.md".into() },
        Provenance::Import { source_path: "skill:file-import".into() },
    ] {
        let mut new = new_text_doc("Outside", "x");
        new.trust = Trust::Owned;
        new.provenance = provenance.clone();
        new.origin = Origin::Import;
        let r = s.svc.create(new).await;
        assert!(matches!(r, Err(DocError::TrustNotAllowed { trust: Trust::Owned })), "{provenance:?}");
    }
    assert!(s.svc.list(None).await.unwrap().is_empty(), "nothing created");
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g23_outside_content_lands_external_or_undetermined() {
    let s = Stack::new("g23-allowed").await;
    for (trust, provenance) in [
        (Trust::External, Provenance::Web { url: "https://example.org/".into() }),
        (Trust::Undetermined, Provenance::Web { url: "https://example.org/".into() }),
        (Trust::Undetermined, Provenance::Import { source_path: "D:/in/a.md".into() }),
        (Trust::External, Provenance::Import { source_path: "D:/in/b.md".into() }),
    ] {
        let mut new = new_text_doc("Outside", "x");
        new.trust = trust;
        new.provenance = provenance;
        new.origin = Origin::Import;
        let id = s.svc.create(new).await.unwrap().id;
        assert_eq!(s.svc.read(&id).await.unwrap().trust, trust);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g23_what_the_user_or_the_assistant_writes_may_be_created_owned() {
    let s = Stack::new("g23-inside").await;
    for (provenance, origin) in [(Provenance::Written, Origin::Editor { session: sovereign_docs::EditSession(1) }), (Provenance::Assistant, Origin::Assistant)] {
        let mut new = new_text_doc("Inside", "x");
        new.provenance = provenance;
        new.origin = origin;
        let id = s.svc.create(new).await.unwrap().id;
        assert_eq!(s.svc.read(&id).await.unwrap().trust, Trust::Owned);
    }
}
