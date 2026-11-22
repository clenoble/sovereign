//! What remains readable of a quarantined conversation log, and what she can do
//! with it, on the app (spec ch. 08, C-LOG, DG3, DG4 and DG6; the Fable maille,
//! by delegation of Céline, 2026-11-18, on what Export gives, which quarantine
//! the choices act on, and where the window opens).
//!
//! The window is the frontend's `QuarantinedLogWindow.svelte`, opened from the
//! control « Show what remains readable » below the quarantine notice. These
//! layers serve it the logged-in persona's most recent quarantine only (DG6).
//! Delete refuses unless `confirmed`: the window asks first (« Delete this
//! quarantined log? It cannot be brought back. »).
//!
//! Export waits: on the phone it is Céline's to decide (the Fable maille,
//! 2026-11-18), and its layer and tests come with her answer.
//!
//! Types and signatures by the test author of C-LOG's host lot; the bodies, the
//! commands over them and their registration are the implementer's.

use crate::tauri_state::AppState;

/// DG3, what the window shows: the entries before the break, each as plain
/// text with visible marks (`log_diagnosis::Readable`); the number of entries
/// from the break on, counted and not shown; whether the log is in the older
/// format (the window then says `log_diagnosis::order_not_guaranteed_sentence`);
/// the earlier quarantines, each named by DG1's sentence. No `Debug`: it is not
/// meant to become text anywhere else.
#[derive(Clone, serde::Serialize)]
pub struct ReadableLogDto {
    pub entries: Vec<String>,
    pub after_the_break: u64,
    pub order_not_guaranteed: bool,
    pub earlier: Vec<String>,
}

/// The logged-in persona's log paths and log key; `None` without an open
/// session (no persona recorded, or no account key installed).
#[cfg(feature = "encrypted-log")]
async fn session_log(state: &AppState) -> Option<(sovereign_ai::OrchestratorStatePaths, [u8; 32])> {
    let persona = state.session_persona()?;
    let key = state.account_key().await?;
    let paths = sovereign_ai::OrchestratorStatePaths::for_persona(&state.profile_dir, persona);
    Some((paths, key.derive_session_log_key()))
}

/// DG3: what remains readable of the logged-in persona's most recent
/// quarantine; `Ok(None)` when it has none. Nothing without an open session.
pub async fn quarantined_log_readable_in(state: &AppState) -> Result<Option<ReadableLogDto>, String> {
    #[cfg(feature = "encrypted-log")]
    {
        use sovereign_ai::log_diagnosis as dg;
        let Some((paths, key)) = session_log(state).await else {
            return Err("No session is open.".to_string());
        };
        let Some(r) = dg::readable_for_the_person(&paths, &key).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let earlier = dg::diagnose(&paths, &key)
            .map_err(|e| e.to_string())?
            .map(|d| d.earlier.iter().map(dg::earlier_quarantine_sentence).collect())
            .unwrap_or_default();
        Ok(Some(ReadableLogDto {
            entries: r.entries,
            after_the_break: r.after_the_break,
            order_not_guaranteed: r.order_not_guaranteed,
            earlier,
        }))
    }
    #[cfg(not(feature = "encrypted-log"))]
    {
        let _ = state;
        Ok(None)
    }
}

/// DG4, Delete: removes the most recent quarantine and nothing else
/// (`log_diagnosis::delete_the_quarantine`). Refuses, removing nothing, unless
/// `confirmed`.
pub async fn delete_quarantined_log_in(state: &AppState, confirmed: bool) -> Result<(), String> {
    if !confirmed {
        return Err("The quarantined log is deleted only once she has confirmed.".to_string());
    }
    #[cfg(feature = "encrypted-log")]
    {
        let Some((paths, _)) = session_log(state).await else {
            return Err("No session is open.".to_string());
        };
        sovereign_ai::log_diagnosis::delete_the_quarantine(&paths).map_err(|e| e.to_string())
    }
    #[cfg(not(feature = "encrypted-log"))]
    {
        let _ = state;
        Err("There is no quarantined log in this build.".to_string())
    }
}

/// DG4 on the app: the choices it carries out, in the order it offers them, which it gives its
/// orchestrator where it builds it (`Orchestrator::set_log_choices`): Keep and Delete, no Export
/// (Céline, 2026-11-18: « Mettre l'export du téléphone au backlog »).
#[cfg(feature = "encrypted-log")]
pub fn choices() -> Vec<sovereign_ai::log_diagnosis::Choice> {
    use sovereign_ai::log_diagnosis::Choice;
    vec![Choice::Keep, Choice::Delete]
}

/// DG3: the window's content, for her window only.
#[tauri::command]
pub async fn quarantined_log_readable(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ReadableLogDto>, String> {
    state.require_unlocked(&webview).await?;
    quarantined_log_readable_in(&state).await
}

/// DG4: Delete, after the window's confirmation.
#[tauri::command]
pub async fn delete_quarantined_log(
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    confirmed: bool,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    delete_quarantined_log_in(&state, confirmed).await
}
