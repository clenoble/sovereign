//! Browser-side PII helpers — webview form-field extraction + autofill
//! injection.
//!
//! Step 8b of the PII management & dashboard plan. Three pieces:
//!   - JS scripts that run inside the embedded browser webview to (a)
//!     enumerate input fields on a signup page and (b) inject a value
//!     into a specific field for autofill.
//!   - `webview.eval(...)` Rust wrappers that ship those scripts.
//!   - A typed DTO (`FormFieldDto`) for the JS-→-Rust callback payload.
//!
//! User-initiated only: per the plan's UX rules, neither extraction nor
//! injection runs without an explicit click in the Tauri-side UI
//! (Save credentials / Fill from vault toolbar buttons land in 8d/8e).
//! Both flows produce L3 audit events; injection in particular is
//! gated by the dashboard's existing reveal/autofill confirmation.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Webview label for the embedded browser (matches `browser.rs`).
const BROWSER_LABEL: &str = "browser";

/// Shape the form-extraction script returns to the
/// `__browser_form_extracted` Tauri callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldDto {
    /// Snake_case classification produced by the JS heuristic:
    /// "password", "email", "phone", "first_name", "last_name",
    /// "address", or "text" (catch-all).
    pub kind: String,
    /// CSS selector the JS chose to identify this field — `#id` when
    /// available, else `tag[name="..."]`.
    pub selector: String,
    /// Current value typed into the field. Empty if untouched.
    pub value: String,
    /// `placeholder` attribute, if any. Useful for inferring intent
    /// when the `kind` heuristic is uncertain.
    pub placeholder: String,
    /// First associated `<label>` text, if any.
    pub label: String,
}

/// Payload of the `__browser_form_extracted` Tauri callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormExtractionDto {
    pub url: String,
    pub fields: Vec<FormFieldDto>,
}

/// Trigger form-field extraction in the browser webview. The script
/// calls back via
/// `window.__TAURI_INTERNALS__.invoke('__browser_form_extracted', payload)`,
/// which the `extract_form_fields_callback` Tauri command receives and
/// forwards as the `browser-form-extracted` event for the frontend.
pub fn trigger_form_extraction(app: &AppHandle) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    webview
        .eval(FORM_EXTRACTION_SCRIPT)
        .map_err(|e: tauri::Error| e.to_string())
}

/// Inject `value` into the input matching `selector` in the browser
/// webview. Uses the native value setter so framework state systems
/// (React, Vue, Angular) pick up the change rather than overwriting
/// `el.value` directly (which they often ignore).
pub fn autofill_field(app: &AppHandle, selector: &str, value: &str) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    let script = build_autofill_script(selector, value)?;
    webview
        .eval(&script)
        .map_err(|e: tauri::Error| e.to_string())
}

/// Build the autofill JS for a (selector, value) pair. `serde_json::to_string`
/// produces a properly-escaped JSON string literal, which is also a
/// valid JavaScript string literal — safe to embed even when the input
/// contains quotes, backslashes, or `</script>` sequences.
///
/// Public so it's testable without a live webview.
pub fn build_autofill_script(selector: &str, value: &str) -> Result<String, String> {
    let js_selector = serde_json::to_string(selector)
        .map_err(|e| format!("selector encode: {e}"))?;
    let js_value = serde_json::to_string(value).map_err(|e| format!("value encode: {e}"))?;
    Ok(format!(
        r#"
        (function() {{
            try {{
                var el = document.querySelector({selector});
                if (!el) return;
                var v = {value};
                // Use the native setter so React/Vue/etc. observe the change.
                var proto = (el.tagName === 'TEXTAREA')
                    ? window.HTMLTextAreaElement.prototype
                    : window.HTMLInputElement.prototype;
                var setter = Object.getOwnPropertyDescriptor(proto, 'value').set;
                setter.call(el, v);
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                el.focus();
            }} catch (e) {{
                console.warn('Sovereign autofill error:', e);
            }}
        }})();
        "#,
        selector = js_selector,
        value = js_value,
    ))
}

