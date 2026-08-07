# Agent-loop design documents

This folder holds per-proposal design documents. Most were produced by the agent-loop fix
campaign: each takes a single `BR-NN` proposal — a numbered entry in the 67-item
master list in [the agent-loop improvement proposals](../../history/agent-loop-review/improvement-proposals.md)
— and specifies how it is built: the problem, the mechanism, the data structures, and
the slices or phases the work is cut into. New post-campaign proposals continue the
`BR-NN` sequence from where the campaign left off (currently BR-71+) and say so in their
status header. Most are *partially shipped*: an early slice
is live code on `main` while the remainder is still the plan of record. Every document's
context header states exactly which parts exist and which do not, so read that header
before treating any section as a description of today's code.

Come here when you are implementing, extending, or reviewing one of these subsystems and
need the reasoning behind its design. Two neighbours cover different needs. For *why*
these proposals exist — the point-in-time diagnosis of the agent loop that generated the
`BR-NN` numbers — read [the agentic loop review](../../history/agent-loop-review/README.md);
for *how the work was sequenced* into waves, worktrees and regression gates, read
[the fix campaign record](../../history/agent-loop-campaign/README.md). Both are historical
records, not active plans. The campaign's cross-platform arm — the parity audit, the
shipped command-safety and CI-gate designs, and the superseded macOS-only Seatbelt design
that BR-69 below generalizes — was archived with the campaign and lives in
[`../../history/agent-loop-campaign/cross-platform/`](../../history/agent-loop-campaign/cross-platform/README.md);
the hook system's user-facing reference lives in [`../hooks/`](../hooks/hooks-reference.md).

## Documents

| Document | What it covers |
|---|---|
| [Command policy engine (BR-21)](command-policy-engine.md) | Replacing the evadable `THREAT_PATTERNS` regex table (since deleted) with an argv-parsing, path-canonicalizing, declarative allow/ask/deny policy engine whose rules carry their own self-tests. Slice 1 is live security code; Slices 2–3 remain the plan, and its tokenization section is superseded by BR-68. |
| [Cross-session memory (BR-17)](cross-session-memory.md) | Replacing three disjoint memory stores with one system: an FTS5-ranked chat index, auto-distillation of durable facts into a knowledge base, and a bounded always-on memory digest injected into every session. Piece 1 is live; the distillation and digest pieces are not built. |
| [OS-level tool sandboxing on Linux and Windows (BR-69)](linux-and-windows-sandboxing.md) | Generalizing the macOS-only Seatbelt sandbox into one `ShellSandbox` trait with three backends — Landlock plus seccomp on Linux with a bubblewrap fallback, and an honest "no containment" tier on Windows — plus the capability reporting that tells a user which tier they actually got. Current and partly implemented: Slices 0, 1, 2 and 4 shipped, while Slice 3 (real Windows containment) and the CI enforcement section remain the plan. |
| [Managed policy tier for guardrails and hooks (BR-65)](managed-policy-tier.md) | A non-overridable admin tier above user and project config: a trusted OS-specific managed-settings location, ownership verification, and the precedence model that BR-20/BR-21 rules and hooks plug into. The first slice is live; `verify_trusted()` is still a no-op on Windows. |
| [Session branching with stable message ids (BR-45)](session-branching.md) | Replacing positional, re-derived message ids with durable per-message UUIDs, and building a real session fork/branch tree on top of them. Phase 1 landed; the branching and tree UX is not built. |
| [Shadow-git checkpoints and `/rewind` (BR-43)](shadow-git-checkpoints.md) | Capturing the workspace into a private git object database at turn boundaries so a user can rewind files, conversation, or both. Slice 1 landed behind the `BIOROUTER_CHECKPOINTS` flag; the GUI rewind affordance, redo and GC remain the plan. |
| [Shared MCP server pool (BR-54)](shared-mcp-server-pool.md) | Eliminating the two axes of MCP process multiplication — one process tree per `Agent`, one daemon per app window — by pooling servers behind a fingerprint-keyed registry. Both slices shipped, so this now doubles as the architecture reference for the live pooling code. |
| [Agent workspace control and glass-box subagents (BR-71)](agent-workspace-control.md) | A `workspace` platform extension giving the agent MCP tools over the GUI and daemon — open/focus/close chat tabs and windows, read any conversation's transcript and tool calls, inject prompts, change a session's extensions/KBs — plus a daemon→GUI command bridge and a session event broadcast that together turn opaque subagents into live, human-interactive tabs. Post-campaign proposal; nothing implemented. |

## Related documentation

- [Agent-loop improvement proposals](../../history/agent-loop-review/improvement-proposals.md) — the master list every `BR-NN` identifier in this folder points back to.
- [Agent-loop fix campaign](../../history/agent-loop-campaign/README.md) — how these designs were sequenced into waves and what actually merged.
- [Cross-platform work in the agent-loop fix campaign](../../history/agent-loop-campaign/cross-platform/README.md) — the archived arm holding the parity audit that coined the `GAP-N` findings, the shipped BR-68 and BR-70 designs, and the [macOS Seatbelt design (BR-64)](../../history/agent-loop-campaign/cross-platform/macos-seatbelt-sandbox.md) whose phasing BR-69 above replaced.
- [Managed enterprise policy](../../security/managed-policy.md) — the administrator-facing guide to the tier BR-65 designs; read it instead if you are deploying rather than building.
