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

.skill-popover {
    background: #1e1e24;
    border-radius: 12px;
    padding: 8px;
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
"#;
