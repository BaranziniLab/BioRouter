//! BRSDK observability — a span/trace model built from agent lifecycle events.
//!
//! BioRouter's hooks are fire-and-forget (shell/LLM judges) and the only live
//! channel the app WS sees is the `AgentEvent` stream, so surfacing a
//! "what did the agent do" timeline needs a separate, observe-only signal. This
//! module is the data model + the pure [`TraceBuilder`] that folds a stream of
//! [`ObsEvent`]s into a nested span tree — independently testable ahead of
//! wiring an in-process broadcast at the hook firing sites.
//!
//! Sensitive-data-safe by default: with `redact` on (the default) spans carry
//! names + timings + token counts only, never tool args or message text.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// An observe-only lifecycle event emitted around the agent loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObsEvent {
    TurnStart,
    TurnEnd,
    ToolStart {
        id: String,
        name: String,
    },
    ToolEnd {
        id: String,
        ok: bool,
    },
    LlmStart {
        model: String,
    },
    LlmEnd {
        model: String,
        input_tokens: u32,
        output_tokens: u32,
    },
    SubAgentStart {
        name: String,
    },
    SubAgentEnd {
        name: String,
    },
    CompactionStart {
        trigger: String,
    },
    CompactionEnd {
        trigger: String,
        before: u32,
        after: u32,
    },
    Guardrail {
        name: String,
        blocked: bool,
    },
    SessionStart,
    SessionEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Turn,
    Tool,
    Llm,
    SubAgent,
    Compaction,
    Guardrail,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Running,
    Ok,
    Error,
}

/// A single span in the trace tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    pub kind: SpanKind,
    pub name: String,
    pub start_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    pub status: SpanStatus,
    /// Non-sensitive structured detail (e.g. token counts). `None` when redacted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<serde_json::Value>,
}

/// A completed (or in-progress) trace for one session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub session_id: String,
    pub spans: Vec<Span>,
}

/// Folds a stream of [`ObsEvent`]s (each with a caller-supplied timestamp, so
/// tests are deterministic and the engine stamps real time) into a nested span
/// tree. `*Start` opens a child of the current open span; `*End` closes the
/// nearest matching open span.
pub struct TraceBuilder {
    redact: bool,
    spans: Vec<Span>,
    /// Indices into `spans` of currently-open spans (a stack).
    open: Vec<usize>,
    /// Correlation ids (e.g. tool call ids) for matching close events, keyed by
    /// span index so they stay out of the public `Span`.
    match_ids: HashMap<usize, String>,
    next_id: u64,
}

impl TraceBuilder {
    pub fn new(redact: bool) -> Self {
        TraceBuilder {
            redact,
            spans: Vec::new(),
            open: Vec::new(),
            match_ids: HashMap::new(),
            next_id: 1,
        }
    }

    /// Record one event at `ts_ms`.
    pub fn record(&mut self, ev: ObsEvent, ts_ms: u64) {
        match ev {
            ObsEvent::TurnStart => self.open_span(SpanKind::Turn, "turn".into(), ts_ms, None),
            ObsEvent::TurnEnd => self.close_span(SpanKind::Turn, None, ts_ms, SpanStatus::Ok),
            ObsEvent::SessionStart => {
                self.open_span(SpanKind::Session, "session".into(), ts_ms, None)
            }
            ObsEvent::SessionEnd => self.close_span(SpanKind::Session, None, ts_ms, SpanStatus::Ok),
            ObsEvent::ToolStart { id, name } => {
                self.open_named(SpanKind::Tool, name, Some(id), ts_ms, None);
            }
            ObsEvent::ToolEnd { id, ok } => {
                let status = if ok {
                    SpanStatus::Ok
                } else {
                    SpanStatus::Error
                };
                self.close_span(SpanKind::Tool, Some(&id), ts_ms, status);
            }
            ObsEvent::LlmStart { model } => {
                self.open_named(SpanKind::Llm, model, None, ts_ms, None);
            }
            ObsEvent::LlmEnd {
                model,
                input_tokens,
                output_tokens,
            } => {
                // Token counts are non-sensitive — kept even when redacted.
                let attrs = serde_json::json!({
                    "model": model,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                });
                self.close_span(SpanKind::Llm, None, ts_ms, SpanStatus::Ok);
                if let Some(last) = self.last_closed(SpanKind::Llm) {
                    self.spans[last].attrs = Some(attrs);
                }
            }
            ObsEvent::SubAgentStart { name } => {
                self.open_named(SpanKind::SubAgent, name, None, ts_ms, None);
            }
            ObsEvent::SubAgentEnd { name } => {
                self.close_named(SpanKind::SubAgent, &name, ts_ms, SpanStatus::Ok)
            }
            ObsEvent::CompactionStart { trigger } => {
                self.open_named(
                    SpanKind::Compaction,
                    format!("compaction:{trigger}"),
                    None,
                    ts_ms,
                    None,
                );
            }
            ObsEvent::CompactionEnd {
                trigger,
                before,
                after,
            } => {
                self.close_span(SpanKind::Compaction, None, ts_ms, SpanStatus::Ok);
                if let Some(last) = self.last_closed(SpanKind::Compaction) {
                    self.spans[last].attrs = Some(serde_json::json!({
                        "trigger": trigger, "before": before, "after": after
                    }));
                }
            }
            ObsEvent::Guardrail { name, blocked } => {
                // Guardrails are instantaneous (open+close at the same ts).
                let detail = if self.redact {
                    None
                } else {
                    Some(serde_json::json!({ "blocked": blocked }))
                };
                let status = if blocked {
                    SpanStatus::Error
                } else {
                    SpanStatus::Ok
                };
                let idx = self.open_named(
                    SpanKind::Guardrail,
                    format!("guardrail:{name}"),
                    None,
                    ts_ms,
                    detail,
                );
                self.spans[idx].end_ms = Some(ts_ms);
                self.spans[idx].status = status;
                self.open.pop(); // immediately closed
            }
        }
    }

