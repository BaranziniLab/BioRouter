//! Fixture generator for the artifact-panel browser harness.
//!
//! Ignored by default. It drives the real MCP transport — the same duplex path
//! `extension_manager` uses — so the HTML it writes is byte-for-byte what the
//! desktop artifact panel receives at runtime.
//!
//!   PREVIEW_FIXTURE_DIR=/tmp/fixtures cargo test -p biorouter-mcp \
//!     --test preview_fixture_dump -- --ignored --nocapture

use base64::{engine::general_purpose::STANDARD, Engine as _};
use biorouter_mcp::AgentDrafterServer;
use rmcp::model::{CallToolRequestParams, RawContent, ResourceContents};
use rmcp::ServiceExt;
use serde_json::json;
use tempfile::TempDir;

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("PREVIEW_FIXTURE_DIR").expect("set PREVIEW_FIXTURE_DIR"))
}

/// Pull the first `ui://` HTML blob out of a tool result.
fn blob_html(result: &rmcp::model::CallToolResult) -> Option<String> {
    for content in &result.content {
        if let RawContent::Resource(embedded) = &content.raw {
            if let ResourceContents::BlobResourceContents { blob, .. } = &embedded.resource {
                return Some(String::from_utf8(STANDARD.decode(blob).unwrap()).unwrap());
            }
        }
    }
    None
}

#[tokio::test]
#[ignore = "fixture generator; writes HTML to $PREVIEW_FIXTURE_DIR"]
async fn dump_agent_drafter_card() {
    let out = fixture_dir();
    std::fs::create_dir_all(&out).unwrap();

    let tmp = TempDir::new().unwrap();
    let server = AgentDrafterServer::with_root(tmp.path().to_path_buf());

    let (server_read, client_write) = tokio::io::duplex(1 << 22);
    let (client_read, server_write) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        if let Ok(running) = server.serve((server_read, server_write)).await {
            let _ = running.waiting().await;
        }
    });
    let client = ().serve((client_read, client_write)).await.unwrap();

    let created = client
        .call_tool(CallToolRequestParams {
            name: "create_app".into(),
            arguments: json!({
                "title": "Cohort Explorer",
                "description": "Ask questions about the trial cohort.",
                "kind": "static",
                "html": "<div class=\"p-6\"><h2 class=\"text-lg font-semibold\">Cohort Explorer</h2>\
                         <p class=\"text-sm opacity-70\">48 paired samples loaded.</p>\
                         <button id=\"go\">Summarise</button></div>"
            })
            .as_object()
            .cloned(),
            task: None,
            meta: None,
        })
        .await
        .unwrap();

    let html = blob_html(&created).expect("create_app must return a ui:// preview card");
    std::fs::write(out.join("agent-drafter-card.html"), &html).unwrap();
    println!("wrote agent-drafter-card.html ({} bytes)", html.len());

    // `launch_app`/`build_app` also return this card; that is asserted in the
    // agent_drafter unit tests, where an SDK-wired app can pass the lint harness.

    client.cancel().await.unwrap();
}
