//! Webview cookie management for the dashboard's Cookies tab.
//!
//! Step 8c of the PII management & dashboard plan. Wraps Tauri 2.10's
//! `Webview::cookies()` / `delete_cookie()` API so the dashboard can
//! list cookies attributable to an entity (joined by `Entity.domains[]`),
//! delete individual cookies, or bulk-clear every cookie for an entity.
//!
//! Sovereign does NOT mirror cookies into `sovereign-db` — they are
//! transient session state owned by the embedded webview's native
//! cookie store (WebView2 SQLite on Windows, etc.). This module is a
//! thin read/write surface over that native store.

use serde::Serialize;
use tauri::{AppHandle, Manager};

const BROWSER_LABEL: &str = "browser";

/// Frontend-facing view of a single cookie.
#[derive(Debug, Clone, Serialize)]
pub struct CookieDto {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// ISO-8601 expiration timestamp, or `null` for session cookies.
    pub expires: Option<String>,
    pub http_only: bool,
    pub secure: bool,
    /// "strict" / "lax" / "none" / "" (when unspecified).
    pub same_site: String,
}

/// Fetch every cookie from the embedded browser's native store and
/// return only those whose domain matches one of `entity_domains`.
///
/// Domain matching: case-insensitive equality with optional leading
/// dot — a cookie `domain = ".example.com"` matches an entity domain
/// `"example.com"` and vice versa. Subdomain matching is also
/// included so `mail.example.com` cookies attribute to an entity that
/// owns `example.com`.
pub fn list_cookies_for_domains(
    app: &AppHandle,
    entity_domains: &[String],
) -> Result<Vec<CookieDto>, String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    let cookies = webview.cookies().map_err(|e: tauri::Error| e.to_string())?;
    let lower_domains: Vec<String> =
        entity_domains.iter().map(|d| d.to_ascii_lowercase()).collect();
    Ok(cookies
        .iter()
        .filter(|c| domain_matches_any(c.domain(), &lower_domains))
        .map(cookie_to_dto)
        .collect())
}

/// Delete one cookie by its (name, domain, path) identity. Tauri's
/// `delete_cookie` takes a cookie spec and removes the matching one.
pub fn delete_one(
    app: &AppHandle,
    name: &str,
    domain: &str,
    path: &str,
) -> Result<(), String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    let mut cookie = cookie::Cookie::new(name.to_string(), String::new());
    cookie.set_domain(domain.to_string());
    cookie.set_path(path.to_string());
    webview
        .delete_cookie(cookie)
        .map_err(|e: tauri::Error| e.to_string())
}

/// Delete every cookie whose domain matches one of `entity_domains`.
/// Returns the count actually deleted.
pub fn clear_for_domains(
    app: &AppHandle,
    entity_domains: &[String],
) -> Result<usize, String> {
    let webview = app
        .get_webview(BROWSER_LABEL)
        .ok_or_else(|| "Browser webview not open".to_string())?;
    let cookies = webview.cookies().map_err(|e: tauri::Error| e.to_string())?;
    let lower_domains: Vec<String> =
        entity_domains.iter().map(|d| d.to_ascii_lowercase()).collect();

    let mut deleted = 0usize;
    for c in &cookies {
        if !domain_matches_any(c.domain(), &lower_domains) {
            continue;
        }
        // Construct a cookie spec matching the original by (name,
        // domain, path) — Tauri identifies cookies by that triple.
        let mut spec = cookie::Cookie::new(c.name().to_string(), String::new());
        spec.set_domain(c.domain().unwrap_or_default().to_string());
        spec.set_path(c.path().unwrap_or("/").to_string());
        match webview.delete_cookie(spec) {
            Ok(()) => deleted += 1,
            Err(e) => tracing::warn!(
                "delete_cookie {}@{} failed: {e}",
                c.name(),
                c.domain().unwrap_or("?")
            ),
        }
    }
    Ok(deleted)
}

