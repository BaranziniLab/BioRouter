# Composer thinking indicator

This folder holds the design work for the affordance that tells the user Biorouter is working
on their turn — today a small breathing dot sitting in its own row above the chat composer.
It covers the measured diagnosis of what the current indicator gets wrong, six candidate
replacements rendered live at 1:1, and the recommendation between them.

Come here when you are changing anything that narrates turn state near the composer: the
working-status row, `LoadingBioRouter`, `TurnActivityIndicator`, or the ambient-loop period the
motion scale reserves for them. The scope is deliberately narrow — the composer box itself, its
radius, padding, fill and hairline, is unchanged by every proposal here. For the design system
these proposals are measured against see [`design.md`](../../../design.md) at the repo root; for
the token values they use, [`../theming/`](../theming/README.md).

As with the rest of `docs/design/`, the work comes in two halves. The Markdown file carries the
argument, the measurements and the decision; the HTML page carries the live specimens, because
the whole question is what a loop looks like over time and Markdown cannot show that.

## Contents

- **[Thinking indicator redesign](thinking-indicator-redesign.md)** — the diagnosis (seven
  measured findings), the six directions with their costs, and the recommendation. **Status:
  Proposed**; no implementation has started.
- **[Thinking indicator studio](thinking-indicator-studio.html)** — the rendered companion.
  Every specimen is live and animated inside a real 760 px chat column, painted from the app's
  own tokens under their real names, in the app's Arial-first stack. Controls for light/dark,
  play/pause/slow-motion, and an alignment grid that shows the two indicators' differing
  anchors.

## Related documentation

- [Biorouter Design System](../../../design.md) — the numbered decisions this work is measured
  against, notably D-14 (decorative motion), D-15 (focus is a surface shift), §4.20 (the
  canonical spinner) and §4.18 (the streaming caret).
- [Theming](../theming/README.md) — the per-family accent values every specimen has to survive.
- [Design](../README.md) — the parent folder index.
