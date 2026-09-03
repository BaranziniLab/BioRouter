//! The sandbox document an MCP App's untrusted guest HTML is rendered inside.
//!
//! The renderer never gives extension HTML a frame of its own. It loads this
//! document in an iframe, hands the guest HTML over `postMessage`, and this
//! document puts the guest in a *second*, sandboxed iframe. The Content Security
//! Policy on this page is the ceiling: the guest inherits it (a `srcdoc`
//! document inherits its embedder's policy) and cannot climb above it.
//!
//! # Extension text never reaches the policy unvalidated
//!
//! The `*_domains` query parameters come from an MCP App resource's `_meta.ui.csp`
//! block — i.e. from the extension, which is exactly what the policy exists to
//! contain. They are therefore run through [`sanitize_domain`] (a domain is a
//! host, never a CSP keyword) and the finished policy is HTML-attribute-escaped
//! by [`escape_html_attribute`] on the way into `content="…"`. The two halves are
//! independent on purpose: validation stops an extension widening the policy,
//! escaping stops it breaking out of the attribute if validation ever has a gap.
//! Dropping an entry narrows the policy, so a malformed declaration can only ever
//! make the sandbox *tighter*.
//!
//! # The daemon secret is not in this URL
//!
//! It used to be (`?secret=<daemon secret>`), and the guest could read it: a
//! `srcdoc` frame carrying `allow-same-origin` is same-origin with this document,
//! so `parent.location.search` handed extension code the key to the whole REST
//! API. The page is now gated by a short-lived **proxy token** minted over the
//! authenticated `POST /mcp-app-proxy/token` route, which authorises exactly one
//! thing — being served this static document — and nothing else. The guest frame
//! also no longer carries `allow-same-origin` (see the template), so it cannot
//! read this document's URL at all. Both, because either alone leaves the other
//! half of the hole open.
//!
//! `POST /mcp-app-proxy/token` is deliberately *not* in `auth::is_unauthenticated_path`
//! — only the exact path `/mcp-app-proxy` is — so it takes `check_token` like any
//! other route. It carries no `#[utoipa::path]` and is absent from the OpenAPI
//! spec for the same reason `/tool_bridge/{nonce}` is: it is a capability
//! handshake for one internal caller, not part of the client API.
//!
//! # Our own bootstrap runs by nonce, the guest's scripts do not
//!
//! `script-src 'self'` does not admit an inline `<script>`, and this document's
//! bootstrap *is* one — so the sandbox never started, and the only way to make it
//! run was for an extension to inject `'unsafe-inline'` through the very
//! `resource_domains` hole above. The bootstrap now carries a per-response nonce
//! instead. The guest cannot use it: it is 128 fresh random bits per response and
//! the guest, being cross-origin, cannot read this document's DOM to find it.

use axum::{
    extract::Query,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct ProxyQuery {
    /// Short-lived proxy token from `POST /mcp-app-proxy/token`. **Never the
    /// daemon secret** — see the module documentation.
    t: Option<String>,
    /// Comma-separated list of domains for connect-src (fetch, XHR, WebSocket)
    connect_domains: Option<String>,
    /// Comma-separated list of domains for resource loading (scripts, styles, images, fonts, media)
    resource_domains: Option<String>,
    /// Comma-separated list of origins for nested iframes (frame-src)
    frame_domains: Option<String>,
    /// Comma-separated list of allowed base URIs (base-uri)
    base_uri_domains: Option<String>,
}

const MCP_APP_PROXY_HTML: &str = include_str!("templates/mcp_app_proxy.html");

/// How long a minted proxy token stays usable. Long enough for the renderer to
/// mint one and put it in an `iframe` `src` (and for the frame to be reloaded a
/// couple of times by a re-render), short enough that a leaked one is stale.
const TOKEN_TTL: Duration = Duration::from_secs(300);

/// Upper bound on live tokens, so a caller that mints and never loads cannot
/// grow the map without limit.
const MAX_LIVE_TOKENS: usize = 256;

/// The route's state: the daemon secret (to gate minting even if the middleware
/// were ever reordered) plus the live proxy tokens.
pub struct McpAppProxyState {
    secret_key: String,
    tokens: Mutex<HashMap<String, Instant>>,
}

impl McpAppProxyState {
    fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            tokens: Mutex::new(HashMap::new()),
        }
    }

    /// Mint a token that authorises being served the sandbox document.
    fn mint(&self) -> String {
        let token = format!("{:032x}", rand::random::<u128>());
        let now = Instant::now();
        let mut tokens = self.tokens.lock().expect("token map poisoned");
        tokens.retain(|_, expiry| *expiry > now);
        if tokens.len() >= MAX_LIVE_TOKENS {
            // Drop the soonest-to-expire rather than refusing: the bound exists
            // to cap memory, and refusing would break the newest app instead of
            // the oldest.
            if let Some(oldest) = tokens
                .iter()
                .min_by_key(|(_, expiry)| **expiry)
                .map(|(k, _)| k.clone())
            {
                tokens.remove(&oldest);
            }
        }
        tokens.insert(token.clone(), now + TOKEN_TTL);
        token
    }

    /// Whether `presented` is a live token. Reusable within its TTL: the frame
    /// this gates can be reloaded by an ordinary re-render, and the document it
    /// serves carries no secret, so single-use would buy brittleness rather than
    /// security.
    fn token_valid(&self, presented: Option<&str>) -> bool {
        let Some(presented) = presented else {
            return false;
        };
        if presented.is_empty() {
            return false;
        }
        let now = Instant::now();
        let mut tokens = self.tokens.lock().expect("token map poisoned");
        tokens.retain(|_, expiry| *expiry > now);
        tokens.contains_key(presented)
    }
}

