# Design

This folder holds BioRouter's **visual** design specifications and their rendered
companions: the brand marks, the theme families, and the desktop UI overhaul that produced
the current look. Nearly every document here comes in two halves — a Markdown file carrying
the reasoning, the exact values and the sign-off decisions, and an HTML page carrying the
mockups, swatches and interactive dials that Markdown cannot reproduce. The Markdown half is
written so an agent, or anyone without a browser, still gets the whole argument.

Come here when you need to know *what a surface should look like and why it was decided
that way* — the token behind a colour, the geometry behind the app icon, the decision
record behind a redesign. Go elsewhere for the rules themselves, for the runtime, or for
the code: the single source of truth for the design language is the root
[Biorouter Design System](../../design.md), which everything here cites by its `D-NN`
decision numbers; the explorer of the *agent runtime* — a system rather than a surface —
lives in [`docs/architecture/`](../architecture/agentic-system-explorer.md); the mechanics
of driving and debugging the real desktop app live in
[`docs/desktop-ui/`](../desktop-ui/agent-browser-debugging.md); and design packets that
shipped or were overtaken by later work are filed under
[`docs/history/`](../history/README.md), not here.

There are no Markdown files directly in this folder. All specifications sit in the five
topic subfolders below, alongside two top-level rendered pages.

## Subfolders

- **[`astryx-adoption/`](astryx-adoption/README.md)** — the proposed comprehensive interface
  revision, in three parts: the
  [design of record](astryx-adoption/astryx-ui-adoption-design.md) carrying the argument, the
  [implementation specification](astryx-adoption/astryx-implementation-spec.html) carrying every
  token value, measurement, per-phase file index and verification gate, and the
  [rendered showcase](astryx-adoption/astryx-design-showcase.html) where every element is live
  and switchable across all three theme families. Together they rebuild the app on the
  *construction* of Meta's Astryx design system — one control ladder, one easing, one state
  model, one radius ladder — while keeping Biorouter's palette, theming pipeline, calm register
  and squared corners. **Proposed**: ten decisions (`A-01`…`A-10`) await sign-off, and no
  implementation has started.
- **[`branding/`](branding/README.md)** — the BioRouter identity. Holds the
  [logo and wordmark specification](branding/logo-and-wordmark-spec.md), which fixes the
  geometry, colour tokens and lockups of the two-colour `BioRouter` wordmark and the `BR`
  monogram, plus the two interactive studios and the exported icon assets.
- **[`chat-groups/`](chat-groups/README.md)** — design spikes for the chat-groups work. Holds
  [the nested `KnowledgeProvider` blocker](chat-groups/knowledge-provider-nesting-blocker.md),
  a spike report proving that two nested providers clobber each other's active knowledge
  base; its prerequisite fix is still **not** made, so provider nesting remains blocked.
- **[`theming/`](theming/README.md)** — the theme families. Holds the
  [theme system architecture](theming/theme-system-architecture.md), which fixes the one
  authored file per family and everything generated from it, plus the two token references it
  governs — [Alma Mater](theming/alma-mater-theme-tokens.md) for the UCSF-brand family and
  [Roche Limit](theming/roche-limit-theme.md) for the JupyterLab-inspired one — each giving the
  authoritative token-by-token light/dark mapping and WCAG contrast ratios. The theme studios
  and the theme-system explorer sit beside them.
- **[`ui-overhaul/`](ui-overhaul/README.md)** — the app-wide half of the 2026-07 desktop redesign:
  its specification and its status record.
  - [Execution status](ui-overhaul/execution-status.md) — the stated source of truth for
    the UI cohesion and chat-groups branch: the 20-step list, commits, gates, the brand
    rollout, and the register of what is still broken or open. All 20 steps are done;
    open items remain.
  - [UI cohesion redesign](ui-overhaul/ui-cohesion-redesign.md) — the "fewer boxes, one
    ink" specification for the markdown layer, preview panel, terminal, tabbed chat groups
    and floating surfaces. A design specification: it was a static sketch on the real
    tokens when written, with execution tracked in the status record above.

  The two **view-level** redesigns specified in the same period — the Home page and the
  Knowledge view — both shipped and were signed off, so they were archived to
  [`docs/history/ui-overhaul-2026-07/`](../history/ui-overhaul-2026-07/README.md).

## Rendered pages and assets

The HTML files are self-contained pages that **must be opened in a browser to be useful** —
they render live mockups, diagrams and interactive controls, and show nothing meaningful as
source text. Each sits beside the Markdown companion that explains it.

- `design-system-gallery.html` — the design system rendered as a gallery of every token,
  element and state; the companion artifact to the root [design system](../../design.md).
- `boot-splash-studio.html` — the studio for the boot splash, the centred `BR` mark that
  assembles itself before the app paints. Replays the animation, switches theme families, and
  puts every timing value on a drag handle. Its written companion is the
  [boot splash design](../history/dashboard-mode/2026-07-18-boot-splash-design.md).
- `branding/logo-wordmark-studio.html` and `branding/logo-icon-studio.html` — interactive
  studios for the wordmark and for centring the `BR` icon, with dials for position, mark
  size and underline gap, and text export/import.
- `theming/alma-mater-theme-studio.html`, `theming/alma-mater-light-theme-studio.html`,
  `theming/roche-limit-theme-studio.html` and `theming/theme-system-explorer.html` — the
  per-family theme studios and the explorer covering the four colour environments.
- `ui-overhaul/ui-cohesion-redesign.html` — a full interactive mockup of the BioRouter
  shell with toggles for theme, Current ⇄ Redesigned, sidebar collapse, split, terminal
  and highlight, plus markdown specimens, live component frames and colour swatches.
- `branding/assets/` — the exported brand icons as PNG and SVG (`br-icon-beige`,
  `br-icon-transparent`, and a review render).

## Related documentation

- [Biorouter Design System](../../design.md) — the root source of truth for the design
  language, and the register the `D-NN` decisions cited throughout this folder belong to.
- [Architecture](../architecture/README.md) — the orientation-level map of how BioRouter is
  put together, and the home of the agentic system explorer, which documents the agent
  runtime rather than any visual surface.
- [UI overhaul, July 2026](../history/ui-overhaul-2026-07/README.md) — the two shipped
  view-level redesigns, Home and Knowledge, archived out of `ui-overhaul/` once signed off.
- [The agent loop](../agent-loop/README.md) — the reasoning loop's own documentation, where
  the guardrail designs behind the runtime live.
- [Chat groups: design judgement and reduced plan](../history/chat-groups/design-judgement-and-plan.md)
  — the historical design packet that preceded the chat-groups work, and the origin of the
  `R7` risk that the nesting blocker spike investigated.
