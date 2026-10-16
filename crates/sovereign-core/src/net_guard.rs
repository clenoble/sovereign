//! SSRF guard — validate that a URL points at a public, routable HTTP(S)
//! destination before fetching it or handing it to an embedded webview
//! (WEB-001/WEB-002). Shared by every UI: the Tauri app's fetch + browser and
//! the native shell's browser, so the guard can't drift between frontends.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use url::Url;

/// How long a caller on a UI thread may wait for a DNS lookup before the
/// navigation is refused (A6/O1). The webview's navigation handler must answer
/// synchronously, on the UI thread; an unbounded lookup there froze the whole
/// app for as long as the OS resolver took to give up.
pub const UI_DNS_TIMEOUT: Duration = Duration::from_millis(1500);

/// Lookups that outlived their caller's timeout keep their thread until the OS
/// resolver returns. Past this many, refuse at once instead of piling threads
/// up behind a dead resolver.
const MAX_PENDING_LOOKUPS: usize = 8;
static PENDING_LOOKUPS: AtomicUsize = AtomicUsize::new(0);

/// Validate that `url` points at a public, routable HTTP(S) destination.
///
/// Rejects:
/// - non-`http`/`https` schemes (e.g. `file://`, `gopher://`)
/// - any host that resolves to a loopback, unspecified, multicast,
///   private (RFC1918), or link-local (incl. 169.254.0.0/16 cloud-metadata)
///   address, CGNAT `100.64.0.0/10`, `0.0.0.0/8`, `198.18.0.0/15`,
///   `240.0.0.0/4` — and the IPv6 equivalents (`::1`, unique-local
///   `fc00::/7`, link-local `fe80::/10`, site-local `fec0::/10`). IPv6 forms
///   that carry an IPv4 (mapped `::ffff:`, compatible `::a.b.c.d`, NAT64
///   `64:ff9b::/96`, 6to4 `2002::/16`) are judged by that IPv4.
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
    let target = parse_target(url)?;
    // Literal IPs are classified directly; hostnames are resolved (blocking)
    // and every resolved address classified.
    let addrs: Vec<SocketAddr> = match target.literal {
        Some(ip) => vec![SocketAddr::new(ip, target.port)],
        None => (target.host.as_str(), target.port)
            .to_socket_addrs()
            .map_err(|e| format!("could not resolve host '{}': {e}", target.host))?
            .collect(),
    };
    check_addrs(&target.host, &addrs)?;
    Ok((target.url, addrs))
}

/// Like [`validate_public_url`], for callers that must not block: the webview
/// navigation handlers, which run on the UI thread (A6/O1).
///
/// Schemes and literal IPs are decided at once, with no lookup. A hostname is
/// resolved on a helper thread, and if the answer takes longer than `timeout`
/// the URL is refused (fail closed) with a message that says it was a timeout,
/// not a verdict on the address. The helper thread finishes on its own when the
/// OS resolver returns; at most [`MAX_PENDING_LOOKUPS`] may be outstanding.
pub fn validate_public_url_bounded(url: &str, timeout: Duration) -> Result<(), String> {
    bounded_with(url, timeout, &PENDING_LOOKUPS, |host, port| {
        (host.as_str(), port).to_socket_addrs().map(|it| it.collect())
    })
}

fn bounded_with<F>(
    url: &str,
    timeout: Duration,
    pending: &'static AtomicUsize,
    resolve: F,
) -> Result<(), String>
where
    F: FnOnce(String, u16) -> std::io::Result<Vec<SocketAddr>> + Send + 'static,
{
    let target = parse_target(url)?;
    if let Some(ip) = target.literal {
        return check_addrs(&target.host, &[SocketAddr::new(ip, target.port)]);
    }

    if pending.fetch_add(1, Ordering::SeqCst) >= MAX_PENDING_LOOKUPS {
        pending.fetch_sub(1, Ordering::SeqCst);
        return Err(format!(
            "too many DNS lookups still pending; '{}' not checked, so not loaded",
            target.host
        ));
    }
    let (tx, rx) = mpsc::channel();
    let (host, port) = (target.host.clone(), target.port);
    std::thread::spawn(move || {
        let result = resolve(host, port);
        pending.fetch_sub(1, Ordering::SeqCst);
        let _ = tx.send(result); // the caller may have timed out and gone
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(addrs)) => check_addrs(&target.host, &addrs),
        Ok(Err(e)) => Err(format!("could not resolve host '{}': {e}", target.host)),
        Err(_) => Err(format!(
            "DNS lookup for '{}' took longer than {} ms; not loaded (the address was not judged, only the wait)",
            target.host,
            timeout.as_millis()
        )),
    }
}

struct Target {
    url: Url,
    host: String,
    port: u16,
    /// The host as an IP, when the URL carries a literal (IPv6 unbracketed).
    literal: Option<IpAddr>,
}