/// Build the outer sandbox CSP based on declared domains.
///
/// This CSP acts as a ceiling - the inner guest UI iframe cannot exceed these
/// permissions, even if it tried. This is the single source of truth for
/// security policy enforcement.
///
/// Every element of `*_domains` must already have been through
/// [`sanitize_domain`]; `script_nonce` is generated here, never supplied.
///
/// Based on the MCP Apps specification (ext-apps SEP):
/// <https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx>
fn build_outer_csp(
    script_nonce: &str,
    connect_domains: &[String],
    resource_domains: &[String],
    frame_domains: &[String],
    base_uri_domains: &[String],
) -> String {
    let resources = if resource_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", resource_domains.join(" "))
    };

    let connections = if connect_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", connect_domains.join(" "))
    };

    let frame_src = if frame_domains.is_empty() {
        "frame-src 'none'".to_string()
    } else {
        format!("frame-src {}", frame_domains.join(" "))
    };

    let base_uris = if base_uri_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", base_uri_domains.join(" "))
    };

    // The nonce admits THIS document's bootstrap only. It is inherited by the
    // guest's `srcdoc` frame, which is the point: an inline script there has no
    // nonce attribute and stays blocked.
    format!(
        "default-src 'none'; \
         script-src 'self' 'nonce-{script_nonce}'{resources}; \
         script-src-elem 'self' 'nonce-{script_nonce}'{resources}; \
         style-src 'self' 'unsafe-inline'{resources}; \
         style-src-elem 'self' 'unsafe-inline'{resources}; \
         connect-src 'self'{connections}; \
         img-src 'self' data: blob:{resources}; \
         font-src 'self'{resources}; \
         media-src 'self' data: blob:{resources}; \
         {frame_src}; \
         object-src 'none'; \
         base-uri 'self'{base_uris}"
    )
}

/// The longest string that could plausibly be a host, with room for a path.
const MAX_DOMAIN_LEN: usize = 255;

