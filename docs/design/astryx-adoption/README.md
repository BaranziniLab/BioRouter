# Astryx adoption

> **What this is.** The folder index for the proposed interface revision that rebuilds Biorouter's UI on the construction of Meta's Astryx design system while keeping Biorouter's palette, theming architecture and calm register.
> **Status:** Current — the design is Proposed and awaiting review; no code has moved.
> **Audience:** maintainers reviewing the direction; developers and agents who will execute it.

Biorouter's design language is documented in [`design.md`](../../../design.md) but was never built as parts, so every view re-derived it: five row-title treatments, four error dialects, five spinner constructions, six modal shells, 140 hand-written `text-[11px]` classes. Astryx is the corrective — not for its colours, but for its discipline: one control height, one easing, one state model, one radius ladder, all expressed as theme tokens.

## Contents

Three documents make the proposal proper, plus one feature specification written against it. They answer different questions, and which one you want depends on what you are doing.

| Document | Read it when | Format |
|---|---|---|
| **[Design of record](astryx-ui-adoption-design.md)** | you want the *argument* — why each change, what it replaces, what evidence backs it | Markdown |
| **[Implementation specification](astryx-implementation-spec.html)** | you are **writing the code** — every token value, every measurement, the file each phase touches, the gates each must pass | HTML, browser |
| **[Design showcase](astryx-design-showcase.html)** | you want to *see* it, or to review quickly | HTML, browser |
| **[Tab tear-off and merge](tab-tear-off-and-merge.md)** | you are implementing the window-management gesture — dragging a chat tab out into its own window and back into another window's strip, plus the session list's "Open in new tab" | Markdown |

- **[Astryx UI adoption — comprehensive interface revision](astryx-ui-adoption-design.md)** — the design of record. Fixes the contract that constrains the work (palette, theming, calm, squared corners, chat density), then specifies foundations, elements, compositions and motion; lists what already agrees and what gets deleted; and ends with a ten-phase execution plan and ten decisions (`A-01`…`A-10`) for sign-off.
- **[Astryx UI adoption — implementation specification](astryx-implementation-spec.html)** — the buildable half, written to be executed top to bottom without needing the argument behind it. **Open in a browser.** Every token with its value; every element with its measurements; the composition specs including the sidebar tally, the terminal grounds per family, the tool-state ladder and the toast layouts; a **per-phase file index** naming what each of the ten phases touches; the verification gates including the known pre-existing test failure; and the ten decisions. Also hosted at <https://claude.ai/code/artifact/9a6a1969-ae6c-4720-b757-4479395335c4>.
- **[Tab tear-off and merge](tab-tear-off-and-merge.md)** — the window-management gesture, specified against the design of record's motion and token vocabulary. Establishes the ground truth of today's drag implementation, then decides the questions that actually govern the feature: why a tab with a turn in flight cannot cross windows (the daemon is shared but a turn's SSE stream has exactly one subscriber, and dropping it cancels the turn), how a drop is resolved when HTML5 drag events cannot cross window boundaries, what the visual for "this will become a new window" is, and which phases can land before the contested files clear. Closes with the session list's "Open in new tab" menu item — which turns out to *name* a gesture the row click already performs.
- **[Astryx design showcase](astryx-design-showcase.html)** — the rendered companion, and the faster way to review the proposal. **Must be opened in a browser**: every control is live and built from the proposed tokens, so the page is a specimen of the system it argues for. Three switches drive it — *Family* (Parchment / Alma Mater / Roche Limit, on the real shipped values) proves the construction survives every palette; *System* flips the whole page between today's geometry and the proposal, so the diff is the page itself; light and dark follow the viewer. Also hosted at <https://claude.ai/code/artifact/c24a639a-9e18-40b5-9cbb-62381acf4bd3>.

## Status

**Proposed. No implementation has started, and none should until the ten decisions in the spec's §10 are settled.** The only code that has moved in this worktree is unrelated chrome work that preceded the proposal (the Home heatmap, the accent strips, the new-window icon) — each committed separately and noted in its own message.

## Related documentation

- [Design](../README.md) — the parent folder index.
- [Biorouter Design System](../../../design.md) — the current source of truth this proposal amends.
- [Theme system architecture](../theming/theme-system-architecture.md) — the pipeline the proposal treats as fixed infrastructure.
