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

/// The loop that never tripped.
///
/// The inspector used to track only the *immediately preceding* call, so an
/// alternating `A, B, A, B, A, B` reset the counter on every iteration and the
/// guard never fired at all. That is how a runaway `ui_describe` / `ui_render`
/// alternation in the test drive ran all the way to the turn cap — every one of
/// those iterations a billed provider call — while the guard sat there believing
/// nothing was wrong.
///
/// A loop is a loop whether or not the model interleaves something else between
/// iterations. Counting by (name, args) signature across the whole conversation is
/// what makes it visible.
#[tokio::test]
async fn an_interleaved_loop_is_detected() {
    let inspector = RepetitionInspector::new(Some(2));

    let looping = tool_call("ui_describe", 1);
    let filler = tool_call("ui_render", 99);

    // A, B, A, B — the same call three times, never twice in a row.
    let messages = vec![
        Message::assistant().with_tool_request("call_1", Ok(looping.clone())),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_2", Ok(filler.clone())),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_3", Ok(looping.clone())),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_4", Ok(filler)),
        Message::user().with_text("tool response"),
    ];

    // …and the model reaches for it a third time.
    let request_message = Message::assistant().with_tool_request("call_5", Ok(looping));
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

    assert_eq!(
        results.len(),
        1,
        "an interleaved repeat is still a repeat — this used to return nothing"
    );
    assert_eq!(results[0].action, InspectionAction::Deny);
    assert_eq!(results[0].inspector_name, "repetition");
}

/// The guard must not fire on healthy work: distinct calls, however many, are not
/// a loop. Over-eager blocking would break every legitimate multi-tool turn.
#[tokio::test]
async fn distinct_calls_are_never_a_loop() {
    let inspector = RepetitionInspector::new(Some(2));

    let messages = vec![
        Message::assistant().with_tool_request("call_1", Ok(tool_call("fetch_user", 1))),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_2", Ok(tool_call("fetch_user", 2))),
        Message::user().with_text("tool response"),
        Message::assistant().with_tool_request("call_3", Ok(tool_call("fetch_user", 3))),
        Message::user().with_text("tool response"),
    ];

    let request_message =
        Message::assistant().with_tool_request("call_4", Ok(tool_call("fetch_user", 4)));
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

    assert!(
        results.is_empty(),
        "four different calls to the same tool are progress, not a loop"
    );
}