/// Validate one extension-supplied CSP source, returning it only if it is
/// plainly a **domain**: `host`, `host:port`, `scheme://host[:port][/path]`.
///
/// Everything else is rejected, and rejection is the safe direction — a dropped
/// entry narrows the policy. In particular this refuses:
///
/// * every CSP keyword — `'self'`, `'unsafe-inline'`, `'unsafe-eval'`,
///   `'strict-dynamic'`, `'nonce-…'`, `'sha256-…'` — because the leading
///   apostrophe is not a legal host character. An extension asking for
///   `'unsafe-inline'` is asking to switch off the containment it is subject to.
/// * `*`, and any wildcard label such as `*.example.com`. A wildcard is not
///   needed to name a host and `*` alone would open the policy completely.
/// * scheme-only sources (`data:`, `blob:`, `filesystem:`), which in `script-src`
///   are a straightforward bypass.
/// * anything carrying a quote, an angle bracket, whitespace or a semicolon —
///   the characters that would end the `content="…"` attribute, append a second
///   source, or start a second directive.
fn sanitize_domain(raw: &str) -> Option<String> {
    let candidate = raw.trim();
    if candidate.is_empty() || candidate.len() > MAX_DOMAIN_LEN {
        return None;
    }

    let rest = match candidate.split_once("://") {
        Some((scheme, rest)) => {
            if !matches!(
                scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "ws" | "wss"
            ) {
                return None;
            }
            rest
        }
        // No `://`. A bare `scheme:` source is caught below, because `data` /
        // `text/html` fails the port check and `blob:` leaves an empty port.
        None => candidate,
    };

    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };

    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };

    if let Some(port) = port {
        // `:*` is a legal CSP port-part and is refused with everything else that
        // is not a literal port number.
        if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if port.parse::<u32>().ok()? > 65535 {
            return None;
        }
    }

    if !is_plausible_host(host) {
        return None;
    }

    // A CSP path-part is a prefix match, so it needs no structure — only a
    // character set that cannot escape the attribute or the directive list.
    if !path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'~' | b'/'))
    {
        return None;
    }

    Some(candidate.to_string())
}

