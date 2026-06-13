use std::collections::HashSet;
use std::sync::Arc;
use wasmtime::component::bindgen;
use wasmtime::StoreLimits;

use crate::manifest::Capability as ManifestCap;
use crate::traits::SkillDbAccess;

// Generate Rust bindings from the WIT definition.
// This creates the SkillPlugin world type and all Host traits.
bindgen!({
    world: "skill-plugin",
    path: "wit",
});

/// Store state for each WASM skill execution.
/// Contains the DB bridge, the granted capability set, and resource limits.
pub(crate) struct PluginState {
    pub db: Option<Arc<dyn SkillDbAccess>>,
    /// Capabilities granted to THIS execution. Every host function re-checks
    /// against this set — the up-front registry check only validates the
    /// skill's self-declared requirements, so without per-call enforcement a
    /// guest could simply call host functions it never declared (Hard
    /// Barriers principle: constraints enforced by code, not manifests).
    pub granted: HashSet<ManifestCap>,
    pub limits: StoreLimits,
    /// WASM-002: the document the skill was invoked on, and its thread. The
    /// single-scope grants (`ReadDocument`, `WriteDocument`) are confined to
    /// this scope; the corpus grants (`ReadAllDocuments`, `WriteAllDocuments`)
    /// ignore it. Without this, a guest holding only `WriteDocument` could pass
    /// any `thread_id`/`id` and write/read anywhere — making the narrow grant
    /// indistinguishable from the all-documents grant. `None` = no scope set
    /// (e.g. metadata loading, or a skill invoked on an unsaved document), in
    /// which case the narrow grant matches nothing and the guest must instead
    /// hold the corresponding all-documents capability.
    pub scope_doc: Option<String>,
    pub scope_thread: Option<String>,
}

impl PluginState {
    /// Require any one of `caps` to be granted (e.g. a single-document read
    /// is covered by either ReadDocument or ReadAllDocuments).
    fn require_any(&self, caps: &[ManifestCap]) -> Result<(), String> {
        if caps.iter().any(|c| self.granted.contains(c)) {
            Ok(())
        } else {
            Err(format!(
                "capability not granted to this skill: requires one of {caps:?}"
            ))
        }
    }

    /// WASM-002: authorize a scoped operation. Pass if the corpus capability
    /// (`all_cap`) is held; otherwise require the narrow capability (`one_cap`)
    /// AND that `target` equals the in-scope value (`scope`). Returns an error
    /// otherwise — a `WriteDocument`-only skill can't reach another thread, and
    /// a skill with neither capability can't act at all.
    fn require_scoped(
        &self,
        one_cap: ManifestCap,
        all_cap: ManifestCap,
        scope: &Option<String>,
        target: &str,
        what: &str,
    ) -> Result<(), String> {
        if self.granted.contains(&all_cap) {
            return Ok(());
        }
        if self.granted.contains(&one_cap) {
            return match scope {
                Some(s) if s == target => Ok(()),
                Some(_) => Err(format!(
                    "{one_cap:?} is scoped to the current {what}; use {all_cap:?} to act on '{target}'"
                )),
                None => Err(format!(
                    "{one_cap:?} has no {what} scope for this invocation; declare {all_cap:?} to act on '{target}'"
                )),
            };
        }
        Err(format!(
            "capability not granted to this skill: requires {one_cap:?} or {all_cap:?}"
        ))
    }
}

/// The `types` interface only defines types, no functions — empty Host impl required.
impl sovereign::skill::types::Host for PluginState {}

