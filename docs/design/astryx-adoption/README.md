# Astryx adoption

> **What this is.** The folder index for the proposed interface revision that rebuilds Biorouter's UI on the construction of Meta's Astryx design system while keeping Biorouter's palette, theming architecture and calm register.
> **Status:** Current — the design is Proposed and awaiting review; no code has moved.
> **Audience:** maintainers reviewing the direction; developers and agents who will execute it.

Biorouter's design language is documented in [`design.md`](../../../design.md) but was never built as parts, so every view re-derived it: five row-title treatments, four error dialects, five spinner constructions, six modal shells, 140 hand-written `text-[11px]` classes. Astryx is the corrective — not for its colours, but for its discipline: one control height, one easing, one state model, one radius ladder, all expressed as theme tokens.

## Contents

- **[Astryx UI adoption — comprehensive interface revision](astryx-ui-adoption-design.md)** — the design of record. Fixes the contract that constrains the work (palette, theming, calm, squared corners, chat density), then specifies foundations, elements, compositions and motion; lists what already agrees and what gets deleted; and ends with a ten-phase execution plan and ten decisions (`A-01`…`A-10`) for sign-off.
- **[Astryx design showcase](astryx-design-showcase.html)** — the rendered companion, and the faster way to review the proposal. **Must be opened in a browser**: every control is live and built from the proposed tokens, so the page is a specimen of the system it argues for. Three switches drive it — *Family* (Parchment / Alma Mater / Roche Limit, on the real shipped values) proves the construction survives every palette; *System* flips the whole page between today's geometry and the proposal, so the diff is the page itself; light and dark follow the viewer. Also hosted at <https://claude.ai/code/artifact/c24a639a-9e18-40b5-9cbb-62381acf4bd3>.

## Related documentation

- [Design](../README.md) — the parent folder index.
- [Biorouter Design System](../../../design.md) — the current source of truth this proposal amends.
- [Theme system architecture](../theming/theme-system-architecture.md) — the pipeline the proposal treats as fixed infrastructure.
