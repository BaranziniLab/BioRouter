//! Issue #63 at the tool-inspection boundary: global (machine-wide) memory
//! access routed through the user's consent, and — the property the whole fix
//! rests on — a `GlobalMemoryInspector` verdict surviving every way the
//! permission layer can say "allowed".
//!
//! The wiring mirrors `Agent::create_tool_inspection_manager`: the gate is
//! registered *before* the permission inspector, so the escalation-only merge in
//! `tool_inspection` applies it as an override on top of the permission
//! baseline. Ordering matters — see `the_gate_survives_*` below.

use std::sync::Arc;

use biorouter::config::permission::PermissionLevel;
use biorouter::config::{BioRouterMode, PermissionManager};
use biorouter::conversation::message::{Message, ToolRequest};
use biorouter::managed::ManagedPolicy;
use biorouter::permission::{PermissionInspector, SmartApproveConfig, ToolRiskRegistry};
use biorouter::security::global_memory::GlobalMemoryInspector;
use biorouter::session::Session;
use biorouter::tool_inspection::ToolInspectionManager;
use rmcp::model::CallToolRequestParams;
use serde_json::{json, Value};
use tempfile::TempDir;

fn tool_request(id: &str, name: &str, arguments: Value) -> ToolRequest {
    Message::assistant()
        .with_tool_request(
            id,
            Ok(CallToolRequestParams {
                task: None,
                meta: None,
                name: name.to_string().into(),
                arguments: Some(arguments.as_object().expect("object args").clone()),
            }),
        )
        .content
        .first()
        .and_then(|content| content.as_tool_request())
        .expect("message contains a tool request")
        .clone()
}

/// The agent's real registration order for the two inspectors under test.
fn build_manager() -> (ToolInspectionManager, Arc<PermissionManager>, TempDir) {
    let temp = TempDir::new().unwrap();
    let permission_manager = Arc::new(PermissionManager::new(temp.path().to_path_buf()));
    let managed = Arc::new(ManagedPolicy::empty());
    let mut manager = ToolInspectionManager::new();
    manager.add_inspector(Box::new(GlobalMemoryInspector));
    manager.add_inspector(Box::new(PermissionInspector::with_smart_config(
        Arc::new(ToolRiskRegistry::new()),
        Arc::clone(&permission_manager),
        managed,
        Arc::new(tokio::sync::Mutex::new(None)),
        SmartApproveConfig::default(),
    )));
    (manager, permission_manager, temp)
}

async fn decide(
    manager: &ToolInspectionManager,
    requests: &[ToolRequest],
    mode: BioRouterMode,
) -> biorouter::permission::permission_judge::PermissionCheckResult {
    let session = Session::default();
    let results = manager
        .inspect_tools(requests, &[], mode, &session)
        .await
        .expect("inspection succeeds");
    manager
        .process_inspection_results_with_permission_inspector(requests, &results)
        .expect("permission inspector present")
}

fn global_read(id: &str, category: &str) -> ToolRequest {
    tool_request(
        id,
        "memory__retrieve_memories",
        json!({"category": category, "is_global": true}),
    )
}

// --- the issue's own scenario ---------------------------------------------

/// The report, verbatim in substance: a session in Auto mode reading a global
/// memory another session wrote, "without the user being asked or told".
#[tokio::test]
async fn auto_mode_no_longer_reads_global_memory_unasked() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![global_read("r1", "clinical")];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;

    assert!(
        result.approved.is_empty(),
        "Auto mode's blanket allow must not reach a machine-wide memory read"
    );
    assert_eq!(result.needs_approval.len(), 1);
    assert_eq!(result.needs_approval[0].id, "r1");
}

/// `retrieve_memories(category="*", is_global=true)` returned the entire
/// machine-wide store in one call. It is refused outright.
#[tokio::test]
async fn auto_mode_refuses_the_whole_store_read() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![global_read("r1", "*")];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;

    assert_eq!(
        result.denied.len(),
        1,
        "the bulk global read must be denied"
    );
    assert_eq!(result.denied[0].id, "r1");
    assert!(result.approved.is_empty() && result.needs_approval.is_empty());
}

/// The write side, which was never gated at all (CLAUDE.md: "a real
/// confirmation needs the permission path in `biorouter::permission`").
#[tokio::test]
async fn auto_mode_asks_before_a_machine_wide_write() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![tool_request(
        "r1",
        "memory__remember_memory",
        json!({"category": "clinical", "data": "cohort 4217", "is_global": true}),
    )];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;

    assert_eq!(result.needs_approval.len(), 1);
    assert!(result.approved.is_empty());
}

// --- the property the fix rests on ----------------------------------------

