use anyhow::Result;
use biorouter_cli::cli::cli;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = biorouter_cli::logging::setup_logging(None, None) {
        eprintln!("Warning: Failed to initialize logging: {}", e);
    }

    cli().await
}
