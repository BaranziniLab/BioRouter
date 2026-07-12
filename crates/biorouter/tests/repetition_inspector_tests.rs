use biorouter::config::BioRouterMode;
use biorouter::conversation::message::{Message, ToolRequest};
use biorouter::tool_inspection::{InspectionAction, ToolInspector};
use biorouter::tool_monitor::RepetitionInspector;
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
