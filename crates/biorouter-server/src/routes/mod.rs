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
pub mod schedule;
pub mod session;
pub mod setup;
pub mod status;
pub mod tunnel;
pub mod utils;
pub mod workflow;
pub mod workflow_utils;

use std::sync::Arc;

use axum::Router;

// Function to configure all routes
pub fn configure(state: Arc<crate::state::AppState>, secret_key: String) -> Router {
    Router::new()
        .merge(status::routes(state.clone()))
        .merge(active_work::routes(state.clone()))
        .merge(reply::routes(state.clone()))
        .merge(action_required::routes(state.clone()))
        .merge(agent::routes(state.clone()))
        .merge(apps::routes(state.clone()))
        .merge(audio::routes(state.clone()))
        .merge(config_management::routes(state.clone()))
        .merge(workflow::routes(state.clone()))
        .merge(session::routes(state.clone()))
        .merge(schedule::routes(state.clone()))
        .merge(setup::routes(state.clone()))
        .merge(llamacpp::routes(state.clone()))
        .merge(tunnel::routes(state.clone()))
        .merge(mcp_ui_proxy::routes(secret_key.clone()))
        .merge(mcp_app_proxy::routes(secret_key))
        .nest(
            "/knowledge",
            knowledge::router(state.knowledge_service.clone()),
        )
}
