# The preview panel

> **What this is.** The working documents for expanding BioRouter's artifact side panel from a
> viewer of things the agent made into a working surface the user and the agent share.
> **Status:** Current — the plan is awaiting approval; nothing here has shipped.
> **Audience:** contributors changing the panel, and anyone reviewing the plan before it runs.

The artifact side panel (`ui/desktop/src/components/artifacts/`) is the one place a generated
figure, an app card, a written file or a directory is displayed. This folder holds the survey of
what it does today and the plan to widen it along five axes: more image formats, Office documents
it can already partly render, live websites, an annotation channel back into the chat, and a way
for the agent to see what the user is looking at.

Start with the [current-state survey](current-state.md) — three of the five things the expansion
was scoped to add turned out to be partly or wholly present already, and one of the two that are
genuinely absent is absent because of an explicit security ruling with a test pinning it. Then
read the [expansion plan](expansion-plan.md), which is the document to comment on.

## Documents

| Document | What it covers |
|---|---|
| [The preview panel as it stands today](current-state.md) | The measured survey: every render branch, the six supported image formats and the four lists that must agree for a seventh, the already-shipped Office renderers, the CSP and the closed-set frame-navigation policy, every cap, and the test coverage holes. The evidence base for the plan. |
| [Preview panel expansion plan](expansion-plan.md) | The plan itself: decision records, five workstreams with their sequencing, the security analysis for embedding live web content, and the testing strategy. **Awaiting approval.** |

## Related documentation

- [Where a generated artifact is displayed](../artifact-display-surfaces.md) — the one-surface rule this expansion must not break, and the record of what the removed inline renderer cost.
- [How an Auto Visualiser figure's libraries reach the renderer](../artifact-cdn-assets.md) — the pre-fetch-and-inline mechanism that keeps displayed content off the network.
- [Renderer testing traps](../renderer-testing-traps.md) — why a frontend test passes while the code it covers is broken.
