//! SSRF guard — validate that a URL points at a public, routable HTTP(S)
//! destination before fetching it or handing it to an embedded webview
//! (WEB-001/WEB-002). Shared by every UI: the Tauri app's fetch + browser and
//! the native shell's browser, so the guard can't drift between frontends.

use std::net::{IpAddr, Ipv6Addr, SocketAddr, ToSocketAddrs};

use url::Url;

/// Validate that `url` points at a public, routable HTTP(S) destination.
///
/// Rejects:
/// - non-`http`/`https` schemes (e.g. `file://`, `gopher://`)
/// - any host that resolves to a loopback, unspecified, multicast,
///   private (RFC1918), or link-local (incl. 169.254.0.0/16 cloud-metadata)
///   address — and the IPv6 equivalents (`::1`, unique-local `fc00::/7`,
///   link-local `fe80::/10`), including IPv4-mapped IPv6 literals.
///
/// DNS resolution is blocking, so this function is synchronous by design.
pub fn validate_public_url(url: &str) -> Result<(), String> {
    validate_and_resolve(url).map(|_| ())
}

/// Like [`validate_public_url`], but also returns the parsed URL and the exact
/// addresses that passed classification, so the caller can PIN the connection
/// to them (WEB-004) — validating and then letting the HTTP client re-resolve
/// independently leaves a DNS-rebinding TOCTOU.
pub fn validate_and_resolve(url: &str) -> Result<(Url, Vec<SocketAddr>), String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL '{url}': {e}"))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme '{scheme}' not allowed (only http/https)"));
    }

    let host = parsed.host_str().ok_or_else(|| "URL has no host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);

    // Literal IPs are classified directly (IPv6 literals come back bracketed);
    // hostnames are resolved (blocking) and every resolved address classified.
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
            return Err(format!("host '{host}' resolves to non-public address {}", sa.ip()));
        }
    }

    Ok((parsed, addrs))
}

/// True if `ip` is an internal / non-routable address we must never fetch.
fn is_blocked_ip(ip: &IpAddr) -> bool {
    // Canonicalize first: an IPv4-mapped IPv6 literal like `::ffff:127.0.0.1`
    // must be classified by its underlying IPv4 rules, not sail through the
    // IPv6 arm as "public" (WEB-001).
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
fn is_unique_local_v6(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xfe00) == 0xfc00
}

/// IPv6 link-local addresses (`fe80::/10`).
fn is_link_local_v6(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback_sidecar() {
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
        assert!(validate_public_url("http://[::ffff:127.0.0.1]/").is_err());
        assert!(validate_public_url("http://[::ffff:169.254.169.254]/").is_err());
        assert!(validate_public_url("http://[::ffff:10.0.0.1]/").is_err());
        assert!(validate_public_url("http://[::ffff:192.168.1.1]:9101/").is_err());
    }

    #[test]
    fn rejects_non_http_scheme() {
        assert!(validate_public_url("file:///etc/passwd").is_err());
        assert!(validate_public_url("gopher://127.0.0.1/").is_err());
    }

    #[test]
    fn allows_public_literal() {
        let (url, addrs) = validate_and_resolve("https://1.1.1.1/page").unwrap();
        assert_eq!(url.host_str(), Some("1.1.1.1"));
        assert_eq!(addrs.len(), 1);
    }
}
