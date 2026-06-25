mod commands;
mod configuration;
mod error;
mod logging;
mod openapi;
mod routes;
mod state;
mod tunnel;

use biorouter::config::paths::Paths;
use biorouter_mcp::{
    mcp_server_runner::{serve, McpCommand},
    AutoVisualiserRouter, ComputerControllerServer, DeveloperServer, MemoryServer, TutorialServer,
};
use clap::{Parser, Subcommand};

// Tuned jemalloc as the global allocator for the long-running daemon (default-on
// `jemalloc` feature). Returns freed pages to the OS far more readily than the
// system allocator under the agent's per-turn allocation churn.
#[cfg(all(feature = "jemalloc", not(target_os = "windows")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the agent server
    Agent,
    /// Run the MCP server
    Mcp {
        #[arg(value_parser = clap::value_parser!(McpCommand))]
        server: McpCommand,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tune_allocator();
    let cli = Cli::parse();

    match cli.command {
        Commands::Agent => {
            commands::agent::run().await?;
        }
        Commands::Mcp { server } => {
            logging::setup_logging(Some(&format!("mcp-{}", server.name())))?;
            match server {
                McpCommand::AutoVisualiser => serve(AutoVisualiserRouter::new()).await?,
                McpCommand::ComputerController => serve(ComputerControllerServer::new()).await?,
                McpCommand::Memory => serve(MemoryServer::new()).await?,
                McpCommand::Tutorial => serve(TutorialServer::new()).await?,
                McpCommand::Developer => {
                    let bash_env = Paths::config_dir().join(".bash_env");
                    serve(
                        DeveloperServer::new()
                            .extend_path_with_shell(true)
                            .bash_env_file(Some(bash_env)),
                    )
                    .await?
                }
            }
        }
    }

    Ok(())
}

/// Lower jemalloc's dirty/muzzy decay so freed pages return to the OS within
/// ~1s instead of being retained (jemalloc's own defaults retain aggressively).
/// Applied to all current arenas (`MALLCTL_ARENAS_ALL` = 4096) and as the
/// default for future arenas. Errors are ignored (`background_thread` is
/// unsupported on macOS). Set `BIOROUTER_DEBUG_ALLOC=1` to print applied values.
#[cfg(all(feature = "jemalloc", not(target_os = "windows")))]
fn tune_allocator() {
    use tikv_jemalloc_ctl::raw;
    unsafe {
        let _ = raw::write(b"arena.4096.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arena.4096.muzzy_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.muzzy_decay_ms\0", 1000_isize);
        let _ = raw::write(b"background_thread\0", true);
        if std::env::var_os("BIOROUTER_DEBUG_ALLOC").is_some() {
            let d: isize = raw::read(b"arenas.dirty_decay_ms\0").unwrap_or(-1);
            let m: isize = raw::read(b"arenas.muzzy_decay_ms\0").unwrap_or(-1);
            eprintln!("[alloc] jemalloc active; dirty_decay_ms={d} muzzy_decay_ms={m}");
        }
    }
}

#[cfg(not(all(feature = "jemalloc", not(target_os = "windows"))))]
fn tune_allocator() {}
