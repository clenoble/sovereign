//! Web content fetching and extraction for the Sovereign Browser.
//!
//! Uses `reqwest` for HTTP fetching and `readability` for article extraction.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

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
    validate_and_resolve(url).map(|_| ())
}

/// Like [`validate_public_url`], but also returns the parsed URL and the
/// exact addresses that passed classification, so the caller can PIN the
/// connection to them (WEB-004). Validating and then letting the HTTP client
/// re-resolve independently leaves a DNS-rebinding TOCTOU: a low-TTL record
/// can answer public for the check and 127.0.0.1 for the fetch.
pub fn validate_and_resolve(url: &str) -> Result<(Url, Vec<SocketAddr>), String> {
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

    // If the host is a literal IP, classify it directly (IPv6 literals come
    // back bracketed from host_str(), so strip the brackets for the parse).
    // Otherwise resolve it (blocking) and classify every resolved address.
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    let addrs: Vec<SocketAddr> = if let Ok(ip) = bare_host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        (host, port)
            .to_socket_addrs()
            .map_err(|e| format!("could not resolve host '{host}': {e}"))?
            .collect()
    };

    if addrs.is_empty() {
        return Err(format!("host '{host}' resolved to no addresses"));
    }

    for sa in &addrs {
        if is_blocked_ip(&sa.ip()) {
            return Err(format!(
                "host '{host}' resolves to non-public address {}",
                sa.ip()
            ));
        }
    }

    Ok((parsed, addrs))
}

/// True if `ip` is an internal / non-routable address we must never fetch.
fn is_blocked_ip(ip: &IpAddr) -> bool {
    // Canonicalize first: an IPv4-mapped IPv6 literal like `::ffff:127.0.0.1`
    // or `::ffff:169.254.169.254` must be classified by its underlying IPv4
    // rules, not sail through the IPv6 arm as "public" (WEB-001). On Linux /
    // Android such a literal routes to the mapped IPv4 target, so without this
    // it was a clean SSRF-guard bypass to loopback / cloud-metadata / RFC1918.
    match ip.to_canonical() {
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
                || is_unique_local_v6(&v6)
                || is_link_local_v6(&v6)
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
    fn rejects_ipv4_mapped_ipv6() {
        // IPv4-mapped IPv6 literals must canonicalize to their IPv4 form and be
        // blocked, not slip through the IPv6 arm as "public" (WEB-001).
        assert!(validate_public_url("http://[::ffff:127.0.0.1]/").is_err()); // loopback
        assert!(validate_public_url("http://[::ffff:169.254.169.254]/").is_err()); // cloud metadata
        assert!(validate_public_url("http://[::ffff:10.0.0.1]/").is_err()); // RFC1918
        assert!(validate_public_url("http://[::ffff:192.168.1.1]:9101/").is_err()); // sidecar via mapped
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(validate_public_url("file:///etc/passwd").is_err());
        assert!(validate_public_url("gopher://127.0.0.1/").is_err());
    }

    #[test]
    fn validate_and_resolve_returns_pinnable_addrs() {
        let (url, addrs) = validate_and_resolve("https://1.1.1.1/page").unwrap();
        assert_eq!(url.host_str(), Some("1.1.1.1"));
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip().to_string(), "1.1.1.1");
        assert_eq!(addrs[0].port(), 443); // scheme default carried through
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
