# Chat groups design

Chat groups are the browser-style tabs and splits in the BioRouter desktop chat area,
where each chat pane carries its own session and its own knowledge-base selection. This
folder holds the **live design records** for that feature — the spikes and specifications
that are still true and still constrain what may be built next.

Come here when you are about to change how chat panes are mounted, nested or scoped, and
you need to know which arrangements are currently unsafe. Two neighbouring folders cover
the rest of the story, and you probably want one of them instead: the design deliberation
that produced chat groups is a historical record in
[`docs/history/chat-groups/`](../../history/chat-groups/design-judgement-and-plan.md), and
the stage-by-stage record of what was actually built, committed and left open lives in
[`docs/design/ui-overhaul/`](../ui-overhaul/execution-status.md). This folder is neither a
plan nor a status board — it is the set of standing conditions the implementation has to
respect.

## Documents

| Document | What it covers |
|---|---|
| [Nested `KnowledgeProvider`: the chat-groups nesting blocker](knowledge-provider-nesting-blocker.md) | A spike report proving experimentally that two nested `KnowledgeProvider`s clobber each other's active knowledge base through `localStorage` and the server, and specifying the prerequisite fix that must land before chat groups may nest providers. Status is **Current, and the prerequisite fix is not made** — it was attempted on 2026-07-16 and reverted for lack of a green regression test, so nesting remains blocked. |

## Related documentation

- [Chat groups design judgement and plan](../../history/chat-groups/design-judgement-and-plan.md) — the historical record of the three competing chat-groups designs, the per-candidate risk registers that include `minimal-shell`'s `R7` — the one risk this folder's spike resolves — and the reduced plan authorised on 2026-07-16.
- [UI overhaul execution status](../ui-overhaul/execution-status.md) — the source-of-truth status record for the chat-groups and UI-cohesion branch, including the open-items list that still marks `KnowledgeProvider` nesting as blocked.
- [Knowledge plan 6: chat integration and closeout](../../history/knowledge-base-buildout/plan-6-chat-integration-and-closeout.md) — how the per-chat knowledge-base chip and `KnowledgeContext` came to exist, i.e. the code the nesting spike dissects.
- [Diverge behaviour checklist](../../desktop-ui/diverge-behavior-checklist.md) — neighbouring desktop-UI behaviour documentation for the same chat surface.
