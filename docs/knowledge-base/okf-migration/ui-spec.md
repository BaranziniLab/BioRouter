# Knowledge section — binding UI specification

> **What this is.** The normative design for BioRouter's Knowledge section after the OKF migration:
> every surface, every token, and the complete graph visual specification. It is the binding output
> of [Stage 7](stages.md) and the thing implementation is checked against.
> **Status:** Current (binding for Stage 7; not yet implemented).
> **Audience:** Contributors working on the Knowledge subsystem and on the desktop design system.

The Knowledge section is the one place in BioRouter where the app draws a *data* surface — a canvas
of typed nodes and edges — rather than chrome around prose. That is why it drifted: the graph subtree
grew a second visual language (thirteen hardcoded hexes, a gradient wash, fractional type sizes), and
the surfaces around it re-rolled objects the app already owns (a field edge, a select, a selected row,
a progress indicator, a dot at five diameters, seven hand-written empty states). This document does
two things at once. It brings every non-canvas surface back onto the app's tokens and primitives, and
it gives the canvas a real, generated, contrast-audited palette so the one genuinely new thing in the
section — 28 node types, 35 predicates, typed lint — is legible without inventing a visual language
for it. Where a decision follows from a decision record it names it (DR-9, DR-9b, DR-10, DR-11,
DR-22); where it follows from the design system it names the rule.

Two identifier schemes appear below. `DR-n` are the OKF migration decision records in
[design.md](design.md). `D-nn`, `A-nn`, `P-n` and `DR-nn` in the *drift register* sense are the
desktop design system's records in the repo-root `design.md`; where both could be read, the OKF
records are written `DR-9b`-style with a section reference and the design-system ones are written
`design system D-15`-style.

---

## 1. Design intent

**A quiet instrument that happens to draw a map.** The Knowledge section should feel like the rest of
BioRouter with one extra faculty, not like a graph tool bolted to a chat app. Everything that is not
the canvas — the header, the sources rail, the inspector, the lint popover, the change log — is built
from the app's rows, badges, popovers and empty states, on the app's shared neutral ground, with the
app's one focus treatment and its one hairline weight. There is no second field edge, no second modal
system, no second dot size, no gradient anywhere. A user who has spent eight hours in Chat and opens
Knowledge should recognise every control before reading a single label, and should be able to tell at
a glance which knowledge base they are pointed at, how healthy it is, and what it is made of.

**Colour is evidence, and on the canvas that evidence is the data.** The one deliberate exception to
"nothing else is coloured" is the canvas itself, and it is the same exception the app already grants
its syntax palette and its terminal ANSI palette: a generated, per-mode, contrast-audited data
palette that no component authors by hand. A node's fill says what kind of thing it is; its ring says
how well-sourced it is; a dash says the link is negated or synthesized; a taper says which way the
claim points. Nothing else on the canvas carries hue. Off the canvas that palette appears in exactly
three places — an 8px legend swatch, an 8px inspector dot, an 8px row dot inside an edge list — and
never as a fill behind text, never as a chrome tint, never as an interaction state. The graph is
dense because a knowledge base is dense; the chrome around it stays silent so the density reads as
information rather than noise.

---

## 2. The section shell

### 2.1 Structure

```text
MainPanelLayout                                   bg-background-canvas, pt-[32px]
└ view column                                     flex flex-col min-h-0 flex-1, data-search-scroll-area
  ├ HEADER BAND        flex-shrink-0  border-b border-border-subtle
  │   └ ReadableContent size="graph"  px-8 pt-12 pb-6
  │       row 1:  <h1 class="text-title">Knowledge</h1>  ·······  [KB selector] [Manage]
  │       row 2:  <p class="text-secondary text-text-muted">…</p>
  ├ SUBJECT BAND       flex-shrink-0  border-b border-border-subtle   h-[--row-height]
  │   └ ReadableContent size="graph"  px-8
  │       [8px dot] Name · OKF · Private · 42 pages · 88 links  ····  [lint pill] [⋯]
  └ WORKSPACE          flex-1 min-h-0
      └ ReadableContent size="graph"  px-8 pt-6 pb-8   grid gap-4
          ┌ SOURCES rail ┐ ┌ GRAPH column ─────────────┐ ┌ DETAIL rail ┐
          │ 300px        │ │ minmax(0,1fr)             │ │ 340px       │
          │              │ │  facet strip   36px       │ │             │
          │              │ │  canvas        flex-1     │ │             │
          │              │ │  legend dock   36px       │ │             │
          └──────────────┘ └───────────────────────────┘ └─────────────┘
```

`size="graph"` is `max-w-[clamp(1440px, 96%, 2200px)]` and already exists in `ReadableContent.tsx` —
it was authored for exactly this view and nothing uses it. Header, subject band and workspace all take
it, so the three left edges are one line. This closes the current fork where the header uses
`size="text"` and the gutters run `px-4 sm:px-6 lg:px-8` against the app's flat `px-8`.

### 2.2 Header band

| Element | Spec |
| --- | --- |
| Outer div | `flex-shrink-0 border-b border-border-subtle` — the hairline is **full-bleed**, outside the measure (design system §4.2) |
| Inner | `<ReadableContent size="graph" className="px-8 pt-12 pb-6">` |
| Title row | `flex items-center justify-between gap-4 mb-1 page-transition` |
| Title | `<h1 className="text-title">Knowledge</h1>` |
| Description | `<p className="text-secondary text-text-muted">Personal knowledge bases Biorouter builds and maintains for you.</p>` |
| Right cluster | KB selector trigger (§3.1) then `<Button variant="outline">Manage bases</Button>` — `--control-md`, `gap-2` |

`page-transition` is carried on the title block for consistency with the other eleven views. It has no
matching CSS rule today and animates nothing; that is known and is not this change's problem.

### 2.3 Subject band

A single `--row-height` row naming the base the whole view is about. It is not a chrome band and
must **not** read `--chrome-height`: the three 44px bands (sidebar titlebar, chat header, artifact tab
strip) meet at one continuous top edge and move together or not at all (design system GEOMETRY-2).
This band is inside the page, below the page header, and takes the content row rhythm instead.

Left, in order, `flex items-center gap-2 min-w-0`:

1. 8px `rounded-full` base colour dot (`aria-hidden`; the name is the label).
2. Base name — `text-label text-text-default truncate`. Not `font-semibold`: `text-label` already
   carries 500, and `text-label font-semibold` (14/600) is not a step in the type scale.
3. Format badge — `<Badge uppercase>OKF</Badge>` / `<Badge uppercase>BioOKF</Badge>` /
   `<Badge uppercase>Legacy</Badge>`, `tone="neutral"`. The legacy badge carries
   `title="Created before the format chooser. It reads fine and is not validated."`
4. `<PrivacyBadge tier dense>` when `tier !== 'public'` — the app's padlock, unchanged.
5. Counts — `text-supporting text-text-muted font-mono tabular-nums`: `42 pages · 88 links`.
   `font-mono tabular-nums` because these are digits that change under the user and must not jitter
   (design system TYPE-13).

Right, `flex items-center gap-2 shrink-0`: the lint pill (§3.9) then a single `⋯` overflow
`<Button variant="ghost" shape="round">` holding Refresh graph, Export as `.brkb`, Change log, Open
folder, and (destructive, at the bottom, separated) Delete knowledge base. Four visible ghost buttons
is above the app's "at most three visible actions, everything else and every destructive action
behind the one `⋯`" rule (design system ROWS-3 / row-action grammar); Refresh stays visible as a
32px `shape="round"` icon button because it is the one action the user repeats.

### 2.4 Workspace grid and responsive behaviour

| Breakpoint | Grid | Detail rail |
| --- | --- | --- |
| ≥ `xl` (1280px) | `grid-cols-[var(--knowledge-rail-sources)_minmax(0,1fr)_var(--knowledge-rail-detail)]` | A real column. It **pushes** the canvas; it never covers it. |
| `--breakpoint-md` (930px) – 1279px | `grid-cols-[var(--knowledge-rail-sources)_minmax(0,1fr)]` | An overlay on the canvas, top-right, `--radius-container` + `--shadow-popover` + `--inset-hairline`, `z-[var(--z-dropdown)]`, `w-[var(--knowledge-rail-detail)]`, `max-h-[calc(100%-2rem)]`. |
| < 930px | Single column with a tab pair | A right-side `Sheet`, `w-[var(--knowledge-rail-detail)]`. |

Below 930px the workspace is a two-tab pair — **Sources** and **Graph** — rendered as the compact
control pair the view already has (`h-control-md rounded-element px-3 text-label`, selected takes
`tint-selected tint-interactive`), moved into the subject band's left slot so the band still names
the base. The pair is `role="tablist"`; each panel keeps its `role="tabpanel"` and its `data-testid`.

Use `--breakpoint-md` (930px), never a literal `768px`. Two hardcoded 768px queries survive elsewhere
in the app (drift register DR-57); do not add a third.

Column gap is `gap-4` (16px, "between groups"). Panes are `rounded-container border border-border-subtle
bg-background-default` with `box-shadow: none` — the app's flat card recipe. The canvas alone paints
`bg-background-muted`, which is what makes it read as a work surface inside a card rather than a card
of its own.

### 2.5 Yield order under a narrow window

The app's yield ladder applies in this order and nothing may reorder it:

1. The sidebar collapses to an overlay below 1120px (global, unchanged).
2. The **detail rail** yields its column first and becomes an overlay — it is the least-often-open pane.
3. The **sources rail** yields next, becoming the `Sources` tab.
4. The **canvas never yields.** It is the reason the view exists.
5. The facet strip's controls collapse label-first: at < 1100px the four facet buttons drop their
   labels to icon + count; below the tab breakpoint they collapse into one `Filters` button opening a
   single menu with four sections. They never wrap to a second row.

---

## 3. Every surface, specified

### 3.1 KB selector trigger

Replaces `KBSelectorTrigger.tsx`'s bespoke field. It is a Select-shaped control, so it takes the
Select trigger's chrome exactly.

| Property | Value | Why |
| --- | --- | --- |
| Height | `h-control-md` (32px) | A selector is `md`, not `lg`. Written as the token, not `h-9`. |
| Radius | `rounded-element` | 8px, the control rung. |
| Border | `border border-border-emphasized` | The app's resting edge for an interactive element. **Not** `border-border-input`: that token is darker than `--border-strong` in light and lighter in dark, so the current `hover:border-border-strong` measurably weakens the edge on hover in both modes (light 1.66:1 → 1.52:1; dark 1.91:1 → 1.59:1). |
| Hover | `hover:inset-ring-2 hover:inset-ring-border-emphasized/30` | The Input primitive's whisper. Nothing shifts, no hue change. |
| Focus | nothing at the call site | Global. `--background-focus` + `--border-focus` (design system D-15). |
| Fill | `bg-background-default` | |
| Gap | `gap-2` (8px) | Not `gap-2.5`; 10px is off the 4px grid. |
| Weight | `text-label` | Not `text-label font-semibold` — 14/600 is not a step in the scale. |
| Trailing | `ChevronDown` at `--icon-row` (16px), `text-text-muted`, `rotate-180` on open over `--dur-fast` | Matches `Select`. |

Content, left to right: 8px `rounded-full` colour dot (`aria-hidden`) · name (`truncate`, `min-w-0
flex-1`) · `+N` in `text-supporting text-text-muted` when other bases are in this chat · format
`Badge uppercase` · chevron. The `KB` badge is dropped: the control sits under a heading that says
Knowledge, and the format badge is the informative one.

**Opening behaviour changes.** Clicking the trigger opens an anchored `DropdownMenu` (`w-64`,
`sideOffset={6}`, `.biorouter-popover-surface`), not a 760px modal. The menu is the app's canonical
searchable picker: an `<Input className="h-8">` in a `p-1` block at the top, then one
`DropdownMenuCheckboxItem` per visible base using `DROPDOWN_ROW_CLASS_NAME`, then a separator, then
`Manage bases…` and `Create knowledge base…`. Picking a base sets the primary and closes. This is
what the composer's model picker and the chat-side KB chip already do, and it makes the same task the
same shape in both places.