fn parse_target(url: &str) -> Result<Target, String> {
    let parsed = Url::parse(url).map_err(|e| format!("invalid URL '{url}': {e}"))?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("scheme '{scheme}' not allowed (only http/https)"));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);
    // IPv6 literals come back bracketed.
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok();
    Ok(Target { url: parsed, host, port, literal })
}

fn check_addrs(host: &str, addrs: &[SocketAddr]) -> Result<(), String> {
    if addrs.is_empty() {
        return Err(format!("host '{host}' resolved to no addresses"));
    }
    for sa in addrs {
        if is_blocked_ip(&sa.ip()) {
            return Err(format!("host '{host}' resolves to non-public address {}", sa.ip()));
        }
    }
    Ok(())
}

/// True if `ip` is an internal / non-routable address we must never fetch.
fn is_blocked_ip(ip: &IpAddr) -> bool {
    // Canonicalize first: an IPv4-mapped IPv6 literal like `::ffff:127.0.0.1`
    // must be classified by its underlying IPv4 rules, not sail through the
    // IPv6 arm as "public" (WEB-001).
    match ip.to_canonical() {
        IpAddr::V4(v4) => is_blocked_v4(&v4),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_unique_local_v6(&v6)
                || is_link_local_v6(&v6)
                || is_site_local_v6(&v6)
                // Transition forms carrying an IPv4 address: classify by the
                // embedded IPv4, so `[64:ff9b::7f00:1]` is loopback, not public.
                || embedded_v4(&v6).is_some_and(|v4| is_blocked_v4(&v4))
        }
    }
}

fn is_blocked_v4(v4: &Ipv4Addr) -> bool {
    let [a, b, _, _] = v4.octets();
    v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_multicast()
        || v4.is_private()
        || v4.is_link_local() // 169.254.0.0/16 (cloud metadata)
        || v4.is_broadcast()
        || a == 0 // 0.0.0.0/8 "this network"
        || (a == 100 && (b & 0xc0) == 64) // 100.64.0.0/10 CGNAT (RFC 6598)
        || (a == 198 && (b & 0xfe) == 18) // 198.18.0.0/15 benchmarking (RFC 2544)
        || a >= 240 // 240.0.0.0/4 reserved (incl. broadcast)
}

/// The IPv4 address carried by an IPv6 transition form, if any:
/// IPv4-compatible `::a.b.c.d` (deprecated), NAT64 `64:ff9b::/96` and 6to4
/// `2002::/16`. (IPv4-mapped `::ffff:a.b.c.d` is handled by `to_canonical`.)
fn embedded_v4(v6: &Ipv6Addr) -> Option<Ipv4Addr> {
    let s = v6.segments();
    let low = |hi: u16, lo: u16| Ipv4Addr::new((hi >> 8) as u8, hi as u8, (lo >> 8) as u8, lo as u8);
    if s[..6] == [0; 6] {
        Some(low(s[6], s[7]))
    } else if s[0] == 0x0064 && s[1] == 0xff9b && s[2..6] == [0; 4] {
        Some(low(s[6], s[7]))
    } else if s[0] == 0x2002 {
        Some(low(s[1], s[2]))
    } else {
        None
    }
}

