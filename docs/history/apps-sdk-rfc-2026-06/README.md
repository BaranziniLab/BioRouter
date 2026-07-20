# Apps SDK RFC, June 2026

This folder holds the two-part RFC that proposed BioRouter's layered **App SDK** over 2026-06-23 and 2026-06-24 — one document arguing *why and what*, the other specifying *how*. The work described here did happen: the direction was adopted and largely built. The `biorouter-sandbox` leaf crate the RFC recommended now exists, `agent_drafter` gained `manifest.rs`, `vault.rs`, `control.rs` and `declare.rs`, and the manifest, capabilities and orchestration blocks these documents proposed are the ones that ship today. The open decisions the RFC left to the maintainer were settled by what was built. Both documents are kept as a historical record of the reasoning, not as current guidance, and both have since been superseded.

Come here to trace *why the shipped design looks the way it does* — the competitive framing against OpenAI's Agents SDK, the ADOPT/CONSIDER/SKIP inventory, and the original type and frame proposals. Do not come here for current behaviour. [`docs/apps-sdk/`](../../apps-sdk/README.md) owns the live contract: [the SDK reference](../../apps-sdk/sdk-reference.md) is the authority on what actually ships, and [the Apps SDK v2 design spec](../../apps-sdk/v2-design.md) (2026-07-12) is the current design of record that superseded these RFCs. [`docs/agent-drafter/`](../../agent-drafter/README.md) is the subsystem map. One caveat that applies to both files here: every `file:line` anchor was verified only against the tree as it stood in June 2026, and the repository has been refactored since — the file paths remain durable, the line numbers do not.

## Documents

| Document | What it covers |
|---|---|
| [BioRouter App SDK strategy RFC and OpenAI comparison](strategy-and-openai-comparison.md) | The "why and what" half, authored 2026-06-23: benchmarks OpenAI's Agents SDK against BioRouter's existing primitives and proposes a layered App SDK — files, databases, orchestration, vault, sandboxed compute, guardrails, context — with a phased roadmap and a list of open decisions for the maintainer. Its OpenAI product descriptions are a snapshot of a competitor SDK on that date and the announcement-level rows were never directly verified. |
| [BioRouter App SDK implementation design](implementation-design.md) | The "how" half, authored 2026-06-24: the code-level companion RFC giving concrete Rust types and traits, the unified app manifest, WebSocket protocol v2 frames, the exact hook points in the tree at the time of writing, schema migration v9, a phased build order and a consolidated test plan. Where it corrects the strategy RFC from the real code — session schema version 8, not 7 — this document is the later and more accurate of the pair. |

## Related documentation

- [Apps SDK](../../apps-sdk/README.md) — the live contract this RFC became; start here for anything current
- [BioRouter Apps SDK v2 design](../../apps-sdk/v2-design.md) — the 2026-07-12 design of record that superseded both documents in this folder
- [Agent Drafter](../../agent-drafter/README.md) — the subsystem map: what a BioRouter app is and how the pieces fit together
- [100-app Agent Drafter test drive](../agent-drafter-testdrive-100/README.md) — archived evidence from the campaign that stress-tested the shipped result
