use std::sync::Arc;
use wasmtime::component::bindgen;
use wasmtime::StoreLimits;

use crate::traits::SkillDbAccess;

// Generate Rust bindings from the WIT definition.
// This creates the SkillPlugin world type and all Host traits.
bindgen!({
    world: "skill-plugin",
    path: "wit",
});

/// Store state for each WASM skill execution.
/// Contains the DB bridge and resource limits.
pub(crate) struct PluginState {
    pub db: Option<Arc<dyn SkillDbAccess>>,
    pub limits: StoreLimits,
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
        let db = self.db.as_ref().ok_or("No DB access available")?;
        db.create_document(&title, &thread_id, &content)
            .map_err(|e| e.to_string())
    }
}
