# Knowledge section — redesign specification

> **What this is.** A design-only specification for rebuilding the Knowledge section's interface:
> the shell and where base selection lives, the graph canvas's marks and palette, the legend, the
> filter bar and how it degrades, the sources rail, the seven pop-up surfaces, and a responsive
> ladder built on container queries. It amends
> [`../okf-migration/ui-spec.md`](../okf-migration/ui-spec.md) — the binding spec the current UI was
> built from — in eight named places and leaves the rest of it standing.
> **Status:** **IMPLEMENTED on `design/knowledge-ui-redesign`.** All ten records are built and
> verified; §10 records the six places where measurement changed the design after this document was
> written, and those corrections are authoritative over the text above them. Measurements were taken
> 2026-08-21/22 against `7ac2de4b`; re-measure rather than trusting a number that has since moved.
> **Audience:** Contributors working on the Knowledge subsystem and on the desktop design system.

The companion mockup is [`knowledge-redesign-studio.html`](knowledge-redesign-studio.html) — open it
in a browser, no server needed. It renders **the same page markup at four real pane sizes** (760,
946, 1040, 1626 px), laid out by the very `@container` queries R-08 specifies; its pop-up section is
live, with six of the seven surfaces opening on click; and it draws every value at BioRouter's real
tokens in both modes.

---

## 1. The brief, and the one place its premise was wrong

The redesign was asked for in six parts, then refined twice. Five of the six are accurate
descriptions of the shipped UI. One is not, and reporting that faithfully matters more than agreeing,
because the fix that follows from the stated premise would be the wrong fix.

| # | The brief | Measured verdict |
|---|---|---|
| 1 | The base selector and *Manage bases* sit **above the app's banner**. | **The premise is wrong; the complaint is right.** They are the second child of the header's title row, co-linear with the `<h1>` (`KnowledgeView.tsx:149-170`; identical in the built bundle, so not stale-bundle drift). Nothing renders above y=80px except window chrome. **But** the real defect is worse than misplacement: the base is identified **twice, 40px apart** — once by the header selector, again by the subject band. See R-01. |
| 2 | The facets are crowded and read as buttons. | **Correct, and literally true.** They *are* buttons — `<Button variant="secondary" size="sm">`, the same component and variant as *Manage bases*. A 36px strip with **zero vertical padding** around a 32px input, and a selected facet never changes its own fill. See R-02. |
| 3 | The ingestion rail is cramped and needs excessive scrolling. | **Correct, and measurable.** The resting empty state is **899px in a 745px viewport**. See R-06. |
| 4 | Every node should be a circle, as in BioOKF. | **Correct as a description; collides with a measured accessibility decision.** Seven silhouettes carry node *family* because cross-family colour distance under simulated dichromacy bottoms out at **ΔE00 0.00**. See R-04. |
| 5 | The node colours are too dark; the legend should move to the side. | **Correct, and the cause is identifiable.** The fills are solved to *text* contrast rungs. See R-05, R-03. |
| 6 | The dialogs are crowded; everything must work at every window size. | **Correct on both halves, and the sizing half is worse than reported.** One `KBManagerDialog` row packs 5 focusable controls and 12 visual objects into a 40px box. And the section's one breakpoint is a *viewport* media query while the thing that changes size is the *pane* — so at the app's own minimum window it lays two columns into a space that cannot hold them. See R-07, R-08. |

**Four of the six are complaints about the specification, not about drift from it.** Stage 7 of the
OKF migration is marked DONE/PASSED and the section shipped almost verbatim. Fixing them means
amending that document, which is what §8 does.

---

## 2. Design intent

**One subject, one canvas, two rails.** The section should read as a single instrument pointed at a
single knowledge base. Today it names that base twice, fences the canvas between two 36px control
strips, and floats its inspector over the very thing it describes. The redesign collapses that to one
statement of subject, one uninterrupted canvas, and two rails that each own one job — *what goes in*
on the left, *what it means* on the right.

**The canvas gets lighter, and the ring does the work.** BioOKF's graph is prettier than ours for a
measurable reason: it puts its contrast in a near-black hairline around every node and lets the fill
be a light mid-tone. Ours puts the contrast in the fill and has no budget left for prettiness. R-05
swaps which channel carries the burden; nothing else about the derivation changes.

**The chrome is the app's, not the section's.** Every control off the canvas is drawn from
BioRouter's own ladder — the switch is `ui/switch.tsx`'s 40×24 pill, the filter is on
`--radius-element` like every other control, the card is on `--radius-container`. Two earlier
revisions of this document invented shapes the app does not use; both are reversed, and the reversals
are recorded rather than quietly edited out.

---

## 3. Decision records

`R-nn` are this document's. `DR-n` are the OKF migration's
([`../okf-migration/design.md`](../okf-migration/design.md)); `D-nn` are the desktop design system's
(repo-root `design.md`).

### R-01 — The subject band becomes the switcher; the header carries no controls

**Measured.** The title row is `h1` + a right-aligned cluster of `KBSelectorTrigger` (32px) and a
*Manage bases* button (32px), all co-linear. 40px below, the subject band already draws the same base
as a dot, a name, a format badge, a privacy badge and a page/link count. Four of the app's eight
top-level views put their actions *below* the description instead; there is no shared page-header
component and **no written "nothing clickable above the banner" rule anywhere in the repo**.

**Ruling.** Delete the header cluster. The header becomes `h1` + description only. **The subject
band's base name becomes the switcher** — one popover trigger carrying the dot, name, caret, badges
and counts. *Manage bases* moves into the picker's footer, where `KBSelectorMenu` already renders a
*Manage…* row.

