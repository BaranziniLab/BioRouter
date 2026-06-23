use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
    // BioRouter apps are opened directly in the browser (and connect a WebSocket),
    // so they can't send the secret-key header. Allow browser-facing GET reads
    // under /apps (serving the bundle + the per-app agent socket); management
    // verbs (POST/DELETE) still require the secret. The daemon binds localhost
    // only, matching the unauthenticated MCP UI proxy.
    if request.method() == axum::http::Method::GET
        && (path == "/apps" || path.starts_with("/apps/"))
    {
        return Ok(next.run(request).await);
    }

    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if !check_rate_limit(&client_ip) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let secret_key = request
        .headers()
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok());

    match secret_key {
        Some(key) if key == state => Ok(next.run(request).await),
        _ => {
            record_failed_attempt(&client_ip);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
