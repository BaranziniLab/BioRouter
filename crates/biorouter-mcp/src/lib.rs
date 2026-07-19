use etcetera::AppStrategyArgs;
use once_cell::sync::Lazy;
use rmcp::{ServerHandler, ServiceExt};
use std::collections::HashMap;

pub static APP_STRATEGY: Lazy<AppStrategyArgs> = Lazy::new(|| AppStrategyArgs {
    top_level_domain: "Block".to_string(),
    author: "Block".to_string(),
    app_name: "biorouter".to_string(),
});

pub mod active_work;
pub mod agent_drafter;
pub mod autovisualiser;
pub mod compute_server;
pub mod computercontroller;
pub mod datasql;
pub mod developer;
pub mod files_server;
pub mod knowledge;
pub mod mcp_server_runner;
mod memory;
pub mod paths;
pub mod secret_guard;
pub mod tutorial;

/// Sandbox-helper entry point (BR-69). Call this as the **first line of
/// `main()`** in every binary that may wrap a shell command (`biorouter`,
/// `biorouterd`): on Linux, if the process was re-exec'd with the hidden
/// `__br-sandbox` marker it applies the in-process Landlock/seccomp restrictions
/// and `execve`s the real program — it never returns. Otherwise it is a no-op
/// and normal startup proceeds. Re-exported here so the binaries do not each
/// need a direct `biorouter-sandbox` dependency.
pub use biorouter_sandbox::shell_sandbox::run_helper_if_invoked as run_shell_sandbox_helper_if_invoked;

/// The cross-platform OS-level shell sandbox (BR-69). Re-exported so callers
/// (`biorouter doctor`, startup logging) can report which tier is active without
/// a direct `biorouter-sandbox` dependency.
pub use biorouter_sandbox::shell_sandbox;

pub use agent_drafter::AgentDrafterServer;
pub use autovisualiser::AutoVisualiserRouter;
pub use computercontroller::ComputerControllerServer;
pub use developer::rmcp_developer::{set_path_jail_relaxed, DeveloperServer};
pub use knowledge::KnowledgeServer;
pub use memory::MemoryServer;
pub use tutorial::TutorialServer;

/// Spawns a builtin MCP server onto the given duplex transport. The optional
/// `working_dir` is the session's working directory; builtins that run shell
/// commands (the developer extension) use it, and the rest ignore it.
pub type SpawnServerFn =
    fn(tokio::io::DuplexStream, tokio::io::DuplexStream, Option<std::path::PathBuf>);

pub struct BuiltinDef {
    pub name: &'static str,
    pub spawn_server: SpawnServerFn,
}

fn spawn_and_serve<S>(
    name: &'static str,
    server: S,
    transport: (tokio::io::DuplexStream, tokio::io::DuplexStream),
) where
    S: ServerHandler + Send + 'static,
{
    tokio::spawn(async move {
        match server.serve(transport).await {
            Ok(running) => {
                let _ = running.waiting().await;
            }
            Err(e) => tracing::error!(builtin = name, error = %e, "server error"),
        }
    });
}

macro_rules! builtin {
    ($name:ident, $server_ty:ty) => {{
        fn spawn(
            r: tokio::io::DuplexStream,
            w: tokio::io::DuplexStream,
            _working_dir: Option<std::path::PathBuf>,
        ) {
            spawn_and_serve(stringify!($name), <$server_ty>::new(), (r, w));
        }
        (
            stringify!($name),
            BuiltinDef {
                name: stringify!($name),
                spawn_server: spawn,
            },
        )
    }};
}

pub static BUILTIN_EXTENSIONS: Lazy<HashMap<&'static str, BuiltinDef>> = Lazy::new(|| {
    HashMap::from([
        (
            "developer",
            BuiltinDef {
                name: "developer",
                // Not built via `builtin!`: the developer server runs shell
                // commands, so it honors the session working directory.
                spawn_server: {
                    fn spawn(
                        r: tokio::io::DuplexStream,
                        w: tokio::io::DuplexStream,
                        working_dir: Option<std::path::PathBuf>,
                    ) {
                        let mut server = DeveloperServer::new();
                        if let Some(dir) = working_dir {
                            server = server.with_working_dir(dir);
                        }
                        spawn_and_serve("developer", server, (r, w));
                    }
                    spawn
                },
            },
        ),
        builtin!(autovisualiser, AutoVisualiserRouter),
        builtin!(computercontroller, ComputerControllerServer),
        builtin!(memory, MemoryServer),
        builtin!(tutorial, TutorialServer),
        builtin!(agent_drafter, AgentDrafterServer),
        (
            "knowledge",
            BuiltinDef {
                name: "knowledge",
                spawn_server: {
                    fn spawn(
                        r: tokio::io::DuplexStream,
                        w: tokio::io::DuplexStream,
                        _working_dir: Option<std::path::PathBuf>,
                    ) {
                        spawn_and_serve(
                            "knowledge",
                            KnowledgeServer::new().expect("init knowledge server"),
                            (r, w),
                        );
                    }
                    spawn
                },
            },
        ),
    ])
});