/// Domain-match predicate: case-insensitive, allows leading dot, and
/// allows subdomain matching against an entity domain.
///
/// `cookie_domain = Some("example.com")` and entity domain `"example.com"` → match.
/// `cookie_domain = Some(".example.com")` and entity `"example.com"` → match.
/// `cookie_domain = Some("mail.example.com")` and entity `"example.com"` → match.
/// `cookie_domain = Some("evil-example.com")` and entity `"example.com"` → no match.
/// `cookie_domain = None` (session cookie not bound to a domain) → no match.
pub(crate) fn domain_matches_any(cookie_domain: Option<&str>, entity_domains: &[String]) -> bool {
    let d = match cookie_domain {
        Some(d) if !d.is_empty() => d.trim_start_matches('.').to_ascii_lowercase(),
        _ => return false,
    };
    entity_domains.iter().any(|ed| {
        let ed = ed.as_str();
        d == ed || d.ends_with(&format!(".{ed}"))
    })
}

fn cookie_to_dto(c: &cookie::Cookie<'_>) -> CookieDto {
    let expires = match c.expires() {
        Some(cookie::Expiration::DateTime(t)) => Some(t.to_string()),
        _ => None,
    };
    let same_site = match c.same_site() {
        Some(cookie::SameSite::Strict) => "strict",
        Some(cookie::SameSite::Lax) => "lax",
        Some(cookie::SameSite::None) => "none",
        None => "",
    }
    .to_string();
    CookieDto {
        name: c.name().to_string(),
        value: c.value().to_string(),
        domain: c.domain().unwrap_or_default().to_string(),
        path: c.path().unwrap_or("/").to_string(),
        expires,
        http_only: c.http_only().unwrap_or(false),
        secure: c.secure().unwrap_or(false),
        same_site,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doms(d: &[&str]) -> Vec<String> {
        d.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn exact_domain_matches() {
        assert!(domain_matches_any(Some("example.com"), &doms(&["example.com"])));
    }

    #[test]
    fn leading_dot_matches() {
        assert!(domain_matches_any(Some(".example.com"), &doms(&["example.com"])));
        // Reverse direction: if entity domain has a leading dot
        // (unusual), still match.
        assert!(domain_matches_any(
            Some("example.com"),
            &doms(&[".example.com"])
        ));
    }

    #[test]
    fn subdomain_matches_parent_entity() {
        assert!(domain_matches_any(
            Some("mail.example.com"),
            &doms(&["example.com"])
        ));
        assert!(domain_matches_any(
            Some("a.b.example.com"),
            &doms(&["example.com"])
        ));
    }

    #[test]
    fn case_insensitive() {
        assert!(domain_matches_any(Some("EXAMPLE.COM"), &doms(&["example.com"])));
        assert!(domain_matches_any(Some("example.com"), &doms(&["EXAMPLE.COM"])));
    }

    #[test]
    fn evil_lookalike_does_not_match() {
        // Defense-in-depth: "evil-example.com" looks like a substring
        // but isn't a real subdomain of example.com.
        assert!(!domain_matches_any(
            Some("evil-example.com"),
            &doms(&["example.com"])
        ));
        // Reverse: entity is a SUBdomain of cookie — no match either
        // (subdomain matching only works one way: cookie subdomain →
        // entity parent, not entity subdomain → cookie parent).
        assert!(!domain_matches_any(
            Some("example.com"),
            &doms(&["mail.example.com"])
        ));
    }

    #[test]
    fn no_domain_no_match() {
        assert!(!domain_matches_any(None, &doms(&["example.com"])));
        assert!(!domain_matches_any(Some(""), &doms(&["example.com"])));
    }

    #[test]
    fn multi_entity_domains() {
        // An entity with multiple domains (acme.com + acme.ch)
        // matches a cookie on either.
        let domains = doms(&["acme.com", "acme.ch"]);
        assert!(domain_matches_any(Some("acme.com"), &domains));
        assert!(domain_matches_any(Some("api.acme.ch"), &domains));
        assert!(!domain_matches_any(Some("other.com"), &domains));
    }

    #[test]
    fn empty_entity_domains_never_matches() {
        assert!(!domain_matches_any(Some("example.com"), &[]));
    }

    #[test]
    fn all_domains_lowercased_for_match_on_input_too() {
        // The list_cookies_for_domains caller lowercases entity
        // domains before passing them in, but verify the predicate
        // is robust if they're already mixed-case.
        assert!(domain_matches_any(Some("Example.COM"), &doms(&["EXAMPLE.com"])));
    }
}
