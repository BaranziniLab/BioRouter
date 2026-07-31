/// Origins the daemon is willing to be driven by. It binds loopback, so this is
/// every origin it legitimately serves.
///
/// CORS does not govern WebSocket handshakes and browsers freely open
/// cross-origin WebSockets, so any endpoint that upgrades must check this itself.
///
/// Parsed rather than prefix-matched: `http://127.0.0.1:` is a prefix of
/// `http://127.0.0.1:8080.evil.com`.
pub fn is_local_origin(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let (host, port) = match rest.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (rest, None),
    };
    if host != "localhost" && host != "127.0.0.1" {
        return false;
    }
    match port {
        None => true,
        Some(port) => !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()),
    }
}

/// Compare secrets without an early return, so a caller cannot recover the key
/// one byte at a time by timing the response.
///
/// It lives here, beside `is_local_origin`, rather than in `auth.rs` with the
/// middleware that is its main caller, for the reason that already put
/// `is_local_origin` here: `auth` is a **lib-only** module (`main.rs`
/// re-declares the module tree and pulls `check_token` from the lib —
/// `commands::agent`'s `use biorouter_server::auth::check_token`), while
/// `src/routes/` is compiled into the `biorouterd` binary as well, so nothing
/// under `src/routes/` can name `crate::auth`. `routes::workspace`'s socket gate
/// checks this very secret and has to use the middleware's own comparator rather
/// than a second copy of it. `auth` re-exports this, so `check_token` and
/// `auth::tests::secret_compare_is_exact` are unchanged.
pub(crate) fn secret_matches(candidate: &str, expected: &str) -> bool {
    let (a, b) = (candidate.as_bytes(), expected.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod origin_tests {
    use super::is_local_origin;

    #[test]
    fn accepts_loopback_origins() {
        assert!(is_local_origin("http://localhost"));
        assert!(is_local_origin("http://localhost:3000"));
        assert!(is_local_origin("http://127.0.0.1"));
        assert!(is_local_origin("http://127.0.0.1:8080"));
    }

    #[test]
    fn rejects_everything_else() {
        assert!(!is_local_origin("https://evil.com"));
        assert!(!is_local_origin("null"));
        assert!(!is_local_origin(""));
        // A suffix must not ride in on the prefix match.
        assert!(!is_local_origin("http://localhost.evil.com"));
        assert!(!is_local_origin("http://127.0.0.1.evil.com"));
        assert!(!is_local_origin("http://127.0.0.1:8080.evil.com"));
        assert!(!is_local_origin("http://127.0.0.1:"));
        // https to loopback is not an origin this server serves.
        assert!(!is_local_origin("https://127.0.0.1:8080"));
    }
}

pub mod action_required;
pub mod active_work;
pub mod agent;
pub mod apps;
pub mod audio;
pub mod config_management;
pub mod errors;
pub mod knowledge;
pub mod llamacpp;
pub mod mcp_app_proxy;
pub mod mcp_ui_proxy;
pub mod reply;
pub mod reset;
pub mod schedule;
pub mod session;
pub mod session_events;
pub mod setup;
pub mod status;
pub mod tunnel;
pub mod usage;
pub mod utils;
pub mod workflow;
pub mod workflow_utils;
pub mod workspace;

use std::sync::Arc;

use axum::Router;

// Function to configure all routes
pub fn configure(state: Arc<crate::state::AppState>, secret_key: String) -> Router {
    Router::new()
        .merge(status::routes(state.clone()))
        .merge(active_work::routes(state.clone()))
        .merge(reply::routes(state.clone()))
        .merge(reset::routes(state.clone()))
        .merge(action_required::routes(state.clone()))
        .merge(agent::routes(state.clone()))
        .merge(apps::routes(state.clone()))
        .merge(audio::routes(state.clone()))
        .merge(config_management::routes(state.clone()))
        .merge(workflow::routes(state.clone()))
        .merge(session::routes(state.clone()))
        .merge(usage::routes(state.clone()))
        .merge(schedule::routes(state.clone()))
        .merge(setup::routes(state.clone()))
        .merge(llamacpp::routes(state.clone()))
        .merge(tunnel::routes(state.clone()))
        .merge(mcp_ui_proxy::routes())
        .merge(mcp_app_proxy::routes(secret_key.clone()))
        .merge(workspace::routes(state.clone(), secret_key.clone()))
        .merge(session_events::routes(state.clone()))
        .nest(
            "/knowledge",
            knowledge::router(state.knowledge_service.clone()),
        )
}
