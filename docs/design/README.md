# Design

This folder holds BioRouter's design specifications and their rendered companions: the
brand marks, the theme families, the desktop UI overhaul that produced the current look,
and the explorer pages that render a system as diagrams rather than prose. Nearly every
document here comes in two halves — a Markdown file carrying the reasoning, the exact
values and the sign-off decisions, and an HTML page carrying the mockups, swatches and
interactive dials that Markdown cannot reproduce. The Markdown half is written so an
agent, or anyone without a browser, still gets the whole argument.

Come here when you need to know *what a surface should look like and why it was decided
that way* — the token behind a colour, the geometry behind the app icon, the decision
record behind a redesign. Go elsewhere for the rules themselves or for the code: the
single source of truth for the design language is the root
[Biorouter Design System](../../design.md), which everything here cites by its `D-NN`
decision numbers; the mechanics of driving and debugging the real desktop app live in
[`docs/desktop-ui/`](../desktop-ui/agent-browser-debugging.md); and superseded design
packets that were overtaken by later work are filed under
[`docs/history/`](../history/chat-groups/design-judgement-and-plan.md), not here.

## Documents in this folder

| Document | What it covers |
|---|---|
| [Biorouter agentic system explorer](agentic-system-explorer.md) | The written companion to the agentic-system explorer: a code-aligned account of how a request becomes model context, inspected tool work, durable state, recovery and a verified answer. Current, and follows the Rust behaviour of the agent runtime. |

## Subfolders

- **[`branding/`](branding/)** — the BioRouter identity. Holds the
  [logo and wordmark specification](branding/logo-and-wordmark-spec.md), which fixes the
  geometry, colour tokens and lockups of the two-colour `BioRouter` wordmark and the `BR`
  monogram, plus the two interactive studios and the exported icon assets.
- **[`chat-groups/`](chat-groups/)** — design spikes for the chat-groups work. Holds
  [the nested `KnowledgeProvider` blocker](chat-groups/knowledge-provider-nesting-blocker.md),
  a spike report proving that two nested providers clobber each other's active knowledge
  base; its prerequisite fix is still **not** made, so provider nesting remains blocked.
- **[`theming/`](theming/)** — the theme families. Holds the
  [Alma Mater theme tokens](theming/alma-mater-theme-tokens.md), the authoritative
  token-by-token light/dark mapping and WCAG contrast ratios for BioRouter's UCSF-brand
  theme, alongside the theme studios and the theme-system explorer.
- **[`ui-overhaul/`](ui-overhaul/)** — the 2026-07 desktop redesign, its specs and its
  status record:
  - [Execution status](ui-overhaul/execution-status.md) — the stated source of truth for
    the UI cohesion and chat-groups branch: the 20-step list, commits, gates, the brand
    rollout, and the register of what is still broken or open. All 20 steps are done;
    open items remain.
  - [UI cohesion redesign](ui-overhaul/ui-cohesion-redesign.md) — the "fewer boxes, one
    ink" specification for the markdown layer, preview panel, terminal, tabbed chat groups
    and floating surfaces. A design specification: it was a static sketch on the real
    tokens when written, with execution tracked in the status record above.
  - [Home screen redesign](ui-overhaul/home-screen-redesign.md) — why the Home column was
    realigned to the chat column, what its token and session numbers actually meant, and
    the eight decisions behind the usage heatmap. Historical record, signed off 2026-07-08
    and shipped.
  - [Knowledge view redesign](ui-overhaul/knowledge-view-redesign.md) — the three defects
    diagnosed in the Knowledge view and the radius, surface and component specifications
    that corrected them. Historical record, signed off 2026-07-10, implemented and
    verified.

## Rendered pages and assets

The HTML files are self-contained pages that **must be opened in a browser to be useful** —
they render live mockups, diagrams and interactive controls, and show nothing meaningful as
source text. Each sits beside the Markdown companion that explains it.

- `agentic-system-explorer.html` — seventeen rendered SVG architecture diagrams of the
  agent runtime: the turn lifecycle, entry paths, request assembly, inspection pipeline,
  vault substitution, dispatch, hook lanes, recovery paths and transport lanes.
- `design-system-gallery.html` — the design system rendered as a gallery of every token,
  element and state; the companion artifact to the root [design system](../../design.md).
- `branding/logo-wordmark-studio.html` and `branding/logo-icon-studio.html` — interactive
  studios for the wordmark and for centring the `BR` icon, with dials for position, mark
  size and underline gap, and text export/import.
- `theming/alma-mater-theme-studio.html`, `theming/alma-mater-light-theme-studio.html`,
  `theming/roche-limit-theme-studio.html` and `theming/theme-system-explorer.html` — the
  per-family theme studios and the explorer covering the four colour environments.
- `ui-overhaul/ui-cohesion-redesign.html`, `ui-overhaul/home-screen-redesign.html` and
  `ui-overhaul/knowledge-view-redesign.html` — the before/after mockups, component frames,
  heatmaps and theme-switchable swatches for each redesign.
- `branding/assets/` — the exported brand icons as PNG and SVG (`br-icon-beige`,
  `br-icon-transparent`, and a review render).

## Related documentation

- [Biorouter Design System](../../design.md) — the root source of truth for the design
  language, and the register the `D-NN` decisions cited throughout this folder belong to.
- [Architecture](../architecture/README.md) — the orientation-level map of how BioRouter is
  put together; read it before the agentic system explorer if the crate and process
  boundaries are unfamiliar.
- [The agent loop](../agent-loop/README.md) — the reasoning loop's own documentation, where
  the guardrail designs behind the runtime described by the agentic system explorer live.
- [Chat groups: design judgement and reduced plan](../history/chat-groups/design-judgement-and-plan.md)
  — the historical design packet that preceded the chat-groups work, and the origin of the
  `R7` risk that the nesting blocker spike investigated.