**Cost.** One click of indirection to the manager, bought with: the base identified once instead of
twice, and Knowledge joining the larger of the app's two header conventions.

**Rejected — a left "Bases" rail, BioOKF-style.** BioRouter already has a global `AppSidebar`, so a
second permanent rail puts four columns on screen; and a rail is a poor trade at the base counts
users actually have. ⌘K already opens the picker.

### R-02 — A filter is a boxed control on the element radius — not a button, and not a pill

**Measured.** `<Button variant="secondary" size="sm">` — 28px, `bg-background-medium`,
`rounded-element` (8px), no border — 8px from an `<Input>` that is 32px, `bg-background-default`,
`rounded-element` (**the same 8px**), 1px `--border-emphasized`. The strip is `--dock-height: 36px`
with **no `padding-block`**. A selected facet's fill never changes; `aria-selected` in the popover
has **no CSS keyed off it anywhere**.

**Ruling.**

1. **`--radius-element` (8px)** — the same step as every button, input, select and tab. A filter is
   *not* differentiated by being a different shape from the rest of the app.
2. **A 1.5px edge on a transparent ground**, against the app's 1px hairlines. That is the
   differentiator: a filter reads as a *field that holds a value*, where a secondary button is a
   filled slab with no edge at all.
3. **Engaged is a solid `--background-accent` fill**, not a tint. Across a 1600px bar a tint is a
   guess and a fill is a fact.
4. **A 13px stroked caret** (`stroke-width: 2.2`), authored as a *sized* inline SVG.
5. **New token `--knowledge-filter-height: 48px`** with `padding-block: 0.5rem`. Not
   `--chrome-height`: that 44px value is shared by three bands that move together, and this bar is
   inside the page.

⚠ **This record has been reversed twice, and both reversals are the point.** Revision 1 specified
`--radius-full` pills, to differentiate a filter from a button — which it did, by introducing a
roundness the app uses nowhere else. Revision 2 over-corrected to `--radius-inner` (4px), which made
a 32px control read squarer than anything around it. **The answer is neither: match the ladder and
differentiate by edge and ground.** Do not reintroduce either extreme.

**A caution the studio caught.** Every inline SVG must carry explicit `width`/`height`. An unsized
SVG falls back to the replaced-element default of **300×150**, which painted a giant chevron across
three of the four size frames before it was caught.

### R-03 — The legend moves to a rail it shares with the inspector, and can be dismissed

**Measured.** The legend is a real flex sibling — **not** an overlay — so it *takes* 36px from the
canvas. Its two states show **disjoint** information: collapsed is one `overflow-x-auto` row of 7
family names + 28 *unlabelled* swatches + 4 credibility rings and is **entirely inert** (no buttons);
expanded is an `overflow-y-auto` column capped at 40% with 28 named chips that **drops the
credibility key completely**. The expand control is the last flex child with `ml-auto`, so when
content overflows it sits at scroll-end. The payload it must carry is large: **28 node types in 7
families, 35 predicates in 5 families, 4 credibility treatments**, plus External and Untyped.

**The token for the fix already exists with no layout consumer.** `--knowledge-rail-detail: 340px` is
declared, mirrored, registered and listed as STRUCTURAL — and used only by two `Sheet`s.

**Ruling.** Make the right rail real, with **two occupants, one at a time**:

- **Legend** when nothing is selected: grouped by family exactly as BioOKF groups it, every chip
  interactive (click filters, hover dims non-members), with sections for *Node types*, *Links*,
  *Evidence* and *Other* — each independently collapsible **with its disclosure in its own heading**,
  which is the direct fix for an unreachable control.
- **Inspector** when a node or edge is selected, replacing the legend in place. This retires
  `NodePreview`/`EdgePreview` as floating panels.

**Below 1400px the rail becomes a canvas-anchored card, and the card must be dismissible.** It is the
only thing on the pane that covers the canvas, so a `✕` in its own header hides it and a `Legend`
control appears in the filter bar to bring it back. The card is opaque `--background-default` on
`--radius-container` with `--shadow-popover`, and carries an explicit `z-index` above the canvas.

**Cost.** 340px of canvas at the widest step, bought back at every step below it and paid for
outright by deleting the 36px bottom dock and the floating inspector.

### R-04 — Every node is a circle; the redundant channel is rebuilt, not deleted

**Measured, and this is the hard one.** Seven silhouettes carry the *family*; fill lightness carries
the member. Cross-family colour distance under simulated dichromacy bottoms out at **ΔE00 0.00**
(dark tritanopia, `Phenotype`/`Food`) and **0.30** (light tritanopia). Shape is the only redundant,
monochrome-safe channel the section ships: §5.12's `aria-live` alternative **was never built**.
Three assertions in `graphPalette.test.ts:303-370` guard the shape assignment, and the legend and
facet glyphs read `families[].shape` from the same generated source.

**Ruling.** Draw every node as a circle, and rebuild the redundant channel in three parts:

1. **Structure, not silhouette.** BioOKF's own top-level split is 20 Biomedical Entities vs 8
   Provenance & Context. Draw the 20 as **solid discs** and the 8 as **hollow rings** — both circles,
   the distinction survives monochrome, and it is semantically apt. External keeps BioOKF's dashed
   ring; retracted keeps its existing treatment.
   **The hollow ring is 1.7px, not 2.6.** At 2.6 on a 7px radius the ring ate most of the mark and
   read as a donut. **The legend swatch is thinned to 1.5px inset to match** — the key and the mark
   must agree or the legend teaches the wrong thing.
