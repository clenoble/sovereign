# 10 — Embedded Browser and Web Access

How Sovereign shows web pages and fetches web content: the embedded webview in the native shell
(the only desktop UI since 2026-10-09; the Tauri desktop build is frozen and not shipped, and
mobile has no browser), what passes between the page and Sovereign, the guard that keeps both away from non-public
addresses, and two decided-but-unbuilt changes — incognito browsing (A5) and a filtering local
proxy (O4). The Identity Firewall of v1.x is recorded here as an open conflict.

*Consolidates:* specification v1.3 §Security Model (Identity Firewall), §Core Tenets;
`doc/legal/sovereign_os_ethics.md` §2.1, §3.1; `doc/plans/archive/plans/pii-management-dashboard.md`
(embedded-browser integration); `doc/plans/archive/plans/native-ui-migration-decisions.md`
(Batch 5, browser); `doc/plans/archive/plans/v0.0.8-release-plan.md` (WEB-001); commit 7fdfed2
(A6/O1, 2026-10-09); `doc/plans/BACKLOG.md` (A5, O4 decisions of 2026-10-09); code in
`crates/sovereign-core/src/net_guard.rs`, `crates/sovereign-app/src/{browser.rs,web.rs,browser_pii.rs,cookie_api.rs}`,
`crates/sovereign-app/src/tauri_commands/browser.rs`, `crates/sovereign-shell/src/app.rs`.

---

## Why an embedded browser

**Status:** ✅ Built — kept by decision.

The embedded browser exists because of its coupling with the PII manager (ch. 09): signup capture,
autofill from the vault, the per-entity cookie view, and disclosures recorded in the share ledger
all act on a live page. An external hand-off to the system browser would keep none of these. The
second reason is reading: a real page can be assessed for reliability and saved as an external
document. During the native-UI migration the browser was a candidate to drop; it was kept, as a
real webview rather than a reader mode, for these reasons.

*Note:* the PII couplings listed above are built only in the frozen Tauri desktop (ch. 09,
*Embedded-browser flows*, *Cookies*, *Share ledger*). Since the freeze, no shipped UI has the
PII reasons the browser was kept for; the shell's browser serves reading and assessment. Bringing
sign-in from the PII manager to the shell is part of A5 (below).

## Hosts

**Status:** ✅ Built — the native shell is the only shipped host (code: `crates/sovereign-shell/src/app.rs`; frozen Tauri desktop: `crates/sovereign-app/src/browser.rs`).

- **Native shell:** a `wry` webview attached as a **child** of the shell's window
  (`build_as_child` + `set_bounds`). The shell draws the chrome (title, back / forward / reload, URL
  field, information strip); the webview fills the browser window's body, is resized each frame, and
  is shown only while the browser window is on top and no modal is open. Closing the window drops the
  webview.
- **Frozen Tauri desktop (not shipped):** a secondary Tauri webview added as a child of the main
  window, positioned by the frontend's browser panel.
- **Mobile:** no embedded browser — the mobile feature set excludes it.
  (code: `crates/sovereign-app/src/lib.rs`, `Cargo.toml` `mobile` feature)

The shell's webview is the platform webview (WebView2 on Windows, WebKitGTK on Linux).

## Page text and what it is used for

**Status:** ◐ Partial — page text and *Assess* built in the native shell; *Save as external* does not yet store the page as specified (see below). The frozen Tauri desktop's page-text path is incomplete and not shipped.

An initialization script injected on every page posts the page's title and visible text (bounded in
length) to Sovereign over the webview's IPC channel. That text feeds two user actions:

- **Assess** — reliability assessment of the page by the local model (ch. 08). Page text
  is external content and is fenced as data before it reaches the model.
- **Save as external** — stores the page as a document with `is_owned = false` in a *Web* thread,
  with its source URL as provenance and any reliability result attached (see ch. 04 for how judgments
  are meant to be stored). *As built in the shell:* the page is saved with `is_owned = false` into the
  first thread rather than a *Web* thread; the URL is only prepended to the body, `source_url` is not
  set, and no reliability result is attached.