/// A user `AlwaysAllow` on the memory tool is a *tool-name* grant. It must not
/// buy a machine-wide read, or the gate is one settings toggle from useless.
/// (This is also the precedent check the brief asks for: a security-inspector
/// override already beats an `always_allow`, and it still does.)
#[tokio::test]
async fn the_gate_survives_an_always_allow_grant() {
    let (manager, pm, _tmp) = build_manager();
    pm.update_user_permission("memory__retrieve_memories", PermissionLevel::AlwaysAllow);

    let requests = vec![global_read("r1", "clinical")];
    let result = decide(&manager, &requests, BioRouterMode::Approve).await;

    assert!(
        result.approved.is_empty(),
        "an AlwaysAllow on the tool name must not cover a global read"
    );
    assert_eq!(result.needs_approval.len(), 1);
}

/// The same grant on the *local* store keeps working. The gate narrows exactly
/// one axis; it does not quietly revoke a permission the user granted.
#[tokio::test]
async fn an_always_allow_grant_still_covers_local_recall() {
    let (manager, pm, _tmp) = build_manager();
    pm.update_user_permission("memory__retrieve_memories", PermissionLevel::AlwaysAllow);

    let requests = vec![tool_request(
        "r1",
        "memory__retrieve_memories",
        json!({"category": "*", "is_global": false}),
    )];
    let result = decide(&manager, &requests, BioRouterMode::Approve).await;

    assert_eq!(
        result.approved.len(),
        1,
        "local recall is untouched by this gate"
    );
}

/// SmartApprove grades tools from their MCP annotations and auto-approves what
/// reads as read-only. A retrieval is read-only *of a store the user cannot
/// see*, so the grade must not carry it.
#[tokio::test]
async fn the_gate_survives_a_smart_approve_read_only_grade() {
    let (manager, pm, _tmp) = build_manager();
    // The cached judge verdict SmartApprove would otherwise honour.
    pm.update_smart_approve_permission("memory__retrieve_memories", PermissionLevel::AlwaysAllow);

    let requests = vec![global_read("r1", "clinical")];
    let result = decide(&manager, &requests, BioRouterMode::SmartApprove).await;

    assert!(result.approved.is_empty());
    assert_eq!(result.needs_approval.len(), 1);
}

// --- the feature still works ----------------------------------------------

/// The whole reason the audit left this unfixed was that a blanket refusal kills
/// global memory. One approved category at a time is the point — nothing here
/// denies a named global read.
#[tokio::test]
async fn a_named_global_read_is_approvable_not_refused() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![global_read("r1", "clinical")];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;

    assert!(
        result.denied.is_empty(),
        "a named global read must remain reachable with the user's consent"
    );
    assert_eq!(result.needs_approval.len(), 1);
}

/// Local memory is not a cross-session channel and keeps running with no prompt
/// in Auto mode — including the bulk read.
#[tokio::test]
async fn local_memory_is_untouched_in_auto_mode() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![
        tool_request(
            "r1",
            "memory__retrieve_memories",
            json!({"category": "*", "is_global": false}),
        ),
        tool_request(
            "r2",
            "memory__remember_memory",
            json!({"category": "development", "data": "black", "is_global": false}),
        ),
    ];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;

    assert_eq!(
        result.approved.len(),
        2,
        "local memory must not start prompting: {result:?}",
        result = (
            result.approved.len(),
            result.needs_approval.len(),
            result.denied.len()
        )
    );
}

/// A tool call that has nothing to do with memory is unaffected in Auto mode.
#[tokio::test]
async fn unrelated_tools_are_unaffected() {
    let (manager, _pm, _tmp) = build_manager();
    let requests = vec![tool_request(
        "r1",
        "developer__shell",
        json!({"command": "ls -la"}),
    )];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;
    assert_eq!(result.approved.len(), 1);
}

// --- ordering -------------------------------------------------------------

/// The merge is escalation-only, so the *registration order* of these two
/// inspectors must not matter — a mode-based blanket allow cannot arrive "ahead
/// of" the override and win. Pinned because a future reordering that broke it
/// would leave every assertion above passing in the order they happen to use.
#[tokio::test]
async fn registration_order_cannot_defeat_the_gate() {
    let temp = TempDir::new().unwrap();
    let permission_manager = Arc::new(PermissionManager::new(temp.path().to_path_buf()));
    let mut manager = ToolInspectionManager::new();
    // Deliberately inverted: permission first, gate last.
    manager.add_inspector(Box::new(PermissionInspector::with_smart_config(
        Arc::new(ToolRiskRegistry::new()),
        Arc::clone(&permission_manager),
        Arc::new(ManagedPolicy::empty()),
        Arc::new(tokio::sync::Mutex::new(None)),
        SmartApproveConfig::default(),
    )));
    manager.add_inspector(Box::new(GlobalMemoryInspector));

    let requests = vec![global_read("r1", "clinical")];
    let result = decide(&manager, &requests, BioRouterMode::Auto).await;
    assert_eq!(
        result.needs_approval.len(),
        1,
        "the gate must win from either position in the inspector list"
    );
}