2. **Labels become the identification channel**, always on and haloed (`paint-order: stroke`, 3px
   ground-coloured). This is why BioOKF's all-circle canvas is readable.
3. **`Distinguish types by shape` — an opt-in preference, default off**, restoring the seven
   silhouettes, beside the app's other accessibility settings.

**Cost, stated plainly.** The default canvas now separates 7 families by hue and lightness alone, and
ΔE00 0.00 still stands — a viewer with tritanopia sees `Phenotype` and `Food` as one colour and one
shape, told apart only by label, legend hover and inspector. That is a real regression against the
current default, taken deliberately, mitigated by 1–3 and reversible by one preference. **It should
not ship without part 3**, and building §5.12's `aria-live` alternative alongside would close the gap
properly.

### R-05 — Lift the fill band; let the ring carry the contrast

**Measured — this is the whole diagnosis.** The 28 fills are solved to hit **WCAG contrast ratios
against the ground**: rungs `[3.5, 4.5, 5.8, 7.3]` and a Provenance ladder to `12.0`
(`themes/graph.mjs`). Those are *text* rungs applied to a *mark*.

| | OKLab L median | OKLab L range | Contrast vs ground |
|---|---|---|---|
| **BioRouter today** | **0.531** | 0.312 – 0.624 | 3.50 – **12.01:1** |
| **BioOKF** | **0.648** | 0.537 – 0.890 | 1.34 – 5.07:1 |
| **Proposed** | **0.690** | 0.580 – 0.780 | 1.80 – 4.17:1 |

Seven of 28 sit at ≥7:1 — darker than *anything* in BioOKF. BioOKF instead puts a near-black 1.1px
hairline around every circle (**17.68:1** against its ground). BioRouter draws a ring too, at
`NODE_RING_ALPHA = 0.5` — half the weight, so it cannot justify a light fill.

**Ruling — two numbers, one generator, no new mechanism.**

1. **`NODE_RING_ALPHA: 0.50 → 0.85`**, still resolved from `theme.ink` so it inverts per mode.
   Measured: `#1f242c` @ 0.85 over `#f4f4f2` → `#3f434a`, **9.02:1** against the ground. That single
   ring satisfies WCAG 1.4.11's boundary requirement for every fill, which is what frees the fill.
2. **Replace the contrast rungs with an OKLab-lightness band** — same hues, chroma, spreads,
   structure and solver; only the target quantity changes:
   ```
   PRIMARY_L    = [0.745, 0.690, 0.635, 0.580]
   PROVENANCE_L = [0.780, 0.752, 0.724, 0.696, 0.668, 0.640, 0.612, 0.584]
   ```

**Verified, not asserted.** The step between adjacent members stays 0.055 L against today's
0.050–0.059, so within-family separation survives by construction. Measured with a real CIEDE2000:

| Family | Current min ΔE00 | Proposed min ΔE00 |
|---|---|---|
| Genomic | 6.51 | **7.41** |
| Molecular & process | 6.77 | **7.02** |
| Anatomy & organism | 8.63 | 7.42 |
| Clinical | 9.25 | 7.93 |
| Exposome | 8.98 | 7.81 |
| Physical | 12.17 | 11.35 |
| Provenance & context | 5.54 | 4.80 |
| **Minimum** | **5.54** | **4.80** — floor is 3.0 ✓ |

**The ground does not move, and that is not negotiable.** `check-contrast.mjs:356-407` asserts a
**six-scope `--background-muted` identity** across three families × two modes, and the single shared
`GRAPH_PALETTE` exists *because* that identity holds; a per-section ground fails that check and forces
the palette to be emitted three times. `ui-spec.md:294-297` forbids it independently.

**What does change on the ground: the dot field.** Adopt BioOKF's `radial-gradient(circle,
ink@0.045 1px, transparent 1.4px)` at 34px. **It must be painted on the canvas in
`onRenderFramePre`, never as a CSS `background-image`** — `getComputedStyle().backgroundColor`
returns `rgba(0,0,0,0)` when the colour lives in a gradient, which breaks `resolveGraphTheme`'s
ground probe.

**Solved light-mode fills** (generator output, pinned here as output not input):

| Family | Members, in ladder order |
|---|---|
| Genomic | `#91a6ff` `#908fec` `#8f79d4` `#8b64bb` |
| Molecular & process | `#59c2a9` `#39b0a5` `#139ea1` `#008a9a` |
| Anatomy & organism | `#8cbd71` `#66af72` `#3ca074` |
| Clinical | `#f582a7` `#e67183` `#d4625f` `#c0543a` |
| Exposome | `#e09c54` `#c49139` `#a7871f` |
| Physical | `#73b5de` `#7e9bd4` |
| Provenance & context | `#a9bdaf` `#9bb5b0` `#91abb0` `#8ca1ae` `#8b95a8` `#8c899e` `#8c7e90` `#897580` |

Dark mode is solved by the same generator against `#232320` and is **not** the light values inverted;
it must be re-solved and re-audited.

### R-06 — The sources rail stops scrolling at rest

**Measured** — every block rebuilt in real Chrome against the repo's own `main.css` at 300px:

| Block | Height | Verdict |
|---|---|---|
| Dropzone | **330px** | 260px is decoration and prose: a 48px medallion, two paragraphs of 251 characters wrapping to 3 and 4 lines, a 10-chip extension row wrapping to 3 rows. None collapsible. **→ ~132px**, extensions behind an ⓘ. |
| "Nothing staged" empty state | **236px** | *Larger than a five-item staged list (232px).* The rail is tallest when it has least to say. **→ deleted.** |
| Sticky footer | 109–149px | Occludes the scroll region and paints over content by DOM order — the defect that already bit the paste box and required a runtime-measured `scroll-margin-bottom`. **→ a grid-row sibling**; the workaround is deleted, not maintained. |
| Tier control | 40px | Draws privacy twice. **→ folds into the subject bar's `PrivacyBadge`.** |
| **Resting total** | **899px in a 745px viewport** | **→ ≈465px. The rail stops scrolling.** |

Every one of these is a **removal**. Nothing was made smaller by cramming, and the rail did not need
to get wider.

### R-07 — De-crowd `KBManagerDialog` by removal, not by a second line

**Measured.** One row: **5 focusable controls and up to 12 visual objects in a 40 × 582px box**, with
a fixed **160px** right-hand cluster reserved before the name column gets anything. **Two facts are
drawn twice**: hidden state as both a switch and a *Not in this chat* badge; primary as both a
`PRIMARY` badge and a `tint-selected` row fill. The same row renders a **third** time at 256px in
`KBSelectorTrigger`'s popover. At the app's minimum 600px window height the 85vh cap leaves ~245px of
list — six rows, three with the rename card open — and the search field and Create/Import buttons
**scroll away with the list**.

**Ruling.**

1. **Delete the `Not in this chat` badge.** The switch already says it.
2. **Delete the `tint-selected` row fill; keep the `PRIMARY` badge.** Not arbitrary: D-15 makes focus
   a *surface shift*, and an opaque row tint is exactly what the legend's `asChild` comment exists to
   avoid.
3. **Collapse download + pencil + ⋯ into one ⋯** on hover/focus. Reclaims ~80px.
4. **Put the reclaimed width to work:** `N pages · updated X`, right-aligned and muted.
5. **Keep the row at 40px, single-line.** A 56px two-line row would cost a third of the visible rows
   at a 600px window. De-crowd by removal.
6. **Pin search and Create/Import outside `scrollBody`.**
7. **Widen to ~640px.**

**Also fix while here:** both drawers lose the `pr-14` gutter `SheetHeader` reserves for its own ✕,
which in `LintDrawer` costs **8px of real overlap** with *Check again*.

### R-08 — The pane responds, not the window

**Measured, and this is the sharpest finding in the document.** `main.ts:1166` sets
`minWidth: 1000, minHeight: 600` with `useContentSize: true`, and the comment above it derives that
1000 as **240px of sidebar plus the 760px reading column**. The sidebar is `SIDEBAR_WIDTH = 15rem`
(240px) expanded, icon + spacing (54px) collapsed.

| Case | Window (content) | Sidebar | **Knowledge pane** | Viewport `md:` ≥ 930? |
| --- | --- | --- | --- | --- |
| **App minimum, sidebar open** | 1000 × 600 | 240 | **760 × 568** | **fires** |
| App minimum, sidebar collapsed | 1000 × 600 | 54 | 946 × 568 | fires |
| Default window, sidebar open | 1000 × 1000 | 240 | 760 × 968 | fires |
| 1280 window, sidebar open | 1280 × 800 | 240 | 1040 × 768 | fires |
| 1680 window, sidebar collapsed | 1680 × 1050 | 54 | 1626 × 1018 | fires |

**The section's one breakpoint is a *viewport* media query.** `md:` (930px, `main.css:14`) tests the
window. At the app's own minimum the window is 1000px — so it fires and lays out two columns — while
the pane is **760px**. The 300px Sources rail then leaves **444px** for a graph pane whose filter
strip needs a measured **757px**. The section is at its most broken at the smallest size the app will
let a user make, and no tuning of the value 930 fixes it, because the quantity tested is wrong.

**Ruling. Every step is a `@container` query on the pane.** There is precedent: `main.css:2449-2452`
declares `.biorouter-home-content { container-name: …; container-type: size }` and drives Home's hero
off `@container … (max-height: …)`. Knowledge gets the same on both axes.

| Step | Pane width | Layout |
| --- | --- | --- |
| **Narrow** | < 860px | One column, Sources/Graph tabs, one `Filters` control beside a full-width search. Legend and inspector become sheets. **This is the step the app minimum lands in, and it is why the minimum fits.** |
| **Two-column** | ≥ 860px | Sources 264 │ canvas. Facets condense (R-09). |
| **+ legend card** | ≥ 940px | Sources 300 │ canvas, legend as a canvas-anchored card. |
| **Full filter row** | ≥ 1060px | All four facets and the `Showing N of M` readout. |
| **Three-column** | ≥ 1400px | Sources 320 │ canvas │ rail 340. The legend becomes a permanent rail; the card retires. |

| Height step | Pane height | Behaviour |
| --- | --- | --- |
| **Short** | < 620px | The header sheds its description line and tightens to 17px. At a 600px window the pane is 568px, so this is the default at the minimum, not an edge case. |

**Every breakpoint is measured, and two moved during the studio build because the first values did not
survive measurement:**

- **940 for the card, 1060 for the filter row — two constraints, two numbers.** The first draft
  bundled both at 1024. The card is a 246px overlay needing only a canvas to sit in; the full filter
  row needs **757px of centre column** (196 search + 1 divider + 90 + 92 + 78 + 75 facets + 149
  readout, plus gaps and padding). With a 300px rail that puts its floor at a 1057px pane — so at
  1040, a very common window, four facets **overflowed by 19px**.