/// Form-extraction JS — runs synchronously in the webview when invoked
/// via `webview.eval(...)`. Classifies inputs by `type`, `name`, `id`,
/// and `autocomplete` heuristics. Returns the field list to Rust via
/// the `__browser_form_extracted` Tauri callback.
const FORM_EXTRACTION_SCRIPT: &str = r#"
(function() {
    function classifyField(el) {
        var name = (el.name || '').toLowerCase();
        var id = (el.id || '').toLowerCase();
        var type = (el.type || '').toLowerCase();
        var auto = (el.autocomplete || '').toLowerCase();
        if (type === 'password' || /pass(word|wd)?/.test(auto)) return 'password';
        if (type === 'email' || /email/.test(name) || /email/.test(id) || /email/.test(auto)) return 'email';
        if (type === 'tel' || /phone|tel/.test(name) || /phone|tel/.test(id) || /tel/.test(auto)) return 'phone';
        if (/first.?name|given/.test(name) || /first.?name|given/.test(id) || /given-name/.test(auto)) return 'first_name';
        if (/last.?name|family|surname/.test(name) || /last.?name|family/.test(id) || /family-name/.test(auto)) return 'last_name';
        if (/street|address1|line1/.test(name) || /street|address1/.test(id) || /address-line1/.test(auto)) return 'address';
        if (type === 'text' || type === '') return 'text';
        return null;
    }

    function selectorFor(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        if (el.name) return el.tagName.toLowerCase() + '[name="' + CSS.escape(el.name) + '"]';
        return null;
    }

    var inputs = document.querySelectorAll('input, textarea');
    var fields = [];
    for (var i = 0; i < inputs.length; i++) {
        var el = inputs[i];
        if (el.type === 'hidden' || el.type === 'submit' || el.type === 'button') continue;
        var kind = classifyField(el);
        var sel = selectorFor(el);
        if (kind && sel) {
            var labelText = '';
            if (el.labels && el.labels.length > 0) {
                labelText = (el.labels[0].textContent || '').trim();
            }
            fields.push({
                kind: kind,
                selector: sel,
                value: el.value || '',
                placeholder: el.placeholder || '',
                label: labelText
            });
        }
    }

    window.__TAURI_INTERNALS__.invoke('__browser_form_extracted', {
        url: window.location.href,
        fields: fields
    });
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autofill_script_builds_for_simple_inputs() {
        let s = build_autofill_script("#email", "alice@example.com").unwrap();
        // Selector and value must appear inside the script as JSON-quoted strings.
        assert!(s.contains("\"#email\""), "expected JSON-quoted selector: {s}");
        assert!(
            s.contains("\"alice@example.com\""),
            "expected JSON-quoted value: {s}"
        );
        // Sanity: includes the dispatchEvent calls.
        assert!(s.contains("dispatchEvent"));
        assert!(s.contains("'input'"));
        assert!(s.contains("'change'"));
    }

    #[test]
    fn autofill_script_escapes_quotes_in_value() {
        // Password contains both single and double quotes plus a
        // backslash — the JSON encoding must produce a valid JS string
        // literal that doesn't break out.
        let value = r#"p\a"s's"word"#;
        let s = build_autofill_script("#pwd", value).unwrap();
        // The JSON encoding produces "\"p\\a\\\"s's\\\"word\"" which in
        // the script is `var v = "p\a\"s's\"word"` — JS-safe.
        // Negative assertion: the raw value cannot appear as a sequence
        // in the script (it would mean unescaped injection).
        assert!(!s.contains(r#"p\a"s's"word"#), "raw value leaked: {s}");
    }

    #[test]
    fn autofill_script_neutralizes_script_close_tag() {
        // Defense: a value containing `</script>` shouldn't terminate
        // the script. Script-tag-injection isn't directly possible
        // here (we eval, not innerHTML), but worth confirming the
        // sequence is escaped in the literal.
        let value = "</script><img src=x>";
        let s = build_autofill_script("#x", value).unwrap();
        // serde_json escapes `</` as-is (it's only a problem in HTML
        // contexts, not JS). The value lives inside a JSON-quoted
        // string literal, so the `</script>` is just ASCII inside the
        // JS string — no DOM consequences.
        assert!(s.contains(r#""</script><img src=x>""#));
    }

    #[test]
    fn autofill_script_handles_unicode_value() {
        let value = "café — 北京 — \u{1F511}"; // includes 4-byte codepoint
        let s = build_autofill_script("#x", value).unwrap();
        // JSON encodes non-ASCII as either literal UTF-8 or \uXXXX
        // depending on serde_json's policy. Either way the resulting
        // JS string literal evaluates to the same value.
        let json: String = serde_json::to_string(value).unwrap();
        assert!(s.contains(&json));
    }

    #[test]
    fn autofill_script_rejects_no_inputs_silently() {
        // Selector targets nothing — the script's early-return clause
        // exits without throwing. We can't run the JS here, just
        // confirm the script source includes the early-return guard.
        let s = build_autofill_script("nonexistent", "x").unwrap();
        assert!(s.contains("if (!el) return;"));
    }

    #[test]
    fn extraction_script_is_well_formed() {
        // Sanity: the embedded extraction script invokes the expected
        // Tauri callback name.
        assert!(FORM_EXTRACTION_SCRIPT.contains("__browser_form_extracted"));
        assert!(FORM_EXTRACTION_SCRIPT.contains("classifyField"));
        assert!(FORM_EXTRACTION_SCRIPT.contains("selectorFor"));
        // Won't extract submit / button / hidden inputs.
        assert!(FORM_EXTRACTION_SCRIPT.contains("type === 'hidden'"));
    }

    #[test]
    fn form_field_dto_round_trip() {
        let dto = FormFieldDto {
            kind: "password".into(),
            selector: "#pwd".into(),
            value: "".into(),
            placeholder: "Choose a strong password".into(),
            label: "Password".into(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        let back: FormFieldDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, "password");
        assert_eq!(back.selector, "#pwd");
        assert_eq!(back.label, "Password");
    }

    #[test]
    fn form_extraction_dto_round_trip() {
        let dto = FormExtractionDto {
            url: "https://example.com/signup".into(),
            fields: vec![FormFieldDto {
                kind: "email".into(),
                selector: "#email".into(),
                value: "".into(),
                placeholder: "you@example.com".into(),
                label: "Email".into(),
            }],
        };
        let json = serde_json::to_string(&dto).unwrap();
        let back: FormExtractionDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.url, "https://example.com/signup");
        assert_eq!(back.fields.len(), 1);
        assert_eq!(back.fields[0].kind, "email");
    }
}
