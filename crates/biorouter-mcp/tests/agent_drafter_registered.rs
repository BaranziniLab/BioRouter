//! End-to-end checks for the Agent Drafter builtin: it is registered in the
//! builtin registry, and it serves its tools correctly over a real MCP transport
//! (the same duplex path `extension_manager` uses to spawn builtins).

use biorouter_mcp::AgentDrafterServer;
use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::ServiceExt;
use tempfile::TempDir;

#[test]
fn agent_drafter_is_in_builtin_registry() {
    assert!(biorouter_mcp::BUILTIN_EXTENSIONS.contains_key("agent_drafter"));
}

#[tokio::test]
async fn agent_drafter_serves_tools_over_mcp() {
    let tmp = TempDir::new().unwrap();
    let server = AgentDrafterServer::with_root(tmp.path().to_path_buf());

    // Mirror extension_manager's builtin spawn: two duplex pipes, server on one
    // side, MCP client on the other.
    let (server_read, client_write) = tokio::io::duplex(65536);
    let (client_read, server_write) = tokio::io::duplex(65536);

    tokio::spawn(async move {
        if let Ok(running) = server.serve((server_read, server_write)).await {
            let _ = running.waiting().await;
        }
    });

    let client = ().serve((client_read, client_write)).await.unwrap();

    // All Agent Drafter tools are advertised.
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    for expected in [
        "create_app",
        "configure_app",
        "update_app",
        "build_app",
        "lint_app",
        "launch_app",
        "list_apps",
        "read_app",
        "preview_app",
        "export_app",
        "delete_app",
        // Wave 1: the model can finally ask what this install actually has,
        // instead of inventing a knowledge-base or skill id to express a need.
        "list_platform_catalog",
        // Wave 2: typed declaration, so the contract can be stated in one call
        // instead of guessed at through whole-manifest rewrites.
        "declare_surface",
        "declare_profiles",
        "set_theme",
        "set_routes",
        // Wave 6: the executing check. Lint that RUNS the app — the only thing that
        // can catch a control which fires and delivers no turn.
        "smoke_app",
    ] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing tool '{expected}'; advertised: {names:?}"
        );
    }

    // create_app runs end-to-end and returns a ui:// preview resource.
    let mut args = serde_json::Map::new();
    args.insert("title".into(), serde_json::json!("Live Test"));
    args.insert("kind".into(), serde_json::json!("agentic"));
    let res = client
        .call_tool(CallToolRequestParams {
            task: None,
            name: "create_app".into(),
            arguments: Some(args),
            meta: None,
        })
        .await
        .unwrap();
    assert_ne!(res.is_error, Some(true), "create_app returned an error");
    let has_resource = res
        .content
        .iter()
        .any(|c| matches!(&c.raw, RawContent::Resource(_)));
    assert!(has_resource, "create_app should return a ui:// resource");

    // It actually persisted to the temp store.
    assert!(tmp.path().join("live-test/manifest.json").exists());

    let _ = client.cancel().await;
}
