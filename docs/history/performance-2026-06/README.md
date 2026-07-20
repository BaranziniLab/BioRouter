# Performance work, June 2026

This folder records **two independent performance efforts that both happened and both shipped**, run over three days in June 2026. The first was an internal whole-app latency review against v1.86.0 (2026-06-22) whose Tier 0 and part of its Tier 1 findings became nine merged commits the next day. The second compared BioRouter against the third-party jcode agent harness (2026-06-24) and implemented its First and Second Wave items the same day on the `perf/jcode-borrows` branch.

Everything here is a **historical record, kept for provenance rather than as current guidance**: the measured numbers, the commit hashes, and every `file:line` anchor are pinned to the tree as it stood on those dates, and each document's own header carries a `Status:` line saying so. Some outcomes are still visibly in the tree — `Cargo.toml` still carries `tikv-jemallocator` and the `release` / `release-dist` / `quick` profiles introduced by the jcode wave — but the documents were not updated as the code moved on.

Come here to find out **why** a performance decision was made: why SQLite runs with `synchronous=NORMAL`, why the streaming renderer coalesces to one repaint per frame, why the release profile strips symbols, or why a proposal you are considering was already rejected as low-value for BioRouter. Do not come here for current behaviour.

The two strategic proposals this work deferred — the shared MCP server pool (#12) and the one-daemon-per-window change (#18) — were **later designed and shipped elsewhere**, and their truth now lives in [the shared MCP server pool design](../../agent-loop/designs/shared-mcp-server-pool.md), not in these files. Likewise, the review's untouched Tier 2 and Tier 3 items were never scheduled here and should not be read as a live backlog.

For the current shape of the system, leave for [`docs/architecture/`](../../architecture/README.md); for the wider archive this folder sits in, see [`docs/history/`](../README.md).

## Documents

| Document | What it covers |
|---|---|
| [Performance and responsiveness review](review-findings.md) | A whole-app latency and resource review across 12 subsystems, synthesizing five cross-cutting themes — per-token streaming cost, blocking I/O on the async runtime, recompute-instead-of-cache, whole-object copies, and polling — into a tiered roadmap backed by `file:line` evidence. Conducted 2026-06-22 against v1.86.0; it is also the index for the `A1`–`L` finding IDs cited across this folder. |
| [Performance fixes: implementation log and benchmarks](implementation-log.md) | The implementation log for the nine fixes that came out of that review — one commit per fix, with behaviour-preservation evidence, before/after benchmark numbers, observable-behaviour caveats, and the list of items deliberately left undone. All nine were implemented on 2026-06-23 and merged and pushed to `origin/main`. |
| [Performance and efficiency comparison: jcode vs BioRouter](jcode-comparison-analysis.md) | A comparative analysis of the third-party jcode agent harness against BioRouter across startup, RAM, agent loop, rendering, compaction, build architecture and process model, ending in a prioritized roadmap of borrowable techniques numbered `#1`–`#24`. Dated 2026-06-24; its strategic items (#12, #18) were not implemented in this pass. |
| [Borrowing from jcode: implementation and benchmark report](jcode-borrows-implementation-report.md) | The completion report for that roadmap's First Wave and Second Wave changes — jemalloc, Cargo profiles, `spawn_blocking`, HTTP client hardening, scheduler and subagent caps, AWS feature gating, and the soft interrupt — with per-change verification and an honest verdict on each, including the ones whose gain for BioRouter turned out to be small. Implemented 2026-06-24 on branch `perf/jcode-borrows`. |

## Related documentation

- [Historical records](../README.md) — the archive index this folder belongs to, and how to read a `Status:` line.
- [Shared MCP server pool (BR-54)](../../agent-loop/designs/shared-mcp-server-pool.md) — where the resource-multiplication problem this folder identified but deferred was actually solved; the current reference for that live pooling code.
- [Architecture](../../architecture/README.md) — the current crate, process and layer boundaries that the jcode analysis argued about.
- [Agent loop review](../agent-loop-review/README.md) — the later, broader whole-system review of the same codebase, run in July 2026 with a correctness rather than latency lens.
