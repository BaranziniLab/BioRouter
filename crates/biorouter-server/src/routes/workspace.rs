//! BR-71 §4.3: each Electron window connects once at startup with a stable
//! window_id. Outbound: workspace command frames. Inbound: workspace_echo
//! (debounced layout report) and workspace_result (resolves parked round
//! trips). Auth: the server secret as a query token (the browser WebSocket API
//! cannot set headers) + the origin gate — same two-gate shape as the app
//! agent socket (apps.rs:538-556), with the Electron file origin allowed.

use std::sync::Arc;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde_json::Value;

use crate::state::AppState;
use crate::workspace::bridge;

/// State for this module's routes: the app state PLUS the server secret.
///
/// The secret is not global — the daemon threads it into
/// `routes::configure(state, secret_key)`, which already hands it by value to
/// exactly one route that needs it, `mcp_app_proxy::routes(secret_key)`. This
/// route is the second, so `configure` clones it for both.
#[derive(Clone)]
struct WorkspaceRouteState {
    /// Held for the socket handler's future use (session lookups when the
    /// renderer starts echoing per-tab state); the auth gate needs only
    /// `secret`.
    state: Arc<AppState>,
    secret: String,
}

fn check_workspace_ws_auth(
    origin: Option<&str>,
    token: Option<&str>,
    expected: &str,
) -> Result<(), &'static str> {
    if let Some(origin) = origin {
        // The packaged renderer is loaded from a `file:` URL
        // (`ui/desktop/src/main.ts`, `pathToFileURL`), so it presents this
        // origin; the dev renderer presents a loopback origin.
        //
        // **"null" is NOT admitted.** It is the opaque origin of any sandboxed
        // frame — including the agent-authored figures this very app renders
        // through `/mcp-ui-proxy`, which is served unauthenticated
        // (`auth.rs`'s `is_unauthenticated_path`) with
        // `sandbox='allow-scripts allow-downloads'` and no `allow-same-origin`
        // (`routes/templates/mcp_ui_proxy.html:43`, pinned by
        // `routes/mcp_ui_proxy.rs:45`). `routes/mod.rs`'s own `origin_tests`
        // rejects it by name (`assert!(!is_local_origin("null"))`).
        // This gate must stay at least as strict as `apps::check_ws_auth`
        // (`apps.rs:538-546`), which is the route the design claims parity with.
        if origin != "file://" && !super::is_local_origin(origin) {
            return Err("cross-origin connect rejected");
        }
    }
    // Constant time, not `!=`. This is the SAME server secret `check_token`
    // guards, and `/ui/workspace` is the one path exempt from `check_token`
    // (`auth::is_unauthenticated_path`) — which means it is also exempt from
    // that middleware's rate limiter, so an attacker here gets unlimited,
    // unthrottled timing samples against the daemon's master key. `str` equality
    // is a length check plus an early-returning memcmp. `secret_matches` is
    // `check_token`'s own comparator, shared rather than re-implemented so the
    // two can never drift; its doc comment carries the invariant.
    if !token.is_some_and(|token| super::secret_matches(token, expected)) {
        return Err("missing or invalid workspace socket secret");
    }
    Ok(())
}

async fn workspace_ws(
    Query(params): Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
    State(rs): State<WorkspaceRouteState>,
) -> Response {
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|o| o.to_str().ok());
    let state = rs.state.clone();
    if let Err(reason) =
        check_workspace_ws_auth(origin, params.get("secret").map(String::as_str), &rs.secret)
    {
        tracing::warn!(
            origin = origin.unwrap_or("<none>"),
            "rejected workspace WS: {reason}"
        );
        return (axum::http::StatusCode::FORBIDDEN, reason).into_response();
    }
    let window_id = params
        .get("window_id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string());
    // Task 31's live gate has to record what the packaged renderer actually
    // sends here: whether Chromium presents `file://` or the opaque `null` on a
    // handshake from a `file:` page is version-dependent. If it is `null`, the
    // fix is on the client (connect through the loopback dev-server origin, or
    // open the socket from the main process) — NOT to widen the gate above,
    // which would admit every sandboxed agent-authored frame in the app.
    tracing::info!(
        origin = origin.unwrap_or("<none>"),
        window_id = %window_id,
        "workspace WS handshake accepted"
    );
    ws.on_upgrade(move |socket| handle_workspace_socket(socket, state, window_id))
}

