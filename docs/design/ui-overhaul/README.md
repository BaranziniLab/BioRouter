# UI overhaul

This folder holds the **app-wide** design specification and the status record for the
BioRouter desktop UI overhaul carried out on the `worktree-redesign-ui-cohesion` branch in
July 2026. It covers two jobs done together: the **cohesion pass**, which made the app look
like one product rather than several, and **chat groups**, which turned the chat area into
browser-style tabs and splits. Both documents describe the shell — the surfaces every view
sits inside — rather than any one view.

Come here when you need to know what the desktop shell was changed to and why, what shipped
versus what is still open, or what a specific `D-NN` design decision was and how it was
resolved. Do **not** come here for the design system itself: the Parchment palette, the
token ladders, the `D-NN` decision register and the `DR-NN` drift register live in
[`design.md`](../../../design.md) at the repo root, and both documents here cite that file
rather than restating it. The two **view-level** redesigns specified in the same period —
the Home page (`H-01`…`H-08`) and the Knowledge view (`K-01`…`K-08`) — shipped and were
signed off, so they were archived to
[`docs/history/ui-overhaul-2026-07/`](../../history/ui-overhaul-2026-07/README.md). Branding
assets, theme token references and the chat-groups nesting blocker live in sibling folders
under `docs/design/`; the superseded chat-groups planning packet lives under `docs/history/`.

## Documents

| Document | What it covers |
|---|---|
| [Execution status](execution-status.md) | The single status record for the UI cohesion and chat-groups branch: the 20-step list, every commit, the gates, what was proven by driving the real app, the register of what is still broken or open, and the brand rollout. Current, and the stated source of truth for the branch — all 20 steps are done, but open items remain. |
| [UI cohesion redesign](ui-cohesion-redesign.md) | The written half of the app-wide cohesion inspection spec (rev 2): the forensics explaining why the shipped app never matched the design sketch, plus specifications for the markdown layer, preview panel, terminal, tabbed chat groups and every floating surface. A design specification only — nothing was committed to the app at the time of writing; execution is tracked in the status record above. |

## Interactive pages

`ui-cohesion-redesign.html` is the rendered companion to the cohesion specification above.
**It must be opened in a browser to be useful** — it carries the pixels, and Markdown
cannot reproduce them. It shows a full interactive mockup of the BioRouter shell — sidebar,
chat transcript, composer, terminal dock, preview panel — with toggles for theme,
Current ⇄ Redesigned, sidebar collapse, split, mid-drag, terminal and highlight, plus
side-by-side markdown specimens, live component frames and colour swatches. The Markdown
companion carries the reasoning, so an agent working without a browser should read that
instead.

## Related documentation

- [BioRouter design system](../../../design.md) — the Parchment palette, the `D-NN`
  decision register (Parts 6 and 6b) and the `DR-NN` drift register (Part 7) that both
  documents in this folder cite; read it before changing any value specified here.
- [UI overhaul, July 2026](../../history/ui-overhaul-2026-07/README.md) — the archived
  Home-page and Knowledge-view redesigns specified alongside this branch, both shipped and
  signed off; read them for the `H-NN` and `K-NN` decisions.
- [Nested `KnowledgeProvider`: the chat-groups nesting blocker](../chat-groups/knowledge-provider-nesting-blocker.md) —
  the `R7` spike proving two nested `KnowledgeProvider`s clobber each other, and the
  prerequisite fix behind the one blocked item in this branch's register.
- [Chat groups: design judgement and reduced plan](../../history/chat-groups/design-judgement-and-plan.md) —
  the adversarial judgement of three competing chat-groups designs and the plan
  authorised on 2026-07-16; a historical record, overtaken by what the branch went on to
  ship.
- [BioRouter logo and wordmark specification](../branding/logo-and-wordmark-spec.md) —
  the normative geometry and colour for the marks whose rollout the execution status
  logs; the execution status is authoritative on the typeface it left open.
