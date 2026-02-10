/// Dark theme CSS for Sovereign OS.
pub const DARK_THEME_CSS: &str = r#"
/* ── Window ────────────────────────────────────────── */
window {
    background-color: #0e0e10;
    color: #e0e0e0;
}

/* ── Canvas placeholder ────────────────────────────── */
.canvas-placeholder {
    font-size: 18px;
    color: #666;
}

/* ── Taskbar ───────────────────────────────────────── */
.taskbar {
    background-color: #141418;
    border-top: 1px solid #2a2a30;
    padding: 6px 12px;
    min-height: 40px;
}

.taskbar-item {
    padding: 4px 12px;
    border-radius: 6px;
    font-size: 13px;
    color: #d0d0d0;
}

.taskbar-item:hover {
    background-color: #222228;
}

.owned-badge {
    color: #5a9fd4;
    font-weight: bold;
}

.external-badge {
    color: #e07c6a;
    font-weight: bold;
}

.search-btn {
    padding: 4px 12px;
    border-radius: 6px;
    font-size: 13px;
    color: #999;
}

.search-btn:hover {
    background-color: #222228;
    color: #d0d0d0;
}

/* ── Search overlay ────────────────────────────────── */
.search-overlay {
    background-color: rgba(14, 14, 16, 0.95);
    border-radius: 12px;
    padding: 16px;
    margin-top: 80px;
}

.search-entry {
    font-size: 16px;
    padding: 10px 16px;
    background-color: #1a1a20;
    color: #e0e0e0;
    border: 1px solid #333;
    border-radius: 8px;
    min-width: 400px;
}

.search-entry:focus {
    border-color: #5a9fd4;
}

.search-hint {
    color: #666;
    font-size: 12px;
    margin-top: 8px;
}

/* ── Search results ───────────────────────────────── */
.search-results {
    padding: 4px 0;
}

.search-result-item {
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 13px;
    color: #d0d0d0;
}

.search-result-item:hover {
    background-color: #222228;
}

.search-result-empty {
    padding: 6px 12px;
    font-size: 13px;
    color: #666;
    font-style: italic;
}

/* ── Voice status ─────────────────────────────────── */
.voice-status {
    color: #5a9fd4;
    font-size: 12px;
    font-weight: bold;
}

/* -- Document panel ---------------------------------------- */
.document-panel {
    background-color: #1a1a20;
}

.markdown-editor {
    background: #1a1a20;
    color: #e0e0e0;
}

.image-gallery {
    padding: 8px;
}

/* -- Orchestrator bubble ----------------------------------- */
.orchestrator-bubble {
    min-width: 56px;
    min-height: 56px;
    border-radius: 28px;
    background: #3a3af4;
    color: white;
    font-weight: bold;
    font-size: 16px;
    padding: 14px 18px;
}

.skill-panel {
    background: #1e1e24;
    border-radius: 12px;
    padding: 8px;
    border: 1px solid #2a2a30;
}

.skill-button {
    padding: 8px 16px;
    border-radius: 8px;
    color: #e0e0e0;
    background: #2a2a32;
    margin: 4px;
}

.skill-button:hover {
    background: #3a3a42;
}

.skill-button:disabled {
    color: #555;
}

/* -- Bubble visual states --------------------------------- */
.bubble-idle {
    background: #3a3af4;
}

.bubble-processing-owned {
    background: #2a5af4;
    animation: pulse-blue 1.5s ease-in-out infinite;
}

.bubble-processing-external {
    background: #d4a05a;
    animation: pulse-amber 1.5s ease-in-out infinite;
}

.bubble-proposing {
    background: #d4a05a;
    min-width: 64px;
    min-height: 64px;
    border-radius: 32px;
}

.bubble-executing {
    background: #3ad47a;
}

.bubble-suggesting {
    background: #6b5ad4;
    animation: pulse-blue 2s ease-in-out infinite;
}

.suggestion-tooltip {
    background: #1e1e24;
    border-radius: 8px;
    padding: 8px 12px;
    border: 1px solid #6b5ad4;
    color: #d0d0e0;
    font-size: 12px;
}

/* -- Confirmation panel ----------------------------------- */
.confirmation-panel {
    background: #1e1e24;
    border-radius: 12px;
    padding: 12px;
    border: 1px solid #d4a05a;
    margin: 4px;
}

.confirmation-label {
    color: #e0e0e0;
    font-size: 13px;
    margin-bottom: 8px;
}

.approve-button {
    padding: 6px 16px;
    border-radius: 8px;
    color: white;
    background: #3a9a4a;
    margin: 4px;
}

.approve-button:hover {
    background: #4aaa5a;
}

.reject-button {
    padding: 6px 16px;
    border-radius: 8px;
    color: white;
    background: #9a3a3a;
    margin: 4px;
}

.reject-button:hover {
    background: #aa4a4a;
}

.rejection-toast {
    color: #e07c6a;
    font-size: 12px;
    font-style: italic;
}
"#;