The IPC channel is reachable by any script on the page, with no origin check. Its payload is
therefore trusted for title and text only, never for identity or location.

(code: `crates/sovereign-shell/src/app.rs`; frozen Tauri desktop: `crates/sovereign-app/src/browser.rs`,
`crates/sovereign-app/src/tauri_commands/browser.rs`)

## The URL bar shows the committed URL (WEB-101)

**Status:** ✅ Built — the native shell, the only UI with a browser.

The address bar and the saved provenance of a page come from the webview's own **committed** URL,
never from the location the page's script reports over IPC. A page could otherwise post an arbitrary
address, spoof the address bar across origins, and forge the provenance of a saved external document
— subverting the Sovereignty Halo's trust signal. (code: `crates/sovereign-shell/src/app.rs`)

## Server-side fetch

**Status:** ◐ Partial — built in `sovereign-app` behind the non-default `web-browse` feature; neither the shell nor mobile has it. (code: `crates/sovereign-app/src/web.rs`; `crates/sovereign-app/src/tauri_commands/browser.rs`)

Sovereign can also fetch a page itself (HTTP client + readability extraction) for reading and
assessment. This path is the strict one:

- every hop is validated by the shared guard (below) and resolved **once**; the connection is
  **pinned** to the validated addresses, so the client never re-resolves (no DNS-rebinding window);
- redirects are followed manually, at most five, and **every redirect target is revalidated** and
  re-pinned;
- the response body is capped (16 MB), whether or not the server declares a length.

## SSRF guard — `net_guard`

**Status:** ✅ Built — (code: `crates/sovereign-core/src/net_guard.rs`).

One validator, in `sovereign-core`, shared by the shell's browser and by the fetch path (and by the
frozen Tauri desktop), so that it cannot drift between hosts (WEB-001 / WEB-002).

- **Schemes:** only `http` and `https`; `about:` pages internal to the webview are allowed in the
  shell's navigation handler.
- **Literal IPs** are classified directly, without a lookup.
- **Hostnames** are resolved and **every** resolved address is classified.
- **Refused:** loopback, unspecified, multicast, private (RFC 1918) and link-local addresses
  (including the cloud-metadata range), and the IPv6 equivalents (`::1`, unique-local `fc00::/7`,
  link-local `fe80::/10`). An **IPv4-mapped IPv6** address is canonicalized to its IPv4 form before
  classification.