/// IPv6 site-local addresses (`fec0::/10`, deprecated but still routed
/// internally by some networks).
fn is_site_local_v6(v6: &Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfec0
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

    fn blocked(s: &str) -> bool {
        is_blocked_ip(&s.parse::<IpAddr>().unwrap())
    }

    #[test]
    fn blocks_cgnat_100_64_slash_10() {
        assert!(blocked("100.64.0.1"));
        assert!(blocked("100.127.255.254"));
        assert!(!blocked("100.63.255.255"));
        assert!(!blocked("100.128.0.1"));
    }

    #[test]
    fn blocks_this_network_0_slash_8() {
        assert!(blocked("0.1.2.3"));
        assert!(blocked("0.255.255.255"));
    }

    #[test]
    fn blocks_benchmarking_198_18_slash_15() {
        assert!(blocked("198.18.0.1"));
        assert!(blocked("198.19.255.255"));
        assert!(!blocked("198.17.255.255"));
        assert!(!blocked("198.20.0.1"));
    }

    #[test]
    fn blocks_reserved_240_slash_4() {
        assert!(blocked("240.0.0.1"));
        assert!(blocked("254.1.2.3"));
        assert!(blocked("255.255.255.255"));
        assert!(!blocked("223.255.255.255"));
    }

    #[test]
    fn blocks_ipv4_compatible_ipv6_by_embedded_v4() {
        assert!(blocked("::127.0.0.1"));
        assert!(blocked("::10.0.0.1"));
        assert!(blocked("::169.254.169.254"));
        assert!(validate_public_url("http://[::192.168.1.1]/").is_err());
    }

    #[test]
    fn blocks_nat64_by_embedded_v4() {
        assert!(blocked("64:ff9b::127.0.0.1"));
        assert!(blocked("64:ff9b::a9fe:a9fe")); // 169.254.169.254
        assert!(blocked("64:ff9b::100.64.0.1"));
        assert!(validate_public_url("http://[64:ff9b::7f00:1]/").is_err());
    }

    #[test]
    fn blocks_6to4_by_embedded_v4() {
        assert!(blocked("2002:7f00:0001::1")); // 127.0.0.1
        assert!(blocked("2002:c0a8:0101::")); // 192.168.1.1
        assert!(blocked("2002:a9fe:a9fe::1")); // 169.254.169.254
    }

    #[test]
    fn blocks_site_local_fec0_slash_10() {
        assert!(blocked("fec0::1"));
        assert!(blocked("feff::1"));
        assert!(validate_public_url("http://[fec0::1]/").is_err());
    }

    /// Public addresses, including transition forms carrying a public IPv4,
    /// must still pass — the guard narrows, it does not close the web.
    #[test]
    fn public_addresses_still_pass() {
        for s in [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.0.1",
            "198.20.0.1",
            "223.255.255.255",
            "2606:4700:4700::1111",
            "64:ff9b::1.1.1.1",
            "2002:0101:0101::1",
            "2001:db8::1",
        ] {
            assert!(!blocked(s), "{s} must not be blocked");
        }
    }

    #[test]
    fn allows_public_literal() {
        let (url, addrs) = validate_and_resolve("https://1.1.1.1/page").unwrap();
        assert_eq!(url.host_str(), Some("1.1.1.1"));
        assert_eq!(addrs.len(), 1);
    }

    // ── bounded variant (A6/O1). Each test owns its counter, and the resolver
    // is injected, so none of these touch the network or each other. ──

    fn addr(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[test]
    fn bounded_decides_literals_without_resolving() {
        static PENDING: AtomicUsize = AtomicUsize::new(0);
        let never = |_: String, _: u16| -> std::io::Result<Vec<SocketAddr>> {
            panic!("a literal must not be resolved")
        };
        assert!(bounded_with("http://127.0.0.1:9100/", Duration::from_millis(50), &PENDING, never).is_err());
        assert!(bounded_with("https://1.1.1.1/", Duration::from_millis(50), &PENDING, never).is_ok());
        assert!(bounded_with("file:///etc/passwd", Duration::from_millis(50), &PENDING, never).is_err());
        assert_eq!(PENDING.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn bounded_classifies_resolved_addresses() {
        static PENDING: AtomicUsize = AtomicUsize::new(0);
        let private = |_: String, _: u16| Ok(vec![addr("10.0.0.1:443")]);
        let public = |_: String, _: u16| Ok(vec![addr("1.1.1.1:443")]);
        let mixed = |_: String, _: u16| Ok(vec![addr("1.1.1.1:443"), addr("127.0.0.1:443")]);
        let t = Duration::from_secs(5);
        assert!(bounded_with("https://evil.example/", t, &PENDING, private).is_err());
        assert!(bounded_with("https://ok.example/", t, &PENDING, public).is_ok());
        assert!(bounded_with("https://rebind.example/", t, &PENDING, mixed).is_err());
    }

    #[test]
    fn bounded_fails_closed_on_timeout_and_says_so() {
        static PENDING: AtomicUsize = AtomicUsize::new(0);
        let slow = |_: String, _: u16| {
            std::thread::sleep(Duration::from_millis(400));
            Ok(vec![addr("1.1.1.1:443")])
        };
        let err = bounded_with("https://slow.example/", Duration::from_millis(30), &PENDING, slow)
            .unwrap_err();
        assert!(err.contains("took longer than"), "{err}");
        // The helper thread outlives the caller, then releases its slot.
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(PENDING.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn bounded_refuses_at_once_past_the_pending_cap() {
        static PENDING: AtomicUsize = AtomicUsize::new(0);
        PENDING.store(MAX_PENDING_LOOKUPS, Ordering::SeqCst);
        let never = |_: String, _: u16| -> std::io::Result<Vec<SocketAddr>> {
            panic!("must not start a lookup past the cap")
        };
        let err = bounded_with("https://any.example/", Duration::from_secs(5), &PENDING, never)
            .unwrap_err();
        assert!(err.contains("too many DNS lookups"), "{err}");
        assert_eq!(PENDING.load(Ordering::SeqCst), MAX_PENDING_LOOKUPS);
    }

    #[test]
    fn bounded_reports_resolution_errors() {
        static PENDING: AtomicUsize = AtomicUsize::new(0);
        let fail = |_: String, _: u16| -> std::io::Result<Vec<SocketAddr>> {
            Err(std::io::Error::other("no such host"))
        };
        let err = bounded_with("https://nx.example/", Duration::from_secs(5), &PENDING, fail)
            .unwrap_err();
        assert!(err.contains("could not resolve"), "{err}");
        assert_eq!(PENDING.load(Ordering::SeqCst), 0);
    }
}
