//! Sandboxed compute MCP server for BRSDK apps — the "run sandboxed compute" ask.
//!
//! Runs commands/code through a [`biorouter_sandbox::SandboxClient`] (the local
//! workspace-jailed backend, or Docker with `--network none` + dropped caps +
//! resource limits), constructed per app from its `compute` capability. Wired
//! live via the same `add_inprocess_server` seam as files/datasql.

use std::sync::Arc;
use std::time::Duration;

use biorouter_sandbox::{create, NetworkPolicy, SandboxClient, SandboxSpec};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, ServerCapabilities,
        ServerInfo,
    },
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::agent_drafter::manifest::ComputeCapability;

fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

#[cfg(unix)]
const SHELL_PROGRAM: &str = "/bin/sh";

#[cfg(not(unix))]
const SHELL_PROGRAM: &str = "sh";

#[derive(Debug, Deserialize, Serialize, rmcp::schemars::JsonSchema)]
pub struct RunParams {
    /// A shell command to run inside the sandbox.
    pub command: String,
}

#[derive(Debug, Deserialize, Serialize, rmcp::schemars::JsonSchema)]
pub struct PythonParams {
    /// Python source to execute (`python3 -c <code>`).
    pub code: String,
}

/// Sandboxed compute server bound to one app's [`SandboxClient`].
#[derive(Clone)]
pub struct ComputeServer {
    tool_router: ToolRouter<Self>,
    sandbox: Arc<dyn SandboxClient>,
}

impl ComputeServer {
    pub fn new(sandbox: Arc<dyn SandboxClient>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            sandbox,
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ComputeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-compute".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Run shell commands or Python inside this app's sandbox (confined to its \
                 workspace; network may be disabled). Returns {stdout, stderr, code, timed_out}."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

#[tool_router(router = tool_router)]
impl ComputeServer {
    #[tool(
        name = "compute_run",
        description = "Run a shell command inside the app sandbox. Returns {stdout, stderr, code, timed_out}."
    )]
    pub async fn compute_run(
        &self,
        params: Parameters<RunParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let cmd = params.0.command;
        let out = self
            .sandbox
            .exec(&[SHELL_PROGRAM.to_string(), "-lc".to_string(), cmd], None)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let json = serde_json::to_string(&out).map_err(|e| invalid(e.to_string()))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "compute_python",
        description = "Run Python (python3 -c <code>) inside the app sandbox. Returns {stdout, stderr, code, timed_out}."
    )]
    pub async fn compute_python(
        &self,
        params: Parameters<PythonParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let code = params.0.code;
        let out = self
            .sandbox
            .exec(&["python3".to_string(), "-c".to_string(), code], None)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let json = serde_json::to_string(&out).map_err(|e| invalid(e.to_string()))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

/// Build a [`ComputeServer`] from an app's `compute` capability over `workspace`.
/// Returns `None` when `sandbox == "none"` (compute not granted). On a hard-fail
/// to construct the requested backend, returns `None` (caller logs + skips) — a
/// granted-but-unbuildable sandbox must NOT silently fall through to host exec.
pub fn for_capability(
    workspace: std::path::PathBuf,
    cap: &ComputeCapability,
) -> Option<ComputeServer> {
    if cap.sandbox == "none" {
        return None;
    }
    let network = if cap.network == "host" {
        NetworkPolicy::Host
    } else {
        NetworkPolicy::None
    };
    let mut spec = SandboxSpec::new(workspace)
        .with_timeout(Duration::from_secs(cap.timeout_s))
        .with_network(network);
    if let Some(m) = &cap.max_mem {
        spec = spec.with_max_mem(m.clone());
    }
    if let Some(c) = cap.cpus {
        spec = spec.with_cpus(c);
    }
    if let Some(img) = &cap.image {
        spec = spec.with_image(img.clone());
    }
    let client = create(spec, &cap.sandbox).ok()?;
    Some(ComputeServer::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(r: &CallToolResult) -> String {
        serde_json::to_string(r).unwrap()
    }

    fn local_server() -> (tempfile::TempDir, ComputeServer) {
        let dir = tempfile::tempdir().unwrap();
        let cap = ComputeCapability {
            sandbox: "local".to_string(),
            timeout_s: 30,
            network: "none".to_string(),
            max_mem: None,
            cpus: None,
            image: None,
        };
        let srv = for_capability(dir.path().to_path_buf(), &cap).expect("local sandbox");
        (dir, srv)
    }

    #[test]
    fn none_sandbox_yields_no_server() {
        let dir = tempfile::tempdir().unwrap();
        let cap = ComputeCapability::default(); // sandbox = "none"
        assert!(for_capability(dir.path().to_path_buf(), &cap).is_none());
    }

    #[tokio::test]
    async fn compute_run_executes_in_local_sandbox() {
        let (_d, srv) = local_server();
        let r = srv
            .compute_run(Parameters(RunParams {
                command: "echo brsdk-compute-ok".to_string(),
            }))
            .await
            .unwrap();
        let t = text(&r);
        // (The ExecOutput JSON is nested as an escaped string inside the
        // CallToolResult, so assert on stable substrings rather than exact JSON.)
        assert!(t.contains("brsdk-compute-ok"), "stdout missing: {t}");
        assert!(t.contains("timed_out"), "expected ExecOutput shape: {t}");
        assert!(
            !t.contains("timed_out\\\":true"),
            "should not have timed out: {t}"
        );
    }

    #[tokio::test]
    async fn compute_run_runs_in_the_workspace_cwd() {
        let (dir, srv) = local_server();
        // The sandbox cwd is the workspace, so a relative write lands there.
        srv.compute_run(Parameters(RunParams {
            command: "echo hi > out.txt".to_string(),
        }))
        .await
        .unwrap();
        assert!(
            dir.path().join("out.txt").exists(),
            "command ran in the workspace cwd"
        );
    }
}