### 3.2 KB manager

The 760px `Dialog` becomes a `ModalShell size="lg"` (`--dialog-lg`, 640px) with `purpose="form"` —
backdrop click must not dismiss it while a name is half-typed. `760px` is not a dialog width; it is
`--measure-chat` borrowed, and there is no fourth width.

| Region | Spec |
| --- | --- |
| Header | `DialogTitle` at `text-subheading` (unchanged primitive). Description as today. |
| Toolbar | `px-4 pt-3 pb-3`, `border-b border-border-subtle`. `<Input>` (the primitive, not a hand-rolled field) with a leading 16px `Search`, then `Create knowledge base` (`variant="default"`) and `Import from .brkb` (`variant="outline"`), both `size="default"` (32px). Icon gap comes from `Button`'s own `gap-2`; no `mr-1.5`. |
| Follow-the-default notice | Unchanged in behaviour and copy. Surface becomes `rounded-element` (an element inside the 12px dialog container, one step down). |
| List | `.biorouter-list-shell` inside `max-h-[52vh] overflow-y-auto px-4`. |
| Footer | `DialogFooter`, `variant="outline"` Close, tip text unchanged. |

**Row.** `.biorouter-list-row flex items-center gap-3 px-3` with `role="option" aria-selected`.

- Selected (primary) row: `tint-selected tint-interactive` — **not** `bg-background-medium`.
  `.biorouter-list-row:hover` is declared *unlayered* in `main.css` and repaints at
  `color-mix(--background-medium 42%, transparent)`, which is *lighter* than the hardcoded fill, so
  today the primary base visibly un-highlights under the pointer. The `tint-selected.tint-interactive`
  pair exists at specificity (0,2,1) for exactly this collision.
- Dot: 8px via a shared `KbDot` component — one diameter for the object everywhere. Today the object
  is drawn at 6, 8, 10 and 12px across five files, and the trigger's 8px and the palette's 12px are
  one click apart.
- Name `text-label truncate`, id `text-supporting font-mono text-text-muted truncate`.
- Badges: format, `BuiltInBadge`, `PrivacyBadge` (only when not public), `Not in this chat`,
  `Primary` (`tone="accent"`).
- Actions: the visibility `Switch` (unwrapped — the ad-hoc `px-2 py-1` bordered box goes; the switch
  is its own affordance and carries an `aria-label`), then Export and Rename as
  `<Button variant="ghost" shape="round">` (32×32, 16px icon, each in a `Tooltip`), then one `⋯`
  overflow holding **Delete** with `variant="destructive"`. A destructive control never sits visible
  in a hover cluster, and `text-text-danger hover:text-text-danger/80` — which lowers contrast on
  hover — goes with it.

### 3.3 The format chooser

New surface. Reached from `Create knowledge base` in the manager and from the trigger menu.

`ModalShell size="md"` (`--dialog-md`, 480px), `purpose="form"`.

Body, in order:

1. **Name** — `<Input>` with label `Name`, `text-label`. Below it, `text-supporting text-text-muted`
   showing the derived id in `font-mono`: `Will be created as knowledge/<id>/`.
2. **Format** — a `role="radiogroup"` of two selectable **rows** in a `.biorouter-list-shell`, not
   two cards. At 480px minus 32px of dialog padding each card would be 216px wide and would truncate
   its guidance; the guidance is the entire point of the surface. Each row is
   `.biorouter-list-row items-start gap-3 px-3 py-3`, carries `role="radio" aria-checked`, takes
   `tint-selected tint-interactive` when chosen, and holds:
   - a `CustomRadio` mark (16px, `--radius-full`, 6px accent dot),
   - a title line: `text-label` name + a `Badge uppercase` with the short code,
   - a "pick this when" line in `text-body text-text-muted`,
   - a three-item fact list in `text-supporting text-text-muted`, each item prefixed by a 4px
     `--radius-full` `bg-background-strong` dot.

   | | **OKF** — general knowledge (default, preselected) | **BioOKF** — curated biomedical |
   | --- | --- | --- |
   | Pick this when | You are keeping notes, project context, retrieval material, or anything that is not curated biology. | You are curating biomedical literature or building a base another institution will read. |
   | Fact 1 | Any page type, any link name. Nothing is ever rejected. | 28 page types and 35 link predicates, checked. |
   | Fact 2 | Best when you do not yet know how the material will be structured. | Every link must name its evidence: knowledge level, agent type, and a primary source. |
   | Fact 3 | Lint reports broken links only. | Lint flags anything outside the vocabulary and names the closest legal value. |

3. **The irreversibility notice.** A `--wash-warning` banner (`rounded-element`, 20px `AlertTriangle`
   in `--text-warning`, `text-supporting`): *"You cannot change a knowledge base's format yet. Pick
   BioOKF only if you want the biomedical vocabulary enforced from the first page."* This is not
   decoration — `kb_migrate_format` is deferred by DR-22, so the choice is currently permanent and
   the UI is obliged to say so. When migration ships, this banner is deleted and the record updated;
   nothing else in the surface changes.

4. Footer: `Cancel` (`variant="outline"`) · `Create knowledge base` (`variant="default"`).

The chooser is also the surface a future `Convert format` action reuses; do not build a second one.

### 3.4 Sources rail — the ingest panel and its five states

The rail is a flat column pane. Header strip: `h-[--row-height]`, `px-3`, `border-b
border-border-subtle`, holding `<h2 className="text-caps text-text-muted">Sources</h2>` and, on the
right, the staged count as a `Badge` when non-zero. Body scrolls; the footer is pinned.

Body order: tier control (§3.11) · dropzone · Paste text · warnings · staged list · digest progress.
Footer (`border-t border-border-subtle p-4`, `flex flex-col gap-2`): model picker · Digest button ·
blocked-reason line.

Fixes that apply across the rail regardless of state:

- **Dropzone medallion** — `h-12 w-12 rounded-container border border-border-subtle bg-background-muted`
  with a 20px icon, matching `EmptyState`'s own plate. `--radius-full` is restricted to status dots,
  the switch knob and avatars; a 40px icon medallion is none of those.
- **Format chips** in the dropzone lose `font-mono`. A file extension in a chip is chrome naming a
  thing, not data to be read character by character (design system TYPE-11 / D-33).
- **Paste box** uses `<Input>` and a real textarea, both at `text-label`, and relies on the global
  focus treatment; the bespoke `focus-within:border-border-strong` goes. Detected URLs become
  `<Badge variant="chip">` toggles, not hand-rolled `py-0.5` spans.
- **Staged rows** sit flush in a `.biorouter-list-shell` — the `gap-1.5` between rows goes, so each
  row's bottom hairline divides rather than floats. Icons are 16px (`--icon-row`) throughout; the
  remove control becomes a `--control-compact` (24px) `shape="round"` ghost button, not a 12×12
  pointer target.
- **Warnings** stop using `.biorouter-list-row` as a card. The panel becomes
  `rounded-container border border-border-subtle`, and the per-warning cards inside it drop to
  `rounded-element` — the nesting ladder runs downward, and today it runs upward. `opacity-90` on the
  message body becomes `text-text-muted`; `opacity-70 hover:opacity-100` on the dismiss becomes a
  `--control-compact` ghost `shape="round"`.
- **One ellipsis.** `…` everywhere. Today `IngestPanel` writes `Stopping…` directly above
  `DispatchProgress`'s `Stopping...`, simultaneously.
- **One sentence case.** `Clear` (capital) everywhere; today `StagedList` says `clear all` and
  `IngestWarnings` says `Clear`, six components apart in the same rail.

#### The five states

| # | State | What renders |
| --- | --- | --- |
| 1 | **Empty** | Dropzone + Paste text. Staged list is `<EmptyState compact icon={Inbox} title="Nothing staged" description="Drop files above, paste text, or choose a folder." />` with no action — the actions are directly above it. Digest button is full-opacity with `cursor-not-allowed` and the helper line `Stage a file to digest.` (K-04 is preserved verbatim: the one primary action never trains the eye to ignore a permanently half-lit button.) |
| 2 | **Staged** | `.biorouter-list-shell` of staged rows under a `text-caps text-text-muted` `Staged · N` header with a `Clear` ghost. Digest enabled. |
| 3 | **Digesting** | `<Progress>` — **determinate on the queue**: `value={completed}` `max={queue.length}`, `aria-label="Digesting staged sources"`. The per-item sub-agent stream stays a log below it. This is the fix for the section's longest operation having no `role="progressbar"` anywhere. `indeterminate` is used only for the pre-flight model check, where there is genuinely no denominator. Digest button becomes `Stop` (`variant="outline"`, `--control-lg`); the bare-text Stop link inside the log goes. |
| 4 | **Blocked** | Digest stays full-opacity and `aria-disabled`; the helper line states the one true reason, in the existing precedence (no base → base unavailable → model loading → no model → nothing staged). `Retry` stays a real `<Button variant="ghost" size="sm">`, not an underlined text span. Unchanged logic; changed spelling. |
| 5 | **Failed** | Errored rows stay, each with its message in `text-supporting text-text-danger`. Above the footer a `--wash-danger` summary row appears: `N sources failed` + `Retry failed` (`variant="outline"`) + `Clear failed` (`variant="ghost"`). Successful rows still auto-clear. |

**Digest button** is `size="lg"` (`--control-lg`, 36px) — the view's single dominant action — and
carries no `className` height override. `size="sm" className="min-h-9"` is a contradiction: it forces
a 28px rung to render at 36px while keeping `sm`'s `gap-1.5`.

**Model picker** takes the same Select chrome as the KB trigger (§3.1) and opens an anchored
`DropdownMenu` (`w-64`, search `<Input className="h-8">` at top, `DropdownMenuLabel` per provider
section, `max-h-[400px] overflow-y-auto`) — not a second 760px modal. The `Set as default` affordance
becomes a `<Badge variant="chip">` inside the menu footer, not a hand-rolled `px-2 py-1` span in the
trigger. Its empty state is `<EmptyState compact icon={Brain} title="No models available"
description="Configure a provider in Settings." actions={<Button variant="outline">Open settings</Button>} />`.

### 3.5 Graph panel

Three stacked regions inside the pane, no gutter between them.

1. **Facet strip** — `h-[--dock-height]` (36px), `px-3`, `border-b border-border-subtle`,
   `bg-background-default`, `flex items-center gap-2`, `overflow-x-auto` with `scrollbar-gutter: stable`.
2. **Canvas** — `flex-1 min-h-0 relative bg-background-muted`. No gradient, no card, no shadow, no
   inner padding. The canvas is the content; the pane's own border is the only edge.
3. **Legend dock** — `h-[--dock-height]`, `px-3`, `border-t border-border-subtle`,
   `bg-background-default`.

`--dock-height` is reused deliberately: this is the same object it already names — a horizontal
control strip docked to a pane edge, one step down from a chrome band. Its comment in `main.css`'s
hand-authored `:root` block is amended in the same change so the name stops claiming to be
terminal-only. The value does not move.

**The inline gradient is deleted.** `ForceGraphCanvas.tsx` currently sets a three-layer
`radial-gradient` + `linear-gradient` on the container. It is the only gradient-washed surface left
in the application, it has no dark variant (the white→black layer paints identically on the near-black
dark ground), its two tints are two of the off-system node hexes leaking into the background of the
panel that displays them, and it breaks the runtime ground resolve —
`getComputedStyle(el).backgroundColor` returns `rgba(0,0,0,0)` when the colour lives in
`backgroundImage`. Removing it is one property and is the single highest visible-effect edit in this
document.

### 3.6 Facet rail

