use crate::configuration;
use crate::state;
use anyhow::Result;
use axum::middleware;
use biorouter_server::auth::check_token;
use http::HeaderValue;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::info;

// Graceful shutdown signal
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

    tokio::select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Read the launcher's SHA-256 user-action digest off stdin, as one hex line
/// (issue #56, DR-16).
///
/// The pipe is at EOF once this returns, so every process the daemon later
/// spawns inherits an fd 0 that carries nothing: the digest is not re-readable
/// by a child, and the raw key was never there to begin with.
///
/// It must **never block a hand-started daemon**, so it is guarded twice.
async fn read_user_action_digest() -> Option<[u8; 32]> {
    use std::io::IsTerminal;
    // (1) A terminal is a human at a prompt, not a launcher with a key. Reading
    //     it would hang `just run-server` forever waiting for a line.
    if std::io::stdin().is_terminal() {
        return None;
    }
    // (2) And a pipe whose writer never closes would hang just as hard, so the
    //     read is bounded. 2s is far longer than a local `write` + `end`.
    //
    //     On a PLAIN OS THREAD rather than `spawn_blocking`, because a blocking
    //     read cannot be cancelled: on the timeout path the reader stays parked
    //     in `read_line`, holding `stdin().lock()`, for the life of the process.
    //     A tokio runtime waits for started blocking tasks when it drops, so a
    //     launcher that opens the pipe and never closes it would convert this
    //     bound from a 2s delay at startup into a hang at SHUTDOWN — the exact
    //     case this guard exists for. A detached thread is not the runtime's to
    //     wait on.
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let read = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line)
            .ok()
            .map(|_| line);
        // The receiver is gone on the timeout path; nothing to report to.
        let _ = tx.send(read);
    });
    let line = tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .ok()?
        .ok()??;
    let bytes = hex::decode(line.trim()).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

pub async fn run() -> Result<()> {
    crate::logging::setup_logging(Some("biorouterd"))?;

    let settings = configuration::Settings::new()?;

    let secret_key = std::env::var("BIOROUTER_SERVER__SECRET_KEY").unwrap_or_else(|_| {
        let bytes: [u8; 16] = rand::random();
        let key = hex::encode(bytes);
        tracing::warn!(
            "BIOROUTER_SERVER__SECRET_KEY not set; using randomly generated key for this session"
        );
        key
    });

    // Issue #56 DR-16. The proof-of-user, minted by whoever launched this
    // daemon and handed over on **stdin** — never in the environment and never
    // on argv, because AR-11 measured both to be recoverable in-process by any
    // tool that reads a caller-named path (`/proc/self/environ`) or, on macOS,
    // by `sysctl(KERN_PROCARGS2)`, which is not a path at all and which no
    // sandbox profile can gate.
    let user_action_digest = read_user_action_digest().await;
    if user_action_digest.is_none() {
        tracing::warn!(
            "no user-action key on stdin: this daemon will refuse every request that raises a \
             session's privacy capability, including one made by the person at the keyboard"
        );
    }
    biorouter_server::auth::install_user_action_digest(user_action_digest);

    let app_state = state::AppState::new().await?;

    // BR-71: publish the daemon's platform services to the `biorouter` crate so
    // the workspace extension's tools can reach the turn lock, the detached turn
    // runner and (Slice 2) the GUI bridge. Without this the tools degrade to
    // their headless behaviour even inside the daemon.
    crate::workspace::services::install_workspace_services(app_state.clone());

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            biorouter_server::routes::is_local_origin(origin.to_str().unwrap_or(""))
        }))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = crate::routes::configure(app_state.clone(), secret_key.clone())
        .layer(middleware::from_fn_with_state(
            secret_key.clone(),
            check_token,
        ))
        .layer(cors)
        // gzip large JSON payloads (config/providers/tools/session bodies). The
        // default predicate skips small bodies and `text/event-stream`, so the
        // streaming `/reply` SSE response is left unbuffered/uncompressed.
        .layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(settings.socket_addr()).await?;
    let local_addr = listener.local_addr()?;
    info!("listening on {}", local_addr);
    // Publish the base URL so in-process MCP tools (e.g. Agent Drafter's
    // `launch_app`) can emit absolute http://host:port/apps/<id>/ URLs.
    std::env::set_var("BIOROUTER_APP_BASE_URL", format!("http://{local_addr}"));

    let tunnel_manager = app_state.tunnel_manager.clone();
    tokio::spawn(async move {
        tunnel_manager.check_auto_start().await;
    });

    // `into_make_service_with_connect_info` is what puts the real peer address
    // in request extensions, so the auth throttle can key on it instead of the
    // client-supplied `x-forwarded-for` header.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    info!("server shutdown complete");
    Ok(())
}
