//! One invariant, checked against the REAL bundled extension registry:
//!
//!   **every tool the `code_execution` sandbox advertises must be callable
//!   through every import form the sandbox documents.**
//!
//! Issue #93 broke that for exactly one tool — `chatrecall`, the only built-in
//! whose extension key equals its own tool name — and no test noticed for the
//! life of the repository, because every fixture in
//! `code_execution_extension.rs` used a server and a tool with *different*
//! names. A hand-written fixture can only cover the shapes its author thought
//! of; this test derives its cases from whatever is actually installed, so a
//! newly bundled extension (or a newly added tool on an existing one) is
//! covered the day it lands, with no edit here.
//!
//! It is deliberately shaped as "enumerate reality, assert the property" rather
//! than "assert chatrecall works": the collision is a *class* of bug, and the
//! third-party MCP ecosystem is full of single-tool servers named after their
//! tool (`fetch`/`fetch`, `sequentialthinking`, …) that this same code path
//! would have silently eaten.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::object;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use biorouter::agents::extension::ExtensionConfig;
use biorouter::agents::extension_manager::ExtensionManager;
use biorouter::session::SessionManager;

const SESSION: &str = "module-invariants";

/// Every bundled extension we can stand up in-process without network,
/// credentials or a spawned subprocess.
async fn manager_with_the_bundled_extensions() -> Arc<ExtensionManager> {
    let temp = tempfile::TempDir::new().unwrap();
    let session_manager = Arc::new(SessionManager::new(temp.path().to_path_buf()));
    let manager = Arc::new(ExtensionManager::new(
        Arc::new(Mutex::new(None)),
        session_manager,
    ));

    for name in [
        "developer",
        "computercontroller",
        "autovisualiser",
        "memory",
        "knowledge",
        "agent_drafter",
    ] {
        // A builtin that cannot start in this environment must not silently
        // shrink the matrix — but it must not fail the run either, since the
        // point is the property, not the inventory. Record and move on; the
        // assertion at the end of the test guards against an EMPTY matrix.
        let _ = manager
            .add_extension(ExtensionConfig::Builtin {
                name: name.to_string(),
                description: name.to_string(),
                display_name: Some(name.to_string()),
                timeout: Some(300),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await;
    }

    for name in [
        "chatrecall",
        "todo",
        "skills",
        "workspace",
        "code_execution",
    ] {
        let _ = manager
            .add_extension(ExtensionConfig::Platform {
                name: name.to_string(),
                description: name.to_string(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await;
    }

    manager
}

async fn exec(manager: &Arc<ExtensionManager>, code: &str) -> String {
    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__execute_code".into(),
        arguments: Some(object!({ "code": code })),
    };
    let dispatched = manager
        .dispatch_tool_call(
            SESSION,
            call,
            biorouter::privacy::CallCapability::public_enforced(),
            CancellationToken::new(),
        )
        .await
        .expect("dispatch");
    let result = dispatched.result.await.expect("tool result");
    result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// JS-source-safe? Module specifiers and property keys are interpolated into a
/// generated script, so a name carrying a quote or a backslash would produce a
/// syntax error that reads like a failed assertion. No bundled name does, and a
/// name that did would be a separate bug — surface it as one.
fn is_js_safe(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[tokio::test]
async fn every_bundled_tool_is_callable_through_every_documented_import_form() {
    let manager = manager_with_the_bundled_extensions().await;

    let tools = manager
        .get_prefixed_tools_excluding("code_execution", None)
        .await
        .expect("tool list");

    let mut by_server: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tool in &tools {
        let Some((server, tool_name)) = tool.name.as_ref().split_once("__") else {
            continue;
        };
        by_server
            .entry(server.to_string())
            .or_default()
            .push(tool_name.to_string());
    }

    assert!(
        by_server.len() >= 4,
        "the matrix collapsed to {} servers — the fixture stopped loading extensions, \
         so a green result here would prove nothing: {:?}",
        by_server.len(),
        by_server.keys().collect::<Vec<_>>()
    );

    let mut checked = 0usize;
    let mut collisions = Vec::new();

    for (server, tool_names) in &by_server {
        if !is_js_safe(server) || !tool_names.iter().all(|t| is_js_safe(t)) {
            panic!(
                "server `{server}` or one of its tools has a name that cannot be written into a \
                 JS module specifier / property key: {tool_names:?}"
            );
        }
        if tool_names.iter().any(|t| t == server) {
            collisions.push(server.clone());
        }

        // One script per server: assert the namespace form, the bracket form and
        // the server-named form all yield a function for EVERY tool.
        let list = tool_names
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let code = format!(
            r#"import * as ns from "{server}";
const names = [{list}];
const bad = [];
for (const n of names) {{
  if (typeof ns[n] !== "function") bad.push(n + ":ns=" + typeof ns[n]);
  const viaServer = ns["{server}"];
  if (viaServer !== undefined && typeof viaServer[n] !== "function") {{
    bad.push(n + ":viaServer=" + typeof viaServer[n]);
  }}
}}
record_result(bad.length === 0 ? "ok" : bad.join(","));"#
        );

        let out = exec(&manager, &code).await;
        assert!(
            out.contains("\"ok\""),
            "module `{server}` does not expose every tool as a callable.\n\
             This is the issue #93 shape: an export that is not a function makes the model's \
             call throw `TypeError: not a callable function`, with no hint that the tool exists.\n\
             tools: {tool_names:?}\nresult: {out}"
        );
        checked += tool_names.len();
    }

    // Not a requirement — a record. If this ever prints an empty list the test
    // above has stopped exercising the case it was written for, and the fixture
    // needs a synthetic collider adding back.
    assert!(
        collisions.contains(&"chatrecall".to_string()),
        "`chatrecall` is the built-in that makes this test non-vacuous (its extension key equals \
         its tool name). It is no longer in the matrix, so this test would now pass even with the \
         issue #93 defect restored. Either re-add it, or add a synthetic collider. \
         servers seen: {:?}",
        by_server.keys().collect::<Vec<_>>()
    );

    eprintln!(
        "checked {checked} tools across {} modules; name-colliding servers: {collisions:?}",
        by_server.len()
    );
}