Four facets plus a search box, in one 36px strip. Semantics: **OR within a facet, AND across facets.**
A node that fails the filter takes the search-miss alpha (0.12) and stays in place — one dimming
mechanism, never a second.

| Control | Spec |
| --- | --- |
| Search | `<Input>` at `h-control-sm` (28px), `w-[200px]`, leading 16px `Search` icon, `placeholder="Filter by name or type"`. Matches `identifier`, `type` and `subtype`, case-insensitively, as a substring — so `Disease` selects a class and `IL6` selects a node without a mode switch. Live on every keystroke, no debounce, no submit. |
| Type | `<Button variant="outline" size="sm">` + `Badge` count when active. Menu: `w-64` `DropdownMenu` with a pinned `<Input className="h-8">`, then `DropdownMenuCheckboxItem` rows carrying an 8px palette swatch, the type name, and a `font-mono tabular-nums` count. **BioOKF:** grouped by the seven families under `DropdownMenuLabel` headers. **OKF:** flat, sorted by count descending. |
| Predicate | Same shape. Rows are `font-mono` (a predicate is a machine token). A negated predicate is listed immediately after its positive, spelled out (`not prevents`) and rendered in `--text-danger` with `line-through`. |
| Source | Same shape. Rows are the source nodes present (`Publication`, `Study`, `Dataset`), plus a synthetic `No primary source` entry. Selecting one keeps nodes and edges whose `primary_source` resolves to it. |
| Status | Fixed set, no search: `draft`, `stable`, `deprecated`, `stale`, `retracted`. |
| Clear | `<Button variant="ghost" size="sm">Clear filters</Button>`, present only when something is active, preceded by `text-supporting text-text-muted font-mono tabular-nums` reading `Showing 18 of 42`. |

Below 1100px the four facet buttons drop their text labels to icon + count; below `--breakpoint-md`
they collapse into a single `Filters` button whose menu carries all four as
`DropdownMenuSub` sections. They never wrap.

**Status is deliberately not a canvas channel.** The canvas already carries three encodings (fill =
type, ring = credibility, dash/hollow = negated / synthesized / external). A fourth is not readable
at a 6px node, and the alternatives all collide: a tinted fill fights the dim system, a dashed ring
is already the external marker. Status lives in the facet and in the inspector badge, and a status
the user has filtered out simply dims. `retracted` keeps its existing `!` badge because a retraction
is a fact, not a lifecycle state.

### 3.7 Legend

Collapsed (default) — one horizontally scrolling row:

- Per family: `text-caps text-text-muted` family name, then a run of 8px `rounded-full` swatches
  (`gap-1`), separated from the next family by `gap-4`.
- Then a `w-px h-4 bg-border-subtle` separator, then the credibility key: three 10px rings
  (2px border, transparent centre) labelled `Well sourced`, `Weakly sourced`, `Not academic`, then a
  fourth in the retracted hue labelled `Retracted`.

  Four entries, not seven, and that is DR-9b's honesty clause made visible: *"a 1.6px ring reads as
  high-versus-low, not as six distinguishable tiers."* The exact tier is in the inspector and in the
  Source facet.

- Right: a `ChevronUp` `--control-compact` ghost toggle. State persists to `localStorage` under
  `biorouter:knowledge-legend-expanded` inside a try/catch, default `false`.

Expanded — the dock grows to `max-h-[40%]` and becomes the chip grid: per family, a `text-caps`
header then `flex flex-wrap gap-1.5` of `<Badge variant="chip" tone="neutral">` entries, each an 8px
swatch plus the type name.

`variant="chip"` and not `badge` is the primitive's own contract: a chip carries a *category or a
filter*, which is what a node type is, and it is the tier that may be acted on. `tone="neutral"`
because the six tones are semantic (accent/info/success/warning/danger) and a node type is none of
them — the hue belongs on the swatch, never on the chip's fill.

**Every chip is a real `<button aria-pressed>`** and toggles the Type facet; selected chips take
`tone="accent"`. Family headers toggle the whole family. Swatches carry `aria-hidden` — the name is
the accessible label, never the colour alone. The current legend is inert, which is the most obvious
missing affordance in the section.

In OKF mode there are no families: the legend lists the types actually present, sorted by count
descending, capped at 24 with a `+N more` that opens the Type facet menu. Two extra rows appear in
both modes when the graph contains them: `External` (a hollow dashed 8px ring in `--text-muted` at
45%, labelled *Referenced, no page yet*) and — **BioOKF only, and only when present** —
`Unrecognised type` as a `tone="warning"` badge that links into the lint popover.

### 3.8 Inspector

One rail, two subjects. It replaces `NodePreview.tsx` and adds an edge inspector, which does not
exist today.

Chrome, shared: `rounded-container border border-border-subtle bg-background-default`, a
`h-[--row-height]` head with `border-b border-border-subtle bg-background-muted`, a scrolling body,
and a `border-t` footer. As a column it carries no shadow; as an overlay it carries
`--shadow-popover` plus `--inset-hairline` and `z-[var(--z-dropdown)]`. **Never `z-10`** — an off-scale
z-index is exactly the class of value that soft-locked the app once already.

Close control: `<Button variant="ghost" shape="round">` — 32×32 with a 16px icon, per the app's one
dismiss geometry for a full panel.

#### Node inspector — body order

1. **Identity.** 8px type dot (palette fill) · `text-subheading` identifier · sub-line with
   `<Badge>` type · `subtype` in `text-supporting text-text-muted` · status badge
   (`draft`→`tone="warning"`, `stable`→ nothing, `deprecated`→`tone="neutral"` with `line-through`) ·
   a `tone="warning"` `Stale` badge when `stale_after` has passed.
2. **Frontmatter, as labelled rows.** The single biggest inspector fix: today this is a raw `<pre>` of
   YAML. Each row is `grid grid-cols-[96px_minmax(0,1fr)] gap-3 py-1.5`; key in `text-caps
   text-text-muted`, value in `text-body`. Arrays render as `Badge variant="chip"` runs (`synonyms`,
   `tags`, `xref`). An `xref` whose prefix is recognised (`DOI`, `PMID`, `PMCID`, `arXiv`,
   `UniProtKB`, `HGNC`, `MONDO`, `HPO`) renders as a real external link. **Unknown keys render with
   the same treatment** — there is no allowlist, so a frontmatter addition appears as another row
   with no renderer change. This is the one property worth borrowing wholesale from BioOKF Studio.
3. **Sources and provenance** — present only when the page carries `sources[]` or `br_credibility`.
   A 10px credibility ring in the tier hue followed by `<Badge tone="neutral">` naming the tier, then
   `confidence` as `font-mono tabular-nums`, then a `tone="danger"` `Retracted` badge when set. Then
   one row per `sources[]` entry: title, author, `last_modified`, and `resource` rendered in
   `font-mono` with a `--control-compact` "reveal in folder" ghost button pointing into `raw/`.
   **The tier hue never sits behind text** — it is a ring, and the word is app ink on the app ground.
4. **Links out, grouped by predicate.** Group header: the predicate in `font-mono text-caps
   text-text-muted` on a `bg-background-medium rounded-inner px-1.5` pill. A negated predicate's
   header is `--text-danger` with `line-through`. Under it a `.biorouter-list-shell` of rows, each a
   real button that selects that edge:

   `[→ or ⇄] · [8px object type dot] · object identifier (truncate) · [ext Badge] · [stat]`

   The stat is one right-aligned `font-mono tabular-nums text-supporting` value — the first of
   `effect_size`, `sensitivity`, `frequency`, `direction`, `unit` that is present. One number, never
   a table.
5. **Referenced by (N)** — the mirror of the same row: the *source* node's dot and identifier on the
   left, the predicate moved to the right-hand slot in `font-mono` with its arrow (`treats →`). One
   row shape, two readings. Capped at 10 with a `Show all N` expander — Studio's cap, with the
   missing expander.
6. **Document** — `MarkdownContent`, body only, `text-body`.
7. **Footer** — `node.path` in `font-mono text-supporting text-text-muted break-all`, with a
   `--control-compact` reveal-in-folder ghost button.

#### Edge inspector — body order

1. **Head.** `<Badge tone="neutral" uppercase>Edge</Badge>`, or
   `<Badge tone="danger" uppercase>Negative edge</Badge>` when the predicate starts with `not_`.
   Sub-line: `directed` / `symmetric` / `synthesized from primary_source`.
2. **Headline — the edge as a sentence.** Three stacked rows at 340px (a single line would truncate
   both endpoints): subject row (8px dot + identifier, a button that selects that node), predicate
   row (`<Badge variant="chip">` in `font-mono`; `tone="danger"` with `line-through` when negated,
   with the arrow glyph), object row (same as subject).
3. **Provenance triplet** — three labelled rows, not a two-column grid: `knowledge_level`,
   `agent_type`, `primary_source`. `primary_source` is a button that selects that source node.
   For a **synthesized** edge the triplet is replaced by a `--wash-info` note: *"Implicit link
   derived from the cited primary source so the provenance is visible. Author an explicit
   `reported_in` edge to make it first-class."*
4. **Publications** — real external links.
5. **Stats** and **Qualifiers** — every key rendered uniformly as `label: value`, with exactly one
   privileged merge: `ci_lower` + `ci_upper` → a single `95% CI` row. Nothing else is special-cased,
   so a vocabulary addition shows up automatically.

#### Loading and selection behaviour

Render partially, immediately: identity from the graph node (which is already in memory), then
`Loading page…` in the body while the page fetch is in flight, and re-render **only if the selection
is still the same object**. A fast click-through must never paint a stale panel. A failed fetch shows
the error in the body; it does not blank the panel.

### 3.9 Lint

**The pill** lives in the subject band. `<Badge variant="chip">` with a tone by worst severity:
`danger` if errors > 0, else `warning` if warnings > 0, else `neutral`. It is `tabindex=0`, opens on
click and on Enter/Space, and its content is:

| Condition | Content |
| --- | --- |
| Findings present | `3 errors · 10 warnings` — counts in `font-mono tabular-nums` |
| Linted, clean | `No issues` |
| Never linted | `Not linted` |
| In flight | `Linting…` with a 14px spinner |

Counts are **derived by counting `findings[].severity`**, never taken from a report's own scalars.
Lint is fetched after the graph paints and cached per base, so it never delays first render.

**The popover** is a `Popover` on `.biorouter-popover-surface`, `w-[var(--dialog-sm)]` (400px),
`max-h-[50vh] overflow-y-auto`, `sideOffset={6}`, `--radius-container`, `--shadow-popover`. Dismissed
by outside click or Escape.

Content: `.biorouter-list-shell` grouped by severity under `text-caps text-text-muted` headers in the
fixed order `Errors` · `Warnings` · `Notices`. Each finding is a row and **the row is a button**:

```text
[8px severity dot]  rule.id (font-mono text-supporting)   subject (text-label truncate)
                    message (text-supporting text-text-muted)
                    path (font-mono text-supporting text-text-subtle)
```

Severity dots take `--background-danger` / `--background-warning` / `--background-info`.

Clicking a row **selects the offending node or edge on the canvas, opens the inspector, and closes
the popover.** That is the cheapest available improvement over the surface this borrows from, whose
own rows are inert and force the user to retype the subject into a search box.

Footer: `Last checked <relative time>` in `text-supporting text-text-muted`, and a `Run lint`
`<Button variant="ghost" size="sm">`. Clean state:
`<EmptyState compact icon={CheckCircle2} title="No issues found" description="Last checked 4m ago." />`.

In OKF mode the rule set is small (broken links, duplicate identifiers) and the pill is usually
neutral; in BioOKF mode it carries the vocabulary and domain/range findings. The surface does not
branch — only the findings differ.

### 3.10 Change-log drawer

Kept as a right-side `Sheet`. Fixes:

- Width: `sm:max-w-[var(--knowledge-rail-detail)]`, so the drawer and the detail rail are the same
  object at the same width instead of two arbitrary numbers.
