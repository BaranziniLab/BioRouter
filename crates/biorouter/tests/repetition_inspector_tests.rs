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