// Implement the host-db interface for PluginState.
// Bridges WASM host function calls to our SkillDbAccess trait.
impl sovereign::skill::host_db::Host for PluginState {
    fn search_documents(
        &mut self,
        query: String,
    ) -> Result<Vec<sovereign::skill::types::SearchResult>, String> {
        // Search spans the whole corpus, not a single document.
        self.require_any(&[ManifestCap::ReadAllDocuments])?;
        let db = self.db.as_ref().ok_or("No DB access available")?;
        db.search_documents(&query)
            .map(|results| {
                results
                    .into_iter()
                    .map(|(id, title, snippet)| sovereign::skill::types::SearchResult {
                        id,
                        title,
                        snippet,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    fn get_document(
        &mut self,
        id: String,
    ) -> Result<sovereign::skill::types::DocDetail, String> {
        // WASM-002: ReadDocument is confined to the invocation's document;
        // ReadAllDocuments may read any document.
        self.require_scoped(
            ManifestCap::ReadDocument,
            ManifestCap::ReadAllDocuments,
            &self.scope_doc,
            &id,
            "document",
        )?;
        let db = self.db.as_ref().ok_or("No DB access available")?;
        db.get_document(&id)
            .map(|(title, thread_id, content)| sovereign::skill::types::DocDetail {
                title,
                thread_id,
                content,
            })
            .map_err(|e| e.to_string())
    }

    fn list_documents(
        &mut self,
        thread_id: Option<String>,
    ) -> Result<Vec<sovereign::skill::types::DocEntry>, String> {
        self.require_any(&[ManifestCap::ReadAllDocuments])?;
        let db = self.db.as_ref().ok_or("No DB access available")?;
        db.list_documents(thread_id.as_deref())
            .map(|docs| {
                docs.into_iter()
                    .map(|(id, title)| sovereign::skill::types::DocEntry { id, title })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    fn create_document(
        &mut self,
        title: String,
        thread_id: String,
        content: String,
    ) -> Result<String, String> {
        // WASM-002: WriteDocument may only create within the invocation's
        // thread; WriteAllDocuments may target any thread. Previously the
        // guest-supplied thread_id was unscoped, so WriteDocument == WriteAll.
        self.require_scoped(
            ManifestCap::WriteDocument,
            ManifestCap::WriteAllDocuments,
            &self.scope_thread,
            &thread_id,
            "thread",
        )?;
        let db = self.db.as_ref().ok_or("No DB access available")?;
        db.create_document(&title, &thread_id, &content)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign::skill::host_db::Host;

    const SCOPE_DOC: &str = "doc:current";
    const SCOPE_THREAD: &str = "thread:current";

    fn state_with(granted: &[ManifestCap]) -> PluginState {
        PluginState {
            db: None,
            granted: granted.iter().cloned().collect(),
            limits: StoreLimits::default(),
            scope_doc: Some(SCOPE_DOC.to_string()),
            scope_thread: Some(SCOPE_THREAD.to_string()),
        }
    }

    #[test]
    fn write_denied_without_write_capability() {
        // A skill that only declared ReadDocument must NOT be able to write,
        // regardless of what its guest code calls.
        let mut s = state_with(&[ManifestCap::ReadDocument]);
        let err = s
            .create_document("t".into(), SCOPE_THREAD.into(), "c".into())
            .unwrap_err();
        assert!(err.contains("capability not granted"), "{err}");
    }

    #[test]
    fn corpus_reads_denied_with_single_doc_grant() {
        let mut s = state_with(&[ManifestCap::ReadDocument]);
        assert!(s.search_documents("x".into()).unwrap_err().contains("capability"));
        assert!(s.list_documents(None).unwrap_err().contains("capability"));
    }

    #[test]
    fn granted_calls_pass_capability_check() {
        // With the right capability + in-scope target the check passes and the
        // call proceeds to the DB layer (errors differently — no DB in tests).
        let mut s = state_with(&[ManifestCap::WriteDocument]);
        let err = s
            .create_document("t".into(), SCOPE_THREAD.into(), "c".into())
            .unwrap_err();
        assert!(err.contains("No DB access"), "{err}");

        let mut s = state_with(&[ManifestCap::ReadAllDocuments]);
        assert!(s.search_documents("x".into()).unwrap_err().contains("No DB access"));
        assert!(s.get_document("doc:1".into()).unwrap_err().contains("No DB access"));
    }

    // --- WASM-002: scope confinement ---

    #[test]
    fn write_document_confined_to_scope_thread() {
        // WriteDocument: in-scope thread passes the cap check (then hits "No DB"),
        // a different thread is refused outright.
        let mut s = state_with(&[ManifestCap::WriteDocument]);
        assert!(s
            .create_document("t".into(), SCOPE_THREAD.into(), "c".into())
            .unwrap_err()
            .contains("No DB access"));
        let err = s
            .create_document("t".into(), "thread:other".into(), "c".into())
            .unwrap_err();
        assert!(err.contains("scoped to the current thread"), "{err}");
    }

    #[test]
    fn write_all_documents_ignores_scope() {
        // WriteAllDocuments may target any thread (passes to the DB layer).
        let mut s = state_with(&[ManifestCap::WriteAllDocuments]);
        assert!(s
            .create_document("t".into(), "thread:anything".into(), "c".into())
            .unwrap_err()
            .contains("No DB access"));
    }

    #[test]
    fn read_document_confined_to_scope_doc() {
        let mut s = state_with(&[ManifestCap::ReadDocument]);
        assert!(s.get_document(SCOPE_DOC.into()).unwrap_err().contains("No DB access"));
        let err = s.get_document("doc:other".into()).unwrap_err();
        assert!(err.contains("scoped to the current document"), "{err}");
    }

    #[test]
    fn scoped_write_with_no_scope_is_refused() {
        // No scope set (e.g. unsaved doc): WriteDocument matches nothing; the
        // skill must hold WriteAllDocuments to create in an arbitrary thread.
        let mut s = PluginState {
            db: None,
            granted: [ManifestCap::WriteDocument].into_iter().collect(),
            limits: StoreLimits::default(),
            scope_doc: None,
            scope_thread: None,
        };
        let err = s
            .create_document("t".into(), "thread:x".into(), "c".into())
            .unwrap_err();
        assert!(err.contains("no thread scope"), "{err}");
    }
}
