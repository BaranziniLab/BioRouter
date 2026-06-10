use biorouter::config::BioRouterMode;
use biorouter::conversation::message::Message;
use biorouter::tool_inspection::{InspectionAction, ToolInspector};
use biorouter::tool_monitor::RepetitionInspector;
use rmcp::model::CallToolRequestParams;
use rmcp::object;

// This test targets RepetitionInspector::check_tool_call
// It verifies that:
// - consecutive identical tool calls are allowed up to max_repetitions times
// - the (max_repetitions + 1)th identical call is denied (returns false)
// - changing the parameters resets the repetition count and allows the call
#[test]
fn test_repetition_inspector_denies_after_exceeding_and_resets_on_param_change() {
    // Allow at most 2 consecutive identical calls
    let mut inspector = RepetitionInspector::new(Some(2));

    // First identical call → allowed
    let call_v1 = CallToolRequestParams {
        task: None,
        meta: None,
        name: "fetch_user".into(),
        arguments: Some(object!({"id": 123})),
    };
    assert!(inspector.check_tool_call(call_v1.clone()));

    // Second identical call → still allowed (at limit)
    assert!(inspector.check_tool_call(call_v1.clone()));

    // Third identical call → should be denied (exceeds limit)
    assert!(!inspector.check_tool_call(call_v1.clone()));

    // Change parameters; this should reset the consecutive counter
    let call_v2 = CallToolRequestParams {
        task: None,
        meta: None,
        name: "fetch_user".into(),
        arguments: Some(object!({"id": 456})),
    };

    assert!(inspector.check_tool_call(call_v2.clone()));

    // Another identical call with new params → allowed (second in a row for this variant)
    assert!(inspector.check_tool_call(call_v2.clone()));

    // One more identical call with new params → denied again
    assert!(!inspector.check_tool_call(call_v2));
}

fn tool_call(name: &str, id: i32) -> CallToolRequestParams {
    CallToolRequestParams {
        task: None,
        meta: None,
        name: name.to_string().into(),
        arguments: Some(object!({"id": id})),
    }
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
