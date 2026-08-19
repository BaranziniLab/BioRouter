//! The coding-agent tool bridge, end to end through the real router.
//!
//! What this pins is the lifecycle, because that is where a capability leaks: a
//! grant must be reachable exactly while its lease is alive, must be unreachable
//! the moment it is dropped, and must never be reachable by a nonce that was not
//! issued. The dispatch half is unit-tested next to the gate stack it runs; what
//! could only go wrong here is the wiring.
//!
//! Every test here is `#[serial]`. The published base URL and the grant registry
//! are both process-global — they have to be, because the HTTP handler runs on a
//! different task from the turn that issued the grant — so two tests publishing
//! different bases concurrently would assert against each other's value. Running
//! them in parallel passed and would have failed intermittently later, which is
//! worse than failing now.

#[path = "../src/test_sandbox.rs"]
mod test_sandbox;

use std::sync::Arc;

use serde_json::json;

use biorouter::agents::extension_manager::ExtensionManager;
use biorouter::config::BioRouterMode;
use biorouter::conversation::Conversation;
use biorouter::privacy::CallCapability;
use biorouter::providers::coding_agent::bridge;
use biorouter::session::session_manager::Session;
use biorouter::tool_inspection::ToolInspectionManager;

/// One advertised tool, so `tools/list` has something to prove it served from the
/// grant rather than from a hardcoded list.
fn advertised_tool() -> rmcp::model::Tool {
    rmcp::model::Tool::new(
        "spokeagent__query_knowledge_graph",
        "Run a Cypher query against SPOKE.",
        Arc::new(
            serde_json::from_value(json!({
                "type": "object",
                "properties": { "cypher": { "type": "string" } },
                "required": ["cypher"]
            }))
            .expect("a valid schema"),
        ),
    )
}

async fn grant() -> bridge::BridgeGrant {
    bridge::BridgeGrant::new(
        Session::default(),
        BioRouterMode::Auto,
        Arc::new(ExtensionManager::new(
            Arc::new(tokio::sync::Mutex::new(None)),
            Arc::new(biorouter::session::SessionManager::instance()),
        )),
        Arc::new(ToolInspectionManager::new()),
        CallCapability::public_enforced(),
        vec![advertised_tool()],
        Conversation::new_unvalidated(vec![]),
    )
}

/// The whole lifecycle in one test, because the assertions are sequential: a grant
/// is reachable, serves its own tool set, and stops existing when its lease drops.
#[tokio::test]
#[serial_test::serial]
async fn a_grant_is_reachable_only_while_its_lease_lives() {
    bridge::publish_base_url("http://127.0.0.1:65535");
    let lease = bridge::issue(grant().await).expect("a base URL is published");
    let nonce = lease
        .url()
        .rsplit('/')
        .next()
        .expect("the url ends in the nonce")
        .to_string();

    // Reachable now.
    assert!(
        bridge::lookup(&nonce).is_some(),
        "the grant must be reachable while the lease lives"
    );
    let found = bridge::lookup(&nonce).expect("reachable");
    assert_eq!(
        found.tools().len(),
        1,
        "the grant serves the tool set it was issued with"
    );
    assert_eq!(
        found.tools()[0].name.as_ref(),
        "spokeagent__query_knowledge_graph"
    );

    // Unreachable the instant the lease is dropped. A grant that outlived its turn
    // would be a live capability onto a session's tools with nothing owning it.
    drop(lease);
    assert!(
        bridge::lookup(&nonce).is_none(),
        "dropping the lease must revoke the grant"
    );
}

/// A nonce that was never issued is refused, and so is one whose turn has ended —
/// with the same message, so the endpoint cannot be used to discover which nonces
/// exist.
#[tokio::test]
#[serial_test::serial]
async fn an_unissued_nonce_is_indistinguishable_from_an_expired_one() {
    bridge::publish_base_url("http://127.0.0.1:65535");

    let lease = bridge::issue(grant().await).expect("a base URL is published");
    let expired = lease
        .url()
        .rsplit('/')
        .next()
        .expect("the url ends in the nonce")
        .to_string();
    drop(lease);

    assert!(bridge::lookup(&expired).is_none());
    assert!(bridge::lookup("0123456789abcdef0123456789abcdef").is_none());
    assert!(bridge::lookup("not-a-nonce").is_none());
    assert!(bridge::lookup("").is_none());
}