async fn handle_workspace_socket(socket: WebSocket, _state: Arc<AppState>, window_id: String) {
    use futures::{SinkExt, StreamExt};
    let bridge = bridge::bridge_for(&window_id);
    let (mut outbound_rx, token) = bridge.attach();
    let (mut socket_tx, mut socket_rx) = socket.split();

    loop {
        tokio::select! {
            frame = outbound_rx.recv() => match frame {
                Some(frame) => {
                    let text = frame.to_string();
                    if socket_tx.send(WsMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                None => break, // a newer connection replaced us
            },
            inbound = socket_rx.next() => match inbound {
                Some(Ok(WsMessage::Text(text))) => {
                    let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                    apply_inbound_frame(&bridge, value);
                }
                Some(Ok(WsMessage::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
    bridge.detach(token);
}

/// The renderer→daemon frame vocabulary, lifted out of the socket loop so it has
/// a test (`inbound_frames_reach_the_bridge_by_type`). Everything above it is
/// transport; this is the behaviour.
fn apply_inbound_frame(bridge: &bridge::WorkspaceBridge, value: Value) {
    match value.get("type").and_then(Value::as_str) {
        Some("workspace_echo") => bridge.store_echo(value),
        Some("workspace_result") => {
            if let Some(id) = value.get("request_id").and_then(Value::as_str) {
                bridge.resolve(id, value.clone());
            }
        }
        _ => {}
    }
}

pub fn routes(state: Arc<AppState>, secret_key: String) -> Router {
    Router::new()
        .route("/ui/workspace", get(workspace_ws))
        .with_state(WorkspaceRouteState {
            state,
            secret: secret_key,
        })
}

/// ⚠ **These are the two pure halves only.** Nothing here reaches
/// `workspace_ws` or `handle_workspace_socket`, so a build that calls NEITHER of
/// them — an unauthenticated socket, or one that drops every renderer frame —
/// keeps this module green, and so does one where `routes::configure` never
/// merges this router or `auth::is_unauthenticated_path` drops
/// `"/ui/workspace"`. The mounted socket is tested end to end over a real
/// loopback connection in **`tests/workspace_socket.rs`**, which fails on all
/// four; it cannot live here because it must name `auth::check_token`, and
/// `src/routes/` is compiled into the `biorouterd` binary as well as the lib.
/// **Run it too:** `cargo test -p biorouter-server --test workspace_socket`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_auth_requires_secret_and_local_or_app_origin() {
        let secret = "test-secret";
        // Browser-set web origins must be loopback (CSWSH — is_local_origin,
        // routes/mod.rs:9-24).
        assert!(check_workspace_ws_auth(Some("https://evil.com"), Some(secret), secret).is_err());
        assert!(
            check_workspace_ws_auth(Some("http://127.0.0.1:5173"), Some(secret), secret).is_ok()
        );
        // Decision 3's Electron allowance, kept to ONE measured literal: the
        // packaged renderer loads from a file: URL (main.ts `pathToFileURL`).
        assert!(check_workspace_ws_auth(Some("file://"), Some(secret), secret).is_ok());
        // "null" is REFUSED. It is the opaque origin of every sandboxed frame,
        // including the agent-authored figures this app serves itself through
        // the unauthenticated /mcp-ui-proxy (sandbox without allow-same-origin
        // — the attribute is set in the served document,
        // routes/templates/mcp_ui_proxy.html:43, and pinned by the assertion at
        // routes/mcp_ui_proxy.rs:45) — and routes/mod.rs's own `origin_tests`
        // rejects it by name. Admitting it would make this gate strictly weaker than
        // `apps::check_ws_auth` (apps.rs:538-546), the route the design claims
        // parity with, leaving the socket secret-only.
        assert!(check_workspace_ws_auth(Some("null"), Some(secret), secret).is_err());
        assert!(check_workspace_ws_auth(None, Some(secret), secret).is_ok());
        // Wrong/missing secret always refuses.
        assert!(check_workspace_ws_auth(None, Some("wrong"), secret).is_err());
        assert!(check_workspace_ws_auth(None, None, secret).is_err());
        // Same length, differing in one byte, and a prefix: the comparison is
        // `secret_matches`, which returns early on LENGTH only. A call that got
        // its arguments confused, or compared lengths alone, passes the two
        // cases above (`"wrong"` is 5 bytes against 11) and fails these.
        assert!(check_workspace_ws_auth(None, Some("test-secreT"), secret).is_err());
        assert!(check_workspace_ws_auth(None, Some("test-secre"), secret).is_err());
    }

    /// The socket loop's INBOUND vocabulary. Without this, `handle_workspace_socket`
    /// — the half that turns renderer frames into bridge state — has no coverage at
    /// all: a loop that parsed `workspace_echo` into nothing would leave
    /// `workspace_list`'s `gui` block permanently empty, and a loop that never
    /// called `resolve` would leave every `emit_and_wait` to time out after 10 s,
    /// with `check_workspace_ws_auth`'s tests still green.
    ///
    /// The dispatch is extracted (`apply_inbound_frame`) so it is testable without
    /// a live WebSocket; the loop below is a two-line `match` over it.
    #[test]
    fn inbound_frames_reach_the_bridge_by_type() {
        use crate::workspace::bridge::WorkspaceBridge;
        let bridge = WorkspaceBridge::new();
        let (_rx, _token) = bridge.attach();

        apply_inbound_frame(
            &bridge,
            serde_json::json!({
                "type": "workspace_echo", "window_id": "w1",
                "focused_session": "s1", "layout": []
            }),
        );
        assert_eq!(
            bridge.last_echo().unwrap()["focused_session"],
            "s1",
            "workspace_echo must land in the bridge's last_echo — it IS workspace_list's `gui`"
        );

        // A result frame resolves the parked request it names, and only that one.
        let (tx, mut rx_result) = tokio::sync::oneshot::channel::<serde_json::Value>();
        bridge.insert_pending_for_test("wsreq-1", tx);
        apply_inbound_frame(
            &bridge,
            serde_json::json!({"type": "workspace_result", "request_id": "wsreq-9", "ok": false}),
        );
        assert!(
            rx_result.try_recv().is_err(),
            "a mismatched request_id resolves nothing"
        );
        apply_inbound_frame(
            &bridge,
            serde_json::json!({"type": "workspace_result", "request_id": "wsreq-1", "ok": true}),
        );
        assert_eq!(rx_result.try_recv().unwrap()["ok"], true);

        // Anything else is ignored, not treated as an echo.
        apply_inbound_frame(
            &bridge,
            serde_json::json!({"type": "hello", "focused_session": "s9"}),
        );
        assert_eq!(bridge.last_echo().unwrap()["focused_session"], "s1");
    }
}