- **Source order is the mechanism, and it bit once.** Every rule sits at the same specificity, so the
  narrow defaults must be declared *before* the `@container` blocks. Declared after,
  `.kb-f.core { display: inline-flex }` leaked into the 760px step and the pane painted both the core
  chips *and* the single `Filters` control.
- **`min-height: 0` on the body grid's children.** A grid item's default `min-height: auto` let the
  Sources rail push the row 59px past the pane.

### R-09 — The filter row degrades by priority, never by wrapping or scrolling

**Measured.** There is no strategy today. The facets, search and readout sit in one `overflow-x-auto`
row: past its width they scroll out of sight, and `Clear` — the only thing that undoes a filter —
goes with them. `GraphFacetStrip.tsx:57-64` records the same failure already happening once: *"the
strip's content measured 769px inside a 550px box"*.

**Ruling — three states, one priority order, no wrapping and no scrolling.**

| Pane | State | Row |
| --- | --- | --- |
| ≥ 1060px | **Full** | search 196 · Type · Predicate · Source · Status · `Showing N of M` · `Clear` |
| 860–1059px | **Condensed** | search 172 · Type · Predicate · **More (2)** · `Clear` as an icon |
| < 860px | **Compact** | search takes the remaining width · **Filters (3)** · `Clear` as an icon |

**Why nothing wraps.** A two-line filter bar takes 48px from the canvas permanently, on the pane
sizes with the least canvas to give. **Why nothing scrolls.** That is the failure the legend dock
already demonstrates, and the control it hides first is the one that undoes invisible state.

**The priority order is a decision.** *Search* collapses last, because it is the only control that
can reach a node whose type the user does not yet know. *Type* survives longest of the four, because
it is the one facet every base has — `Predicate` and `Source` are empty on a legacy base and `Status`
holds four values. What folds into **More** carries its own count, so **a filter you cannot see is
still reported by the number on the control that swallowed it**, and `Clear` never leaves the row.

### R-10 — One rule for all seven pop-ups: popover, modal, or the rail

**Measured.** Seven surfaces open over this page, on **four different mechanisms**.
`KBManagerDialog`/`KbFormatChooser` → `ModalShell` → `ui/dialog.tsx`; `LintDrawer`/`ChangeLogDrawer`
→ `ui/sheet.tsx`; the facet and switcher pickers → `Popover` + `Command`. And
`NodePreview`/`EdgePreview` are neither — **plain absolutely-positioned `role="dialog"` panels at
`z-[var(--z-dropdown)]` with no portal, no scrim and no focus trap.**

**Ruling — the mechanism follows what the surface *is*.**

- Returns a value to the page → **popover**, anchored to the control that opened it.
- Is a task → **modal with a scrim**.
- Describes the current selection → **the rail**, or its fallback (card ≥940, sheet below). **Never a
  bare dialog.**

| Surface | Change |
| --- | --- |
| Base switcher | Anchors to the subject bar. Gains a `Manage bases` footer, its only route. |
| Type / Predicate / Source | Mechanism unchanged. Rows gain a real **checkbox** so multi-select is legible before the first click. |
| `More` | **New**, only between 860 and 1059px. Lists the folded facets with their counts. |
| Actions ⋯ | Unchanged; destructive stays behind the separator. |
| Node / edge inspector | Stops being a bare dialog. Rail → card → sheet by pane width. |
| Manage bases | R-07 in full, at 640px. |
| Lint / Change log | Restore the `pr-14` header gutter. |

**One caution.** Nested modals opened from inside a drawer tie at `--z-modal` (400), separated only
by portal mount order, and their scrims (300) never dim the surface that spawned them. Nothing here
risks the documented soft-lock — the unlayered `.biorouter-modal-surface` floor covers all of them —
but a new nested surface would be the way to introduce one.

---

## 4. The shell

```
┌─ 32px drag strip ─ window chrome, untouched ───────────────────────────────────┐
├────────────────────────────────────────────────────────────────────────────────┤
│  Knowledge                                                    ← h1 only        │
│  Personal knowledge bases Biorouter builds and maintains for you.              │
├────────────────────────────────────────────────────────────────────────────────┤
│  ● e-cigarette ⌄  BIOOKF  PRIVATE       210 pages · 480 links   [◫] [↻] [⋯]  │  48px subject bar
├───────────────┬──────────────────────────────────────────┬─────────────────────┤
│  SOURCES      │ ⌕ Filter…  [Type 2 ⌄][Predicate ⌄][…]    │  NODE TYPES     ⌄   │  48px filter bar
│               ├──────────────────────────────────────────┤  GENOMIC            │
│  ⬆ Drop or    │                                          │    ■ Gene       41  │
│    choose     │                                          │    ■ Variant    18  │
│               │                CANVAS                    │  CLINICAL           │
│  ▸ staged     │                                          │    ■ Disease    27  │
│               │                                          │                     │
│               │                                  [+ − ⤢] │  LINKS          ⌄   │
│  ─────────    │                                          │  EVIDENCE       ⌄   │
│  MODEL  ⌄     │                                          │                     │
│  [ Digest ]   │                                          │                     │
└───────────────┴──────────────────────────────────────────┴─────────────────────┘
      320px                                                        340px
```