/// `host` is one or more dot-separated DNS labels: alphanumeric at each end,
/// hyphens allowed inside, 63 bytes or less each. Covers `localhost`, an IPv4
/// literal, and ordinary names; excludes wildcards, IPv6 literals (whose
/// brackets are not worth admitting here) and anything with punctuation.
fn is_plausible_host(host: &str) -> bool {
    if host.is_empty() || host.len() > MAX_DOMAIN_LEN {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}

/// Parse comma-separated domains, dropping every entry that is not a domain.
fn parse_domains(domains: Option<&String>) -> Vec<String> {
    domains
        .map(|d| {
            d.split(',')
                .filter_map(|entry| {
                    let accepted = sanitize_domain(entry);
                    if accepted.is_none() && !entry.trim().is_empty() {
                        tracing::warn!(
                            "MCP App declared a CSP source that is not a domain; dropping it"
                        );
                    }
                    accepted
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Escape for interpolation into a double-quoted HTML attribute value.
///
/// The second, independent half of the fix: even if [`sanitize_domain`] ever
/// admitted something it should not, the result cannot close `content="…"` and
/// start writing markup into a document that shares the daemon's origin.
///
/// `'` is deliberately left alone. It cannot terminate a double-quoted value,
/// the policy needs literal apostrophes for `'none'` / `'self'` / `'nonce-…'`,
/// and escaping it would not stop keyword injection anyway — the HTML parser
/// decodes `&#39;` back to `'` before the CSP is parsed. Keeping keywords out is
/// [`sanitize_domain`]'s job, not this function's.
fn escape_html_attribute(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Mint a short-lived token for one load of the sandbox document.
///
/// Gated by `check_token` like every other route (the exemption in
/// `auth::is_unauthenticated_path` is an exact match on `/mcp-app-proxy`). The
/// header is re-checked here so the route is safe on its own terms, not only by
/// virtue of the layer above it.
async fn mint_proxy_token(
    axum::extract::State(state): axum::extract::State<Arc<McpAppProxyState>>,
    headers: HeaderMap,
) -> Response {
    let presented = headers
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if presented != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    Json(json!({ "token": state.mint() })).into_response()
}

#[utoipa::path(
    get,
    path = "/mcp-app-proxy",
    params(
        ("t" = String, Query, description = "Short-lived proxy token from POST /mcp-app-proxy/token"),
        ("connect_domains" = Option<String>, Query, description = "Comma-separated domains for connect-src"),
        ("resource_domains" = Option<String>, Query, description = "Comma-separated domains for resource loading"),
        ("frame_domains" = Option<String>, Query, description = "Comma-separated origins for nested iframes (frame-src)"),
        ("base_uri_domains" = Option<String>, Query, description = "Comma-separated allowed base URIs (base-uri)")
    ),
    responses(
        (status = 200, description = "MCP App proxy HTML page", content_type = "text/html"),
        (status = 401, description = "Unauthorized - invalid or missing proxy token"),
    )
)]
async fn mcp_app_proxy(
    axum::extract::State(state): axum::extract::State<Arc<McpAppProxyState>>,
    Query(params): Query<ProxyQuery>,
) -> Response {
    if !state.token_valid(params.t.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Parse domains from query params. Extension-supplied, so validated.
    let connect_domains = parse_domains(params.connect_domains.as_ref());
    let resource_domains = parse_domains(params.resource_domains.as_ref());
    let frame_domains = parse_domains(params.frame_domains.as_ref());
    let base_uri_domains = parse_domains(params.base_uri_domains.as_ref());

    let script_nonce = format!("{:032x}", rand::random::<u128>());

    // Build the outer CSP based on declared domains
    let csp = build_outer_csp(
        &script_nonce,
        &connect_domains,
        &resource_domains,
        &frame_domains,
        &base_uri_domains,
    );

    // Replace the placeholders in the HTML template. Both substitutions are
    // attribute values, so both are escaped.
    let html = MCP_APP_PROXY_HTML
        .replace("{{OUTER_CSP}}", &escape_html_attribute(&csp))
        .replace("{{SCRIPT_NONCE}}", &escape_html_attribute(&script_nonce));

    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::HeaderName::from_static("referrer-policy"),
                "no-referrer",
            ),
        ],
        Html(html),
    )
        .into_response()
}

pub fn routes(secret_key: String) -> Router {
    Router::new()
        .route("/mcp-app-proxy", get(mcp_app_proxy))
        .route("/mcp-app-proxy/token", post(mint_proxy_token))
        .with_state(Arc::new(McpAppProxyState::new(secret_key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csp_with(resource_domains: &[&str]) -> String {
        let joined = resource_domains.join(",");
        let parsed = parse_domains(Some(&joined));
        build_outer_csp("deadbeef", &[], &parsed, &[], &[])
    }

    /// A domain is a host. Every CSP keyword an extension might reach for is a
    /// request to switch off the containment it is subject to, and is refused.
    #[test]
    fn a_csp_keyword_is_not_a_domain() {
        for keyword in [
            "'unsafe-inline'",
            "'unsafe-eval'",
            "'self'",
            "'strict-dynamic'",
            "'unsafe-hashes'",
            "'nonce-abc123'",
            "'sha256-abc123'",
            "*",
            "*.example.com",
            "https://*",
            "data:",
            "blob:",
            "filesystem:",
            "javascript:alert(1)",
        ] {
            assert_eq!(
                sanitize_domain(keyword),
                None,
                "{keyword} must not survive validation"
            );
        }

        let csp = csp_with(&["'unsafe-inline'", "*"]);
        // Scoped to the script directives on purpose: `style-src` carries a
        // legitimate `'unsafe-inline'`, so a whole-string search would pass
        // whatever an extension managed to inject.
        for directive in csp.split(';').map(str::trim) {
            // `script-src-elem` first: it also starts with `script-src`.
            if directive.starts_with("script-src-elem") {
                assert_eq!(
                    directive, "script-src-elem 'self' 'nonce-deadbeef'",
                    "an extension widened script-src-elem; full policy: {csp}"
                );
            } else if directive.starts_with("script-src") {
                assert_eq!(
                    directive, "script-src 'self' 'nonce-deadbeef'",
                    "an extension widened script-src; full policy: {csp}"
                );
            }
        }
        assert!(!csp.contains('*'), "the wildcard must not survive: {csp}");
    }

    /// The attribute-breakout half. A `"` would close `content="…"` and let an
    /// extension write markup into a document on the daemon's own origin.
    #[test]
    fn a_quote_can_neither_survive_validation_nor_escaping() {
        assert_eq!(sanitize_domain("x.test\" onload=\"alert(1)"), None);
        assert_eq!(sanitize_domain("x.test'"), None);
        assert_eq!(sanitize_domain("x.test><script>"), None);
        assert_eq!(sanitize_domain("a.test b.test"), None);
        assert_eq!(sanitize_domain("a.test; script-src *"), None);

        // Independent of validation: nothing that reaches the template can end
        // the attribute.
        let escaped = escape_html_attribute("a\" onload=\"x <b> &amp;");
        assert!(!escaped.contains('"'), "{escaped}");
        assert!(!escaped.contains('<'), "{escaped}");
        assert!(!escaped.contains('>'), "{escaped}");
        assert_eq!(escaped, "a&quot; onload=&quot;x &lt;b&gt; &amp;amp;");
        // The policy's own apostrophes must survive verbatim, or every keyword
        // in the default policy would be mangled.
        assert_eq!(escape_html_attribute("'none'"), "'none'");
    }

    /// Real declarations still work — the fix must narrow the policy, not delete
    /// the feature.
    #[test]
    fn ordinary_hosts_are_accepted() {
        for host in [
            "example.com",
            "cdn.example.com",
            "localhost",
            "localhost:3000",
            "127.0.0.1:8080",
            "https://cdn.example.com",
            "https://cdn.example.com:8443",
            "wss://api.example.com",
            "https://cdn.example.com/lib/",
            "my-host.example.co.uk",
        ] {
            assert_eq!(
                sanitize_domain(host).as_deref(),
                Some(host),
                "{host} is a legitimate domain and must be kept"
            );
        }

        let csp = csp_with(&["https://cdn.example.com", " example.com "]);
        assert!(
            csp.contains("script-src 'self' 'nonce-deadbeef' https://cdn.example.com example.com;"),
            "{csp}"
        );
    }

    /// The bootstrap is an inline script under `script-src 'self'`, so without a
    /// nonce the sandbox never runs — which is what made an extension-injected
    /// `'unsafe-inline'` the only way to start it.
    #[test]
    fn the_bootstrap_runs_by_nonce_and_the_template_carries_it() {
        let csp = build_outer_csp("cafebabe", &[], &[], &[], &[]);
        assert!(csp.contains("script-src 'self' 'nonce-cafebabe';"), "{csp}");
        assert!(
            csp.contains("script-src-elem 'self' 'nonce-cafebabe';"),
            "{csp}"
        );
        assert!(
            MCP_APP_PROXY_HTML.contains("<script nonce=\"{{SCRIPT_NONCE}}\">"),
            "the template's bootstrap must carry the nonce placeholder"
        );
    }

    /// The guest is untrusted extension HTML. With `allow-same-origin` it is
    /// same-origin with this document and can read `parent.location` — which is
    /// how the daemon secret used to leak.
    #[test]
    fn the_guest_frame_is_not_granted_the_daemon_origin() {
        // The `sandbox` value itself, not the file: the comment beside it names
        // the flag it is explaining, so a whole-file search would fail on prose.
        let sandbox_call = MCP_APP_PROXY_HTML
            .split("setAttribute('sandbox', ")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .expect("the template must set the guest frame's sandbox attribute");
        assert!(
            !sandbox_call.contains("allow-same-origin"),
            "the guest iframe must not be given allow-same-origin: {sandbox_call}"
        );
        assert!(
            sandbox_call.contains("allow-scripts") && sandbox_call.contains("allow-forms"),
            "the guest still needs scripts and forms: {sandbox_call}"
        );
    }

    /// The document is gated by a minted token, and the daemon secret is never
    /// an accepted way in — the whole point is that it stops appearing in a URL.
    #[test]
    fn only_a_live_minted_token_opens_the_sandbox_document() {
        let state = McpAppProxyState::new("the-daemon-secret".to_string());

        assert!(!state.token_valid(None), "a missing token must not pass");
        assert!(!state.token_valid(Some("")), "an empty token must not pass");
        assert!(
            !state.token_valid(Some("the-daemon-secret")),
            "the daemon secret must not be usable as a proxy token"
        );
        assert!(!state.token_valid(Some("0".repeat(32).as_str())));

        let token = state.mint();
        assert_eq!(token.len(), 32, "128 bits, hex: {token}");
        assert!(state.token_valid(Some(&token)));
        assert!(
            state.token_valid(Some(&token)),
            "a token stays usable within its TTL so an iframe re-render is not fatal"
        );
        assert_ne!(state.mint(), state.mint(), "tokens must not repeat");
    }

    /// An expired token is refused, and the map does not grow without bound.
    #[test]
    fn tokens_expire_and_the_store_is_bounded() {
        let state = McpAppProxyState::new("s".to_string());
        let stale = "f".repeat(32);
        state
            .tokens
            .lock()
            .unwrap()
            .insert(stale.clone(), Instant::now() - Duration::from_secs(1));
        assert!(!state.token_valid(Some(&stale)), "an expired token is dead");

        for _ in 0..(MAX_LIVE_TOKENS + 50) {
            state.mint();
        }
        assert!(
            state.tokens.lock().unwrap().len() <= MAX_LIVE_TOKENS,
            "the live-token map must stay bounded"
        );
    }
}
