use biorouter::config::BioRouterMode;
use biorouter::conversation::message::{Message, ToolRequest};
use biorouter::tool_inspection::{
    collect_warning_reasons, frame_loop_warnings, InspectionAction, ToolInspector,
};
use biorouter::tool_monitor::{
    RepetitionInspector, REPETITION_HARD_FINDING_ID, REPETITION_SOFT_FINDING_ID,
};
use rmcp::model::CallToolRequestParams;
use rmcp::object;

fn tool_call(name: &str, id: i32) -> CallToolRequestParams {
    CallToolRequestParams {
        task: None,
        meta: None,
        name: name.to_string().into(),
        arguments: Some(object!({"id": id})),
    }
}

fn tool_request(id: &str, call: CallToolRequestParams) -> ToolRequest {
    let message = Message::assistant().with_tool_request(id, Ok(call));
    message
        .content
        .first()
        .and_then(|content| content.as_tool_request())
        .expect("request message should contain a tool request")
        .clone()
}

// This test targets the production RepetitionInspector::inspect path.
// It verifies that within a single batch of tool requests:
// - consecutive identical tool calls are allowed up to max_repetitions times
// - the (max_repetitions + 1)th identical call is denied
// - changing the parameters resets the repetition count and allows the call
#[tokio::test]
async fn test_repetition_inspector_denies_after_exceeding_and_resets_on_param_change() {
    // Allow at most 2 consecutive identical calls
    let inspector = RepetitionInspector::new(Some(2));

    let call_v1 = tool_call("fetch_user", 123);
    let call_v2 = tool_call("fetch_user", 456);

    let requests = vec![
        tool_request("call_1", call_v1.clone()), // 1st identical → allowed
        tool_request("call_2", call_v1.clone()), // 2nd identical → allowed (at limit)
        tool_request("call_3", call_v1),         // 3rd identical → denied
        tool_request("call_4", call_v2.clone()), // param change → resets, allowed
        tool_request("call_5", call_v2.clone()), // 2nd with new params → allowed (at limit)
        tool_request("call_6", call_v2),         // 3rd with new params → denied
    ];

    let results = inspector
        .inspect(
            &requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    // Only the calls that exceed the consecutive limit are denied.
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tool_request_id, "call_3");
    assert_eq!(results[0].action, InspectionAction::Deny);
    assert_eq!(results[0].finding_id.as_deref(), Some("REP-001"));
    assert_eq!(results[1].tool_request_id, "call_6");
    assert_eq!(results[1].action, InspectionAction::Deny);
    assert_eq!(results[1].finding_id.as_deref(), Some("REP-001"));
}

#[tokio::test]
async fn test_repetition_inspector_denies_current_request_after_history_repeats() {
    let inspector = RepetitionInspector::new(Some(2));
    let call = tool_call("fetch_user", 123);
    let messages = vec![
        Message::assistant().with_tool_request("call_1", Ok(call.clone())),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_2", Ok(call.clone())),
        Message::user().with_text("tool response"),
    ];
    let request_message = Message::assistant().with_tool_request("call_3", Ok(call));
    let request = request_message
        .content
        .first()
        .and_then(|content| content.as_tool_request())
        .expect("request message should contain a tool request")
        .clone();

    let results = inspector
        .inspect(
            &[request],
            &messages,
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, InspectionAction::Deny);
    assert_eq!(results[0].finding_id.as_deref(), Some("REP-001"));
}