Three bands become two. The canvas starts ~128px from the top of the route surface instead of ~220px,
and gains the 36px the legend dock used to take from its bottom.

**Geometry, as token edits.** Each is registered in four places (`:root`, `@theme inline`,
tailwind-merge, `theme-contract.mjs`); `knowledgeTokens.test.ts` fails first if a registry is missed.

| Token | Now | Proposed |
|---|---|---|
| `--knowledge-rail-sources` | 300px | **stepped**: 264 / 300 / 320 by pane (R-08) |
| `--knowledge-rail-detail` | 340px (no layout consumer) | **340px**, now the legend/inspector rail |
| `--dock-height` | 36px | unchanged; the legend dock that used it is deleted |
| `--knowledge-filter-height` | — | **48px** (new) |
| `--knowledge-subject-height` | — | **48px** (new; replaces the `h-row` 40px band) |
| pane breakpoints | — | **860 / 940 / 1060 / 1400** + a 620px height step (new) |
| `--measure-graph` | `clamp(1440px, 96%, 2200px)` | unchanged |

**The Sources rail is the one width that steps.** 264 at the two-column step, 300 from 940, 320 from
1400 — because the pane it divides varies by more than 2×, and a rail that is 40% of a 760px pane is
a different object from one that is 20% of a 1626px pane.

**Shape discipline, off the canvas.** Everything follows the app's ladder and nothing invents a step:

| Object | Radius | Source of truth |
|---|---|---|
| Switch track and thumb | `rounded-full` | `ui/switch.tsx` — 40×24 track, thumb **16 → 20px** as it travels 16px, on `--text-on-accent` |
| Filter control, buttons, inputs, selects, tabs, rows | `--radius-element` (8px) | `button.tsx` (`shape: 'pill'` maps to `rounded-element` — the name is a misnomer) |
| Badges, chips, counts, swatches, type tags, checkboxes | `--radius-inner` (4px) | `badge.tsx` — `rounded-inner` |
| Cards, panels, dialogs, popovers, the legend card | `--radius-container` (12px) | the ladder |

---

## 5. The canvas

| Property | Now | Proposed |
|---|---|---|
| Silhouettes | 7 | **1 — circle.** Entities solid, Provenance hollow at **1.7px**, External dashed. |
| Ring alpha | 0.50 | **0.85**, resolved from `theme.ink` |
| Ring vs ground | ~5:1 | **9.02:1** measured |
| Fill target | contrast rungs 3.5 / 4.5 / 5.8 / 7.3 (+12.0) | **OKLab L band** 0.580–0.780 |
| Fill L median | 0.531 | **0.690** |
| Ground | `--background-muted`, flat | unchanged + a **34px dot field** painted in `onRenderFramePre` |
| Labels | on hover / by degree | **always on**, haloed |
| Edges | resolved ink, alpha ladder, 2:1 taper | unchanged, plus negatives **dashed + danger**, provenance **faint dashed** |

---

## 6. What this changes, file by file

| File | Change |
|---|---|
| `KnowledgeView.tsx` | Header cluster deleted (R-01); subject band → 48px switcher bar; workspace becomes a **container-query** grid — `container-type: size` on the pane, all `md:` classes removed (R-08). |
| `KBSelector/KBSelectorTrigger.tsx` | Becomes the subject-bar trigger. |
| `KBSelector/KBSelectorMenu.tsx` | Gains the *Manage bases* footer as the only route to the manager. |
| `KBSelector/KBManagerDialog.tsx` | R-07 in full. |
| `graph/GraphFacetStrip.tsx` | `Button` → a boxed control on `--radius-element` with a 1.5px edge and transparent ground; 48px bar; solid accent fill when engaged; sized 13px caret; the three-state overflow ladder and the new `More` popover (R-02, R-09). |
| `graph/GraphLegend.tsx` | Rewritten as the rail's legend occupant, plus the dismissible card form (R-03). |
| `graph/KnowledgeGraphPanel.tsx` | Bottom dock deleted; right rail added; rail ⇄ card ⇄ sheet by step; the `Legend` restore control. |
| `graph/NodePreview.tsx`, `EdgePreview.tsx` | Become rail occupants rather than bare dialogs. |
| `graph/nodeShapes.ts` | Reduced to circle + hollow + dashed; the 7-silhouette path retained behind the R-04 preference. |
| `graph/ForceGraphCanvas.tsx` | `NODE_RING_ALPHA` 0.5 → 0.85; hollow ring 1.7px; dot field in `onRenderFramePre`; always-on haloed labels; negated/provenance edge treatments. |
| `graph/CredibilityRing.tsx` | Ring weights re-checked against the thinned hollow mark. |
| `themes/graph.mjs` | `PRIMARY_RUNGS`/`PROVENANCE_RUNGS` → `PRIMARY_L`/`PROVENANCE_L`; `shape` retained for the preference. |
| `scripts/lib/graph-palette.mjs` | The second copy of the solver — **edit both**; `graphPalette.test.ts` sweeps them for byte-identity. |
| `IngestPanel/Dropzone.tsx` | 330px → ~132px. |
| `IngestPanel/StagedList.tsx` | "Nothing staged" empty state deleted. |
| `IngestPanel/IngestPanel.tsx` | Footer `sticky` → grid row; `--br-ingest-footer-inset` deleted. |
| `KbTierControl.tsx` | Moves into the subject bar's `PrivacyBadge`. |
| `styles/main.css` | New tokens ×4 registries; the `@container` ladder authored here (**narrow defaults before the container blocks — order is the mechanism**); `.br-graph-dock` legend usage removed; `.br-ingest-summoned` deleted; `.br-swatch-ring` moves to `--radius-inner` off-canvas. |
| `lint/LintDrawer.tsx`, `changelog/ChangeLogDrawer.tsx` | Restore the `pr-14` header gutter. |