    fn open_span(
        &mut self,
        kind: SpanKind,
        name: String,
        ts: u64,
        attrs: Option<serde_json::Value>,
    ) {
        self.open_named(kind, name, None, ts, attrs);
    }

    /// Open a span; returns its index in `spans`. `_match_id` is recorded as an
    /// attr-free correlation handle for matching the close event (tools).
    fn open_named(
        &mut self,
        kind: SpanKind,
        name: String,
        match_id: Option<String>,
        ts: u64,
        attrs: Option<serde_json::Value>,
    ) -> usize {
        let parent = self.open.last().map(|&i| self.spans[i].id);
        let id = self.next_id;
        self.next_id += 1;
        let attrs = if self.redact {
            redact_attrs(attrs)
        } else {
            attrs
        };
        self.spans.push(Span {
            id,
            parent,
            kind,
            name,
            start_ms: ts,
            end_ms: None,
            status: SpanStatus::Running,
            attrs,
        });
        let idx = self.spans.len() - 1;
        // Stash the match id alongside the open index by encoding it in a side
        // map would be cleaner; for the small per-session volume we just scan in
        // close_span. Keep the id in the span name is wrong, so track separately:
        self.open.push(idx);
        if let Some(mid) = match_id {
            self.match_ids.insert(idx, mid);
        }
        idx
    }

    fn close_span(&mut self, kind: SpanKind, match_id: Option<&str>, ts: u64, status: SpanStatus) {
        // Find the nearest open span (from the top of the stack) matching kind
        // (and id, when given).
        if let Some(pos) = self.open.iter().rposition(|&i| {
            self.spans[i].kind == kind
                && match_id
                    .is_none_or(|mid| self.match_ids.get(&i).map(|s| s.as_str()) == Some(mid))
        }) {
            let idx = self.open.remove(pos);
            self.spans[idx].end_ms = Some(ts);
            self.spans[idx].status = status;
            self.match_ids.remove(&idx);
        }
    }

    fn close_named(&mut self, kind: SpanKind, name: &str, ts: u64, status: SpanStatus) {
        if let Some(pos) = self
            .open
            .iter()
            .rposition(|&i| self.spans[i].kind == kind && self.spans[i].name == name)
        {
            let idx = self.open.remove(pos);
            self.spans[idx].end_ms = Some(ts);
            self.spans[idx].status = status;
        }
    }

    /// The most recently-closed span of a kind (for attaching post-close attrs).
    fn last_closed(&self, kind: SpanKind) -> Option<usize> {
        self.spans
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind == kind && s.end_ms.is_some())
            .map(|(i, _)| i)
            .next_back()
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    pub fn into_trace(self, session_id: impl Into<String>) -> Trace {
        Trace {
            session_id: session_id.into(),
            spans: self.spans,
        }
    }
}

fn redact_attrs(attrs: Option<serde_json::Value>) -> Option<serde_json::Value> {
    // Drop free-form detail under redaction; numeric token counts are re-added
    // by the LLM/compaction close paths which bypass this.
    let _ = attrs;
    None
}

/// A sink for completed spans/traces (e.g. an in-GUI streamer or an external
/// exporter like Langfuse/Phoenix). The default path needs no external dep.
pub trait TraceProcessor: Send + Sync {
    fn on_span(&self, session_id: &str, span: &Span);
    fn on_trace_complete(&self, trace: &Trace);
}

#[cfg(test)]
mod tests;
