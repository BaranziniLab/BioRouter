# Agent error model

> **What this is.** A design note on Biorouter's two-tier error model: infrastructure failures that are raised to the caller, versus model-generated "agent errors" that are fed back to the LLM as recoverable prompts.
> **Status:** Superseded — the *concept* described here still governs the agent loop, but every concrete type name is obsolete. The `AgentError` enum and the `Result<T, AgentError>` alias no longer exist anywhere in `crates/`; rmcp's `ErrorData` replaced them. Current truth lives in the source: [`crates/biorouter/src/mcp_utils.rs`](../../../crates/biorouter/src/mcp_utils.rs), [`crates/biorouter/src/conversation/message.rs`](../../../crates/biorouter/src/conversation/message.rs), [`crates/biorouter/src/agents/agent.rs`](../../../crates/biorouter/src/agents/agent.rs), and [`crates/biorouter/src/providers/errors.rs`](../../../crates/biorouter/src/providers/errors.rs).
> **Audience:** developers working on the agent loop and on provider integrations.

Error handling is a key performance-driving part of Biorouter. There are many ways that the
non-determinism in the LLM can introduce an error that it can in turn recover from. In a typical
Biorouter session, it is expected for there to be several agent errors that the model can see
directly and correct, perhaps entirely behind the scenes.

The design turns on one distinction. An error is either something the *system* got wrong — a
dropped connection, an unavailable model — which no amount of re-prompting will fix, or something
the *model* got wrong — a misspelled tool name, malformed arguments — which the model can very
likely fix if it is simply told what happened. The two travel down different paths.

> **Note on type names.** This document was written against an earlier API. The table under
> [Current type names](#current-type-names) maps each name used below to what the code calls it
> today. The behaviour it describes is unchanged.

## Infrastructure errors

While the agent is operating, there can be intermittent issues in the network, availability of the
foundational model, etc. These are raised as errors in the agent API to the caller, who can decide
how to handle that. We generally handle these with [anyhow::Error][anyhow-error].

These never reach the model. The caller — the CLI, or the `biorouterd` route serving the desktop
UI — decides whether to retry, surface a message, or abort the turn.

## Agent errors

There are several types of errors where everything is working correctly, but the model generations
themselves are somehow causing errors. Things like generating an unknown tool name, incorrect
parameters, or a well formed tool call that results in an error in the tool itself. All of these can
be surfaced to the LLM to have it attempt to recover.

The error messages are in some ways prompting — they give instructions to the LLM on how it might go
about recovering. We handle these with [thiserror::Error][this-error] and carefully maintain a
collection.

To cover all these cases, both `ToolUse` and `ToolResult` are typically passed through the API as
part of a `Result<T, AgentError>`. An error in a `ToolUse` will immediately become an error in a
`ToolResult` and passed back to the LLM. A valid `ToolUse` might still end up in an error
`ToolResult`, which is also passed back to the LLM.

The providers then handle translating the agent errors into the various API specs as valid messages.

## Current type names

The shape the sections above describe survives intact — a tool use and a tool result each carry a
`Result` whose error variant is destined for the model. Only the names changed when Biorouter moved
onto rmcp's error type.

| Name used above | Name in the code today | Defined in |
|---|---|---|
| `AgentError` | `rmcp::model::ErrorData` (re-exported as `biorouter::mcp_utils::ErrorData`) | [`crates/biorouter/src/mcp_utils.rs`](../../../crates/biorouter/src/mcp_utils.rs) |
| `Result<T, AgentError>` | `ToolResult<T>` | [`crates/biorouter/src/mcp_utils.rs`](../../../crates/biorouter/src/mcp_utils.rs) |
| `ToolUse` | `ToolRequest` | [`crates/biorouter/src/conversation/message.rs`](../../../crates/biorouter/src/conversation/message.rs) |
| `ToolResult` (the message part) | `ToolResponse` | [`crates/biorouter/src/conversation/message.rs`](../../../crates/biorouter/src/conversation/message.rs) |

The alias is a one-liner:

```rust
pub use rmcp::model::ErrorData;

/// Type alias for tool results
pub type ToolResult<T> = Result<T, ErrorData>;
```

Both message parts embed it, which is what keeps a failed tool call representable as a message the
provider can serialize and send back:

```rust
pub struct ToolRequest {
    pub id: String,
    pub tool_call: ToolResult<CallToolRequestParams>,
    // ...
}

pub struct ToolResponse {
    pub id: String,
    pub tool_result: ToolResult<CallToolResult>,
    // ...
}
```

## Where this is implemented

| Concern | Location |
|---|---|
| Tool dispatch, and the construction of error results handed back to the model | [`crates/biorouter/src/agents/agent.rs`](../../../crates/biorouter/src/agents/agent.rs) |
| The `ToolResult` alias and the `ErrorData` re-export | [`crates/biorouter/src/mcp_utils.rs`](../../../crates/biorouter/src/mcp_utils.rs) |
| `ToolRequest` / `ToolResponse` message parts | [`crates/biorouter/src/conversation/message.rs`](../../../crates/biorouter/src/conversation/message.rs) |
| Provider-side error translation into each vendor's API spec | [`crates/biorouter/src/providers/errors.rs`](../../../crates/biorouter/src/providers/errors.rs) |

[anyhow-error]: https://docs.rs/anyhow/latest/anyhow/
[this-error]: https://docs.rs/thiserror/latest/thiserror/

## Related documentation

- [System overview](../../architecture/system-overview.md) — places this error policy inside the wider agent interaction loop.
- [Extension trait design](extension-trait-design.md) — the historical extension API this error model was designed alongside; it shares the same superseded vocabulary.
- [Context engineering](../../agent-loop/context-engineering.md) — what happens to error messages once they are in the conversation and the context window fills up.
- [Diagnostics and bug reports](../../troubleshooting/diagnostics-and-bug-reports.md) — how to capture the logs when an error is *not* recovered from.
