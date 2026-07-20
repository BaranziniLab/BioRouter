# Research

This folder holds **external research** — studies of software outside BioRouter, written to
inform BioRouter's own design. Its single topic today is the coding-agent landscape: nine
reports reviewing how other agentic coding tools build their feedback loops, each covering
the same ten dimensions so they can be read side by side.

Come here when you want to know **how somebody else solved a loop problem** — how Gemini CLI
detects a stuck agent, how Cline checkpoints edits, how Codex CLI sandboxes a command — before
designing the BioRouter equivalent. Go elsewhere if you want BioRouter's own behaviour: how
the agent loop works *today* lives in [`docs/agent-loop/`](../agent-loop/README.md), and the
point-in-time diagnosis of BioRouter's loop, the head-to-head comparison chapters and the
`BR-NN` proposals derived from that diagnosis live under
[`docs/history/agent-loop-review/`](../history/agent-loop-review/README.md). The reports here
describe *other projects* and cite no BioRouter source, which is why they stay current while
BioRouter changes around them.

## The `BR-NN` identifiers

Every report cites proposal numbers of the form `BR-1`, `BR-43`, `BR-67`. These are the
improvement proposals assigned by BioRouter's agentic-loop review; the full index is
[the improvement proposals register](../history/agent-loop-review/improvement-proposals.md).
A report naming a `BR-NN` number is the cited external source behind that proposal.

## Coding agent landscape

All nine reports live in [`coding-agent-landscape/`](coding-agent-landscape/), which has no
index of its own — this table is it. Each report covers the same ten dimensions: system prompt
and context injection, tool loop mechanics, compaction and memory, hooks and extensibility,
guardrails and permissions, loop and stuck detection, long-running tasks and background
processes, state tracking and checkpoints, self-verification, and ideas worth stealing.

| Document | What it covers |
|---|---|
| [Aider](coding-agent-landscape/aider.md) | Aider's repo map, git integration, lint/test-after-edit and architect/editor split — a deliberately human-in-the-loop agent whose minimalism is the design lesson; the cited source for BR-1 and BR-47. |
| [Claude Code](coding-agent-landscape/claude-code.md) | Anthropic's terminal-native agent, treated as the reference design this corpus benchmarks against; the source for hook-model parity, BR-6 and BR-9. Pinned to the v2.1.x line of a closed-source product, so it dates faster than the open-source reports. |
| [Cline](coding-agent-landscape/cline.md) | Cline's shadow-git checkpoints, three-axis restore, mistake tracker and rules system; the source for the checkpoint design that BR-43 implemented. Researched 2026-07-12 against a project that had just refactored into a monorepo, so expect drift. |
| [Codex CLI](coding-agent-landscape/codex-cli.md) | OpenAI Codex CLI's per-model system-prompt files, `execpolicy` Starlark command policy, OS sandboxing and ranked memories layer; a Rust workspace whose crate layout maps onto BioRouter's own. Source for BR-2, BR-3, BR-19 and BR-21. |
| [Gemini CLI](coding-agent-landscape/gemini-cli.md) | Google's agent: layered `LoopDetectionService`, declarative TOML policy engine, shadow-git checkpointing with `/rewind`, and 30%-verbatim-tail compaction. An independent architecture with no shared lineage, and the deepest report here. Source for BR-10, BR-29/BR-30 and BR-43. |
| [Goose](coding-agent-landscape/goose.md) | Upstream Goose (Block, now the Agentic AI Foundation) — the project BioRouter forked from — emphasising what upstream added in 2025–2026 that a mid-2025 fork lacks. The repository's only record of upstream divergence; a July 2026 snapshot, so each "fork gap" needs re-verification before being acted on. |
| [OpenCode](coding-agent-landscape/opencode.md) | SST's OpenCode: the client/server split, private git-object-DB snapshots with `/undo` and `/redo`, and prune-before-summarize compaction; the cited source for the checkpoint approach adopted in BR-43. Researched 2026-07-12. |
| [OpenHands](coding-agent-landscape/openhands.md) | OpenHands' five-heuristic `StuckDetector`, the structured summarizing condenser that preserves task IDs, and risk-graded confirmation via a per-action `security_risk` field; the source for BR-30's oscillation detection and BR-18's risk-graded permissions. |
| [Pi](coding-agent-landscape/pi.md) | Pi's deliberately subtractive loop — a ~1000-token system prompt, no MCP, no subagents, session-tree branching, and typed extension hooks that rewrite the prompt and message array; the source for BR-9's per-directory Project Trust and BR-11's split-turn compaction fallback. |

All nine are marked **Current**: they describe external projects, so BioRouter's own changes do
not invalidate them. Several are explicitly dated snapshots of fast-moving repositories, noted
per report above.

## Related documentation

- [Agentic loop review](../history/agent-loop-review/README.md) — the executive report these nine external reviews fed into, including the four head-to-head comparison chapters that score the agents against each other.
- [Improvement proposals register](../history/agent-loop-review/improvement-proposals.md) — the `BR-1`…`BR-67` index; the destination for every proposal number cited in this folder.
- [Agent loop](../agent-loop/README.md) — how BioRouter's own loop, context engineering, hooks and subagents work today, as opposed to how other tools do it.
- [Agent-loop campaign](../history/agent-loop-campaign/README.md) — the implementation campaign that acted on the gaps these reports identified.
