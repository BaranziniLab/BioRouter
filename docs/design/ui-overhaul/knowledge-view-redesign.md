# Knowledge view redesign

> **What this is.** The written argument behind the Knowledge view's visual
> redesign: the three defects diagnosed, the radius/surface/component
> specifications that correct them, the eight sign-off decisions `K-01`…`K-08`,
> and the execution list that shipped them.
> **Status:** Historical record — signed off 2026-07-10, all eight decisions
> accepted (option A in every case), implemented and verified.
> **Audience:** developers working on the BioRouter desktop Knowledge view, and
> agents that need the reasoning without opening a browser.

The Knowledge section of the desktop app worked correctly but did not look like
the rest of the product. Its corners rounded by guess rather than by role, its
two columns spoke two different surface languages, and depth was faked with
tinted glass and a stray shadow. This redesign maps every element onto the value
that [`design.md`](../../../design.md) — the BioRouter design system — already
specifies. It is alignment, not invention: no new tokens were introduced.

> **The rendered mockups and visual frames live in
> [`knowledge-view-redesign.html`](knowledge-view-redesign.html) and must be
> opened in a browser to be seen.** That page builds its before/after mockups
> live from the app's own colour tokens in both light and dark themes, and shows
> the radius ladder as real swatches. This companion carries the reasoning and
> the exact values, not the pixels.

## Identifier key

`K-01`…`K-08` are the eight sign-off decisions in this document, each with a
recommended option A and an alternative option B. All eight were resolved to A.
`D-NN` identifiers referenced from the element table (for example `D-04`) belong
to the app-wide decision register in [`design.md`](../../../design.md).

## Headline numbers

| Measure | Value |
|---|---|
| Radii in play before | 7 |
| Radii by role after | 4 |
| Ad-hoc tints and shadows left | 0 |
| Content column, matching the app | 1120px |
| Decisions shipped | 8/8 |

## The read: three things that make it feel off

Everything flagged in review — awkward positioning, off styling, inconsistent
rounding, the way compartments are divided — traces back to these three, in
order of how loudly they register.

**Corners don't round by role.** The "Soul · KB" selector and the MODEL row are
**12px** (a card radius) where a select should be **8px**; the graph panel is
**16px** (a modal radius) where a panel should be **12px**; every chip — the
"KB" tag, the `.pdf`/`.pptx` file types, the status pills — is **8px** where a
chip should be **4px**. Big things and small things round almost the same
amount, so the whole surface reads slightly mushy without the viewer being able
to name why.

**The two columns don't match.** The right (graph) column is a bordered, tinted,
rounded panel; the left column — the KB selector and everything you ingest with
— is bare page content with no surface at all. One half reads as a card, the
other as loose scraps. On top of that, nearly every sub-panel invents its own
translucent tint (`/74`, `/82`, `color-mix()`), and the graph legend carries a
drop-shadow, which the system explicitly forbids. Depth looks like frosted glass
instead of paper.

**Edges sit slightly out of line.** The left column is indented an extra
**16px** inside its cell, so the KB selector doesn't line up with the
"Knowledge" title above it or the graph's left edge. And the four graph-header
buttons are a mismatched set — one is a circle, three are rectangles, and all
four reinvent a bordered style the button system doesn't actually ship.

## Round by role: one radius scale, four values

`design.md` already defines this scale. The fix is not new tokens — it is using
the one that matches each element's job, and reserving the big radii for the
things that actually float.

| Radius | Role |
|---|---|
| 4px | chips · tags · pills |
| 8px | buttons · inputs · selects · rows |
| 12px | cards · panels |
| 16px | modals · popovers |

> **Rendered in the HTML.** The four radii appear as a ladder of filled swatches
> so the steps between them can be compared by eye.

| Element | Now | Role | Correct |
|---|---|---|---|
| Graph panel container | 16px | panel | → 12px |
| "Soul · KB" selector trigger | 12px | select | → 8px |
| MODEL row trigger | 12px | select | → 8px |
| Palette search field | 12px | input | → 8px |
| "KB" chip · file-type chips · status pills | 8px | chip | → 4px |
| Paste-flow Cancel / Stage buttons | 4px | button | → 8px |
| File-chooser modal rows | 16px | row | → 8px |
| Dropzone · PasteText panels · menu items | 12/8px | panel/item | already right |

> **Why.** Correcting a radius is one class each — but doing it by *role*, not
> by taste, is what makes the section stop feeling arbitrary. The last row
> matters too: a few elements are already correct, and the redesign leaves them
> alone rather than "fixing" them into inconsistency.

## Surfaces, not frosted glass