---

## 7. Verification

**jsdom can confirm none of this.** No canvas 2D context, no layout engine, no viewport, Tailwind
never runs, `:has()` and `:focus-visible` are not evaluated — and **jsdom evaluates no container
queries at all**, so every step in R-08's ladder is browser-only.

**Use the harness that already exists:**

```bash
cd ui/desktop && npx vite --config .knowledge-harness/vite.config.mts --port 5200
```

It mounts the real components against seven fixture files, faking only `window.electron` and `fetch`.
It has no npm script, no Justfile target, no CI wiring and asserts nothing — it is a place to look.
It is also outside `tsconfig`, ESLint and vitest, so it drifts silently against a component API
change. A `container-type: size` element needs a definite height or the query never resolves.

**What is pinned today, and how hard:**

| Surface | Pinned by |
|---|---|
| Legend layout, KB-selector placement | **Nothing. Zero tests.** |
| Facet strip | One brittle class-string assertion |
| Node shape channel | **Hard** — 3–4 assertions in `graphPalette.test.ts`. The two *component* tests that look like they pin it are self-referential and would pass if every node became a circle. |
| Ingest rail | A mechanism test whose subject is a CSS class name plus the sticky-footer measurement |
| Section geometry | `knowledgeTokens.test.ts` (14 tests) across four registries per token |
| Palette correctness | `graphPalette.test.ts` (42), `check-contrast.mjs` (332 assertions) |

Baseline: **209 tests** under `src/components/knowledge/` across 23 files, passing in 3.39s. There is
**no** visual-regression or screenshot-baseline infrastructure anywhere in `ui/desktop`.

**Five defects in this document's own studio were found by measuring rather than looking**, and each
is a shape the implementation can repeat:

1. A `display` rule declared after its `@container` block leaked into the narrow step.
2. A grid child without `min-height: 0` pushed the row 59px past the pane.
3. An inline SVG with no `width`/`height` painted at the replaced-element default of 300×150.
4. A swatch class scoped to a descendant selector painted nothing in the dialog rows.
5. **`.kb-canvas svg` was a descendant selector, and the legend card is a child of the canvas** — so
   the render loop matched every caret icon inside the legend and drew the entire graph into a 13px
   disclosure arrow. It presented exactly as a legend z-index bug, and two rounds of stacking-order
   work were spent on it before the cause was found. **Scope a render loop to the element it owns,
   never to a container-descendant selector.**

None of the five would be caught by a component test.

**Re-run before believing any palette claim here:** the CVD audit in `graphPalette.test.ts` against
the proposed ladder. §3's R-05 table verifies *within-family* separation only; cross-family separation
under dichromacy is expected to stay at ΔE00 ≈ 0 — the accepted cost of R-04, not something the new
ladder repairs.

---

## 8. Amendments to the OKF migration UI spec

Eight named changes to [`../okf-migration/ui-spec.md`](../okf-migration/ui-spec.md). Everything not
listed stands.

| ui-spec § | Amendment | Record |
|---|---|---|
| §3.1, §3.2 | The KB selector and *Manage bases* leave the title row; the subject band becomes the switcher. | R-01 |
| §4.6 | Facets become boxed controls on `--radius-element` with a 1.5px edge, a transparent ground and a solid accent fill when engaged — **not** `Button variant="secondary"`. In a 48px bar. | R-02 |
| §4.5, §4.7, §3.1 | The legend leaves the bottom dock for a right rail; the dock is deleted; the card form is dismissible. | R-03 |
| §5.3.1 | The seven-silhouette shape channel becomes an opt-in preference; the default canvas is all circles with a two-state structural channel at a 1.7px hollow ring. | R-04 |
| §5.2, §5.3 | Fill lightness is solved to an OKLab band, not contrast rungs; ring alpha 0.5 → 0.85. §5.3's pinned tables are re-solved. | R-05 |
| §4.8 | The inspectors become rail occupants and stop being bare `role="dialog"` panels with no portal, scrim or focus trap. | R-03, R-10 |
| §4.6 | **New:** the filter row degrades by a fixed priority order into `More` and then `Filters`; it never wraps and never scrolls. | R-09 |
| §3.4, §3.5 | The responsive ladder is specified in full, in both axes, and **as `@container` queries on the pane rather than media queries on the viewport** — the defect that makes the section worst at the app's own minimum window. | R-08 |
| §6.3 | The DOM palette swatch moves to `--radius-inner` with a solid fill; the hollow variant thins to 1.5px to match the canvas mark. | R-02, R-04 |

§5.12's `aria-live` text alternative remains **unbuilt** and becomes materially more important under
R-04. It is named here as a dependency, not amended.

---

## 10. As built — where measurement changed the design

Six claims in this document did not survive contact with a browser or a test run. They are corrected
here rather than edited away above, because each records something worth knowing.

### 10.1 `NODE_RING_ALPHA` was dead, and R-05's headline number was wrong

R-05 says "ring alpha 0.50 → 0.85". **The constant read `0.5` and the painter hardcoded `0.92` in two
places**, so the number a reader would have trusted was never the number on screen. The ring was
already strong — composited, **10.88:1** against the light ground. The constant is now wired to the
value that actually ships and documented as load-bearing. The fill lightening stands on its own and
is *better* justified than the document claimed.

