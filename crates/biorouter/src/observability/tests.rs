use super::*;

fn find<'a>(spans: &'a [Span], kind: SpanKind, name_contains: &str) -> &'a Span {
    spans
        .iter()
        .find(|s| s.kind == kind && s.name.contains(name_contains))
        .unwrap_or_else(|| panic!("no {kind:?} span containing {name_contains:?}"))
}

#[test]
fn builds_nested_turn_with_tool_and_llm_children() {
    let mut b = TraceBuilder::new(true);
    // A realistic turn: model call, a tool call, another model call.
    b.record(ObsEvent::TurnStart, 100);
    b.record(ObsEvent::LlmStart { model: "claude-opus-4-8".into() }, 110);
    b.record(
        ObsEvent::LlmEnd { model: "claude-opus-4-8".into(), input_tokens: 1200, output_tokens: 64 },
        180,
    );
    b.record(ObsEvent::ToolStart { id: "t1".into(), name: "knowledge__search".into() }, 200);
    b.record(ObsEvent::ToolEnd { id: "t1".into(), ok: true }, 260);
    b.record(ObsEvent::TurnEnd, 300);

    let spans = b.spans();
    let turn = find(spans, SpanKind::Turn, "turn");
    let llm = find(spans, SpanKind::Llm, "claude-opus-4-8");
    let tool = find(spans, SpanKind::Tool, "knowledge__search");

    // Nesting: llm + tool are children of the turn.
    assert_eq!(llm.parent, Some(turn.id));
    assert_eq!(tool.parent, Some(turn.id));
    // Timings closed correctly.
    assert_eq!(turn.start_ms, 100);
    assert_eq!(turn.end_ms, Some(300));
    assert_eq!(tool.end_ms, Some(260));
    assert!(matches!(turn.status, SpanStatus::Ok));
    assert!(matches!(tool.status, SpanStatus::Ok));
    // Token counts survive redaction (non-sensitive).
    let attrs = llm.attrs.as_ref().expect("llm token attrs");
    assert_eq!(attrs["input_tokens"], 1200);
    assert_eq!(attrs["output_tokens"], 64);
}

#[test]
fn failed_tool_marked_error_and_matched_by_id() {
    let mut b = TraceBuilder::new(true);
    b.record(ObsEvent::TurnStart, 0);
    // Two concurrent-ish tools; the second fails. Closing must match by id.
    b.record(ObsEvent::ToolStart { id: "a".into(), name: "shell".into() }, 10);
    b.record(ObsEvent::ToolStart { id: "b".into(), name: "fetch".into() }, 20);
    b.record(ObsEvent::ToolEnd { id: "b".into(), ok: false }, 30);
    b.record(ObsEvent::ToolEnd { id: "a".into(), ok: true }, 40);
    b.record(ObsEvent::TurnEnd, 50);

    let spans = b.spans();
    let shell = find(spans, SpanKind::Tool, "shell");
    let fetch = find(spans, SpanKind::Tool, "fetch");
    assert!(matches!(shell.status, SpanStatus::Ok), "shell (id a) succeeded");
    assert!(matches!(fetch.status, SpanStatus::Error), "fetch (id b) failed");
    assert_eq!(fetch.end_ms, Some(30));
    assert_eq!(shell.end_ms, Some(40));
}

#[test]
fn subagent_and_compaction_nest_and_carry_attrs() {
    let mut b = TraceBuilder::new(true);
    b.record(ObsEvent::TurnStart, 0);
    b.record(ObsEvent::SubAgentStart { name: "researcher".into() }, 5);
    b.record(ObsEvent::ToolStart { id: "x".into(), name: "kb".into() }, 6);
    b.record(ObsEvent::ToolEnd { id: "x".into(), ok: true }, 7);
    b.record(ObsEvent::SubAgentEnd { name: "researcher".into() }, 8);
    b.record(ObsEvent::CompactionStart { trigger: "auto".into() }, 9);
    b.record(ObsEvent::CompactionEnd { trigger: "auto".into(), before: 100000, after: 40000 }, 12);
    b.record(ObsEvent::TurnEnd, 15);

    let spans = b.spans();
    let turn = find(spans, SpanKind::Turn, "turn");
    let sub = find(spans, SpanKind::SubAgent, "researcher");
    let tool = find(spans, SpanKind::Tool, "kb");
    let comp = find(spans, SpanKind::Compaction, "auto");
    assert_eq!(sub.parent, Some(turn.id));
    // The tool ran INSIDE the sub-agent, so its parent is the sub-agent.
    assert_eq!(tool.parent, Some(sub.id));
    let attrs = comp.attrs.as_ref().expect("compaction attrs");
    assert_eq!(attrs["before"], 100000);
    assert_eq!(attrs["after"], 40000);
}

#[test]
fn guardrail_span_is_instantaneous_and_status_reflects_block() {
    let mut b = TraceBuilder::new(true);
    b.record(ObsEvent::TurnStart, 0);
    b.record(ObsEvent::Guardrail { name: "pii".into(), blocked: true }, 5);
    b.record(ObsEvent::TurnEnd, 10);
    let spans = b.spans();
    let g = find(spans, SpanKind::Guardrail, "pii");
    assert_eq!(g.start_ms, 5);
    assert_eq!(g.end_ms, Some(5)); // instantaneous
    assert!(matches!(g.status, SpanStatus::Error)); // blocked → error status
}

#[test]
fn redaction_drops_free_form_attrs_but_keeps_token_counts() {
    // Redacted: guardrail detail is dropped.
    let mut redacted = TraceBuilder::new(true);
    redacted.record(ObsEvent::Guardrail { name: "injection".into(), blocked: false }, 0);
    assert!(redacted.spans()[0].attrs.is_none(), "redacted guardrail has no attrs");

    // Un-redacted: guardrail detail is present.
    let mut open = TraceBuilder::new(false);
    open.record(ObsEvent::Guardrail { name: "injection".into(), blocked: false }, 0);
    assert!(open.spans()[0].attrs.is_some(), "unredacted guardrail keeps attrs");
}

#[test]
fn trace_serializes_to_json() {
    let mut b = TraceBuilder::new(true);
    b.record(ObsEvent::TurnStart, 0);
    b.record(ObsEvent::TurnEnd, 1);
    let trace = b.into_trace("app:demo:c1");
    let json = serde_json::to_string(&trace).unwrap();
    let back: Trace = serde_json::from_str(&json).unwrap();
    assert_eq!(back.session_id, "app:demo:c1");
    assert_eq!(back.spans.len(), 1);
}
