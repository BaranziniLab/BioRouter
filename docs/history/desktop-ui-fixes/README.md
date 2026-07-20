# Desktop UI fixes

This folder holds the implementation plan for a single shipped release: the four bug
fixes batched into BioRouter v1.72.1 in May 2026. The work described here **did happen**
— all four fixes were implemented and released — but the repository has moved well past
v1.72.1, so the code samples inside are a record of what was written then, not current
source. Read this folder for provenance, never as guidance. For how the skills extension
behaves today, see the [skills extension reference](../../extensions/built-in/skills.md),
which supersedes the Rust code in Task 1.

Come here when you are asking why the disabled-skill enforcement or the import-modal code
looks the way it does, or when you are reconstructing what v1.72.1 announced — that
release never got a notes file under [`docs/releases/notes/`](../../releases/notes/README.md),
so the heredoc at the end of the plan is the only surviving record of its changelog. If
you instead want to *run* the desktop app and check its current behaviour, leave for
[`docs/desktop-ui/`](../../desktop-ui/README.md). For later, broader desktop defect
sweeps rather than this one release batch, see the sibling folders listed in the
[history index](../README.md).

## Documents

| Document | What it covers |
|---|---|
| [v1.72.1 bug fix batch](v1-72-1-bug-fix-batch.md) | The implementation plan for the four fixes batched into v1.72.1: runtime enforcement of disabled skills in the Rust agent, plus a drop-zone-only redesign of the Import Session, Import Workflow, and Add Skill modals. All four shipped. |

## Related documentation

- [Skills extension reference](../../extensions/built-in/skills.md) — the current behaviour of the skills extension, superseding this folder's Rust code samples.
- [Desktop UI testing and debugging](../../desktop-ui/README.md) — how to launch and drive the real desktop app, which this historical plan does not tell you.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later, broader sweep of desktop UI defects in the same subsystem.
- [Historical records index](../README.md) — the other archived topic folders, including the neighbouring desktop-UI histories.
