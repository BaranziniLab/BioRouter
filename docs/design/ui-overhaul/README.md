# UI overhaul

This folder holds the design specifications and the status record for the BioRouter
desktop UI overhaul carried out on the `worktree-redesign-ui-cohesion` branch in July
2026. It covers two jobs done together: the **cohesion pass**, which made the app look
like one product rather than several, and **chat groups**, which turned the chat area
into browser-style tabs and splits. Alongside those sit two view-level redesigns — the
Home page and the Knowledge view — that were specified and shipped in the same period.

Come here when you need to know what the desktop UI was changed to and why, what shipped
versus what is still open, or what a specific redesign decision (`D-NN`, `H-01`…`H-08`,
`K-01`…`K-08`) was and how it was resolved. Do **not** come here for the design system
itself: the Parchment palette, the token ladders, the `D-NN` decision register and the
`DR-NN` drift register live in [`design.md`](../../../design.md) at the repo root, and
every document in this folder cites that file rather than restating it. Branding assets,
theme token references and the chat-groups nesting blocker live in sibling folders under
`docs/design/`; the superseded chat-groups planning packet lives under `docs/history/`.

## Documents

| Document | What it covers |
|---|---|
| [Execution status](execution-status.md) | The single status record for the UI cohesion and chat-groups branch: the 20-step list, every commit, the gates, what was proven by driving the real app, the register of what is still broken or open, and the brand rollout. Current, and the stated source of truth for the branch — all 20 steps are done, but open items remain. |
| [UI cohesion redesign](ui-cohesion-redesign.md) | The written half of the app-wide cohesion inspection spec (rev 2): the forensics explaining why the shipped app never matched the design sketch, plus specifications for the markdown layer, preview panel, terminal, tabbed chat groups and every floating surface. A design specification only — nothing was committed to the app at the time of writing; execution is tracked in the status record above. |
| [Home screen redesign](home-screen-redesign.md) | Why the Home column was realigned to the chat column, what the token and session numbers on Home actually meant, and the eight decisions (`H-01`…`H-08`) that produced the usage heatmap replacing the tiles. Historical record — signed off 2026-07-08, steps 1–7 implemented and shipped. |
| [Knowledge view redesign](knowledge-view-redesign.md) | The three defects diagnosed in the Knowledge view, the radius, surface and component specifications that correct them, the eight sign-off decisions `K-01`…`K-08`, and the execution list that shipped them. Historical record — signed off 2026-07-10, all eight decisions accepted as option A, implemented and verified. |

## Interactive pages

Each Markdown document above is the written companion to a rendered HTML page. **These
pages must be opened in a browser to be useful** — they carry the pixels, and Markdown
cannot reproduce them. The companions carry the reasoning, so an agent working without a
browser should read the `.md` files instead.

| Page | What it shows |
|---|---|
| `ui-cohesion-redesign.html` | A full interactive mockup of the BioRouter shell — sidebar, chat transcript, composer, terminal dock, preview panel — with toggles for theme, Current ⇄ Redesigned, sidebar collapse, split, mid-drag, terminal and highlight, plus side-by-side markdown specimens, live component frames and colour swatches. |
| `home-screen-redesign.html` | Live before/after mockups of the Home page, the interactive heatmap with hover and keyboard tooltips, the intensity-formula histograms, the width-comparison bars, and theme-switchable colour swatches. |
| `knowledge-view-redesign.html` | Before/after mockups built live from the app's own colour tokens in both light and dark themes, and the radius ladder shown as real swatches. |

## Related documentation

- [BioRouter design system](../../../design.md) — the Parchment palette, the `D-NN`
  decision register (Parts 6 and 6b) and the `DR-NN` drift register (Part 7) that every
  document in this folder cites; read it before changing any value specified here.
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
