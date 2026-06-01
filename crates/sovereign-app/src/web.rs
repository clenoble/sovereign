//! Web content fetching and extraction for the Sovereign Browser.
//!
//! Uses `reqwest` for HTTP fetching and `readability` for article extraction.

use std::net::{IpAddr, ToSocketAddrs};

use url::Url;

#[cfg(feature = "web-browse")]
use std::io::Cursor;
#[cfg(feature = "web-browse")]
use std::time::Duration;

/// Validate that `url` points at a public, routable HTTP(S) destination
/// before we fetch it or hand it to the embedded webview (SSRF guard,
/// WEB-001/WEB-002).
///
/// Rejects:
/// - non-`http`/`https` schemes (e.g. `file://`, `gopher://`)
/// - any host that resolves to a loopback, unspecified, multicast,
///   private (RFC1918), or link-local (incl. 169.254.0.0/16 cloud-metadata)
///   address — and the IPv6 equivalents (`::1`, unique-local `fc00::/7`,
///   link-local `fe80::/10`).
///
/// Literal-IP hosts are validated against the same rules. DNS resolution is
/// blocking, so this function is synchronous by design.
pub fn validate_public_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL '{url}': {e}"))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme '{scheme}' not allowed (only http/https)"));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    // Default port is irrelevant to address classification, but
    // `to_socket_addrs` needs one — fall back to the scheme default.
    let port = parsed.port_or_known_default().unwrap_or(80);

    // If the host is a literal IP, classify it directly. Otherwise resolve
    // it (blocking) and classify every resolved address.
    let addrs: Vec<IpAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        (host, port)
            .to_socket_addrs()
            .map_err(|e| format!("could not resolve host '{host}': {e}"))?
            .map(|sa| sa.ip())
            .collect()
    };

    if addrs.is_empty() {
        return Err(format!("host '{host}' resolved to no addresses"));
    }

    for ip in &addrs {
        if is_blocked_ip(ip) {
            return Err(format!(
                "host '{host}' resolves to non-public address {ip}"
            ));
        }
    }

    Ok(())
}

/// True if `ip` is an internal / non-routable address we must never fetch.
fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16 (cloud metadata)
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_unique_local_v6(v6)
                || is_link_local_v6(v6)
        }
    }
}

/// IPv6 unique-local addresses (`fc00::/7`).
fn is_unique_local_v6(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

/// IPv6 link-local addresses (`fe80::/10`).
fn is_link_local_v6(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

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
    // SSRF guard (WEB-001): reject internal/non-public targets before we
    // ever open a connection.
    validate_public_url(url_str).map_err(|e| anyhow::anyhow!("blocked URL '{}': {}", url_str, e))?;

    let parsed_url =
        Url::parse(url_str).map_err(|e| anyhow::anyhow!("Invalid URL '{}': {}", url_str, e))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Sovereign-GE/0.1 (https://github.com/clenoble/sovereign)")
        // Re-validate EVERY redirect hop (WEB-002): a public URL could 30x
        // to 127.0.0.1 / 169.254.169.254 / RFC1918. Keep the 5-hop cap.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                attempt.error("too many redirects (max 5)")
            } else if validate_public_url(attempt.url().as_str()).is_err() {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()?;

    let response = client.get(parsed_url.clone()).send().await?;

    if !response.status().is_success() {
        anyhow::bail!(
            "HTTP {} for {}",
            response.status(),
            url_str
        );
    }

    let html = response.text().await?;

    // readability::extractor::extract takes &mut Read + &Url
    let mut cursor = Cursor::new(html.as_bytes());
    let product = readability::extractor::extract(&mut cursor, &parsed_url)
        .map_err(|e| anyhow::anyhow!("Content extraction failed: {}", e))?;

    Ok(FetchedPage {
        url: url_str.to_string(),
        title: product.title,
        content_html: product.content,
        text: product.text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "web-browse")]
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

    #[test]
    fn rejects_loopback_sidecar() {
        // jiminy sidecar ports
        assert!(validate_public_url("http://127.0.0.1:9100").is_err());
        assert!(validate_public_url("http://127.0.0.1:9101/listen").is_err());
    }

    #[test]
    fn rejects_cloud_metadata() {
        assert!(validate_public_url("http://169.254.169.254/").is_err());
    }

    #[test]
    fn rejects_rfc1918() {
        assert!(validate_public_url("http://10.0.0.1/").is_err());
        assert!(validate_public_url("http://192.168.1.1/").is_err());
        assert!(validate_public_url("http://172.16.0.1/").is_err());
    }

    #[test]
    fn rejects_ipv6_loopback() {
        assert!(validate_public_url("http://[::1]/").is_err());
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(validate_public_url("file:///etc/passwd").is_err());
        assert!(validate_public_url("gopher://127.0.0.1/").is_err());
    }

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
