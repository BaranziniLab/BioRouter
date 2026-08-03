use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The constant-time secret comparison this middleware gates on.
///
/// The implementation moved to `routes` (with its rationale) so that
/// `routes::workspace`'s WebSocket gate — which checks the same server secret,
/// on a path this module exempts from the header check *and therefore from the
/// rate limiter* — can share it instead of re-implementing it. `src/routes/` is
/// compiled into the `biorouterd` binary as well as the lib and cannot name
/// `crate::auth`, so the shared direction is this one. Re-exported here so
/// `check_token` and `mod tests` are unaffected.
use crate::routes::secret_matches;

static FAILED_ATTEMPTS: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();

/// The SHA-256 digest of the user-action key, handed to this process on **stdin**
/// by whoever launched it (issue #56, DR-16).
///
/// `Option` inside the `OnceLock` so "installed, and there is no key" is
/// representable and distinct from "never installed". Both fail closed; keeping
/// them apart is what lets `commands::agent` log the warning exactly once.
///
/// ⚠ The **digest**, never the key. AR-11 measured this daemon's own API secret
/// to be recoverable from inside the daemon, so a credential the daemon holds in
/// full is a credential the model can present. A tool that reads this heap
/// recovers a value it cannot use: the guard hashes what the caller presented
/// and compares, so the stored bytes authenticate nothing. That asymmetry is the
/// only part of AR-11's residual this closes — the raw key lives in the Electron
/// main process, and a caller who can read *that* is unaffected (Open
/// question 20).
static USER_ACTION_DIGEST: OnceLock<Option<[u8; 32]>> = OnceLock::new();

/// Publish the digest read off stdin at startup. Called once, before the router
/// is built, from `commands::agent::run`.
pub fn install_user_action_digest(digest: Option<[u8; 32]>) {
    let _ = USER_ACTION_DIGEST.set(digest);
}