- `SheetTitle` keeps `text-subheading` — the `text-label` override goes. The section currently has two
  overlay titles at two sizes, opened from the same screen.
- Kind filters become `<Badge variant="chip">` toggles as real `<button aria-pressed>`, matching the
  legend's chips exactly. Today they are hand-rolled at ~20px with no background in the unselected
  state, so the filter row reads as a run of lowercase words.
- **`ChangeKindChip` stops using status hues for a taxonomy.** `flag` currently renders danger-red and
  `query` renders success-green, so a routine log entry looks like an error. All seven kinds become
  `variant="chip" tone="neutral"` with a 14px leading kind glyph; only `flag` keeps a `danger` tone,
  because a flag genuinely is a problem marker.
- Row actions become `size="sm"` (28px). `size="xs"` is the compact tier and its contract is
  glyph-only — *"a control carrying a label never uses it."*
- `tint-interactive` comes off the non-clickable entry rows (the targets are the two buttons inside
  them) and the row becomes a `.biorouter-list-row` for its hairline only.
- Loading / error / empty all become `EmptyState compact`.

### 3.11 Tier control

Behaviour, copy and the typed-phrase confirmation are unchanged — they are a signed-off privacy
design (issue #56 DR-18) and this pass does not touch them. Two visual corrections only:

- The confirmation dialog's summary block (`KbTierControl.tsx:134-138`) is the section's only pocket
  of pre-system classes: `rounded-lg` → `rounded-element`; `bg-background-muted/40` → `bg-background-muted`
  (an arbitrary alpha on an opaque surface step); `text-sm font-medium` → `text-label`;
  `text-xs` → `text-supporting`.
- The panel itself moves to the top of the Sources rail, above the dropzone, inside a
  `rounded-element border border-border-subtle` block — beside the base it acts on, which is the
  placement DR-18 already argues for.

### 3.12 Empty, loading and error states

Every one is the `EmptyState` primitive. The section currently hand-rolls seven of them as bare
centred sentences, which is what makes it read thinner than its siblings on exactly the screen a new
user sees first.

| # | Where | Icon | Title | Description | Actions |
| --- | --- | --- | --- | --- | --- |
| 1 | Workspace — no bases at all | `KnowledgeIcon` | No knowledge bases yet | Create one and Biorouter will build and maintain notes, sources and links for you. | `Create knowledge base` (default) · `Import .brkb` (outline) |
| 2 | Workspace — bases exist, none primary | `Target` | No primary knowledge base | Choose which base this chat reads and writes. | `Choose a base` (default) |
| 3 | Canvas — base primary, zero pages | `Sparkles` | Nothing digested yet | Stage a source in the Sources rail and press Digest. | none |
| 4 | Canvas — graph load failed | `AlertCircle` | Could not load the graph | *the error message* | `Try again` (outline) |
| 5 | Canvas — filters exclude everything | `Filter` | No pages match these filters | *(compact)* | `Clear filters` (ghost) |
| 6 | Staged list — nothing staged | `Inbox` | Nothing staged | Drop files above, paste text, or choose a folder. *(compact)* | none |
| 7 | Lint popover — clean | `CheckCircle2` | No issues found | Last checked *<relative>*. *(compact)* | none |
| 8 | Change log — no history | `History` | No changes yet | Digesting a source records a commit here. *(compact)* | none |
| 9 | Model menu — no models | `Brain` | No models available | Configure a provider in Settings. *(compact)* | `Open settings` (outline) |
| 10 | KB manager list — search matches nothing | `Search` | No knowledge bases match | Try a different name or id. *(compact)* | none |

**Loading.** Two distinct behaviours, and conflating them is the current bug:

- **First load of a base's graph** — a centred `role="status"` block with an `sr-only` label and a
  16px spinner over `text-secondary text-text-muted` `Loading graph`. Cross-faded against the canvas
  as two absolutely-stacked opacity layers (loading out at `--dur-fast`, content in at `--dur-med`),
  never a swap.
- **Refresh of a graph already on screen** — the canvas is **not** blanked. The Refresh button's icon
  spins and the facet strip shows nothing else. Blanking a graph the user is reading, to redraw the
  same graph, is a regression disguised as feedback.

The sources rail and the KB manager use `Skeleton` rows shaped like the rows they will replace, with
staggered negative `animationDelay`, wrapped in `role="status"` with `aria-hidden` children.

---
## 4. The graph visual specification

Every number below is stated. Where a number is measured, the measurement is reported and must be
re-run by the guard rather than trusted from this page.

### 4.1 The ground, and why one palette serves three families

`--background-muted` resolves to **`#f4f4f2`** in light and **`#232320`** in dark in Parchment, Alma
Mater *and* Roche Limit — verified by reading all three `themes/*.theme.mjs` files. The graph paints
that token, so the palette needs a light pair and a dark pair, not a per-family set. That is what
DR-10's "node hues get a light/dark pair" means in practice.

**Nothing enforces the sharing today.** `check-contrast.mjs` audits each family independently, so a
diverged neutral would pass every existing assertion while silently invalidating the palette.
Therefore `generate-themes.mjs` must, before emitting the shared palette, resolve `--background-muted`
in all six (family × mode) scopes and **die** if the three light values are not identical or the three
dark values are not identical, with the message:

```text
graph palette is emitted once because the three families share --background-muted;
they no longer do — move GRAPH_PALETTE per-family or re-derive it.
```

That assertion is the entire justification for a shared block. Without it the block is an assumption.

### 4.2 The derivation rule

The rule is normative; the hex tables in §4.3 are its pinned output. If the solver ever produces a
different last bit, the **table is corrected from the measurement**, never the other way round.

Working space is OKLCH — perceptually uniform, so a fixed chroma reads as an equal amount of colour
across hues and a hue spread reads as an equal amount of rotation.

**Step 1 — family anchor hue `H0` and family chroma `C`.**

| Family | `H0` (OKLCH°) | `C` | Spread `S`(°) | Members, in order |
| --- | --- | --- | --- | --- |
| Genomic | 288 | 0.135 | 30 | Gene, Variant, SequenceFeature, Structure |
| Molecular & process | 192 | 0.105 | 34 | Molecule, MolecularClass, BiologicalPathway, BiologicalFunction |
| Anatomy & organism | 148 | 0.115 | 26 | Anatomy, CellType, Organism |
| Clinical | 18 | 0.145 | 34 | Disease, Phenotype, BiomedicalMeasure, MethodOrProcedure |
| Exposome | 78 | 0.120 | 24 | Exposure, SocialFactor, Food |
| Physical | 250 | 0.090 | 26 | Device, MaterialSample |
| Provenance & context | 250 | 0.030 | 190 | Publication, Study, Dataset, Agent, Population, GeographicLocation, Concept, Other |

**Step 2 — hue within the family**, distributed evenly across the spread:

```text
hue_i = H0 + (n === 1 ? 0 : (i / (n - 1) - 0.5) * S)      // i = index in the declared order, n = family size
```

**Step 3 — the contrast rung**, by index within the family. Only Provenance reaches indices 4–7:

```text
RUNG = [3.50, 4.50, 5.80, 7.30, 4.00, 5.10, 6.50, 8.20]
```

**Step 4 — solve `L`.** `L` is the OKLab lightness at which the resulting sRGB hex hits the rung's
contrast ratio against the mode's ground. Bisection on `L ∈ [0.05, 0.99]`, 50 iterations; at each
probe the chroma is gamut-mapped (bisection on `C`, 24 iterations, in-gamut tolerance ±1e-4 on linear
RGB) and the result **rounded to 8 bits before the ratio is taken**, so the measured value is the
shipped value.

Reproduce this convention exactly or the strings differ in the last bit: on a **light** ground
contrast falls as `L` rises, so keep the largest `L` whose ratio ≥ target; on a **dark** ground
contrast rises with `L`, and the implementation keeps the largest `L` whose ratio ≤ target — which is
why the dark floor measures 3.48 against a 3.50 nominal rather than 3.50 or above.

`GROUND = { light: '#f4f4f2', dark: '#232320' }`.

**Why not BioOKF Studio's 28 hexes.** They are hand-picked against one near-white
`radial-gradient(#fff → #eef1ef)`. Nine fall below 3:1 even there; `SequenceFeature` `#AAA6DA`,
`Organism` `#8FCBA6`, `Other` `#AEB2B8` and `EXTERNAL_COL` `#D7DBE1` (≈1.2:1) would be near-invisible
on `#f4f4f2`, and all 28 are unusable on `#232320`. The rule above keeps what is actually recognisable
about that palette — the family hue grouping — and makes contrast a derived property instead of an
accident.

### 4.3 The 28-type palette

Contrast is against the graph's own ground. Measured with WCAG 2.x relative-luminance arithmetic
identical to `scripts/lib/theme-tokens.mjs::luminance` / `contrast`.

| Type | hue | C | rung | Light | vs `#f4f4f2` | Dark | vs `#232320` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Gene | 273.0 | 0.135 | 3.50 | `#6a7cd4` | 3.50 | `#5f70c8` | 3.48 |
| Variant | 283.0 | 0.135 | 4.50 | `#6965be` | 4.53 | `#817fdb` | 4.50 |
| SequenceFeature | 293.0 | 0.135 | 5.80 | `#6750a7` | 5.81 | `#a48fec` | 5.79 |
| Structure | 303.0 | 0.135 | 7.30 | `#643d90` | 7.30 | `#c69efb` | 7.27 |
| Molecule | 175.0 | 0.105 | 3.50 | `#1d927a` | 3.50 | `#00866f` | 3.48 |
| MolecularClass | 186.3 | 0.105 | 4.50 | `#007d75` | 4.55 | `#13998f` | 4.49 |
| BiologicalPathway | 197.7 | 0.105 | 5.80 | `#006a6d` | 5.81 | `#2fadb0` | 5.80 |
| BiologicalFunction | 209.0 | 0.105 | 7.30 | `#005963` | 7.31 | `#4cbfd1` | 7.26 |
| Anatomy | 135.0 | 0.115 | 3.50 | `#608d44` | 3.54 | `#558239` | 3.48 |
| CellType | 148.0 | 0.115 | 4.50 | `#367e45` | 4.51 | `#50985d` | 4.49 |
| Organism | 161.0 | 0.115 | 5.80 | `#006d48` | 5.81 | `#4dae81` | 5.77 |
| Disease | 1.0 | 0.145 | 3.50 | `#cb5d82` | 3.51 | `#bf5177` | 3.50 |
| Phenotype | 12.3 | 0.145 | 4.50 | `#ba4a5e` | 4.51 | `#d76476` | 4.48 |
| BiomedicalMeasure | 23.7 | 0.145 | 5.80 | `#a73939` | 5.80 | `#ef7a76` | 5.79 |
| MethodOrProcedure | 35.0 | 0.145 | 7.30 | `#942b0f` | 7.30 | `#ff9379` | 7.28 |
| Exposure | 66.0 | 0.120 | 3.50 | `#b47327` | 3.52 | `#a86817` | 3.49 |
| SocialFactor | 78.0 | 0.120 | 4.50 | `#966700` | 4.50 | `#b18023` | 4.49 |
| Food | 90.0 | 0.120 | 5.80 | `#755c00` | 5.80 | `#ba9938` | 5.78 |
| Device | 237.0 | 0.090 | 3.50 | `#4788b0` | 3.52 | `#3b7da4` | 3.49 |
| MaterialSample | 263.0 | 0.090 | 4.50 | `#546fa5` | 4.55 | `#6d89c0` | 4.49 |
| Publication | 155.0 | 0.030 | 3.50 | `#738679` | 3.52 | `#697b6e` | 3.50 |
| Study | 182.1 | 0.030 | 4.50 | `#5c7570` | 4.50 | `#758e8a` | 4.50 |
| Dataset | 209.3 | 0.030 | 5.80 | `#4b6368` | 5.81 | `#87a1a6` | 5.76 |
| Agent | 236.4 | 0.030 | 7.30 | `#40525e` | 7.37 | `#9fb3c1` | 7.27 |
| Population | 263.6 | 0.030 | 4.00 | `#6f788b` | 4.03 | `#778093` | 3.97 |
| GeographicLocation | 290.7 | 0.030 | 5.10 | `#686579` | 5.12 | `#9390a5` | 5.08 |
| Concept | 317.9 | 0.030 | 6.50 | `#605364` | 6.53 | `#afa2b4` | 6.49 |
| Other | 345.0 | 0.030 | 8.20 | `#56434e` | 8.26 | `#cbb5c1` | 8.19 |

**The ladder inverts between modes by construction.** In light a higher rung is a darker colour
(`Structure` is the darkest violet); in dark a higher rung is a lighter one (`Structure` is the palest
violet). Same rung index, same relative position within the family, opposite direction — which is what
keeps a family readable in both modes without a second authored table.

#### Measured worst cases

| Measurement | Light | Dark |
| --- | --- | --- |
| Contrast floor on the graph ground | **3.50** (Gene) | **3.48** (Gene) |
| Contrast floor on `--background-default` | 3.86 (Gene, `#ffffff`) | 3.81 (Gene, `#1b1b19`) |
| Contrast floor on `--background-canvas` | 3.86 (Gene, `#ffffff`) | 4.11 (Gene, `#131312`) |
| Contrast floor on `--background-medium` | 3.26 (Gene, `#ecece9`) | — |
| Minimum CIEDE2000 over all 378 pairs | **6.33** (Concept / Other) | **6.95** (SequenceFeature / Structure) |

Both floors clear **WCAG 2.1 §1.4.11 non-text contrast (3:1)**, which is the correct criterion for a
coloured dot; a node fill is never text, so 4.5:1 is not the bar and must not be asserted.

> **Correction to a claim in the research inputs.** The graph ground is *not* uniformly the worst case:
> on `--background-medium` (`#ecece9`) the light floor drops to 3.26. It still clears 3:1, so the
> palette remains legal on every app ground, but the 3.5 floor holds only on the graph's own ground
> and on lighter or darker ones. Do not repaint the pane `--background-medium` without re-measuring.

In both modes the closest pair is **within** a family, and every cross-family pair is further apart
than every within-family pair. That falls out of the construction: family members share hue and
chroma and are separated only by the rung ladder, while different families differ in hue, in chroma,
and usually in rung.

### 4.4 Arbitrary OKF types — the DR-11 fallback

In OKF mode essentially every node takes this path, so it must look native, not like an error state.

**Hash.** FNV-1a, 32-bit, unsigned — the same hash the renderer already uses for jitter, so there is
one hash in the file:

```ts
function fnv1a(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}
```

The input is the **raw type string, byte for byte** — no case folding, no trimming. `Gene` and `gene`
are different types to OKF, so they get different colours; folding would make an exact match with a
curated type invisible in the UI.

**Derivation**, given `H = fnv1a(type)`:

```text
hue    = H % 360
chroma = 0.055                                   // FIXED
rung   = [3.90, 4.95, 6.20, 7.90][(H >>> 9) & 3]
L      = solved against GROUND[mode] for `rung`, by the §4.2 solver
```

`chroma = 0.055` sits deliberately between Provenance (0.030) and every biological family
(0.090–0.145), so an unrecognised type reads as quieter than a curated biological family and more
coloured than provenance — an honest signal, at no cost, that the vocabulary did not recognise it.

The rungs are offset from the curated ladder so a hashed colour can never coincide in lightness with a
curated one, which puts a floor under `ΔE` from the lightness term alone. Measured over all 1440
`(hue, rung)` combinations: light contrast floor **3.90**, closest approach to any of the 28
**ΔE00 3.50**; dark floor **3.86**, closest approach **ΔE00 5.02**. With the naive scheme (chroma
0.075 on the curated rungs) the measured closest approach was **ΔE00 0.00** — an exact collision at
hue 207 / rung 7.30 with `BiologicalFunction`. Say plainly in the code comment that ΔE 3.50 is a
subtle-but-nonzero difference, not a guarantee of distinguishability; the guarantee is only that no
arbitrary string can exactly reproduce a curated colour.

**Test vectors** — pin these; they fix both the hash and the solver. Hashes verified.

| Type | `H` | hue | rung | Light | Dark |
| --- | --- | --- | --- | --- | --- |
| `ClinicalTrial` | 3701338732 | 172 | 4.95 | `#457264` (4.96) | `#6b998a` (4.91) |
| `Protocol` | 1247275989 | 189 | 4.95 | `#40726e` (4.95) | `#669995` (4.92) |
| `Cohort` | 1729218152 | 272 | 7.90 | `#414a6a` (7.90) | `#abb6dc` (7.85) |
| `Assay` | 1972765544 | 104 | 4.95 | `#6e6b45` (4.95) | `#95916a` (4.90) |
| `Recipe` | 890010351 | 351 | 4.95 | `#855e6f` (4.98) | `#ae8596` (4.95) |
| `Person` | 3278826400 | 40 | 4.95 | `#876053` (4.98) | `#b08779` (4.94) |
| `Meeting` | 3228369114 | 354 | 3.90 | `#976e7e` (3.93) | `#9c7383` (3.90) |
| `Repository` | 3882076341 | 141 | 3.90 | `#668162` (3.91) | `#6b8567` (3.88) |

**Runtime cost.** The solver is a 50-iteration bisection with a 24-iteration inner gamut bisection —
far too slow per frame. Memoise on `` `${type}|${mode}` `` in a module-level `Map`, computed lazily on
first sight of a type. A base has O(10) distinct types, so the map is tiny and the cost is paid once
per session. Do **not** precompute all 360 hues.

**Rejected:** a hollow or donut marker to distinguish "not one of the 28". In OKF mode that marker
would apply to every node in the base, so it carries no information and only costs fill-rate. In
BioOKF mode an off-vocabulary type is already a lint finding and gets its own legend row (§3.7), which
is the right place for that signal.

### 4.5 Credibility on the ring — DR-9b

**Which nodes.** Exactly those with a verdict: `kind === 'source'` **and** `credibility_tier != null`.
Everything else keeps the neutral separation ring, including a source with no tier — absence of a
verdict is not a verdict.

**Geometry — an orbit ring, not an outline.** Two concentric strokes:

| Stroke | Radius | Width |
| --- | --- | --- |
| Neutral separation ring | `r` (on the fill path) | from the density ladder (§4.9) |
| Credibility ring | `r + 1.8 / globalScale` | `1.6 / globalScale` |

`1.8` = 1.0px of ground gap + half the 1.6px stroke. **The gap is load-bearing** and is the one
deliberate deviation from the source renderer worth defending: without it the ring's legibility
depends on ring-versus-*fill* contrast, which cannot be guaranteed across 28 fills × 7 tiers. Several
ring/fill pairs land at 1.02–1.06:1 luminance contrast (`gray_lit` `#768290` on `Publication`
`#738679` is 1.03:1 — luminance-identical, separable only chromatically at ΔE00 ≈ 14). With the gap
the ring is read against the ground alone, and its contrast is guaranteed ≥ 3.55:1 in both modes.

**LOD degradation.** Draw the orbit ring only when `r * globalScale >= 3.5`; below that the 1px gap
collapses into the anti-aliasing and the two strokes merge into mud. When suppressed, the **neutral**
ring takes the credibility hue instead, at `max(1.1, densityStrokeWidth) / globalScale`. The signal
degrades from "ring around the node" to "coloured outline" rather than disappearing. This is the only
place in the spec where a colour is drawn against an arbitrary fill, and it happens only when the node
is under 7px across, where fidelity is not claimed anyway.

**Colours**, by the same derivation, hue and chroma stated so they are regenerable:

| Tier | hue | C | rung | Light | vs `#f4f4f2` | Dark | vs `#232320` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `peer_reviewed` | 250 | 0.120 | 5.80 | `#1c619f` | 5.85 | `#5fa1e4` | 5.78 |
| `book` | 250 | 0.090 | 4.80 | `#406e9d` | 4.85 | `#6291c2` | 4.76 |
| `preprint` | 250 | 0.060 | 4.00 | `#5d7b9a` | 4.00 | `#6583a3` | 4.00 |
| `gray_lit` | 250 | 0.025 | 3.55 | `#768290` | 3.55 | `#6d7986` | 3.55 |
| `web` | 60 | 0.110 | 4.40 | `#a26227` | 4.42 | `#ba783f` | 4.39 |
| `personal` | 345 | 0.100 | 4.40 | `#9f5c83` | 4.40 | `#b67299` | 4.39 |
| `retracted` | 25 | 0.160 | 4.60 | `#c04441` | 4.60 | `#e1625d` | 4.58 |

Measured: ring contrast floor **3.55:1 in both modes** (`gray_lit`); minimum ΔE00 within the ramp
**5.18 light / 5.92 dark** (`peer_reviewed` vs `book`).

**The four academic tiers share one hue (250)** and differ only in chroma and lightness. That is the
design, not a shortcut, and it is DR-9b's honesty clause made structural:
`peer_reviewed → book → preprint → gray_lit` is a monotone fade from saturated blue to neutral grey,
which reads as *how much blue is left* — a strength ramp — while `web` (amber) and `personal` (rose)
step off the ramp into different hues because they are a different **kind** of source, not a weaker
one. The eye gets a three-band read; the exact tier lives in the inspector, the Source facet and the
legend.

**Retracted is a flag, not a tier.** The existing badge is kept: a filled disc at `(x + 0.7r,
y − 0.7r)`, radius `max(3, 0.45r)` in world units, filled `retracted`, with a `!` glyph at
`700 ${0.6 * badgeRadius}px`. Two changes: the glyph fill was `'#fff'`, which is invisible on the
light-mode badge ground — use the **resolved ground**; and the badge is suppressed below
`r * globalScale >= 4` along with the orbit ring. A retracted source also takes the retracted colour
on its orbit ring, overriding its tier — retraction is the more important fact.

**The ring hue never sits behind text.** In the legend it is a 10px ring with a transparent centre; in
the inspector it is a 10px ring beside a `tone="neutral"` badge carrying the tier word. Nothing
anywhere fills a surface with a ring hue.

### 4.6 Node geometry

Radii and centrality replace the fixed 4.5 / 7.5 pair and `HUB_TOP_N = 6`.

```text
deg(n)       = incident edge count, floored at any server-supplied n.degree
max          = largest degree (min 1)
p75          = degrees sorted ascending, value at index floor((count - 1) * 0.75)
pivot        = max(2, min(max(3, p75), sqrt(max) * 1.6))
hubThreshold = max(3, degrees[floor((count - 1) * 0.82)] || 3)

centrality   = max > 0 ? log1p(deg) / log1p(max) : 0
radius       = external ? clamp(4.5 + 1.4 * centrality, 4.5, 6.2)
                        : clamp(5.4 + 7.6 * (1 - exp(-deg / pivot)), 5.6, 13.4)
hub          = !external && deg >= hubThreshold
```

Non-finite radius falls back to `external ? 5 : hub ? 10 : 6`. All radii are **world** units and scale
with zoom.

**Why the percentile replaces top-N.** Top-N is size-blind: on a 12-node base it makes half the graph
a hub; on a 2,000-node base it makes six. The 82nd percentile is proportional by construction. The
same objection applies to the fixed 4.5 / 7.5 pair, which encodes no centrality at all — the most
connected page in a base looks identical to a leaf.

Setting this radius also fixes the LOD calibration for free: with base `r ≈ 5.6–6.0` world units, the
world scale matches the renderer these constants come from, so every threshold in §4.9 transfers to
`globalScale` 1:1 with no recalibration. Keep `nodeRelSize={5.6}` so force-graph's shadow-canvas hit
circles track the fills.

**Focus glow** (focused node only), painted **before** the fill:

```text
createRadialGradient(x, y, r, x, y, r + 13 / globalScale)
  stop 0.00 → fill at alpha 0.34
  stop 0.55 → fill at alpha 0.14
  stop 1.00 → fill at alpha 0.00
filled over arc(x, y, r + 13 / globalScale)
```

**Neutral ring** (every node not showing an orbit ring):

```text
sw = isFocus ? max(1.1, densityStrokeWidth) / globalScale : densityStrokeWidth / globalScale
sa = isFocus ? 0.92 : 0.92 * densityStrokeAlpha
colour = resolved ink at sa
drawn only if sw > 0.05 / globalScale && sa > 0.03 && r * globalScale > 1.2
```

**Node alpha** (`globalAlpha` around the whole node paint):

| Condition | Alpha |
| --- | --- |
| Search or facet active, node fails | 0.12 |
| Focus active, node is neither focus nor neighbour | 0.26 |
| Outside `visibleSet` (preview-at-SHA) | 0.26 |
| Otherwise | 1.0 |

Two dim levels replace `DIMMED_OPACITY = 0.22`, because "not related to what I am looking at" and
"does not match my filter" are different states and one constant cannot say both.

**External nodes** — a referenced entity with no page yet, *not* a type — are hollow: fill = resolved
ground, ring = resolved ink at 0.45, width `1.0 / globalScale`, dash `[2.5, 2] / globalScale`. No orbit
ring, no glow, never labelled below priority 4, and **no palette entry**. This is the deviation from
`EXTERNAL_COL = '#D7DBE1'`, a fill that measures ≈1.2:1 on the light ground and is simply not visible.
A hollow dashed marker says *placeholder* better than a pale fill does, and cannot fail contrast
because it is drawn in ink.

### 4.7 Edge rendering

`linkCanvasObject` receives a ctx already transformed to world space, so **every constant expressed in
screen pixels is divided by `globalScale`**. Radii and world offsets are not.

**Emphasis and dim**, per edge per frame:

```text
emph = 2 if this is the selected-or-hovered edge
     | 1 if a node is focused and this edge touches it
     | 0
dim  = a node is focused and this edge does not touch it
     | another edge is focused
     | a search/facet is active and either endpoint fails   (which also forces emph = 0)
```

**Alpha composition** — an emphasised edge never fades out under density:

```text
restore    = emph === 2 ? 0.92 : emph === 1 ? 0.72 : 0
mul        = max(densityEdgeAlpha, restore)
finalAlpha = baseAlpha * mul
```

**Geometry:**

```text
ux, uy            = unit vector source → target
rawPx, rawPy      = (-uy, ux)
canonicalForward  = String(source.id) <= String(target.id)
px, py            = canonicalForward ? (rawPx, rawPy) : (-rawPx, -rawPy)
trim              = r + 1.5 / globalScale at each end;  len = max(1, len - trims)
laneKey           = min(a,b) + '\0' + max(a,b)
lane_i            = count > 1 ? i - (count - 1) / 2 : 0
bend  (screen px) = max(12, min(44, 14 + globalScale * 6))
cx, cy            = midpoint + (px, py) * lane * bend / globalScale;  curved iff |lane| > 0.001
```

Canonicalising the perpendicular by id makes both directions of a pair bend the same way. `bend` is
screen-**constant**, so parallel edges stay legibly apart at every zoom. Set force-graph's own
`linkCurvature` accessor to `lane * (bend / globalScale) / len` as well, or the shadow canvas
hit-tests the straight chord and hovering a multi-edge picks the wrong one.

**Five render cases, in this order:**

| # | Case | Treatment |
| --- | --- | --- |
| 1 | **Synthesized** (provenance-derived) | `setLineDash([2,3]/gs)`, `lineWidth 0.8 * densityEdgeWidth / gs`, resolved ink at 0.13 (0.05 when dim). No taper. Painted first, returns early. |
| 2 | **Negative** (`predicate.startsWith('not_')`) | `setLineDash([4,3]/gs)`, `lineWidth 1.1 * densityEdgeWidth / gs`, **resolved `--text-danger`** at 0.10 / 0.34 / 0.46 / 0.62 (dim / base / emph1 / emph2). A dashed stroke, never a taper: the dash *is* the negation signal. |
| 3 | **Symmetric** (non-negative) | Plain stroke, no taper, `lineWidth 0.9 * densityEdgeWidth / gs`. A symmetric relation has no direction to encode. |
| 4 | **Curved** (non-negative, `|lane| > 0`) | Plain quadratic stroke, `lineWidth (emph===2 ? 1.35 : emph===1 ? 1.05 : 0.85) * densityEdgeWidth / gs`. |
| 5 | **Default — the tapered quad** | A filled quadrilateral, not a stroke. `w0 = 0.85`, `w1 = 0.42` (screen-px **half**-widths, so painted 1.70px at the source and 0.84px at the object end), `wm = densityEdgeWidth / gs`. Path: `(sx+px·w0·wm, sy+py·w0·wm) → (ex+px·w1·wm, ey+py·w1·wm) → (ex−px·w1·wm, ey−py·w1·wm) → (sx−px·w0·wm, sy−py·w0·wm) → close → fill`. |

The taper is what makes direction readable without arrowheads, which at 5,000 edges would be
unaffordable.

**Base edge colour** for cases 1, 3, 4 and 5 is the **resolved ink** at:

| dim | base | emph 1 | emph 2 |
| --- | --- | --- | --- |
| 0.07 | 0.18 | 0.32 | 0.46 |

This is the largest DR-10 correction in the file. Today the file hardcodes
`rgba(119, 128, 145, 0.42)` for a resting edge and `rgba(99, 141, 104, 0.75)` for a focused one — a
cool blue-grey against the app's warm neutrals, and a **green** focus state in an app whose
interaction accent is coral. Resolving the ink means the identical alpha ladder gives a dark web on
light and a light web on dark, automatically, in every family.

**Edge labels** — drawn only at `emph === 2`, and only when the trimmed length exceeds 26 screen px:

- position: the curve point at `t = 0.5` (quadratic Bézier if curved, midpoint if straight)
- `font = 500 ${11 / globalScale}px ${resolvedMonoFamily}`, `textAlign center`, `textBaseline middle`
- halo: `shadowColor` = resolved ground at 0.95, `shadowBlur = 4 / globalScale`, and `fillText` called
  **twice** at the same point — the double call doubles the shadow density and is the mechanism, not
  a bug
- fill: resolved `--text-danger` for negatives, resolved ink otherwise
- text: negatives spell the word out — `not_prevents` → `not prevents`
- strike-through on negatives: measure the text, then stroke a `1 / globalScale` line in the danger
  ink from `(mx − tw/2, my)` to `(mx + tw/2, my)`. The dash carries the negation on the wire; the
  strike carries it on the word.

**Edges become selectable.** Wire `onLinkClick` and `onLinkHover`; a click opens the edge inspector
(§3.8). This is new — an edge is decoration today.

### 4.8 Labels

**The pass moves.** Labels cannot live in `nodeCanvasObject`: force-graph paints nodes one at a time,
so a later node overdraws an earlier node's label and collision avoidance has no global view. Draw
them in `onRenderFramePost(ctx, globalScale)`, after every node and edge is down. This is a required
restructuring of `ForceGraphCanvas.tsx`.

**Priority ladder:**

```text
search/facet active: passes ? pr = 2 : skip entirely
otherwise:  focused → 5 · hovered → 4 · hub → 3 · neighbour-of-focus → 2 · globalScale >= 1.55 → 1 · else skip
label alpha: 0.3 when a node is focused and this node is neither the focus nor a neighbour, else 1.0
```

Sort candidates by priority **descending**, then place greedily. The sort is the whole point: a hub's
label beats a leaf's for the same square of canvas.

**Collision avoidance — greedy AABB, first-come-first-served:**

```text
lx = n.x + r + 6 / globalScale ;  ly = n.y            // left-aligned, vertically centred, right of the node
rect = { x: lx - 1/gs, y: ly - (fs/2 + 1)/gs, w: (tw + 2)/gs, h: (fs + 2)/gs }
if rect overlaps ANY already-placed rect → skip this label entirely (do not nudge, do not shrink)
else push the rect and paint
```

Do the test in world space with the screen-constant terms divided by `globalScale`. That is exactly
equivalent to testing in screen space — the canvas transform is a uniform scale plus a translation and
AABB overlap is invariant under both — and it avoids needing the pan offset, which the callback does
not hand you.

**Font:** `${pr >= 4 || n.hub ? 600 : 450} ${fs / globalScale}px ${resolvedFontFamily}`, with
`fs = n.hub ? 12 : 11.5` screen px, `textBaseline: 'middle'`, `textAlign: 'left'`. Weight 450 is
deliberate and not a typo for 400.

> **Note.** 11.5 and 12.5 are fractional sizes off a type scale whose floor is 11px. They are
> tolerated **only** on the canvas, which is a pixel surface with its own optical requirements and is
> already outside the `--text-*` role system; nothing in the DOM may use them. The DOM label sizes in
> this section are `text-supporting` (12) and `text-caps` (11) and nothing smaller.

**Truncation** is by character count, not width: `text.length > 32 ? text.slice(0, 31) + '…' : text`,
applied to `prettyLabel(n.label, n.kind)`.

**Memoisation — the DR-9 fix, in two halves.**

1. `prettyLabel` + truncation are pure functions of `(label, kind)`. Compute once per node into a
   `Map<nodeId, string>`, rebuilt only when `graph` changes. Zero regex per frame.
2. Width still needs `measureText` for the AABB, and the font size changes at every zoom step.
   Exploit linearity: measure each unique `(displayText, weight)` **once** at a fixed 100px font,
   cache `w100`, and compute `tw = w100 * fs / 100`. Advance widths scale linearly with font size, so
   this is exact to within sub-pixel hinting — irrelevant for a collision box. The result is one
   `measureText` per unique label per weight for the lifetime of the graph, instead of one per *word*
   per labelled node per frame.

**Halo:** resolved ground at 0.95, `shadowBlur = 4 / globalScale`, `fillText` twice.
**Fill:** the resolved ink. The `#1c2128` this pattern comes from is a near-black that is correct in
light and invisible in every dark theme — the exact bug `graphStyle.ts` was written to prevent.

**Single line, not three.** `wrapLabel()` is deleted. Three reasons in order: a wrapped label has no
single AABB, so priority-ranked collision avoidance cannot work against it; it is the per-word
`measureText` loop plus a per-character shrink loop that DR-9 identifies as the real per-frame cost;
and at 28 node types the graph is read as a map, and a map wants short consistent labels — the full
title is one click away in the inspector. `prettyLabel` stays (it rescues UUID and hash filenames and
earns its place); `wrapLabel` and its tests go.

### 4.9 Density, level of detail, and culling

```text
edgeDensity     = clamp01((visibleEdges - 260) / 2200)
nodeDensity     = clamp01((visibleNodes - 220) / 1200)
zoomCrowd       = clamp01((0.72 - globalScale) / 0.62)
fade            = clamp01(max(edgeDensity, nodeDensity) * zoomCrowd)
outlineFade     = clamp01(max(nodeDensity, edgeDensity * 0.75) * zoomCrowd)

edgeAlpha       = 1 - 0.82 * fade
edgeWidth       = 1 - 0.34 * fade
nodeStrokeAlpha = 1 - outlineFade
nodeStrokeWidth = outlineFade >= 0.72 ? 0 : 1.1 * (1 - outlineFade / 0.72)
```

Read them as: nothing fades until you are zoomed out past `globalScale 0.72` **and** the viewport
holds more than 220 nodes or 260 edges; at `globalScale ≤ 0.10` the crowding term is fully on. Node
outlines vanish entirely at `outlineFade ≥ 0.72`, where the outlines would be more ink than the nodes.

> **Do not add a "suppress labels when dense" guard.** The bottom label rung fires at
> `globalScale >= 1.55`, and `zoomCrowd` is 0 for any `globalScale >= 0.72`, so `fade` is provably 0
> whenever every node is labelled. Density fade and full labelling are mutually exclusive by
> construction, and the rule would be unreachable.

**Culling is an architectural change, not an optimisation.** `force-graph@1.51.4` does no viewport
culling: `paintNodes` iterates `graphData.nodes.filter(nodeVisibility)` and `paintLinks` iterates
`graphData.links.filter(linkVisibility)`, both over the full arrays, every frame. So:

1. `onRenderFramePre(ctx, globalScale)` — verified to run **before** `tickFrame()` → `paintLinks()` →
   `paintNodes()` — computes the visible world rect from `screen2GraphCoords` on the four canvas
   corners plus 80 screen px of padding, sweeps the node and edge arrays to count what is inside,
   computes the density style, stashes both in a ref, and paints the grid.
2. `nodeVisibility` / `linkVisibility` accessors read that ref and return the in-rect test. Both props
   are in force-graph's `bindBoth` list, so they apply to the shadow (hit-test) canvas as well — which
   is correct: an off-screen node must not be hoverable.
3. Every painter reads the density style from the ref. It is exactly one frame fresh, by construction.

An edge is visible if **either** endpoint is in the rect. Culling is skipped entirely while a node or
edge is focused, so a focused relation is never culled out from under the user.

> **Do not add `nodePointerAreaPaint`.** DR-9 records the measurement: `nodeCanvasObject`,
> `nodeCanvasObjectMode` and `linkCanvasObject` are all in `bindFG` (visible canvas only), and the
> shadow graph is built with `.nodeColor('__indexColor')` and no canvas object, so it paints plain
> arcs. There is no duplicate label pass to remove, and adding a pointer-area painter would
> *introduce* a second per-node painter where none exists.

**Grid.** Dots on a 34-world-unit lattice, radius `0.8 / globalScale`, **resolved ink** at alpha
0.045, skipped entirely when `34 * globalScale < 11` (i.e. `globalScale < 0.3235`). Drawn in
`onRenderFramePre`, iterating only the culled world rect. In dark mode this is a faint *light* dot
field on a dark ground, which is right and which a hardcoded `rgba(20, 24, 31, 0.045)` could never
produce.

### 4.10 Layout

Parameters band by node count: **small ≤ 350 · mid 351–900 · large 901–1500 · huge > 1500**.

| Parameter | small | mid | large | huge |
| --- | --- | --- | --- | --- |
| `forceLink` base distance `L` | 92 | 78 | 82 | 96 |
| `forceLink` base strength | 0.22 | 0.20 | 0.16 | 0.12 |
| `forceManyBody` strength | −135 | −155 | −175 | −200 |
| `forceManyBody` `distanceMax` | 260 | 300 | 340 | 380 |
| `forceCollide` radius pad | +6 | +6 | +5 | +4 |
| `forceCollide` iterations | 2 | 2 | 1 | 1 |
| `forceCollide` strength | 0.64 | 0.64 | 0.62 | 0.62 |
| component pull strength | 0.006 | 0.007 | 0.0035 | 0.0022 |
| anchor pull strength | 0.024 | 0.020 | 0.0075 | 0.0045 |
| `d3VelocityDecay` | 0.18 | 0.20 | 0.24 | 0.28 |
| `d3AlphaDecay` | 0.060 | 0.090 | 0.100 | 0.090 |
| `cooldownTicks` | 240 | 140 | 124 | 36 |
| `cooldownTime` (ms) | 4200 | 2400 | 2600 | 900 |
| `maxVelocity` (world/tick) | 30 | 18 | 15 | 18 |
| warm ticks before first paint | 10 | 2 | 0 | 0 |

The `L` row is non-monotonic on purpose: mid-sized graphs pull tighter because they are dense enough
to sprawl, and huge ones need room back. Copy it; do not "fix" it.

**Per-link accessors, not scalars:**

```text
distance(l) = L * (l.source.hub || l.target.hub ? 1.18 : 1) + (r(l.source) + r(l.target)) * 1.8
strength(l) = baseStrength / sqrt(max(1, (deg(l.source) + deg(l.target)) * 0.5))
```

A hub earns 18% more room so its spokes do not crush it. The degree weighting **replaces** d3's
default `1 / min(deg)`; without it a hub's many links out-pull everything and the graph collapses to a
star.

**Three mechanisms stop a disconnected node flying off, and the first two already exist:**

1. `charge.distanceMax` — already set to 260 with a correct comment. Keep it and scale it by band.
2. `forceX(0)/forceY(0).strength(0.07)` — also already present. These are **subsumed by, not replaced
   by**, the component pull below: an isolated node is its own component, so it gets an anchor. But
   that is only true when component anchors exist. Register the component pull as forces named
   `cx`/`cy` and **keep `x`/`y` registered at 0.07 as the fallback path**, active only when the
   component layout was skipped (restored cached positions, or a graph handed in with pre-set
   coordinates). Deleting them unconditionally looks safe and fails exactly on the reload path.
3. `containNode` — a **hard post-tick clamp**, and the strongest of the three. In `onEngineTick`, for
   every node: let `c` be its component anchor, `d = hypot(n.x − c.x, n.y − c.y)`,
   `lim = max(220, min(4200, c.r * 1.10 + 120))`; if `d > lim`, snap the node onto the limit circle
   and multiply `vx, vy` by 0.25. Also clamp speed to the band's `maxVelocity`. A non-finite `x` or
   `y` is re-seeded at the anchor plus a 6–8 unit jitter from `jitterUnit(id)`. The forces are a
   tendency; this is a guarantee.

**Seeded positions.** force-graph honours pre-set `x`/`y` on the node objects, so the component layout
ports as an initialiser with no new machinery:

```text
components by BFS over the undirected adjacency, sorted by size DESCENDING
denseScale = n > 1500 ? 1.85 : n > 900 ? 1.52 : n > 350 ? 1.22 : 1
root       = max(140, sqrt(n) * 18 * denseScale)
component 0 at the origin, radius max(240, sqrt(|c|) * 28 * denseScale)
the rest on concentric rings: a new ring starts once used >= max(8, ring * 8); slots = max(8, ring * 8)
  angle = (used / slots) * 2π + ring * 0.37
  dist  = max(root, mainR + r + (|c| > 18 ? 112 : 64)) + ring * 64
inside a component:
  hubCount  = min(max(1, ceil(sqrt(|c|) / 2)), 18/14/10/8/5/3 by size band 1200/700/70/32/14)
  threshold = max(2, min(maxDeg, ceil(maxDeg * (|c| > 900 ? 0.12 : 0.34))))
  minHubs   = 12/9/6/1 by 1200/700/250
  hubs on a ring of radius min(c.r * 0.66, 64 + hubs.length * 26 + sqrt(|c|) * 13 * denseScale)
       at angle (i / hubs) * 2π + ci * 0.31
  every other node joins the hub maximising  direct*4 + shared*0.35 + deg(hub)*0.025 − hubIndex*0.01
       (direct = 4 if adjacent to that hub; shared = count of common neighbours)
  members on a golden-angle spiral around their hub:
       a      = i * 2.39996323 + (deg(hub) % 11) * 0.17
       localR = max(52, sqrt(m) * 22 * denseScale + 34)
       rr     = localR * (0.62 + 0.82 * sqrt((i + 0.5) / m)) + (deg ? 0 : 24)
each node's seeded position is also its anchorX/anchorY; its component's centre is its cx/cy
```

A seeded layout is worth the code: it is why the first paint already looks like a graph instead of a
hairball relaxing for two seconds.

**Fit.** After the cooldown, `zoomToFit(500, pad)` with `pad = max(56, min(132, min(W, H) * 0.085))`
and the scale capped at `n > 1500 ? 1.75 : n > 700 ? 2.15 : n > 250 ? 2.55 : 3.1`. Zoom limits:
`min = max(0.0005, min(0.02, fitScale * 0.08))`, `max = 32`.

### 4.11 Resolving structural colour — DR-10

A canvas cannot parse `var(--…)` in `ctx.font` or `ctx.fillStyle`: those strings are parsed against the
canvas, not the cascade, so the assignment is silently dropped and the previous value stays. Reading
the custom property by name does not help either — `getPropertyValue('--text-default')` returns the
*declared* value, which in the dark blocks is itself `var(--color-neutral-100)`. **Only the used value
is safe.**

Extend the existing `useCanvasTheme(ref)` hook — do **not** add a second, parallel piece of theme
plumbing, which is exactly how the ink came to be hardcoded the first time. One hook, one
`MutationObserver` on `document.documentElement` filtered to `['class', 'data-theme']`, one state
object:

| Field | Resolved from |
| --- | --- |
| `fontFamily` | `getComputedStyle(container).fontFamily` |
| `monoFamily` | `getComputedStyle(probeMono).fontFamily` — a 0×0 `<span className="font-mono" aria-hidden>` |
| `ink` | `getComputedStyle(container).color` |
| `ground` | `getComputedStyle(container).backgroundColor` — **requires deleting the inline gradient** (§3.5) |
| `danger` | `getComputedStyle(probeDanger).color` — a 0×0 `<span className="text-text-danger" aria-hidden>` |
| `muted` | `getComputedStyle(probeMuted).color` |
| `border` | `getComputedStyle(probeBorder).borderTopColor` |
| `mode` | `useResolvedTheme()` — selects `GRAPH_PALETTE.light` vs `.dark`; it falls back to `light` outside a provider instead of throwing like `useTheme()` |

The three probe spans are the cost of a correct dark mode. Keep the existing `CANVAS_FONT_FALLBACK`
and `CANVAS_INK_FALLBACK` for jsdom, where `getComputedStyle` reports nothing; add
`GROUND_FALLBACK = '#f4f4f2'` and `DANGER_FALLBACK = '#c4232b'`.

`withAlpha()` already parses `rgb()/rgba()` and hex and returns anything unrecognised **unchanged**
rather than mangling it into an invalid colour a canvas would silently ignore. Every alpha in §4.5,
§4.6, §4.7 and §4.9 goes through it.

**What stays a literal, and why that is not a contradiction.** The 28 type fills and the 7 credibility
hues. They are not tokens, they have no CSS consumer, and they are contrast-audited against a ground
§4.1 proves is constant. Everything drawn in the app's *own* colours — glyphs, outlines, the edge web,
the halo, the grid, the danger red — resolves. That is exactly the line DR-10 draws.

---

## 5. New tokens to generate, and where

### 5.1 The graph palette — generated, never hand-written

**Authoring file (new): `ui/desktop/themes/graph.mjs`.** It holds ~45 numbers — the seven family rows
(anchor hue, chroma, spread, ordered type list), the `RUNG` ladder, the fallback chroma and rungs, and
the seven credibility rows. It holds **no hex values**. It sits beside `themes/*.theme.mjs` but is not
a theme (it has no family id); the generator imports it directly.

**Emitted into `ui/desktop/src/styles/themes.generated.ts`**, at **module scope** — *not* inside
`GENERATED_THEMES[family]`, because §4.1 proves one pair serves all three families:

```ts
export type GraphPalette = {
  types: Record<string, string>;                              // the 28, keyed by OKF type name
  credibility: Record<CredibilityTier | 'retracted', string>; // the 7 ring hues
  fallbackChroma: number;                                     // 0.055
  fallbackRungs: [number, number, number, number];            // [3.90, 4.95, 6.20, 7.90]
  ground: string;                                             // the ground the palette was measured against
};

export const GRAPH_PALETTE: { light: GraphPalette; dark: GraphPalette } = { /* generated */ };
```

The precedent is exact: `syntax`, `terminal`, `codeGround` and `surface` are already emitted into this
file *because* "xterm paints to a canvas and react-syntax-highlighter takes a JS object, so neither
can read a custom property". A 2D canvas is the same category of consumer.

**Do not add anything to `SEMANTIC_TOKENS` in `scripts/lib/theme-contract.mjs`.** These are not
tokens, no CSS consumes them, and adding them would force every family to author 28 values it does not
need. **This design adds zero semantic tokens** — no theme family has to author anything new, which is
the property that keeps a fourth family cheap.

**Consumption.** `graphStyle.ts` exports `typeFill(type, mode)` reading
`GRAPH_PALETTE[mode].types[type] ?? hashedFill(type, mode)`. `mode` comes from `useResolvedTheme()`,
threaded through the same `useCanvasTheme` hook that carries family and ink — one theme-plumbing path,
never two.

**`credColors.ts` is deleted.** `nodeFill()` is what DR-9b replaces; `kindColor` is a five-entry
pre-OKF map with no place in a 28-type world; `retractedColor` moves into
`GRAPH_PALETTE.credibility.retracted`. All 13 of its hardcoded hexes go with it.

**The knowledge-base identity dot.** A base's `manifest.color` currently defaults to `#5a6394` — a
slate-indigo baked into Rust that appears nowhere in the app's palette. This design does **not** change
the daemon. The renderer's shared `KbDot` component instead resolves its fill as:

```text
manifest.color, unless it is the legacy default, in which case
GRAPH_PALETTE[mode] hashed fill of the base id, by the §4.4 rule
```

That gives every base a stable, theme-correct, contrast-audited colour with no migration and no
backend change, and it keeps a colour a user has deliberately set. Changing the daemon's default is a
separate, optional follow-up.

### 5.2 Structural CSS tokens

Two new, one comment amendment. All three live in `main.css`'s hand-authored `:root` structural block
— they are identical across every family under every `data-theme`, so they are declared once and no
theme definition restates them.

| Token | Value | Why it is a token |
| --- | --- | --- |
| `--knowledge-rail-sources` | `300px` | Written three times otherwise: the xl grid column, the lg grid column, and the `<lg` tab panel width. A literal written three times is exactly the drift `--chrome-height` exists to prevent. |
| `--knowledge-rail-detail` | `340px` | Written four times otherwise: the xl column, the lg overlay, the `<lg` `Sheet`, and the change-log `Sheet` — which is deliberately the same object at the same width. |
| `--dock-height` | `36px` **(unchanged)** | Comment amended from "terminal dock strip" to "a pane-docked control strip: the terminal dock, the knowledge facet strip, the knowledge legend dock". The value does not move; the name stops lying. |

Both new names must also be added to the `spacing` list in `src/utils.ts` if they are ever used
through a `w-`/`h-` utility, or tailwind-merge drops them out of their class group and source order
silently decides which paints. They are used here through `grid-cols-[var(…)]` and `w-[var(…)]`, which
are arbitrary values and unaffected — but note the rule so the next edit does not trip it.

### 5.3 Guards

| Guard | Addition |
| --- | --- |
| `scripts/generate-themes.mjs` | The `--background-muted` identity assertion (§4.1), run before emitting `GRAPH_PALETTE`, dying with the stated message. |
| `scripts/lib/theme-tokens.mjs` | `deltaE00(hexA, hexB)` — CIEDE2000, Sharma-Wu-Dalal, on CIELAB D65. **One implementation**, beside `contrast` and `blend`, for the reason that module exists at all. |
| `scripts/check-contrast.mjs` | (a) 56 type-contrast assertions at `min = 3.0`; (b) 14 ring assertions at `min = 3.0`; (c) a 378-pair ΔE00 assertion at `min = 5.0`; (d) **exact-string assertions** that the generated hexes equal the tables in §4.3 and §4.5. |
| `npm run themes -- --check` | Already wired into `lint:check`; fails CI if `GRAPH_PALETTE` is stale relative to `themes/graph.mjs`. |

Assertion (d) is the one that matters most: it makes the palette a **pinned artefact** rather than a
re-derivable one, so a future tweak to the solver cannot silently repaint every graph in the app.

---

## 6. What changes for the user

- **Pick a format when you create a knowledge base.** OKF (permissive, the default) or BioOKF (the
  strict biomedical profile), each with guidance on when to pick it — and a plain warning that the
  choice cannot be changed yet.
- **The graph shows what things are.** Node colour now means the page's *type*, drawn from a
  28-colour palette grouped into seven families; the node's ring carries how well-sourced it is.
  Arbitrary types in an OKF base get a stable colour of their own that never changes between sessions.
- **Links carry meaning.** A tapered line shows which way a claim points; a dashed red line with a
  struck-through label is a negation; a faint dashed line is provenance the system derived rather than
  something you asserted. Hovering a link names its predicate.
- **Click a link.** Edges are selectable and have their own inspector: the claim as a sentence, its
  provenance triplet, its publications, its statistics.
- **The inspector reads the page instead of dumping it.** Frontmatter renders as labelled rows with
  clickable cross-references, and — new — the page's outgoing links grouped by predicate and its
  incoming links as "Referenced by". Clicking either moves you there without losing the canvas.
- **Filter the graph.** By node type, predicate, source or status, plus a live text filter. Filtering
  dims rather than removes, so the map you have learned stays where it was. The legend is now the type
  filter: click a swatch to isolate it.
- **See the base's health.** A lint pill sits beside the base name with error and warning counts, and
  its findings are clickable — each one selects the page or link it is about.
- **Digest shows real progress.** A progress bar, announced to screen readers, instead of a spinner
  and an event count.
- **Everything else looks like the rest of BioRouter.** The base picker is a dropdown, not a
  full-screen dialog; every empty and error state has an icon, a title and a way forward; the primary
  base no longer un-highlights when you point at it; and the gradient behind the graph is gone.

---

## 7. What is NOT changing

Stated so the diff stays bounded. Anything below is out of scope for this spec and must not be
touched in the same change.

**Behaviour and data**

- The visible-set / primary-pointer model in `KnowledgeContext.tsx`, `.active-kb` and
  `.hidden-kb-sessions`, and every rule about primary repair and promotion. Untouched.
- The tier control's behaviour, copy and typed-phrase confirmation (issue #56 DR-18) — visual
  corrections only (§3.11).
- The ingest dispatch logic in `IngestPanel.tsx`: model resolution, the `modelOverride` stamping, the
  `kbPending`/`kbUnavailable` three-state precedence, the pre-flight `checkModel`, the abort path, and
  the auto-clear of succeeded items. Only presentation changes.
- K-04 — the one primary action stays full-opacity with a helper line, never half-lit.
- `useIngestStream`'s SSE contract and terminal-frame handling.
- Every `data-testid` in the section. Eight test files select on them; they survive the restyle.
- The `.brkb` export/import path and its provenance sidecar (DR-21).

**Deferred by DR-22, and therefore absent from this design**

- `kb_migrate_format` and any convert-format UI. The format chooser reserves the surface a future
  conversion would reuse and says plainly that conversion does not exist yet.
- `br_page_id` stamping (DR-3). Identity resolves through the ladder; nothing in the UI shows a page id.
- Attested Computations. No surface.
- A renderer swap (WebGL, sigma, cosmos). DR-9 defers it on measured graph sizes; this design keeps
  `react-force-graph-2d` and ports technique only.
- A plain-OKF bundle export (DR-21). `.brkb` stays the only transfer door.

**Design-system scope held elsewhere**

- `--row-height` is still 40px against the canonical 36px (drift register OPEN-2 / A-03). This design
  consumes `.biorouter-list-row` and inherits whatever the token says; fixing the token is an
  app-wide sweep with three hardcoded call sites and is not owned here.
- The ~148 raw `<button>` elements outside `components/ui/` (OPEN-1). New code in this section uses
  the primitives; the existing app-wide backlog is not this change's.
- `--color-block-teal`, the 11 hand-rolled `.biorouter-modal-surface` users, and the D-08 stage-2
  react-select → Radix swap. All app-wide, all out of scope.
- The `page-transition` class, which is carried for consistency and animates nothing.
- Theme families, neutrals, accents, status hues, shadows and the focus treatment. Zero changes; zero
  new semantic tokens.

---

## 8. Verification

Stage 7's gate is a **real browser**, and most of this document cannot be checked anywhere else.
jsdom has no canvas layout, no WebGL, does not run Tailwind and does not evaluate `:has()`.

| Where | What it checks |
| --- | --- |
| `scripts/check-contrast.mjs` (Node, no browser) | The 56 type-contrast assertions, the 14 ring assertions, the 378-pair ΔE00 assertion, the exact-hex pins, and the `--background-muted` identity across all six scopes. |
| `npm run themes -- --check` | `GRAPH_PALETTE` is not stale relative to `themes/graph.mjs`. |
| Vitest (jsdom) — **pure functions only** | `fnv1a` against the eight vectors in §4.4; the density formulas over a table of `(visibleNodes, visibleEdges, globalScale)`; the AABB overlap predicate; the priority ladder over a focus/hover/hub/neighbour/zoom matrix; the `w100 * fs / 100` width scaling; `prettyLabel`; the facet predicate (OR-within, AND-across); the lint severity counter. Each is deliberately free of React and the DOM, for the reason `utils/messageClamp.ts` records: a threshold you can only exercise by rendering a component is one nobody re-tests. |
| **Browser harness** (new, on the `.artifact-harness` pattern: a plain Vite page mounting the real `ForceGraphCanvas` and the real panes against fixture graphs) | Everything else. |

**Explicitly requires the browser harness — jsdom cannot see any of it:**

- That the resolved `ink` / `ground` / `danger` / `monoFamily` actually change when `data-theme` and
  `.dark` flip. jsdom's `getComputedStyle` returns nothing, so the fallbacks pass the test and the
  real bug ships. This is the exact shape of the trap `styles/composerFocus.test.ts` documents.
- That labels do not overlap at three zoom levels on a 400-node fixture.
- That the orbit ring and its 1px gap survive at `r * globalScale` just above and just below 3.5.
- That the taper reads directionally, and that a `not_` edge reads as negated.
- That the legend chips, facet menus and lint popover render at the right heights with Tailwind
  actually applied.
- Frame-time at 250 / 900 / 2,550 nodes.
- All three theme families, light and dark. Contrast **measured** in the running app, not asserted.

**Fixtures** — one OKF base (arbitrary types, exercising §4.4) and one BioOKF base (all 28 types plus
one off-vocabulary string, negated edges, a synthesized provenance edge, a retracted source), both
emitted from the real Rust graph deriver rather than hand-written JSON, on the
`preview_fixture_dump` pattern.

> **Warning.** Launch the dev GUI with `BIOROUTER_NO_HMR=1` for any interactive check, or a save
> anywhere under `ui/desktop/src/` full-reloads the renderer and destroys the session under test. But
> the same flag sets `watch: { ignored: ['**'] }`, which blinds Tailwind's class scanner, so a
> **newly written** utility class in the legend or facet markup may silently never reach the
> stylesheet. Nothing load-bearing may depend on class-scanning having worked — author such rules in
> `main.css`, as `.biorouter-composer-card:has(textarea:focus)` already is.

---

## Related documentation

- [OKF migration design and decision records](design.md) — DR-9, DR-9b, DR-10, DR-11 and DR-22, which this specification implements.
- [OKF migration stepwise plan](stages.md) — Stage 7, whose gate this document is the input to.
- [OKF migration progress](progress.md) — what has landed.
- [Multi-KB implementation plan](../multi-kb-implementation-plan.md) — the visible-set / primary-pointer model §3.1 and §3.2 must not disturb.
- [Theme system architecture](../../design/theming/theme-system-architecture.md) — the shared-neutrals rule §4.1 depends on, and the generator contract §5.1 extends.
- [Privacy tiers](../../security/privacy-tiers.md) — the design the tier control in §3.11 implements.