**Where it runs.** On the initial URL and every explicit navigation; on every in-page navigation and
redirect the webview reports (the shell's navigation handler); and on every hop of the server-side
fetch.

### Bounded lookups on the UI thread

**Status:** ✅ Built — shipped 2026-10-09, commit 7fdfed2 (A6 / O1).

The webview navigation handlers must answer synchronously on the UI thread. There, a hostname is
resolved on a helper thread and the navigation is **refused (fail closed)** if the answer takes
longer than **1.5 s**; the message says the wait was judged, not the address. At most **8** lookups
may outlive their caller; beyond that, navigations are refused at once rather than piling threads
behind a dead resolver. Schemes and literal IPs are still decided immediately. The shell's URL bar
runs its check **off the UI thread** and navigates when the answer returns.

### Design limits of the current guard

**Status:** ◐ Partial — addressed by O4.

The guard covers top-level navigation; O4 extends the same checks to all webview traffic.

## Popups

**Status:** ✅ Built — (code: `crates/sovereign-shell/src/app.rs`; behaviour of `wry` 0.55 on WebView2).

Requests to open a new window (`window.open`, `target=_blank`) are suppressed: no new-window handler
is installed, and the webview's default without one is to refuse. Verified for the native shell on
WebView2; WebKitGTK [unverified].

## A5 — Incognito browsing

**Status:** ○ Decided 2026-10-09 (Céline), to be specified.

- The embedded browser **writes no browser profile to disk**: no persistent cookies, cache, history
  or site storage outlive the session.
- **Sign-in is made easy from credentials held in the PII manager** (ch. 09, *Vault* and
  *Embedded-browser flows*). The exact gesture is for the specification to define.
- **Reconnecting each session is the safe default**, and it must be effortless once the credentials
  are in Sovereign.

Open for the specification: how this interacts with the per-entity cookie view (ch. 09, *Cookies*),
whose subject becomes session-scoped; how it applies to each persona; which of the PII browser flows
built in the frozen Tauri desktop (ch. 09) it carries into the shell; and the role of the PII idle
sweep, which Céline decided is specified together with A5 (ch. 09, *Idle sweep*).

## O4 — Filtering local proxy

**Status:** ○ Decided 2026-10-09 (Céline), to be specified. Planned together with A5. The bounded
lookup (O1, above) is the interim.

All webview traffic goes through a local proxy owned by Sovereign, which:

- **resolves each host once**, **refuses non-public addresses** with the same classification as
  `net_guard`, and **connects to the validated IP** — so the address checked is the address used;
- therefore covers **iframes and subresources**, not only top-level navigations, and **closes DNS
  rebinding** for the webview.

Constraints to carry into the specification:

- Proxy configuration is supported by `wry` on Windows and Linux (HTTP CONNECT and SOCKSv5;
  macOS 14+ behind a feature flag), **not on mobile** (Android, iOS) — which has no browser anyway.
- **One classification** of addresses, shared by `net_guard`, the server-side fetch and the proxy.
- **Loopback bypass.** Chromium (hence WebView2) implicitly sends loopback traffic around a
  configured proxy; closing it needs an explicit bypass-list entry
  (`--proxy-bypass-list=<-loopback>`). *Assumption from Chromium behaviour, to verify.*
- **`wry` 0.55 drops its own `--proxy-server` argument when additional browser arguments are set**,
  so the shell must build the complete argument string itself.
- **WebRTC / UDP** is not carried by an HTTP or SOCKS proxy and is not covered by it.
- **Linux:** the proxy and incognito (A5) settings compose on WebKitGTK.

## Identity Firewall

**Status:** ⚠ Conflict — decision pending. Nothing of the Identity Firewall is built.

Placed in this chapter because what it would act on is web traffic; ch. 09 points here.

> ⚠ **Conflict — decision pending (Céline).** Specification v1.x (§Security Model → Identity
> Firewall; §Core Tenets) describes "automated, kernel-level management of PII and cookies with
> synthetic identity generation": a kernel-level proxy intercepting web requests, **synthetic
> identities generated per domain**, real credentials only for whitelisted domains (banks,
> government, healthcare) with a per-site *Always / Once / Never* prompt, visual cues for real versus
> synthetic identity, leak warnings, auto-revocation, and virtual numbers for second factors.
> `doc/legal/sovereign_os_ethics.md` §3.1 — stated there as a binding design constraint — scopes the
> firewall to **tracking prevention only**: per-domain cookie isolation, fingerprint randomization,
> referrer stripping, blocking of known tracking scripts from community lists; a curated **Trusted
> Domain List** on which the firewall is disabled and real identity passes; and explicitly **no
> synthetic names, addresses, IDs or e-mail addresses**, and no auto-filling of signup forms with
> fabricated data. The two cannot both hold. Appendix C is itself inconsistent on "synthetic": its
> §1.1 lists "Identity Firewall (synthetic identities)", and §3.1 has "synthetic cookies" active on
> non-trusted domains while ruling out synthetic names, addresses and IDs. Not resolved here.

What exists that touches the same intent: the PII manager holds real credentials per entity, fills
them only on a user action, and records every web-form disclosure in the share ledger (ch. 09). A5
and O4 are decided independently of this conflict; O4's proxy is local and in user space, not a
kernel-level interceptor.

## Not settled

**Status:** Open.

- The Identity Firewall conflict above.
- A5 and O4 specifications, including persona behaviour, the cookie view under incognito, and the
  proxy constraints listed under O4.
