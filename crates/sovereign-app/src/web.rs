//! Web content fetching and extraction for the Sovereign Browser.
//!
//! Uses `reqwest` for HTTP fetching and `readability` for article extraction.

#[cfg(feature = "web-browse")]
use std::io::Cursor;
#[cfg(feature = "web-browse")]
use std::net::IpAddr;
#[cfg(feature = "web-browse")]
use std::time::Duration;
#[cfg(feature = "web-browse")]
use url::Url;

// SSRF guard moved to `sovereign-core::net_guard` so the native shell shares the
// exact same validator (WEB-001 must not drift between frontends). Re-exported
// here so existing callers (`crate::web::validate_public_url`) keep working.
pub use sovereign_core::net_guard::{validate_and_resolve, validate_public_url};

/// A fetched and extracted web page.
#[cfg(feature = "web-browse")]
#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    /// Cleaned article HTML (from readability).
    pub content_html: String,
    /// Plain text of the article body (for LLM assessment).
    pub text: String,
}

/// Fetch a web page and extract readable content.
///
/// Uses the `readability` crate (port of arc90's readability algorithm)
/// to extract the main article content, stripping navigation, ads, etc.
#[cfg(feature = "web-browse")]
pub async fn fetch_and_extract(url_str: &str) -> anyhow::Result<FetchedPage> {
    const MAX_REDIRECTS: usize = 5;

    // Redirects are followed MANUALLY: for every hop we validate the target,
    // resolve it once, and pin the connection to the validated addresses
    // (WEB-001/WEB-002/WEB-004). reqwest's built-in redirect handling would
    // re-resolve each hop independently, reopening the rebinding TOCTOU.
    // Hard ceiling on the response body we will buffer. readability roughly
    // doubles peak memory while parsing, so keep this well under available RAM
    // — a hostile or misbehaving server must not be able to OOM the only UI
    // (WEB-002).
    const MAX_BODY_BYTES: usize = 16 * 1024 * 1024; // 16 MB

    let mut current = Url::parse(url_str)
        .map_err(|e| anyhow::anyhow!("Invalid URL '{}': {}", url_str, e))?;

    for _hop in 0..=MAX_REDIRECTS {
        let (parsed_url, addrs) = validate_and_resolve(current.as_str())
            .map_err(|e| anyhow::anyhow!("blocked URL '{}': {}", current, e))?;
        let host = parsed_url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("URL has no host"))?
            .to_string();

        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Sovereign-GE/0.1 (https://github.com/clenoble/sovereign)")
            .redirect(reqwest::redirect::Policy::none());
        // Pin name->address so the connect step cannot diverge from the
        // addresses that passed classification (no-op for IP-literal hosts).
        if host.parse::<IpAddr>().is_err() {
            builder = builder.resolve_to_addrs(&host, &addrs);
        }
        let client = builder.build()?;

        let mut response = client.get(parsed_url.clone()).send().await?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("redirect without a Location header"))?;
            // join() handles relative redirects; the next loop iteration
            // re-validates and re-pins the new target.
            current = parsed_url
                .join(location)
                .map_err(|e| anyhow::anyhow!("invalid redirect target '{location}': {e}"))?;
            continue;
        }

        if !response.status().is_success() {
            anyhow::bail!("HTTP {} for {}", response.status(), current);
        }

        // Reject an advertised oversized body up front, then stream with a hard
        // cap so a server that lies about (or omits) Content-Length still can't
        // make us allocate without bound (WEB-002).
        if let Some(len) = response.content_length() {
            if len as usize > MAX_BODY_BYTES {
                anyhow::bail!(
                    "response body too large ({len} bytes > {MAX_BODY_BYTES} cap) for {current}"
                );
            }
        }
        let mut body: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > MAX_BODY_BYTES {
                anyhow::bail!("response body exceeded {MAX_BODY_BYTES} byte cap for {current}");
            }
            body.extend_from_slice(&chunk);
        }
        let html = String::from_utf8_lossy(&body).into_owned();

        // readability::extractor::extract takes &mut Read + &Url
        let mut cursor = Cursor::new(html.as_bytes());
        let product = readability::extractor::extract(&mut cursor, &parsed_url)
            .map_err(|e| anyhow::anyhow!("Content extraction failed: {}", e))?;

        return Ok(FetchedPage {
            url: url_str.to_string(),
            title: product.title,
            content_html: product.content,
            text: product.text,
        });
    }

    anyhow::bail!("too many redirects (max {MAX_REDIRECTS}) for {url_str}")
}

#[cfg(all(test, feature = "web-browse"))]
mod tests {
    use super::*;

    #[test]
    fn test_fetched_page_struct() {
        let page = FetchedPage {
            url: "https://example.com".into(),
            title: "Example".into(),
            content_html: "<p>Hello</p>".into(),
            text: "Hello".into(),
        };
        assert_eq!(page.title, "Example");
        assert!(!page.text.is_empty());
    }

    // SSRF-guard tests moved with the validator to `sovereign_core::net_guard`.

    #[test]
    fn accepts_public_ip_literal() {
        // Public IP literals need no DNS and must pass classification.
        assert!(validate_public_url("https://1.1.1.1/").is_ok());
        assert!(validate_public_url("http://8.8.8.8/").is_ok());
        // Public IPv6 literal (Cloudflare DNS).
        assert!(validate_public_url("https://[2606:4700:4700::1111]/").is_ok());
    }

    #[test]
    fn accepts_public_host_when_resolvable() {
        // A normal public host classifies as public *iff* DNS is available.
        // The CI sandbox may have no resolver, so only assert the accept
        // path when resolution actually succeeds — a resolution failure is
        // an environment artifact, not a validator bug. Crucially, a
        // resolvable public host must never be *rejected as non-public*.
        match validate_public_url("https://example.com") {
            Ok(()) => {}
            Err(e) => assert!(
                e.contains("could not resolve") || e.contains("resolved to no addresses"),
                "public host wrongly rejected as non-public: {e}"
            ),
        }
    }
}
