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

/// Compare secrets without an early return, so a caller cannot recover the key
/// one byte at a time by timing the response.
fn secret_matches(candidate: &str, expected: &str) -> bool {
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

static FAILED_ATTEMPTS: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();

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

pub async fn check_token(
    State(state): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();
    if path == "/status" || path == "/mcp-ui-proxy" || path == "/mcp-app-proxy" {
        return Ok(next.run(request).await);
    }
    // Biorouter apps are opened directly in the browser (and connect a WebSocket),
    // so they can't send the secret-key header. Allow browser-facing GET reads
    // of a *specific* app (serving the bundle + the per-app agent socket);
    // management verbs (POST/DELETE) still require the secret.
    //
    // `GET /apps` -- the list -- is deliberately NOT exempt: it enumerates app
    // ids, and an id is all `/apps/{id}/agent` needs. That socket runs agent
    // turns and carries its own tool-approval frames, so it additionally
    // validates `Origin` (see `apps::agent_ws`).
    if request.method() == axum::http::Method::GET && path.starts_with("/apps/") {
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
    use super::secret_matches;

    #[test]
    fn secret_compare_is_exact() {
        assert!(secret_matches("abc", "abc"));
        assert!(!secret_matches("abc", "abd"));
        assert!(!secret_matches("ab", "abc"));
        assert!(!secret_matches("", "abc"));
    }
}