/// Does `presented` hash to `expected`?
///
/// Pure, so the whole rule is testable without a process global or a server —
/// which matters here because the alternative home for these assertions is an
/// `AppState`-backed HTTP test, and `AppState::new()` opens the developer's REAL
/// session database (see `routes::agent::working_dir_lock_tests`).
///
/// `expected: None` is "this daemon was handed no key" and fails closed: `just
/// run-server`, a hand-run `biorouterd agent` and every headless deployment land
/// there, and they refuse every raise — including one made by the person at the
/// keyboard (open question 23).
///
/// Compared without an early return, the same way [`secret_matches`] is, so a
/// caller cannot recover the digest one byte at a time by timing the response.
pub fn user_action_matches(presented: Option<&str>, expected: Option<&[u8; 32]>) -> bool {
    let (Some(presented), Some(expected)) = (presented, expected) else {
        return false;
    };
    let got = <sha2::Sha256 as sha2::Digest>::digest(presented.as_bytes());
    let mut diff = 0u8;
    for (x, y) in got.iter().zip(expected.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// What [`user_action_proof`] found. Three answers, not two, because two of them
/// need to be *said* differently: a caller who presented no proof is being told
/// "the user decides this", while a caller on a daemon that holds no key is
/// being told "this control is unavailable on this daemon" — and reporting the
/// second as the first sends the person at the keyboard hunting for a permission
/// they can never obtain (open question 23).
///
/// Both non-`Proven` answers refuse. The distinction is in the message only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserActionProof {
    /// The header carried a key that hashes to the installed digest.
    Proven,
    /// A digest is installed and the request did not match it.
    Unproven,
    /// This daemon was handed no user-action key on stdin, so there is nothing
    /// to verify against and every caller is refused — including the human.
    /// `just run-server`, a hand-run `biorouterd agent` and any headless
    /// deployment land here. (`just debug-server` does not: it pipes the
    /// published dev key.)
    NoKeyInstalled,
}

/// Did this request come from the user rather than from the model?
///
/// A **per-route** requirement, not a second gate on every request:
/// [`check_token`] is untouched, because refusing every identity-free request
/// would take the user's own model picker away along with the model's — the
/// posture DR-16 rejected. CORS already passes the header through; the daemon's
/// layer is `.allow_headers(Any)`.
///
/// ONE header, ONE key, ONE comparison. Every route that needs a proof of user
/// reads this; a second header name anywhere is the defect the tier route's own
/// gate looks for.
pub fn user_action_proof(headers: &axum::http::HeaderMap) -> UserActionProof {
    let Some(expected) = USER_ACTION_DIGEST.get().and_then(|d| d.as_ref()) else {
        return UserActionProof::NoKeyInstalled;
    };
    if user_action_matches(
        headers.get("X-User-Action").and_then(|v| v.to_str().ok()),
        Some(expected),
    ) {
        UserActionProof::Proven
    } else {
        UserActionProof::Unproven
    }
}

/// The boolean form, for the five raise channels that have one refusal to give.
///
/// Defined in terms of [`user_action_proof`] so there is exactly one place where
/// "does this request carry the proof" is decided — a second implementation that
/// happened to read the same header would still be a second answer.
pub fn is_user_action(headers: &axum::http::HeaderMap) -> bool {
    matches!(user_action_proof(headers), UserActionProof::Proven)
}

fn get_failed_attempts() -> &'static Mutex<HashMap<String, Vec<Instant>>> {
    FAILED_ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn check_rate_limit(client_ip: &str) -> bool {
    let mut map = get_failed_attempts()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let window = Duration::from_secs(60);
    let entry = map.entry(client_ip.to_string()).or_default();
    entry.retain(|t| now.duration_since(*t) < window);
    entry.len() < 20
}

fn record_failed_attempt(client_ip: &str) {
    let mut map = get_failed_attempts()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(client_ip.to_string())
        .or_default()
        .push(Instant::now());
}

fn is_public_app_get(method: &axum::http::Method, path: &str) -> bool {
    if method != axum::http::Method::GET {
        return false;
    }
    let Some(rest) = path.strip_prefix("/apps/") else {
        return false;
    };
    let mut segments = rest.split('/');
    let Some(id) = segments.next() else {
        return false;
    };
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return false;
    }
    // Keep this route list explicit so a future management GET does not become
    // unauthenticated merely because it lives below `/apps/{id}`.
    let tail = segments.collect::<Vec<_>>();
    matches!(
        tail.as_slice(),
        [] | [""] | ["agent"] | ["models"] | ["runstate"]
    ) || matches!(tail.as_slice(), ["dist" | "assets", _, ..])
}

/// Paths served without the `X-Secret-Key` header. Each one carries its own
/// gate; the list is a predicate rather than a chain of `||` inside
/// `check_token` so it is unit-testable — a security allowlist that no test
/// can reach is one refactor away from admitting `/ui/workspaceX`.
fn is_unauthenticated_path(path: &str) -> bool {
    matches!(
        path,
        "/status"
            | "/mcp-ui-proxy"
            | "/mcp-app-proxy"
            // BR-71: the desktop renderer opens this WebSocket, and a browser
            // WebSocket cannot send headers. The route carries its own two
            // gates — the same secret as a query token, plus the Origin check
            // (CSWSH) — in `routes::workspace::check_workspace_ws_auth`,
            // exactly as the app agent socket does (`apps::agent_ws`).
            | "/ui/workspace"
    )
}