Depth is a surface step and a hairline — never alpha or a shadow. The section
currently builds depth with a dozen one-off translucent tints and a shadow on a
panel that never moves. The design principle is the opposite: pick a discrete
background step, add a 1px hairline, stop.

| Where | Now | Becomes |
|---|---|---|
| Graph canvas ground | `color-mix(… 68%)` | `background-muted` |
| Dropzone fill | `color-mix(… 60%)` | `background-muted` |
| Selector / model / paste hovers | `bg-…/82`, `/74`, `/52` | `background-medium` |
| Floating graph legend | `shadow-popover` | hairline + surface, no shadow |
| Drag-active dropzone edge | `block-teal` (deprecated) | `border-strong` |

The only shadows that survive are on things that genuinely float — the
node-preview popover and modals. Everything persistent becomes paper.

## The layout: two panes of one console

Both columns become the same flat panel — `background-default`, a 1px hairline,
12px corners, no shadow — sharing the single `px-8` page gutter and separated
only by the grid gap. Enumerable things (KB list, staged files, change-log)
become hairline rows, not cards.

> **Rendered in the HTML.** Two full-width mockups of the Knowledge view sit
> side by side in sequence: a *Before* frame labelled "one card, one bare column
> · mixed radii · tinted glass" and an *After* frame labelled "matching 12px
> panels · round-by-role · flat surfaces · aligned gutter", each showing the
> KB selector, dropzone with file-type chips, paste CTA, model row, Digest
> button, and the graph panel with its toolbar, force-graph and credibility
> legend.

Both mockups are rendered live from the app's own colour tokens in both themes —
toggling the theme shows the dark surface. Structure and spacing are the
proposal; the force-graph itself is illustrative.

## Element by element: every control mapped to its primitive

The redesign is mostly deletion — removing overrides and hand-rolled spans so
each element falls back onto the shared `Button`, `Input`, `Select`, `Badge`,
and list-row primitives. One control, one look.

| Element | Corrected to |
|---|---|
| KB selector trigger | Input-shaped **Select**: 8px, 36px, real `border-input`, trailing chevron that rotates on open. Focus = surface shift, no ring. |
| "KB" / "Focused" / "Hidden" tags | The one **Badge**: 4px, 20px, 11px. Neutral tone; "Focused" gets the accent tone. |
| KB list (palette) | Hairline **rows** (40px), not gapped cards. Selection is a background shift, not a heavier border. |
| Dropzone | Flat 12px panel on `background-muted`. File types become 4px neutral Badges. Drag-active firms the hairline (drop the deprecated teal). |
| Paste Cancel / Stage | **Button** ghost + **Button** primary (coral) — replacing the raw near-black button. |
| "Paste text" CTA | Plain `outline` Button — drop the height/border/fill overrides. |
| Model row + menu | Input-shaped Select (8px); "Set default" becomes a pill; selected model shows a leading check, not a fill. |
| Digest button | Coral primary stays. Full opacity with a helper line when empty, so the one primary action never sits half-lit (see `D-04`). |
| Graph header buttons | One variant across all four (`ghost`), refresh stays a round icon. No bordered-secondary override. |
| Graph legend | Flat inline panel — hairline + surface, shadow removed. |
| Change-log Preview / Restore | Canonical `xs` size instead of a height override; kind filters become quiet toggles. |
| Empty / loading / error | The shared empty-state block, so Knowledge matches the other list views. |

## The eight decisions

Signed off 2026-07-10 — every recommendation (all option A) accepted and
shipped. The HTML keeps the original radio pickers live; they now record what
was chosen rather than what might be.

### K-01 — How should the two columns be compartmentalised?

This is the "compartments feel off" complaint at its root.

- **A (recommended, accepted) — Matching 12px panels.** Both columns become
  identical flat panels — hairline, 12px, no shadow — separated by the grid gap.
  Symmetric, clearly "two panes of one console."
- B — Both bare on the page. Both columns go fully flat on the page ground with
  just a hairline between them. Maximally paper-like, but weaker grouping of the
  ingest controls.

### K-02 — Inside the left column, one panel or stacked cards?

The selector, ingest, model+digest, and staged list can be one surface or
several.

- **A (recommended, accepted) — One panel, hairline dividers.** A single panel
  with internal hairlines between blocks. Fewest corners; reads as one tool.
- B — Stacked sub-cards. Separate cards with gaps between them. Clearer
  functional grouping, but more boxes and a slightly busier column.

### K-03 — Dropzone edge, solid hairline or dashed?

A dashed edge is the familiar "drop here" convention, but it would be the only
dashed edge in the app.

- **A (recommended, accepted) — Solid hairline.** Matches every other panel
  edge; drag-active firms to a stronger border. Quiet and consistent.
