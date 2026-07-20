# Chat groups design history

Chat groups are the browser-style tabs and splits in the BioRouter desktop chat area. This
folder holds the deliberation that produced them: the adversarial judgement of three
competing designs and the reduced plan authorised on 2026-07-16. It documents completed
work and is kept for the record, not as current guidance. The work did happen — but the
plan's central decision, to ship tabs and defer splitting, was overtaken within the same
branch, which went on to ship the split with drag, drop zones and the global terminal dock
as well. For what was actually built, see
[UI overhaul — execution status](../../design/ui-overhaul/execution-status.md).

Come here only when you need to know *why* chat groups are shaped the way they are — which
alternatives were rejected, on what grounds, and which risks were measured rather than
assumed. If you need the standing constraints the implementation must still respect, go to
[`docs/design/chat-groups/`](../../design/chat-groups/README.md) instead; if you need the
stage-by-stage record of what shipped, go to
[`docs/design/ui-overhaul/`](../../design/ui-overhaul/execution-status.md). Nothing in this
folder is a specification, and its file-by-file work list should not be executed.

## Documents

| Document | What it covers |
|---|---|
| [Chat groups: design judgement and reduced plan](design-judgement-and-plan.md) | The adversarial reading of three candidate designs (`lift-state`, `minimal-shell`, `reuse-dashboard`) against each other, each one's fatal flaw, the synthesis of the parts that survived scrutiny, and the reduced plan authorised on 2026-07-16 — chat tabs in a single group, with the splitting machinery's state model landed but unrendered. Includes the measured results that closed the `WebkitAppRegion` drag risk and found N-mounted BaseChats affordable. Status is **historical record, overtaken by the work that followed**. |

> **Note.** The three candidate designs judged here are not themselves preserved under
> `docs/`, so this file is the only surviving record of their content. Its source citations
> (`BaseChat.tsx:1370` and similar) are pinned to the tree as it stood on 2026-07-16 and
> have since drifted — treat every line number as a pointer to a symbol, not an address.

## Related documentation

- [Chat groups design](../../design/chat-groups/README.md) — the live design records for
  the same feature, including the nesting blocker that resolved the one risk this plan
  deferred rather than answered.
- [UI overhaul — execution status](../../design/ui-overhaul/execution-status.md) — the
  source-of-truth status record for the branch this plan fed into, showing that tabs *and*
  the split both shipped.
- [Dashboard mode — removal record](../dashboard-mode/README.md) — what happened to the
  Dashboard whose cost, bugs and possible deletion are weighed throughout this judgement.
- [Project history](../README.md) — the index of all archived BioRouter work, of which
  this folder is one entry.
