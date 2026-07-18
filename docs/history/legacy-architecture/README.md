# Legacy architecture

This folder holds superseded architecture documents — designs for BioRouter internals that were drafted, then replaced by something else. It is kept for the record and for provenance, not as guidance: **nothing described here is an API you can call today.** Neither document's vocabulary survives in the workspace. The extension trait design was never shipped at all — the hand-written `Extension` trait, `ToolRegistry` and `biorouter_macros` proc macro it proposes do not exist; extensions were rebuilt on MCP (Model Context Protocol) servers using the `rmcp` Rust SDK. The agent error model *was* built, and the two-tier policy it describes still governs the agent loop, but every concrete type name in it is obsolete: `AgentError` and `Result<T, AgentError>` are gone, replaced by rmcp's `ErrorData` and the `ToolResult<T>` alias. Neither document carries a date, so this folder has no date range.

Come here for one reason only: to understand *why* the extension API and the error paths are shaped the way they are — which naming, error-propagation and testing conventions predate MCP and partly survived the rewrite. If you want to know how extensions are authored, installed or configured today, leave for [`docs/extensions/`](../../extensions/README.md).

**For the error types actually in force, read the source, not another document** — no page under `docs/` restates them, and a hand-maintained copy is exactly what went stale here. The current definitions live in [`crates/biorouter/src/mcp_utils.rs`](../../../crates/biorouter/src/mcp_utils.rs) (the `ErrorData` re-export and the `ToolResult<T>` alias), [`crates/biorouter/src/conversation/message.rs`](../../../crates/biorouter/src/conversation/message.rs) (`ToolRequest` and `ToolResponse`, the message parts that embed a tool error), and [`crates/biorouter/src/providers/errors.rs`](../../../crates/biorouter/src/providers/errors.rs) (`ProviderError` and `ProviderErrorKind`, the infrastructure tier). The [agent error model](agent-error-model.md) below carries a mapping table from its own obsolete names to those, which is the fastest way in. For the rest of BioRouter's archive — completed campaigns, executed plans, removed features — see the [historical records index](../README.md).

## Documents

| Document | What it covers |
|---|---|
| [Agent error model](agent-error-model.md) | A design note on the two-tier error model: infrastructure failures raised to the caller with `anyhow::Error`, versus model-generated "agent errors" fed back to the LLM as recoverable prompts. Superseded — the concept still holds, but every type name is obsolete, so the page carries a table mapping each one to the code's current name and defining file. |
| [Extension trait design](extension-trait-design.md) | The original design sketch for an extension framework built around a hand-written `Extension` trait, a `ToolRegistry` and a `#[tool]` proc macro. Superseded and never shipped; every code sample is illustrative only, and one is preserved unfixed even though it does not compile. |

## Related documentation

- [Extensions and skills guide](../../extensions/extensions-and-skills-guide.md) — how extensions are actually authored, installed and configured today, on MCP rather than a Rust trait.
- [Extension manager](../../extensions/built-in/extension-manager.md) — the component that owns MCP extension lifecycle and tool registration, the role this design assigned to the `Extension` trait.
- [Architecture](../../architecture/README.md) — the current orientation-level map of how BioRouter is put together, and the agentic system explorer that documents how a failed tool call actually travels back to the model.
- [Historical records](../README.md) — the rest of BioRouter's archive, and how to check any archived document's standing.
