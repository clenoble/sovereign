use super::*;

// ---------------------------------------------------------------------------
// Memory consolidation — AI-suggested links
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SuggestionDto {
    pub id: String,
    pub from_doc_id: String,
    pub from_title: String,
    pub to_doc_id: String,
    pub to_title: String,
    pub relation_type: String,
    pub strength: f32,
    pub rationale: String,
    pub source: String,
}

/// List all pending AI-suggested links.
#[tauri::command]
pub async fn list_pending_suggestions(
    state: State<'_, AppState>,
) -> Result<Vec<SuggestionDto>, String> {
    let suggestions = state
        .db
        .list_pending_suggestions()
        .await
        .str_err()?;

    let mut dtos = Vec::new();
    for s in suggestions {
        let sugg_id = s.id_string().unwrap_or_default();
        let from_id = s.out.as_ref().map(|t| sovereign_db::schema::thing_to_raw(t)).unwrap_or_default();
        let to_id = s.in_.as_ref().map(|t| sovereign_db::schema::thing_to_raw(t)).unwrap_or_default();

        let from_title = state.db.get_document(&from_id).await.map(|d| d.title).unwrap_or_default();
        let to_title = state.db.get_document(&to_id).await.map(|d| d.title).unwrap_or_default();

        dtos.push(SuggestionDto {
            id: sugg_id,
            from_doc_id: from_id,
            from_title,
            to_doc_id: to_id,
            to_title,
            relation_type: s.relation_type.to_string(),
            strength: s.strength,
            rationale: s.rationale,
            source: format!("{:?}", s.source),
        });
    }
    Ok(dtos)
}

/// Accept an AI-suggested link (promotes to a real relationship).
#[tauri::command]
pub async fn accept_link_suggestion(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .resolve_suggestion(&id, sovereign_db::schema::SuggestionStatus::Accepted)
        .await
        .str_err()?;

    let _ = state.orch_tx.send(OrchestratorEvent::LinkSuggestionResolved {
        suggestion_id: id,
        accepted: true,
    });
    Ok(())
}

/// Dismiss an AI-suggested link (will not be re-suggested).
#[tauri::command]
pub async fn dismiss_link_suggestion(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .resolve_suggestion(&id, sovereign_db::schema::SuggestionStatus::Dismissed)
        .await
        .str_err()?;

    let _ = state.orch_tx.send(OrchestratorEvent::LinkSuggestionResolved {
        suggestion_id: id,
        accepted: false,
    });
    Ok(())
}

/// Manually trigger a consolidation cycle (for testing / debug).
#[tauri::command]
pub async fn trigger_consolidation(
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "Orchestrator not available".to_string())?;
    orch.consolidate_memory()
        .await
        .str_err()?;

    let count = state
        .db
        .list_pending_suggestions()
        .await
        .str_err()?
        .len();
    Ok(count as u32)
}

