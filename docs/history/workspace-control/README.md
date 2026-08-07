# Workspace control records

This folder holds the by-products of the August 2026 documentation pass over workspace control — the BR-71 feature that lets an agent open, watch, steer and reconfigure other BioRouter sessions. The pass itself produced living documentation, which is where the current truth lives; what lands here is the residue it could not file there: defects the pass found in **source** rather than in prose, and deliberately did not fix, because the pass was documentation-only and its worktree could not compile Rust. Each note therefore describes something that was **true of the tree on the date in its header** and may since have been repaired — check the source before acting on one.

Come here only for provenance: to find out whether a source-level oddity in the CLI or the workspace tools was already noticed, and what was decided about it. If you want to know how workspace control behaves today, leave for [`docs/agent-loop/workspace-control.md`](../../agent-loop/workspace-control.md) (the user guide), [`docs/agent-loop/workspace-control-tools.md`](../../agent-loop/workspace-control-tools.md) (the tool reference) or [`docs/agent-loop/designs/br71-execution-plan.md`](../../agent-loop/designs/br71-execution-plan.md) (the plan of record, still the authority on what each task was meant to build). Nothing in this folder is guidance, and nothing in it is a checklist to re-run.

> **Identifier scheme.** `BR-71` is the ticket for the workspace control feature as a whole; task numbers such as Task 20 refer to sections of its execution plan, which is the index for them.

## Documents

| Document | What it covers |
|---|---|
| [CLI plural alias defect](cli-plural-alias-defect.md) | `session_watch.rs:44` tells the user to run `biorouter sessions watch <id>`, but `sessions` is neither a command nor an alias — `Command::Session` carries only `visible_alias = "s"`. The exact wrong and correct strings, the evidence from the clap tree, and how to verify a fix. Open as of 2026-08-03. |

## Related documentation

- [Workspace control](../../agent-loop/workspace-control.md) — the live task-oriented guide the documentation pass produced.
- [Workspace control tools](../../agent-loop/workspace-control-tools.md) — the per-tool reference that accompanies it.
- [BR-71 workspace control implementation plan](../../agent-loop/designs/br71-execution-plan.md) — the plan of record for the feature, including the task-level amendments that predate these notes.
- [Historical records](../README.md) — the archive index, and the rules for what belongs in a campaign folder.