pub async fn check_token(
    State(state): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();
    if is_unauthenticated_path(path) {
        return Ok(next.run(request).await);
    }
    // Biorouter apps are opened directly in the browser (and connect a WebSocket),
    // so they can't send the secret-key header. Allow browser-facing GET reads
    // of a *specific* app (serving the bundle + the per-app agent socket);
    // management operations and source/content export still require the secret.
    //
    // `GET /apps` -- the list -- is deliberately NOT exempt: it enumerates app
    // ids, and an id is all `/apps/{id}/agent` needs. That socket runs agent
    // turns and carries its own tool-approval frames, so it additionally
    // validates `Origin` (see `apps::agent_ws`).
    if is_public_app_get(request.method(), path) {
        return Ok(next.run(request).await);
    }

    // Key the throttle on the real peer. `x-forwarded-for` is client-supplied
    // and there is no reverse proxy in front of this daemon, so an attacker
    // could rotate it and defeat the limit entirely.
    let client_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if !check_rate_limit(&client_ip) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let secret_key = request
        .headers()
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok());

    match secret_key {
        Some(key) if secret_matches(key, &state) => Ok(next.run(request).await),
        _ => {
            record_failed_attempt(&client_ip);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_public_app_get, is_unauthenticated_path, secret_matches, user_action_matches};
    use axum::http::Method;

    /// SHA-256 of `key` — what the launcher hands the daemon on stdin, and the
    /// only form of the user-action key the daemon ever holds.
    fn digest_of(key: &str) -> [u8; 32] {
        <sha2::Sha256 as sha2::Digest>::digest(key.as_bytes()).into()
    }

    /// The body of `signature`'s function in `src`: everything after the
    /// signature up to the next line that is a bare `}` at column 0 — which is
    /// `awk '/sig/,/^}/'` in Rust, so the source scan below and Step 5's gate
    /// read the same span.
    ///
    /// `split_once` rather than byte-index slicing so the whole thing is
    /// panic-free by construction (`clippy::string_slice`).
    fn body_of<'a>(src: &'a str, signature: &str) -> &'a str {
        let (_, from_signature) = src
            .split_once(signature)
            .unwrap_or_else(|| panic!("`{signature}` is not in this file"));
        from_signature
            .split_once("\n}\n")
            .map_or(from_signature, |(body, _)| body)
    }

    #[test]
    fn a_daemon_with_no_user_action_key_refuses_every_raise() {
        // Open question 23, as an assertion. `just run-server`, a hand-run
        // `biorouterd agent`, and every headless deployment land here.
        assert!(!user_action_matches(Some("anything"), None));
        assert!(!user_action_matches(None, None));
        assert!(!user_action_matches(None, Some(&digest_of("k"))));
    }

    #[test]
    fn the_daemon_stores_a_digest_and_never_the_key() {
        let expected = digest_of("the-real-key");
        assert!(user_action_matches(Some("the-real-key"), Some(&expected)));
        assert!(!user_action_matches(Some("the-real-ke"), Some(&expected)));
        // The stored value is not itself presentable: handing the daemon back
        // what it holds must NOT authenticate. This is the assertion that fails
        // an implementation which stores the raw key "for simplicity".
        assert!(!user_action_matches(
            Some(&hex::encode(expected)),
            Some(&expected)
        ));
    }

    #[test]
    fn all_five_raise_channels_call_the_guard() {
        // A source scan, because the alternative -- five HTTP tests -- has to
        // build `AppState`, which opens the user's real session DB
        // (routes/agent.rs's `working_dir_lock_tests` doc comment). This is the
        // test that fails a PARTIAL implementation: covering `update_provider`
        // and leaving `add_extension` or the config routes open.
        //
        // `add_extension`'s guard is not `is_user_action`, and that is DR-16
        // (c) rather than an omission: attaching a private extension to a
        // public session is not a raise the USER can authorize either, so the
        // route refuses it outright and its guard is the refusal itself.
        //
        // `/config/remove` is the fifth, and the task's own enumeration said
        // four: deleting a capability key restores its DEFAULT, and for
        // `OLLAMA_HOST` that default is loopback, i.e. Private. See the comment
        // on `remove_config`.
        //
        // ⚠ What this scan CANNOT see, stated so nobody reads it as more than
        // it is: it proves each handler mentions its guard, not that the guard
        // runs before the mutation, and not that its condition is right. A
        // guard placed after `config.set`, or one reduced to `if
        // is_user_action(..) {}`, passes here. Step 5's gate group 1 is what
        // holds the shape of the condition; nothing in CI exercises these
        // routes at the HTTP layer, because `AppState::new()` may not be built
        // in a test.
        let agent_rs = include_str!("routes/agent.rs");
        let config_rs = include_str!("routes/config_management.rs");
        for (src, func, guard) in [
            (
                agent_rs,
                "async fn update_agent_provider",
                "is_user_action(",
            ),
            (
                agent_rs,
                "async fn agent_add_extension",
                "PrivateExtensionOverHttp",
            ),
            (config_rs, "pub async fn upsert_config", "is_user_action("),
            (config_rs, "pub async fn remove_config", "is_user_action("),
            (
                config_rs,
                "pub async fn set_config_provider",
                "is_user_action(",
            ),
        ] {
            assert!(
                body_of(src, func).contains(guard),
                "{func} does not consult the user-action guard (`{guard}`)"
            );
        }

        // A negative control, so the scan is provably not vacuous. This handler
        // sits in the same file, is not a raise channel, and must come back
        // WITHOUT the guard — if it does not, `body_of` is over-reading past a
        // function end and every assertion above is passing on someone else's
        // body.
        assert!(
            !body_of(agent_rs, "async fn agent_remove_extension").contains("is_user_action("),
            "the body scan is over-reading: a handler with no guard reported one"
        );
    }

    /// Issue #56 §12.4. Declassification is not a *raise*, so it is not one of
    /// the five channels above — it is the one channel that goes the other way,
    /// and it needs the same proof for the same reason: `check_token` compares
    /// one machine-wide bearer that AR-11 measured to be recoverable from inside
    /// the daemon, so an authenticated request is not evidence of a human.
    ///
    /// Its own module is where the behaviour is pinned
    /// (`routes::session::declassify_tests::the_route_needs_more_than_the_secret_key`
    /// drives all three credential sets through the real `check_token` layer).
    /// This scan is the cheap tripwire that survives a refactor which moves that
    /// test: `is_public_app_get` needs no change for this route — it only ever
    /// matches GETs under `/apps/{id}`, so a POST under `/sessions` can never
    /// reach the exemption — and with nothing in `check_token` to change either,
    /// the ONLY thing standing between the model and this route is the line this
    /// asserts is present.
    ///
    /// It also pins WHERE the proof-of-user is minted, which is the half its
    /// sibling in `privacy::declassify` cannot see.
    /// `the_proof_of_user_is_constructed_in_exactly_two_places` counts the
    /// constructor across the tree and requires exactly one call in this file
    /// (the other permitted site being the CLI's `declassify` subcommand, which
    /// has no route and no guard to assert about); this requires that call to be
    /// inside the body of the handler asserted above to consult the guard.
    /// Neither alone is enough — a count of one says
    /// nothing about which function holds it, and a guarded handler says nothing
    /// about a second, unguarded one next to it — and together they say the
    /// proof is minted once, behind the guard.
    #[test]
    fn the_declassify_route_consults_the_user_action_guard() {
        let session_rs = include_str!("routes/session.rs");
        let handler = body_of(session_rs, "async fn declassify_session");
        assert!(
            handler.contains("is_user_action("),
            "the declassify route does not consult the user-action guard"
        );
        // Split across two literals so this file does not itself become a place
        // that names the proof-of-user: its sibling audit asserts the set of
        // files containing that name is exactly {routes/session.rs}, and a
        // spelled-out needle here would make this test break that one.
        let mint = concat!("User", "Confirmation::from_typed_confirmation(");
        assert!(
            handler.contains(mint),
            "the proof-of-user is no longer minted inside the handler that checks the guard"
        );
        // Same negative control as above, in the same file: a handler that has
        // neither must come back with neither, or `body_of` is over-reading past
        // a function end and both assertions above are passing on someone else's
        // body. The mint is the one that needs this most — it sits at the very
        // bottom of `declassify_session`, so a scan that ran long would find it
        // no matter which function it was asked about.
        let unguarded = body_of(session_rs, "async fn get_session_extensions");
        assert!(
            !unguarded.contains("is_user_action("),
            "the body scan is over-reading: a handler with no guard reported one"
        );
        assert!(
            !unguarded.contains(mint),
            "the body scan is over-reading: a handler that mints nothing reported the proof"
        );
    }

    /// Issue #56 DR-18. The knowledge-base tier route is the second channel that
    /// goes the *other* way, and it needs the same proof for the same reason:
    /// `check_token` compares one machine-wide bearer that AR-11 measured to be
    /// recoverable from inside the daemon, so an authenticated request is not
    /// evidence of a human.
    ///
    /// The behaviour is pinned over HTTP by
    /// `knowledge_routes::tier_route::the_tier_route_needs_more_than_the_secret_key`
    /// and its keyless sibling; this is the cheap tripwire that survives a
    /// refactor which moves those. `is_public_app_get` needs no change for this
    /// route — it only ever matches GETs under `/apps/{id}`, so a POST under
    /// `/knowledge` can never reach the exemption — and with nothing in
    /// `check_token` to change either, the ONLY thing standing between the model
    /// and this route is the line this asserts is present.
    ///
    /// It also pins WHERE the proof-of-user is minted, the half its sibling in
    /// `knowledge::tier_user` cannot see: that test counts the constructor across
    /// the tree and requires exactly one call in `routes/knowledge.rs`; this
    /// requires that call to be inside the body of the handler that consults the
    /// guard. Neither alone is enough.
    #[test]
    fn the_kb_tier_route_consults_the_user_action_guard() {
        let knowledge_rs = include_str!("routes/knowledge.rs");
        let handler = body_of(knowledge_rs, "pub async fn set_kb_tier");
        assert!(
            handler.contains("user_action_proof("),
            "the knowledge-base tier route does not consult the user-action guard"
        );
        // Split across two literals so this file does not itself become a place
        // that names the proof-of-user: `tier_user`'s audit asserts the set of
        // files containing that name is exactly {service.rs, routes/knowledge.rs},
        // and a spelled-out needle here would break it.
        let mint = concat!("User", "KbTierChange::from_user_action(");
        assert!(
            handler.contains(mint),
            "the proof-of-user is no longer minted inside the handler that checks the guard"
        );
        // Both refusal arms, so a handler that admits the keyless daemon — the
        // easy mistake, because `Proven` is the only arm the happy path needs —
        // fails here rather than in production.
        assert!(
            handler.contains("NoKeyInstalled"),
            "the tier route does not distinguish a daemon with no user-action key"
        );

        // The negative control, so the scan is provably not vacuous: a handler in
        // the same file that has neither must come back with neither, or
        // `body_of` is over-reading past a function end.
        let unguarded = body_of(knowledge_rs, "pub async fn get_kb_tier");
        assert!(
            !unguarded.contains("user_action_proof("),
            "the body scan is over-reading: a handler with no guard reported one"
        );
        assert!(
            !unguarded.contains(mint),
            "the body scan is over-reading: a handler that mints nothing reported the proof"
        );
    }

    #[test]
    fn the_workspace_socket_is_exempt_and_nothing_that_merely_starts_with_it_is() {
        assert!(is_unauthenticated_path("/ui/workspace"));
        // Exact match only. A `starts_with` would exempt every future route
        // under this prefix, and the daemon has no other authentication.
        assert!(!is_unauthenticated_path("/ui/workspaceX"));
        assert!(!is_unauthenticated_path("/ui/workspace/admin"));
        assert!(!is_unauthenticated_path("/ui/workspace?secret=x"));
        // The three that were already exempt still are.
        assert!(is_unauthenticated_path("/status"));
        assert!(is_unauthenticated_path("/mcp-ui-proxy"));
        assert!(is_unauthenticated_path("/mcp-app-proxy"));
        // …and nothing else is.
        assert!(!is_unauthenticated_path("/reply"));
        assert!(!is_unauthenticated_path("/sessions"));
    }

    #[test]
    fn secret_compare_is_exact() {
        assert!(secret_matches("abc", "abc"));
        assert!(!secret_matches("abc", "abd"));
        assert!(!secret_matches("ab", "abc"));
        assert!(!secret_matches("", "abc"));
    }

    #[test]
    fn app_exports_still_require_the_server_secret() {
        assert!(is_public_app_get(&Method::GET, "/apps/example/"));
        assert!(is_public_app_get(&Method::GET, "/apps/example/dist/app.js"));
        assert!(is_public_app_get(&Method::GET, "/apps/example/agent"));
        assert!(!is_public_app_get(&Method::GET, "/apps/example/export"));
        assert!(!is_public_app_get(&Method::GET, "/apps/example/export/"));
        assert!(!is_public_app_get(
            &Method::GET,
            "/apps/example/future-admin"
        ));
        assert!(!is_public_app_get(&Method::GET, "/apps/bad%2Fid/"));
        assert!(!is_public_app_get(&Method::POST, "/apps/example/build"));
    }
}