### 10.2 The light end is capped at 0.78, and a first pass broke the shape channel

R-05's proposed band (primary 0.745–0.580, provenance 0.780–0.584) failed on measurement twice:

- **Within-family separation fell below the ΔE00 3.0 floor** — 2.11 at dark/protan
  (`Population`/`GeographicLocation`), 2.71 at light/tritan (`Dataset`/`Agent`). Eight members at
  chroma 0.030 need roughly 0.36 of OKLab L between them; a parametric sweep over span and chroma
  found no narrow band that works.
- Widening to 0.80–0.50 fixed that and broke something worse. **Above ~0.80 the sRGB gamut clips
  chroma**, so every family's lightest member desaturates toward the same pale tint and *cross*-family
  pairs collapse. Measured over the 21 family pairs: the old palette had **7 below ΔE00 3.0**, the
  0.80 band had **13** — and **no assignment of the seven shapes can cover 13**, which would have
  silently voided the shape channel R-04 keeps as its accessibility escape hatch.

The shipped band caps the light end at **0.78**, leaves 11 pairs below 3.0, and those 11 *are*
covered. A sweep found 36 bands where a valid shape assignment exists; this is the lightest of them.
**Lightening past 0.78 breaks the guard in `graphPalette.test.ts`, and the right response is to come
back to `themes/graph.mjs`, not to relax the guard.**

### 10.3 Four families swapped silhouettes

Genomic `square`→`rounded-square`, Clinical `rounded-square`→`square`, Exposome `pentagon`→`circle`,
Physical `circle`→`pentagon`. Not churn: the set of pairs that collapse under dichromacy changed with
the palette, so the set the shape channel must carry changed with it. This is the minimal-change
permutation that leaves no collapsed pair on a "weak" shape pair.

### 10.4 Measured palette movement

| | Old | Shipped |
|---|---|---|
| Light-mode contrast, median | 5.00:1 | **2.67:1** |
| Light-mode contrast, max | 12.01:1 | **8.53:1** |
| Within-family min ΔE00, all vision types | 5.54 | **4.28** |
| Family pairs below ΔE00 3.0 | 7 of 21 | 11 of 21, all shape-covered |

### 10.5 The canvas-anchored legend card was built, then deleted

R-03 specified a BioOKF-style card floating over the canvas between 940 and 1400px. It was
implemented, then measured at a 946px pane: **it covered 44% of the canvas width and sat on the
nodes** — the overlap complaint this redesign exists to fix, reintroduced by the fix. It is replaced
by a `Legend` control in the filter bar opening the same component in a popover. **Nothing overlaps
the canvas at any size**, and the legend is dismissible by construction rather than by a bespoke
hide/restore pair.

### 10.6 Three container-query defects jsdom cannot see

All three were found in the browser and none would fail a component test:

1. **A container cannot query itself.** `br-knowledge-pane` and `br-knowledge-body` were on the same
   element, so the grid could never restyle itself and every step silently produced one column. The
   container also had to be hoisted *above* the header and subject bands, because the height step
   yields the header's description and the two-column step hides the tabs.
2. **Source order decides, twice.** A `display` default declared *after* the `@container` blocks wins
   at equal specificity — once for `.br-facet-core` leaking into the narrow step, once for
   `.br-facet-legend` surviving into the rail step.
3. **`min-height: 0` on the body grid's children**, or a tall rail pushes the row past the pane.

**The unlayered-vs-Tailwind interaction is the trap to remember.** The authored ladder beats
Tailwind's `.hidden` regardless of specificity, so the narrow step must say *nothing* about the
Sources panel's display — React's tab state owns it there.

---

## 9. Open questions

1. **R-04's trade — accept, or soften?** Options: (a) ship as specified with the opt-in preference;
   (b) keep circles but let *hollow vs solid* carry more, e.g. four structural classes; (c) build
   §5.12's `aria-live` alternative first. This document specifies (a) and recommends (c) alongside.
2. **Does the `More` popover earn its keep?** R-09 gives it a 200px band of pane width and nothing
   else. The alternative jumps straight from the full row to `Filters`.
3. **Should the Sources rail's stepped width be one token or three?** `knowledgeTokens.test.ts`
   asserts each token is declared exactly once in `:root`, so a stepped value needs that assertion
   re-read before it is written.
4. **`--knowledge-rail-detail` at 340px** — drawn by the old spec, never built. Confirm rather than
   re-derive.
5. **Dark mode for the graph is unreferenced by BioOKF**, which has none. The proposed ladder is
   solved for light; dark needs its own targets and audit.
6. **Scope — resolved.** All ten records shipped together on
   `design/knowledge-ui-redesign`. R-08 turned out to be the one with a user-visible bug behind it
   rather than a preference, which is why it was built first.

---

## Related documentation

- [Knowledge section — binding UI specification](../okf-migration/ui-spec.md) — the spec this
  document amends in eight places. Read its "What shipped, and what did not" framing first.
- [OKF migration](../okf-migration/README.md) — the `DR-n` records and progress tracker upstream.
- [Knowledge base](../README.md) — the subsystem index.
- [`design.md`](../../../design.md) — the desktop design system: the `D-nn` records, the shadow
  policy, the radius ladder and the D-15 focus rule this document is bound by.
- [Theme system architecture](../../design/theming/theme-system-architecture.md) — why the graph
  palette is generated once for three families, and what the six-scope `--background-muted` identity
  guards.
