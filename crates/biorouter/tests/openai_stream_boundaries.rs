use biorouter::conversation::message::{MessageContent, ToolRequest};
use biorouter::providers::base::ProviderStreamItem;
use biorouter::providers::formats::openai::response_to_streaming_message;
use futures::{pin_mut, StreamExt};
use serde_json::{json, Value};

fn chunk(delta: Value, finish: Option<&str>) -> String {
    format!(
        "data: {}",
        json!({"model":"synthetic-gpt","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
    )
}

fn start(arguments: &str, finish: Option<&str>) -> String {
    chunk(
        json!({"tool_calls":[{"index":0,"id":"call_sqlite","function":{"name":"developer__text_editor","arguments":arguments}}]}),
        finish,
    )
}

async fn decode(lines: Vec<String>) -> anyhow::Result<Vec<ProviderStreamItem>> {
    let stream = response_to_streaming_message(tokio_stream::iter(lines.into_iter().map(Ok)));
    pin_mut!(stream);
    let mut result = Vec::new();
    while let Some(item) = stream.next().await {
        result.push(item?);
    }
    Ok(result)
}

fn requests(items: &[ProviderStreamItem]) -> Vec<&ToolRequest> {
    items
        .iter()
        .filter_map(|(message, _, _)| message.as_ref())
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ToolRequest(request) => Some(request),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn eof_without_completion_never_dispatches_even_valid_json() -> anyhow::Result<()> {
    let items = decode(vec![start(r#"{"path":"/tmp/synthetic.py"}"#, None)]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}

#[tokio::test]
async fn done_without_completion_reports_pending_call_instead_of_losing_it() -> anyhow::Result<()> {
    let items = decode(vec![
        start(r#"{"path":"/tmp/"#, None),
        "data: [DONE]".into(),
    ])
    .await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}

#[tokio::test]
async fn length_stop_does_not_dispatch_syntactically_valid_arguments() -> anyhow::Result<()> {
    let items = decode(vec![start("{}", None), chunk(json!({}), Some("length"))]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}

#[tokio::test]
async fn first_chunk_length_is_not_reinterpreted_as_success() -> anyhow::Result<()> {
    let items = decode(vec![start("{}", Some("length")), "data: [DONE]".into()]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}

#[tokio::test]
async fn non_object_arguments_are_failed_calls_not_panics_or_empty_objects() -> anyhow::Result<()> {
    for arguments in ["[]", "null", "42", "\"text\""] {
        let items = decode(vec![start(arguments, Some("tool_calls"))]).await?;
        let calls = requests(&items);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].tool_call.is_err(), "non-object arguments accepted");
    }
    Ok(())
}

#[tokio::test]
async fn sse_data_field_without_space_preserves_complete_call() -> anyhow::Result<()> {
    let line = start("{}", Some("tool_calls")).replacen("data: ", "data:", 1);
    let items = decode(vec![line, "data:[DONE]".into()]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_ok());
    Ok(())
}

#[tokio::test]
async fn usage_only_chunk_does_not_close_inflight_arguments() -> anyhow::Result<()> {
    let items = decode(vec![
        start(r#"{"path":""#, None),
        format!("data: {}", json!({"model":"synthetic-gpt","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":2,"total_tokens":14}})),
        chunk(json!({"tool_calls":[{"index":0,"function":{"arguments":"/tmp/synthetic.py\"}"}}]}), None),
        chunk(json!({}), Some("tool_calls")),
        "data: [DONE]".into(),
    ]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    let call = calls[0].tool_call.as_ref().expect("complete call");
    assert_eq!(
        call.arguments.as_ref().unwrap().get("path"),
        Some(&json!("/tmp/synthetic.py"))
    );
    Ok(())
}

#[tokio::test]
async fn completed_fragmented_call_retains_usage_and_exact_arguments() -> anyhow::Result<()> {
    let items = decode(vec![
        start(r#"{"path":""#, None),
        chunk(json!({"tool_calls":[{"index":0,"function":{"arguments":"/tmp/分析.py\"}"}}]}), None),
        chunk(json!({}), Some("tool_calls")),
        format!("data: {}", json!({"model":"synthetic-gpt","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20}})),
        "data: [DONE]".into(),
    ]).await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    let call = calls[0].tool_call.as_ref().expect("complete call");
    assert_eq!(
        call.arguments.as_ref().unwrap().get("path"),
        Some(&json!("/tmp/分析.py"))
    );
    let usage = items
        .iter()
        .filter_map(|(_, usage, _)| usage.as_ref())
        .last()
        .unwrap();
    assert_eq!(usage.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(usage.usage.total_tokens, Some(20));
    Ok(())
}

#[tokio::test]
async fn done_after_pending_call_ignores_all_later_bytes() -> anyhow::Result<()> {
    let items = decode(vec![
        start("{}", None),
        "data: [DONE]".into(),
        "data: not-json-after-terminal-event".into(),
    ])
    .await?;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}

#[tokio::test]
async fn done_after_pending_call_does_not_wait_for_connection_close() -> anyhow::Result<()> {
    let input = tokio_stream::iter(vec![Ok(start("{}", None)), Ok("data: [DONE]".into())])
        .chain(futures::stream::pending::<anyhow::Result<String>>());
    let stream = response_to_streaming_message(input);
    pin_mut!(stream);
    let items = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            items.push(item?);
        }
        anyhow::Ok(items)
    })
    .await??;
    let calls = requests(&items);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].tool_call.is_err());
    Ok(())
}