#[tokio::test]
async fn test_repetition_inspector_allows_changed_arguments_after_history_repeats() {
    let inspector = RepetitionInspector::new(Some(2));
    let prior_call = tool_call("fetch_user", 123);
    let messages = vec![
        Message::assistant().with_tool_request("call_1", Ok(prior_call.clone())),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_2", Ok(prior_call)),
        Message::user().with_text("tool response"),
    ];
    let request_message =
        Message::assistant().with_tool_request("call_3", Ok(tool_call("fetch_user", 456)));
    let request = request_message
        .content
        .first()
        .and_then(|content| content.as_tool_request())
        .expect("request message should contain a tool request")
        .clone();

    let results = inspector
        .inspect(
            &[request],
            &messages,
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    assert!(results.is_empty());
}

// BR-29: the guard is staged. The soft stage nudges the model (the call still
// runs); only a model that keeps repeating itself through the nudge is stopped.
#[tokio::test]
async fn test_staged_repetition_warns_before_it_stops() {
    // Warn on the 3rd identical call in a row, deny on the 5th.
    let inspector = RepetitionInspector::staged(3, 5);
    let call = tool_call("fetch_user", 123);

    let requests = vec![
        tool_request("call_1", call.clone()), // 1st → silent
        tool_request("call_2", call.clone()), // 2nd → silent
        tool_request("call_3", call.clone()), // 3rd → soft warning, still runs
        tool_request("call_4", call.clone()), // 4th → soft warning, still runs
        tool_request("call_5", call.clone()), // 5th → hard stop
        tool_request("call_6", call),         // 6th → hard stop
    ];

    let results = inspector
        .inspect(
            &requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    assert_eq!(results.len(), 4);

    assert_eq!(results[0].tool_request_id, "call_3");
    assert_eq!(results[0].action, InspectionAction::Warn);
    assert_eq!(
        results[0].finding_id.as_deref(),
        Some(REPETITION_SOFT_FINDING_ID)
    );
    assert_eq!(results[1].tool_request_id, "call_4");
    assert_eq!(results[1].action, InspectionAction::Warn);

    assert_eq!(results[2].tool_request_id, "call_5");
    assert_eq!(results[2].action, InspectionAction::Deny);
    assert_eq!(
        results[2].finding_id.as_deref(),
        Some(REPETITION_HARD_FINDING_ID)
    );
    assert_eq!(results[3].tool_request_id, "call_6");
    assert_eq!(results[3].action, InspectionAction::Deny);
}

// The soft warning names the tool, the count, and the approaching hard stop, so
// the model can act on it.
#[tokio::test]
async fn test_soft_warning_is_actionable_guidance() {
    let inspector = RepetitionInspector::staged(3, 5);
    let call = tool_call("fetch_user", 123);
    let requests = vec![
        tool_request("call_1", call.clone()),
        tool_request("call_2", call.clone()),
        tool_request("call_3", call),
    ];

    let results = inspector
        .inspect(
            &requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    assert_eq!(results.len(), 1);
    let reason = &results[0].reason;
    assert!(reason.contains("fetch_user"), "reason: {reason}");
    assert!(reason.contains('3'), "reason: {reason}");
    assert!(reason.contains('5'), "reason: {reason}");
    assert!(
        !reason.to_lowercase().contains("declined"),
        "a warning must not claim anyone declined: {reason}"
    );
}

// BR-29 core bug: on a repetition stop the model used to be handed
// DECLINED_RESPONSE ("The user has declined to run this tool"), which is false.
// The deny reason must state the real cause.
#[tokio::test]
async fn test_hard_stop_reason_is_honest_not_a_fake_user_decline() {
    let inspector = RepetitionInspector::staged(3, 5);
    let call = tool_call("fetch_user", 123);
    let requests: Vec<ToolRequest> = (1..=5)
        .map(|i| tool_request(&format!("call_{i}"), call.clone()))
        .collect();

    let results = inspector
        .inspect(
            &requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    let deny = results
        .iter()
        .find(|result| result.action == InspectionAction::Deny)
        .expect("the 5th identical call should be denied");

    let reason = deny.reason.to_lowercase();
    assert!(
        !reason.contains("the user has declined"),
        "deny reason must not blame the user: {}",
        deny.reason
    );
    assert!(
        reason.contains("did not decline"),
        "deny reason should say the user did not decline: {}",
        deny.reason
    );
    assert!(
        reason.contains("repetition"),
        "deny reason should name the repetition guard: {}",
        deny.reason
    );
}

// A soft warning is advisory: it must not deny the call or force an approval
// prompt.
#[test]
fn test_warn_action_does_not_change_the_permission_verdict() {
    use biorouter::permission::permission_judge::PermissionCheckResult;
    use biorouter::tool_inspection::{apply_inspection_results_to_permissions, InspectionResult};

    let request = tool_request("call_1", tool_call("fetch_user", 123));
    let permission_result = PermissionCheckResult {
        approved: vec![request.clone()],
        needs_approval: vec![],
        denied: vec![],
    };

    let inspection_results = vec![InspectionResult {
        tool_request_id: request.id.clone(),
        action: InspectionAction::Warn,
        reason: "Repetition warning: you have called 'fetch_user' 3 times in a row.".to_string(),
        confidence: 1.0,
        inspector_name: "repetition".to_string(),
        finding_id: Some(REPETITION_SOFT_FINDING_ID.to_string()),
    }];

    let updated = apply_inspection_results_to_permissions(permission_result, &inspection_results);

    assert_eq!(updated.approved.len(), 1);
    assert!(updated.denied.is_empty());
    assert!(updated.needs_approval.is_empty());
}

// The warnings the agent injects into the model's context: de-duplicated,
// framed, and only from Warn results.
#[test]
fn test_collect_and_frame_warning_reasons() {
    use biorouter::tool_inspection::InspectionResult;

    let warn = |id: &str, reason: &str| InspectionResult {
        tool_request_id: id.to_string(),
        action: InspectionAction::Warn,
        reason: reason.to_string(),
        confidence: 1.0,
        inspector_name: "repetition".to_string(),
        finding_id: Some(REPETITION_SOFT_FINDING_ID.to_string()),
    };

    let results = vec![
        warn("call_1", "repeating 'fetch_user'"),
        warn("call_2", "repeating 'fetch_user'"), // duplicate reason → collapsed
        InspectionResult {
            tool_request_id: "call_3".to_string(),
            action: InspectionAction::Deny,
            reason: "hard stop".to_string(),
            confidence: 1.0,
            inspector_name: "repetition".to_string(),
            finding_id: Some(REPETITION_HARD_FINDING_ID.to_string()),
        },
    ];

    let reasons = collect_warning_reasons(&results);
    assert_eq!(reasons, vec!["repeating 'fetch_user'".to_string()]);

    let framed = frame_loop_warnings(&reasons);
    assert!(framed.contains("<biorouter-loop-guard>"));
    assert!(framed.contains("repeating 'fetch_user'"));
    assert!(!framed.contains("hard stop"));
}

// Degenerate config (soft >= hard) must not fire a warning that can never be
// acted on — it degrades to the old hard-stop-only behavior.
#[tokio::test]
async fn test_soft_stage_is_inert_when_thresholds_collapse() {
    let inspector = RepetitionInspector::staged(5, 5);
    let call = tool_call("fetch_user", 123);
    let requests: Vec<ToolRequest> = (1..=5)
        .map(|i| tool_request(&format!("call_{i}"), call.clone()))
        .collect();

    let results = inspector
        .inspect(
            &requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_request_id, "call_5");
    assert_eq!(results[0].action, InspectionAction::Deny);
}

// ---------------------------------------------------------------------------
// BR-30: semantic / near-duplicate / oscillation loop detection
// ---------------------------------------------------------------------------

use biorouter::tool_monitor::{
    SemanticLoopConfig, REPETITION_NEAR_DUP_HARD_FINDING_ID, REPETITION_NEAR_DUP_SOFT_FINDING_ID,
    REPETITION_OSCILLATION_HARD_FINDING_ID, REPETITION_OSCILLATION_SOFT_FINDING_ID,
};
use serde_json::json;

/// A tool call with arbitrary arguments (the `tool_call` helper above only
/// varies an integer `id`).
fn call_with(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    CallToolRequestParams {
        task: None,
        meta: None,
        name: name.to_string().into(),
        arguments: args.as_object().cloned(),
    }
}

fn shell(command: &str) -> CallToolRequestParams {
    call_with("shell", json!({ "command": command }))
}

async fn inspect(
    inspector: &RepetitionInspector,
    requests: &[ToolRequest],
) -> Vec<biorouter::tool_inspection::InspectionResult> {
    inspector
        .inspect(
            requests,
            &[],
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed")
}

fn requests(calls: Vec<CallToolRequestParams>) -> Vec<ToolRequest> {
    calls
        .into_iter()
        .enumerate()
        .map(|(index, call)| tool_request(&format!("call_{}", index + 1), call))
        .collect()
}

// The BR-30 case the byte-exact guard misses entirely: the model keeps calling
// the same tool, nudging the arguments by a character each time.
#[tokio::test]
async fn test_near_duplicate_arg_tweaks_are_flagged() {
    let inspector = RepetitionInspector::staged(3, 5);

    let requests = requests(vec![
        shell("grep -rn 'assemble_turn_context' crates/biorouter/src/agents/"),
        shell("grep -rn 'assemble_turn_context' crates/biorouter/src/agents"),
        shell("grep -rn 'assemble_turn_context' crates/biorouter/src/agent"),
        shell("grep -rn 'assemble_turn_context' crates/biorouter/src/agen"),
    ]);

    let results = inspect(&inspector, &requests).await;

    assert_eq!(results.len(), 1, "only the 4th call trips the soft stage");
    assert_eq!(results[0].tool_request_id, "call_4");
    assert_eq!(results[0].action, InspectionAction::Warn);
    assert_eq!(
        results[0].finding_id.as_deref(),
        Some(REPETITION_NEAR_DUP_SOFT_FINDING_ID)
    );
    assert!(results[0].reason.contains("shell"), "{}", results[0].reason);
    assert!(
        !results[0].reason.to_lowercase().contains("declined"),
        "a warning must never claim anyone declined: {}",
        results[0].reason
    );
}

// The false-positive that would make this heuristic unusable: a model working
// through a list of distinct inputs is making progress, not looping.
#[tokio::test]
async fn test_iterating_over_distinct_inputs_is_not_a_loop() {
    let inspector = RepetitionInspector::staged(3, 5);

    let results = inspect(
        &inspector,
        &requests(vec![
            call_with("read_file", json!({"path": "crates/a.rs"})),
            call_with("read_file", json!({"path": "crates/b.rs"})),
            call_with("read_file", json!({"path": "crates/c.rs"})),
            call_with("read_file", json!({"path": "crates/d.rs"})),
            call_with("read_file", json!({"path": "crates/e.rs"})),
            call_with("read_file", json!({"path": "crates/f.rs"})),
        ]),
    )
    .await;

    assert!(
        results.is_empty(),
        "distinct targets must not read as repetition: {results:?}"
    );
}

// OpenHands' A/B/A/B heuristic: two calls alternating, neither making progress.
#[tokio::test]
async fn test_ab_ab_oscillation_is_flagged() {
    let inspector = RepetitionInspector::staged(3, 5);

    let results = inspect(
        &inspector,
        &requests(vec![
            call_with("read_file", json!({"path": "main.rs"})),
            shell("cargo build"),
            call_with("read_file", json!({"path": "main.rs"})),
            shell("cargo build"),
        ]),
    )
    .await;

    assert_eq!(results.len(), 1, "the 4th call closes the second cycle");
    assert_eq!(results[0].tool_request_id, "call_4");
    assert_eq!(results[0].action, InspectionAction::Warn);
    assert_eq!(
        results[0].finding_id.as_deref(),
        Some(REPETITION_OSCILLATION_SOFT_FINDING_ID)
    );
    assert!(
        results[0].reason.contains("alternate"),
        "{}",
        results[0].reason
    );
}

// The byte-exact guard (BR-29) owns plain repetition; BR-30 must not pile a
// second nudge onto the same call.
#[tokio::test]
async fn test_exact_repeats_get_exactly_one_verdict() {
    let inspector = RepetitionInspector::staged(3, 5);
    let call = shell("cargo test");

    let results = inspect(
        &inspector,
        &requests(vec![
            call.clone(),
            call.clone(),
            call.clone(),
            call.clone(),
            call.clone(),
            call,
        ]),
    )
    .await;

    assert_eq!(
        results.len(),
        4,
        "one verdict per repeated call: {results:?}"
    );
    for result in &results {
        let finding = result.finding_id.as_deref();
        assert!(
            finding == Some(REPETITION_SOFT_FINDING_ID)
                || finding == Some(REPETITION_HARD_FINDING_ID),
            "byte-exact repeats belong to the BR-29 guard, got {finding:?}"
        );
    }
}

// Both semantic stages are warn-only by default. Enforcement is opt-in.
#[tokio::test]
async fn test_semantic_hard_stops_are_opt_in() {
    let tweaks = vec![
        shell("cat /etc/hosts | grep -n biorouter-dev-host"),
        shell("cat /etc/hosts | grep -n biorouter-dev-hos"),
        shell("cat /etc/hosts | grep -n biorouter-dev-ho"),
        shell("cat /etc/hosts | grep -n biorouter-dev-h"),
    ];

    let default_results = inspect(
        &RepetitionInspector::staged(3, 5),
        &requests(tweaks.clone()),
    )
    .await;
    assert!(
        default_results
            .iter()
            .all(|result| result.action == InspectionAction::Warn),
        "default config must never deny on a heuristic: {default_results:?}"
    );

    let strict = RepetitionInspector::staged(3, 5).with_semantic(SemanticLoopConfig {
        near_dup_hard_stop: Some(4),
        ..SemanticLoopConfig::default()
    });
    let strict_results = inspect(&strict, &requests(tweaks)).await;

    assert_eq!(strict_results.len(), 1);
    assert_eq!(strict_results[0].action, InspectionAction::Deny);
    assert_eq!(
        strict_results[0].finding_id.as_deref(),
        Some(REPETITION_NEAR_DUP_HARD_FINDING_ID)
    );
    assert!(
        strict_results[0].reason.contains("did NOT decline"),
        "a heuristic stop must still be honest about its cause: {}",
        strict_results[0].reason
    );
}

#[tokio::test]
async fn test_oscillation_hard_stop_is_opt_in() {
    let cycle = vec![
        call_with("read_file", json!({"path": "main.rs"})),
        shell("cargo build"),
        call_with("read_file", json!({"path": "main.rs"})),
        shell("cargo build"),
    ];

    let strict = RepetitionInspector::staged(3, 5).with_semantic(SemanticLoopConfig {
        oscillation_hard_stop: Some(4),
        ..SemanticLoopConfig::default()
    });
    let results = inspect(&strict, &requests(cycle)).await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, InspectionAction::Deny);
    assert_eq!(
        results[0].finding_id.as_deref(),
        Some(REPETITION_OSCILLATION_HARD_FINDING_ID)
    );
}

// The whole feature is switchable off, back to byte-exact detection only.
#[tokio::test]
async fn test_semantic_detection_can_be_disabled() {
    let inspector = RepetitionInspector::staged(3, 5).with_semantic(SemanticLoopConfig::disabled());

    let results = inspect(
        &inspector,
        &requests(vec![
            shell("ls /tmp/biorouter-scratch-dir"),
            shell("ls /tmp/biorouter-scratch-di"),
            shell("ls /tmp/biorouter-scratch-d"),
            shell("ls /tmp/biorouter-scratch-"),
        ]),
    )
    .await;

    assert!(
        results.is_empty(),
        "semantic detection was off: {results:?}"
    );
}

// The heuristics look at the current turn. A genuine user message resets the
// window; the agent's own (user-role, user-invisible) loop-guard injections and
// tool responses do not.
#[tokio::test]
async fn test_window_resets_on_a_real_user_turn_only() {
    let inspector = RepetitionInspector::staged(3, 5);

    let earlier: Vec<Message> = [
        "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agents/",
        "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agents",
        "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agent",
    ]
    .iter()
    .enumerate()
    .map(|(index, command)| {
        Message::assistant().with_tool_request(format!("hist_{index}"), Ok(shell(command)))
    })
    .collect();

    let next = requests(vec![shell(
        "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agen",
    )]);

    // Continuing the same turn: the 4th near-duplicate trips the soft stage.
    let results = inspector
        .inspect(
            &next,
            &earlier,
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].finding_id.as_deref(),
        Some(REPETITION_NEAR_DUP_SOFT_FINDING_ID)
    );

    // A real user turn in between wipes the slate.
    let mut with_user_turn = earlier.clone();
    with_user_turn.push(Message::user().with_text("actually, try the other crate"));
    let results = inspector
        .inspect(
            &next,
            &with_user_turn,
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");
    assert!(
        results.is_empty(),
        "a user turn must reset the loop window: {results:?}"
    );

    // The agent's own loop-guard nudge is user-role but user-invisible — it is
    // not a user turn and must not launder a loop.
    let mut with_guard_nudge = earlier;
    with_guard_nudge.push(
        Message::user()
            .with_text("<biorouter-loop-guard>…</biorouter-loop-guard>")
            .with_visibility(false, true),
    );
    let results = inspector
        .inspect(
            &next,
            &with_guard_nudge,
            BioRouterMode::Approve,
            &biorouter::session::Session::default(),
        )
        .await
        .expect("inspection should succeed");
    assert_eq!(
        results.len(),
        1,
        "an agent-authored injection must not reset the window"
    );
}