- B — Dashed drop-target. Signals the affordance more explicitly, at the cost of
  a louder, less paper-like edge that is unique in the app.

### K-04 — The Digest button when nothing is staged

Today it sits at 50% opacity by default, which trains the eye to ignore the one
primary action.

- **A (recommended, accepted) — Full opacity + helper line.** Coral stays
  full-strength with a disabled cursor and a quiet "Stage a file to digest"
  line — the CTA never reads washed-out.
- B — Standard 50% disabled. Keeps the app-wide disabled look, accepting the
  half-lit primary whenever the panel is empty.

### K-05 — The four graph-header buttons, which variant?

They currently reinvent a bordered style; pick one real variant for the set.

- **A (recommended, accepted) — Ghost.** Transparent, no border, hover fills —
  the quietest over the canvas. Refresh stays a round icon.
- B — Outline. A visible 1px edge against the graph for stronger affordance, at
  the cost of more weight in the header.

### K-06 — The graph legend, floating or docked?

Either way the forbidden shadow goes; the question is placement.

- **A (recommended, accepted) — Flat inline, in-canvas.** Stays docked in a
  corner of the graph as a flat hairline panel — no shadow — right next to what
  it annotates.
- B — Docked in the toolbar. Moves the legend into a header/footer strip —
  nothing floats over the canvas, but it is separated from the graph.

### K-07 — Graph canvas background

Today it is a bespoke `color-mix`; both options put it on a real token.

- **A (recommended, accepted) — Flatten to one token.** The canvas becomes a
  single `background-muted` surface — on-scale and calm.
- B — Subtle distinct tint. Keep the canvas one named step off the panel fill,
  so the graph area separates visually from its toolbar — one more surface value
  to carry.

### K-08 — Content width for the Knowledge view

It is 1440px today while the rest of the app is 1080, so switching tabs reflows
the column.

- **A (recommended, accepted) — Match app at 1080px.** Cap at the canonical
  measure — no reflow when moving between Knowledge and every other view.
- B — Wider, applied app-wide. Keep a wider measure (say 1280px) because the
  graph likes room — but apply it everywhere, not just here, so nothing reflows.

> **Note.** The decision text names 1080px as the canonical measure, while the
> page's headline figures and execution step 1 both record the shipped column as
> `size="text"` (1120px). Both values appear in the source page as written.

## Execution: what changed, in order

All front-end, all inside `ui/desktop/src/components/knowledge/` plus one new
shared primitive. No backend, no data changes. 14 files touched. Every step below
is marked complete in the source page.

1. **Shell and grid** — one `px-8` gutter, matching 12px panels for both columns,
   single min-height, width `size="text"` (1120px). `KnowledgeView.tsx`
2. **Round by role** — the radii from the radius section corrected across
   selector, ingest, model, graph, chooser.
3. **Surfaces** — every `/NN` tint and `color-mix` replaced with a discrete
   token; legend shadow dropped; `block-teal` retired. Sweep confirms 0 left.
4. **Selects and inputs** — KB trigger, model row, palette search now match the
   `Input` shape (36px, 8px, `border-input`, chevrons rotate on open).
5. **Badges and rows** — new `ui/badge.tsx`; every chip/pill onto it; KB list and
   staged list are hairline rows.
6. **Buttons** — graph header all `ghost`; paste "Stage" is the coral primary;
   Digest is full-opacity + helper when empty; change-log to `xs`.
7. **States** — empty/loading/error kept calm and consistent.
8. **Verify** — `tsc`, eslint, 64/64 contrast, 11 knowledge unit tests, a
   both-theme browser sweep, and a 3-agent adversarial review (30 checks, 0
   high/med issues).

> **Shipped.** Every value came straight from `design.md` — this was alignment,
> not invention. The redesign was verified in a real browser in both light and
> dark themes, and an adversarial review confirmed each of `K-01`…`K-08` landed
> with no regressions.

## Related documentation

- [Knowledge view redesign (rendered)](knowledge-view-redesign.html) — the
  source page, with the live before/after mockups and radius swatches this
  companion describes.
- [UI overhaul — execution status](execution-status.md) — the branch-level status
  record: the 20-step list, gates, commits, and the register of open items.
- [UI cohesion redesign](ui-cohesion-redesign.html) — the app-wide visual spec
  this Knowledge pass aligns to, with a Current ⇄ Redesigned toggle.
- [Home screen redesign](home-screen-redesign.html) — the sibling view-level
  redesign from the same overhaul.
- [BioRouter design system](../../../design.md) — the `D-NN` decision register
  and the radius, surface, and component tokens every value above comes from.
- [Documentation index](../../README.md) — the top-level map of `docs/`.
