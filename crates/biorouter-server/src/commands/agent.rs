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
