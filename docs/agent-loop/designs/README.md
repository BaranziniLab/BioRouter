# Agent-loop design documents

This folder holds the per-proposal design documents produced by the agent-loop fix
campaign. Each one takes a single `BR-NN` proposal — a numbered entry in the 67-item
master list in [the agent-loop improvement proposals](../../history/agent-loop-review/improvement-proposals.md)
— and specifies how it is built: the problem, the mechanism, the data structures, and
the slices or phases the work is cut into. Most are *partially shipped*: an early slice
is live code on `main` while the remainder is still the plan of record. Every document's
context header states exactly which parts exist and which do not, so read that header
before treating any section as a description of today's code.

Come here when you are implementing, extending, or reviewing one of these subsystems and
need the reasoning behind its design. Two neighbours cover different needs. For *why*
these proposals exist — the point-in-time diagnosis of the agent loop that generated the
`BR-NN` numbers — read [the agentic loop review](../../history/agent-loop-review/README.md);
for *how the work was sequenced* into waves, worktrees and regression gates, read
[the fix campaign record](../../history/agent-loop-campaign/README.md). Both are historical
records, not active plans. Designs whose scope is cross-platform behaviour live one folder
over in [`../cross-platform/`](../cross-platform/linux-and-windows-sandboxing.md), and the
hook system's user-facing reference lives in [`../hooks/`](../hooks/hooks-reference.md).

## Documents

| Document | What it covers |
|---|---|
| [Command policy engine (BR-21)](command-policy-engine.md) | Replacing the evadable `THREAT_PATTERNS` regex table with an argv-parsing, path-canonicalizing, declarative allow/ask/deny policy engine whose rules carry their own self-tests. Slice 1 is live security code; Slices 2–3 remain the plan, and its tokenization section is superseded by BR-68. |
| [Cross-session memory (BR-17)](cross-session-memory.md) | Replacing three disjoint memory stores with one system: an FTS5-ranked chat index, auto-distillation of durable facts into a knowledge base, and a bounded always-on memory digest injected into every session. Piece 1 is live; the distillation and digest pieces are not built. |
| [macOS Seatbelt sandbox for the shell tool (BR-64)](macos-seatbelt-sandbox.md) | The first kernel-enforced containment of the developer shell tool — a Seatbelt profile with injected writable roots and outbound network denied, kept separate from the approval policy. **Superseded historical record:** read it for the profile design and the two-axis model, but not for its phasing, which BR-69 replaced wholesale. |
| [Managed policy tier for guardrails and hooks (BR-65)](managed-policy-tier.md) | A non-overridable admin tier above user and project config: a trusted OS-specific managed-settings location, ownership verification, and the precedence model that BR-20/BR-21 rules and hooks plug into. The first slice is live; `verify_trusted()` is still a no-op on Windows. |
| [Session branching with stable message ids (BR-45)](session-branching.md) | Replacing positional, re-derived message ids with durable per-message UUIDs, and building a real session fork/branch tree on top of them. Phase 1 landed; the branching and tree UX is not built. |
| [Shadow-git checkpoints and `/rewind` (BR-43)](shadow-git-checkpoints.md) | Capturing the workspace into a private git object database at turn boundaries so a user can rewind files, conversation, or both. Slice 1 landed behind the `BIOROUTER_CHECKPOINTS` flag; the GUI rewind affordance, redo and GC remain the plan. |
| [Shared MCP server pool (BR-54)](shared-mcp-server-pool.md) | Eliminating the two axes of MCP process multiplication — one process tree per `Agent`, one daemon per app window — by pooling servers behind a fingerprint-keyed registry. Both slices shipped, so this now doubles as the architecture reference for the live pooling code. |

## Related documentation

- [Agent-loop improvement proposals](../../history/agent-loop-review/improvement-proposals.md) — the master list every `BR-NN` identifier in this folder points back to.
- [Agent-loop fix campaign](../../history/agent-loop-campaign/README.md) — how these designs were sequenced into waves and what actually merged.
- [OS-level tool sandboxing on Linux and Windows (BR-69)](../cross-platform/linux-and-windows-sandboxing.md) — the current plan of record that supersedes the Seatbelt design's phasing.
- [Managed enterprise policy](../../security/managed-policy.md) — the administrator-facing guide to the tier BR-65 designs; read it instead if you are deploying rather than building.
