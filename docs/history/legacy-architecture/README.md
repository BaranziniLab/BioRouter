# Legacy architecture

This folder holds superseded architecture documents — designs for BioRouter internals that were drafted, then replaced by something else before or instead of shipping. It is kept for the record and for provenance, not as guidance: nothing described here is an API you can call today. The single document in it, the extension trait design, was **never shipped at all** — the hand-written `Extension` trait, `ToolRegistry` and `biorouter_macros` proc macro it proposes do not exist in the workspace. Extensions were rebuilt on MCP (Model Context Protocol) servers using the `rmcp` Rust SDK, and the current truth lives in [the extensions and skills guide](../../extensions/extensions-and-skills-guide.md) and [the extension manager reference](../../extensions/built-in/extension-manager.md). The document carries no date, so this folder has no date range.

Come here for one reason only: to understand *why* the extension API is shaped the way it is — which naming, error-propagation and testing conventions predate MCP and partly survived the rewrite. If you want to know how extensions are authored, installed or configured today, leave for [`docs/extensions/`](../../extensions/README.md). If you want the error types actually in force, leave for [`docs/architecture/`](../../architecture/README.md). For the rest of BioRouter's archive — completed campaigns, executed plans, removed features — see the [historical records index](../README.md).

## Documents

| Document | What it covers |
|---|---|
| [Extension trait design](extension-trait-design.md) | The original design sketch for an extension framework built around a hand-written `Extension` trait, a `ToolRegistry` and a `#[tool]` proc macro. Superseded and never shipped; every code sample is illustrative only, and one is preserved unfixed even though it does not compile. |

## Related documentation

- [Extensions and skills guide](../../extensions/extensions-and-skills-guide.md) — how extensions are actually authored, installed and configured today, on MCP rather than a Rust trait.
- [Extension manager](../../extensions/built-in/extension-manager.md) — the component that owns MCP extension lifecycle and tool registration, the role this design assigned to the `Extension` trait.
- [Agent error model](../../architecture/agent-error-model.md) — the error types that replaced the `AgentResult` / `ToolResult` vocabulary sketched in the legacy design.
- [Historical records](../README.md) — the rest of BioRouter's archive, and how to check any archived document's standing.