/// The nonce is the whole credential, so it has to be long and unguessable. A
/// short or predictable one would make the bridge reachable by anything on the
/// machine that can reach loopback.
#[tokio::test]
#[serial_test::serial]
async fn every_nonce_is_long_random_and_unique() {
    bridge::publish_base_url("http://127.0.0.1:65535");
    let a = bridge::issue(grant().await).expect("issued");
    let b = bridge::issue(grant().await).expect("issued");

    assert_ne!(a.url(), b.url(), "two leases must never share a nonce");
    for lease in [&a, &b] {
        assert!(lease.url().contains("/tool_bridge/"));
        let nonce = lease.url().rsplit('/').next().unwrap();
        assert_eq!(nonce.len(), 32, "a short nonce is a guessable capability");
        assert!(
            nonce.chars().all(|c| c.is_ascii_hexdigit()),
            "the nonce must be hex so it survives a URL unencoded"
        );
    }
}

/// Without a published base URL there is nothing for a child to connect to, so no
/// grant may be handed out. The providers read that as "run with no tools", which
/// is why it must be `None` rather than a URL pointing nowhere.
#[tokio::test]
#[serial_test::serial]
async fn the_url_carries_the_nonce_and_the_published_base() {
    bridge::publish_base_url("http://127.0.0.1:8123");
    let lease = bridge::issue(grant().await).expect("issued");
    assert!(
        lease.url().starts_with("http://127.0.0.1:8123/tool_bridge/"),
        "the child is given an absolute URL on the daemon: {}",
        lease.url()
    );
}

/// A trailing slash on the published base must not produce a doubled separator —
/// the child would request a path the router does not match, and the failure would
/// look like an authentication problem.
#[tokio::test]
#[serial_test::serial]
async fn a_trailing_slash_on_the_base_does_not_double_the_separator() {
    bridge::publish_base_url("http://127.0.0.1:8123/");
    let lease = bridge::issue(grant().await).expect("issued");
    assert!(
        !lease.url().contains("//tool_bridge"),
        "malformed bridge URL: {}",
        lease.url()
    );
}

// ---------------------------------------------------------------------------
// Live end-to-end, against the real vendor CLI. `--ignored`, because it needs
// `claude` installed and signed in to a subscription, and it spends the user's
// own plan quota.
//
//   cargo test -p biorouter-server --test tool_bridge_routes -- --ignored --nocapture
//
// This is the only test that proves the whole path rather than a layer of it: the
// real router serving the real registry, reached by the real Claude Code over
// HTTP, discovering Biorouter's tool set. Every other assertion in this file would
// still pass if the two halves never spoke.
// ---------------------------------------------------------------------------

/// Serve the real bridge route on an ephemeral port and publish it, so anything
/// that asks for a grant gets a URL that actually resolves.
async fn serve_real_bridge() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let addr = listener.local_addr().expect("a bound address");
    let app = biorouter_server::routes::tool_bridge::routes();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let base = format!("http://{addr}");
    bridge::publish_base_url(base.clone());
    base
}

