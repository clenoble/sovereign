//! Embedded browser webview lifecycle management.
//!
//! Creates and manages a secondary Tauri webview for browsing external URLs
//! inside the main Sovereign window. Requires the `unstable` feature on tauri.

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl};

const BROWSER_LABEL: &str = "browser";

/// Logical rectangle for positioning the browser webview.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// JavaScript snippet injected into every page load to extract readable content.
const EXTRACTION_SCRIPT: &str = r#"
(function() {
    if (window.__sovereign_extracted) return;
    window.__sovereign_extracted = true;

    function extractContent() {
        var title = document.title || '';
        var selectors = ['article', '[role="main"]', 'main', '.post-content', '.entry-content', '.article-body'];
        var mainEl = null;
        for (var i = 0; i < selectors.length; i++) {
            mainEl = document.querySelector(selectors[i]);
            if (mainEl) break;
        }
        if (!mainEl) mainEl = document.body;

        var clone = mainEl.cloneNode(true);
        var remove = clone.querySelectorAll('script, style, nav, header, footer, aside, [role="navigation"], [role="banner"]');
        for (var j = 0; j < remove.length; j++) {
            remove[j].remove();
        }
        var text = (clone.innerText || clone.textContent || '').trim();
        if (text.length > 12000) text = text.substring(0, 12000);

        return { title: title, text: text, url: window.location.href };
    }

    if (document.readyState === 'complete') {
        var result = extractContent();
        window.__TAURI_INTERNALS__.invoke('__browser_content_extracted', result);
    } else {
        window.addEventListener('load', function() {
            var result = extractContent();
            window.__TAURI_INTERNALS__.invoke('__browser_content_extracted', result);
        });
    }
})();
"#;

/// Create the browser webview as a child of the main window.
///
/// Must be called from `spawn_blocking` on Windows to avoid deadlock.
pub fn create_browser_webview(
    app: &AppHandle,
    url: &str,
    bounds: LogicalRect,
) -> Result<(), String> {
    let parsed: url::Url = url.parse().map_err(|e| format!("Invalid URL: {e}"))?;

    // If browser already exists, just navigate
    if let Some(wv) = app.get_webview(BROWSER_LABEL) {
        wv.navigate(parsed).map_err(|e: tauri::Error| e.to_string())?;
        return Ok(());
    }

    let window = app
        .get_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    let builder = tauri::webview::WebviewBuilder::new(BROWSER_LABEL, WebviewUrl::External(parsed))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .initialization_script(EXTRACTION_SCRIPT)
        .on_navigation({
            let app_handle = app.clone();
            move |nav_url| {
                let url_str = nav_url.to_string();
                let title = nav_url.host_str().unwrap_or("").to_string();
                let _ = app_handle.emit("browser-navigated", serde_json::json!({
                    "url": url_str,
                    "title": title,
                }));
                true // allow all navigation
            }
        });

    window
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        )
        .map_err(|e| format!("Failed to create browser webview: {e}"))?;

    Ok(())
}

/// Navigate the existing browser webview to a new URL.
pub fn navigate_browser(app: &AppHandle, url: &str) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    let parsed: url::Url = url.parse().map_err(|e| format!("Invalid URL: {e}"))?;
    webview.navigate(parsed).map_err(|e: tauri::Error| e.to_string())
}

/// Destroy the browser webview.
pub fn destroy_browser(app: &AppHandle) -> Result<(), String> {
    if let Some(webview) = app.get_webview(BROWSER_LABEL) {
        webview.close().map_err(|e: tauri::Error| e.to_string())?;
    }
    Ok(())
}

/// Update the browser webview bounds (position + size).
pub fn set_browser_bounds(app: &AppHandle, bounds: LogicalRect) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    webview
        .set_position(LogicalPosition::new(bounds.x, bounds.y))
        .map_err(|e: tauri::Error| e.to_string())?;
    webview
        .set_size(LogicalSize::new(bounds.width, bounds.height))
        .map_err(|e: tauri::Error| e.to_string())
}

/// Show or hide the browser webview.
pub fn set_browser_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    if let Some(webview) = app.get_webview(BROWSER_LABEL) {
        if visible {
            webview.show().map_err(|e: tauri::Error| e.to_string())?;
        } else {
            webview.hide().map_err(|e: tauri::Error| e.to_string())?;
        }
    }
    Ok(())
}

/// Go back in browser history.
pub fn browser_back(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    webview
        .eval("history.back()")
        .map_err(|e: tauri::Error| e.to_string())
}

/// Go forward in browser history.
pub fn browser_forward(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    webview
        .eval("history.forward()")
        .map_err(|e: tauri::Error| e.to_string())
}

/// Reload the current page.
pub fn browser_refresh(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    webview.eval("location.reload()").map_err(|e: tauri::Error| e.to_string())
}

/// Get the current URL of the browser webview.
pub fn browser_url(app: &AppHandle) -> Result<String, String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    Ok(webview.url().map_err(|e: tauri::Error| e.to_string())?.to_string())
}
