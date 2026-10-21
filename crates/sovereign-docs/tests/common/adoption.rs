//! A small corpus for the adoption gestures (G21, G22).

use chrono::{DateTime, TimeZone, Utc};
use sovereign_docs::{AdoptionPreview, Origin, Provenance, Trust};

use super::{new_text_doc, Stack};

pub struct Corpus {
    pub s: Stack,
    /// Thread A, undetermined, import `/in/corpus/a.md`, created 2015, modified 2016.
    pub old_import: String,
    /// Thread A, undetermined, import `/in/corpus/sub/b.md`, created 2024.
    pub new_import: String,
    /// Thread A, external web clip, created 2012.
    pub old_web: String,
    /// Thread A, owned, written, created 2011.
    pub owned: String,
    /// Thread B, undetermined, import `/in/corpusX/c.md`, created 2010.
    pub other_import: String,
    /// Thread B, undetermined, import `C:\in\win\d.md`, created 2023.
    pub windows_import: String,
    /// Thread A, undetermined, import `/in/corpus/t.md`, created 2013, in the trash.
    pub trashed: String,
}

/// 1 June of year `y`, noon UTC.
pub fn at(y: i32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, 6, 1, 12, 0, 0).unwrap()
}

pub fn import(path: &str) -> Provenance {
    Provenance::Import { source_path: path.into() }
}

pub async fn make(
    s: &Stack,
    title: &str,
    thread: &str,
    trust: Trust,
    provenance: Provenance,
    created: i32,
    modified: i32,
) -> String {
    let mut new = new_text_doc(title, "body");
    new.thread_id = thread.into();
    new.trust = trust;
    new.provenance = provenance;
    new.origin = if trust == Trust::Owned { new.origin } else { Origin::Import };
    new.created_at = Some(at(created));
    new.modified_at = Some(at(modified));
    s.svc.create(new).await.expect("create").id
}

pub async fn corpus(tag: &str) -> Corpus {
    let s = Stack::new(tag).await;
    let old_import = make(&s, "old", "thread:a", Trust::Undetermined, import("/in/corpus/a.md"), 2015, 2016).await;
    let new_import = make(&s, "new", "thread:a", Trust::Undetermined, import("/in/corpus/sub/b.md"), 2024, 2025).await;
    let web = Provenance::Web { url: "https://example.org/old".into() };
    let old_web = make(&s, "web", "thread:a", Trust::External, web, 2012, 2012).await;
    let owned = make(&s, "mine", "thread:a", Trust::Owned, Provenance::Written, 2011, 2011).await;
    let other_import = make(&s, "other", "thread:b", Trust::Undetermined, import("/in/corpusX/c.md"), 2010, 2010).await;
    let windows_import =
        make(&s, "win", "thread:b", Trust::Undetermined, import("C:\\in\\win\\d.md"), 2023, 2023).await;
    let trashed = make(&s, "trashed", "thread:a", Trust::Undetermined, import("/in/corpus/t.md"), 2013, 2013).await;
    s.svc.trash(&trashed).await.unwrap();
    Corpus { s, old_import, new_import, old_web, owned, other_import, windows_import, trashed }
}

/// The previewed ids, sorted.
pub fn ids(p: &AdoptionPreview) -> Vec<String> {
    sorted(p.candidates.iter().map(|c| c.id.clone()).collect())
}

pub fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}