#[tokio::test]
#[serial_test::serial]
#[ignore = "needs the `claude` CLI installed and signed in; spends the user's own plan quota"]
async fn the_real_claude_cli_discovers_biorouters_tools_over_the_bridge() {
    serve_real_bridge().await;
    let lease = bridge::issue(grant().await).expect("the base URL is published");

    // Point the CLI at the bridge exactly the way the provider does.
    let config = serde_json::json!({
        "mcpServers": { "biorouter": { "type": "http", "url": lease.url() } }
    });
    let dir = tempfile::tempdir().expect("a temp dir");
    let config_path = dir.path().join("mcp.json");
    std::fs::write(&config_path, config.to_string()).expect("write the bridge config");

    let mut child = tokio::process::Command::new("claude")
        .args([
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "haiku",
            "--tools",
            "",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--permission-mode",
            "bypassPermissions",
            "--no-session-persistence",
            "--system-prompt",
            "You are Biorouter.",
        ])
        .arg("--mcp-config")
        .arg(&config_path)
        // ⚠ The prompt goes on STDIN, not as a trailing positional, and that is not
        // a style choice. `--mcp-config` is variadic ("space-separated"), so a
        // positional prompt after it is swallowed as a second config path and the
        // run dies with "MCP config file not found: <your prompt>". The provider
        // writes the prompt to stdin for the same reason (plus argv length limits),
        // so doing it here keeps this a faithful reproduction.
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env_remove("ANTHROPIC_API_KEY")
        .spawn()
        .expect("the claude CLI runs");

    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin
            .write_all(b"List the tool names you have been given, and nothing else.")
            .await
            .expect("write the prompt");
        stdin.shutdown().await.expect("close stdin");
    }
    let output = child.wait_with_output().await.expect("the CLI finishes");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let init = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["type"] == "system" && v["subtype"] == "init")
        .unwrap_or_else(|| {
            panic!(
                "no system/init frame; stdout was:\n{stdout}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });

    // The bridge connected...
    let servers = init["mcp_servers"].as_array().cloned().unwrap_or_default();
    assert!(
        servers
            .iter()
            .any(|s| s["name"] == "biorouter" && s["status"] == "connected"),
        "the bridge should be connected: {init}"
    );
    assert!(
        init["mcp_server_errors"].is_null(),
        "the bridge reported a config error: {init}"
    );

    // ...and served Biorouter's tool set, which the CLI re-prefixes as
    // `mcp__<server>__<tool>`.
    let tools = init["tools"].as_array().cloned().unwrap_or_default();
    assert!(
        tools
            .iter()
            .any(|t| t.as_str().unwrap_or_default()
                == "mcp__biorouter__spokeagent__query_knowledge_graph"),
        "the grant's tool should have reached the model: {tools:?}"
    );

    // The run must be on the subscription, not on a key. This is the assertion the
    // whole feature rests on, so it is made here too rather than only in the unit
    // tests, where the value is a fixture.
    assert_eq!(
        init["apiKeySource"], "none",
        "the live run must be subscription-billed: {init}"
    );
}

#[tokio::test]
#[serial_test::serial]
#[ignore = "needs the `codex` CLI installed and signed in; spends the user's own plan quota"]
async fn the_real_codex_provider_reaches_biorouters_tools_over_the_bridge() {
    use biorouter::model::ModelConfig;
    use biorouter::providers::base::Provider;
    use biorouter::providers::codex::CodexProvider;
    use biorouter::conversation::message::Message;

    serve_real_bridge().await;
    let lease = bridge::issue(grant().await).expect("the base URL is published");

    // Drive the PROVIDER, not the CLI directly. `codex exec` cannot answer an
    // approval request, so an MCP tool call there fails with "user cancelled MCP
    // tool call" unless the sandbox is opened up wholesale — which is the entire
    // reason the provider speaks `codex app-server` instead. Testing `exec` here
    // would be testing a surface we deliberately do not use, and it would fail for
    // a reason that has nothing to do with the bridge.
    let provider = CodexProvider::from_env(ModelConfig::new("gpt-5.5").expect("a known model"))
        .await
        .expect("the codex CLI is installed");

    let messages = vec![Message::user().with_text(
        "Call the spokeagent__query_knowledge_graph tool with cypher='MATCH (n) RETURN n LIMIT 1'. \
         Then report, in one line, the exact text the tool returned.",
    )];

    let outcome = bridge::ACTIVE_BRIDGE_URL
        .scope(
            Some(lease.url().to_string()),
            provider.complete(
                "You are Biorouter. Use the tools you are given.",
                &messages,
                &[],
            ),
        )
        .await;

    match outcome {
        Ok((message, usage)) => {
            let text = message.as_concat_text();
            // The grant's ExtensionManager holds no real extension, so the call is
            // refused by the gate stack rather than executed — and that refusal is
            // the proof: it can only have come from Biorouter's side of the bridge.
            // A child that never reached the bridge would report a missing tool
            // instead.
            assert!(
                text.contains("spokeagent__query_knowledge_graph"),
                "the model should have reached Biorouter's tool; it said: {text}"
            );
            assert_eq!(
                usage.provider.as_deref(),
                Some("codex"),
                "the usage row must be attributed to this provider"
            );
        }
        Err(e) => panic!("the codex turn failed: {e}"),
    }
}
