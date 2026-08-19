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
