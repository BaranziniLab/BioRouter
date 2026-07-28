# Astryx UI adoption — comprehensive interface revision

> **What this is.** The full design of record for rebuilding Biorouter's interface on the *construction* of Meta's Astryx design system while keeping Biorouter's own palette, theming architecture and calm register. It specifies every foundation, every element, every composition, and the order in which they land.
> **Status:** Proposed — awaiting review. Nothing in this document is implemented.
> **Audience:** maintainers reviewing the direction; developers and agents who will execute it.

Biorouter's interface is coherent in intent and inconsistent in fact. Three parallel audits of the shipped app found five row-title treatments, four error dialects, five spinner constructions, six modal shells, three durations for one hover, 140 hand-written `text-[11px]` classes, and a toast system that carries a 400-pixel scrollable report inside a surface designed for one line. None of this is decay from a bad plan — it is the residue of a design system whose *rules* were written down (`design.md`) but whose *parts* were never built, so every new view re-derived them.

[Astryx](https://astryx.atmeta.com/) is the corrective. It is an open-source React/StyleX design system whose value here is not its colours — its default light body is `#F8F4ED`, a warm parchment cream within a hair of Biorouter's own, which is a pleasant accident and nothing more — but its **discipline**: one control height, one easing curve, one state model, one overlay recipe, one radius ladder, everything expressed as theme tokens so a component's construction never encodes a look. Adopting that discipline is how Biorouter stops re-deriving.

This document proposes what to take, what to keep, and what to refuse. It is organised as: the contract that constrains everything (§1), foundations (§2), elements (§3), compositions (§4), motion (§5), what already agrees (§6), the deletion list (§7), execution (§8), and the decisions you need to settle (§9).

> **Rendered companion:** [`astryx-design-showcase.html`](astryx-design-showcase.html) — open it in a browser. Every control specified below is live there, built from the proposed tokens, with switches for theme family and a Today/Proposed flip that turns the whole page into the diff. It is the faster way to review this proposal; this document is the argument behind it.

---

## 1. The contract

Five things are **not** on the table. Every proposal below is written to hold them.

1. **The palette stays.** Parchment's warm neutrals, the coral accent, Alma Mater's UCSF navy and teal, Roche Limit's orange — unchanged hex for hex. Astryx contributes *roles* and *formulas*, never values.
2. **The theming architecture stays.** One authored `.theme.mjs` per family, `npm run themes` generating the CSS/TS/HTML regions, the light-before-dark ordering invariant, per-family `terminalGround`, the generator that refuses to emit on a contrast failure, and the 252 contrast assertions. The audit called this the strongest part of the system; the redesign treats it as fixed infrastructure and *extends* what a family may declare.
3. **The calm register stays.** No bounce, no glow, no gradient, no lift-on-hover, no colour as decoration. Astryx agrees with this almost everywhere, which is why it is a viable donor.
4. **Corners stay square-ish.** Astryx makes buttons, badges and avatars full pills (`--radius-full`). Biorouter does not. We adopt Astryx's radius *ladder* — a semantic scale with a container-greater-than-element ratio and concentric nesting — and map it onto squarer values. Pills survive only on status dots, the switch knob, and avatars.
5. **The chat surface's information density stays.** The 760px column, the tool-call-as-a-line rule (D-17), rows-not-cards (P2), and mono-for-data (P6) are Biorouter's identity as an instrument. Astryx's roomier marketing rhythm is not imported into the transcript.

One thing **is** explicitly on the table, at your request: the typeface (§2.1), and the sidebar's vertical budget (§4.1).

---

## 2. Foundations

### 2.1 Typeface

**Today.** `--font-sans: ui-sans-serif, -apple-system, …` — the OS default on every platform, chosen by D-06 for zero-download determinism. The one bundled webfont is Inter, embedded as a 64KB data-URI for the wordmark only. The result is that Biorouter has no typographic voice: it renders as San Francisco on macOS, Segoe on Windows, and whatever a lab Linux box resolves.

**Astryx.** One family for everything — `Figtree` (a geometric humanist sans, variable, open-licensed) for both body and headings, `SF Mono`-first for code. Weights are used narrowly: 400 body, 500 for control labels, 600 for headings. Display sizes get *lighter*, not bolder — the marketing H1 is 42px at weight 400.

**Proposal.** Bundle Figtree as a latin-subset variable woff2 and make it `--font-sans`, exactly as Inter is bundled today (same mechanism, same licence class, ~30–40KB subset). Keep the mono stack unchanged — it is already byte-shared between code blocks and the terminal and is one of the few places with zero drift. Adopt the three-role structure as tokens (`--font-body`, `--font-heading` aliasing body by default, `--font-code`) so a theme family can later change its heading face without touching a component.

This is decision **A-01** — the single most visible change in the document, and the one you asked to align with "what is advertised". A native stack remains a legitimate answer if determinism outranks voice.

### 2.2 Type scale — tokenized, geometric, on a 4px grid

**Today.** The canonical scale lives in `design.md` and *has no tokens*. Tailwind's defaults stand in, the two most characteristic steps (13px secondary, 11px caps label) have no utility at all, and the result is 140 × `text-[11px]`, 18 × `text-[13px]`, 16 × `text-[10px]`, plus one-off `text-[12.5px]`, `text-[9px]`, `text-[8px]`, and a `font-size: 13px` literal in the tab strip. Four different tracking values implement one documented `+0.08em`.

**Astryx.** Base 14px, ratio 1.2, every line-height snapped to the 4px grid, and **semantic** styles on top of the raw sizes: `body` 14/20/400, `label` 14/20/**500** (all control text), `supporting` 12/20/400 (timestamps, metadata, captions), `heading-1…6`, `display-1…3`, `code` 14/20 — mono at the *same* size as body, never smaller.

**Proposal.** Tokenize the scale in `@theme` so every step has a utility, and re-express it as Astryx-style semantic roles:

| Role | Size / line-height | Weight | Tracking | Replaces |
|---|---|---|---|---|
| `display` | 30 / 36 | 400 | −0.01em | metric readouts |
| `title` | 24 / 32 | 600 | −0.01em | page titles (`text-2xl`) |
| `heading` | 20 / 28 | 600 | 0 | section headings |
| `subheading` | 17 / 24 | 600 | 0 | card and panel titles (`text-lg`) |
| `body` | 14 / 20 | 400 | 0 | `text-sm` |
| `label` | 14 / 20 | 500 | 0 | every control's text |
| `secondary` | 13 / 18 | 400 | 0 | the 18 × `text-[13px]` |
| `supporting` | 12 / 16 | 400 | 0 | `text-xs` metadata |
| `caps` | 11 / 16 | 500 | +0.08em | the 140 × `text-[11px]` |
| `code` | 13 / 20 | 400, mono | 0 | unchanged (terminal parity) |

Everything below 11px is deleted, not migrated. Body line-height moves from the documented-but-unenforced 21 to 20 so it lands on the grid, which is also what `text-sm` already renders — the doc changes, not the pixels. **A-02.**

### 2.3 Density — one control ladder, one row rhythm

**Astryx.** Three control heights and nothing else: **28 / 32 / 36px**, with 32 as the default for buttons, inputs, selects, icon buttons, nav rows, tabs and menu items alike. Font size does *not* change with height — only the box. Heights are composed (20px line + symmetric padding), not fixed, so a row that gains an icon keeps its rhythm. App header is 48px. Dense data rows 32–40px. Rails 240–300px, secondary rails ~232px, reading columns ~750px.

**Today.** Biorouter runs four bar heights (52 / 52 / 52 / 40), four item heights (34 tabs / 32 sidebar / 40 content rows / 28 dock tabs), three icon-label gaps (8 / 7 / 6), two label sizes (14 / 13), Button sizes at 24/32/36/40 that row actions bypass wholesale for an off-scale 28px, and 14px icons inside 28px buttons in Scheduler where every other view uses 16px.

**Proposal.**

| Slot | Value | Notes |
|---|---|---|
| Control sm / **md** / lg | 28 / **32** / 36px | md is the default everywhere; lg only for a view's single dominant action |
| Icon button | 32×32 with 16px icon; 24×24 compact tier | one size for row actions — the 28px zoo ends |
| Chrome bar (titlebar band, chat header, artifact header) | **44px** | one `--chrome-height` token, three declarations collapse to it |
| Terminal dock strip | 36px | one step down, same tab language |
| Tab (chat, artifact, dock) | 32px | pulled from 34; matches every other control |
| Sidebar / nav / menu / recents row | 32px | already there — now blessed as the rail rhythm |
| Content list row | **36px** | down from 40; Astryx's dense-data band, still comfortable |
| Table row | 36px | one value with the list row |
| Icon-label gap | 8px | everywhere; retires 7px and 6px |
| Sidebar width | 240px | unchanged (Astryx's 260 is a docs rail, not an app rail) |
| Reading column | 760px chat · 1120px pages | unchanged; the 896px replay fork is deleted |

**A-03** is the content-row change: 40 → 36px contradicts D-12·B, which fixed 40px as "one rhythm, no compact mode". The rhythm survives — it just tightens by one grid step, and the app gains ~10% more rows per screen.

### 2.4 Radius — a semantic ladder, mapped square

**Astryx.** Radius is semantic, not per-size: `inner` (nested) → `element` (controls) → `container` (cards, dialogs, popovers) → `page` → `full`. Containers are always larger than the elements inside them, and nesting follows `inner = outer − padding`, which is what makes nested corners look deliberate rather than accidental. Radius is a *theme token* with a 0–2 multiplier — the same component renders at 10px under one theme and 8px under another with no code change.

**Today.** Five tokens where `--radius-lg` is a silent alias of `--radius-md`, so 234 call sites share 8px under two names and you cannot grep which were meant to be 12px cards. The doc's table (`lg = 12`, `xl = 16`) disagrees with the shipped CSS (`lg = 8`, `xl = 12`, `2xl = 16`). Off-scale stragglers: 3px legend chips, 5px heat cells, a hardcoded `border-radius: 16px` in the modal surface.

**Proposal.** Four steps, semantically named, mapped to Biorouter's squarer taste, plus `full` restricted to three uses:

| Token | Value | Applies to |
|---|---|---|
| `--radius-inner` | 4px | inline code, chips, checkbox, swatches, elements nested inside a control |
| `--radius-element` | 8px | buttons, inputs, selects, tabs, list rows, menu items, icon buttons |
| `--radius-container` | 12px | cards, panels, dialogs, popovers, toasts, code blocks, the composer |
| `--radius-surface` | 16px | reserved: the artifact/preview sheet only |
| `--radius-full` | 9999px | status dots, the switch knob, avatars — **nothing else** |

The composer and modals move 16 → 12px, which is the single change that most makes the app read as one system rather than two. `--radius-md`/`lg`/`xl`/`2xl` become deprecated aliases for one release, then die. Nesting rule becomes normative: *an element inside a container uses the next step down; when a container's padding is `p`, its inner radius is `outer − p`.* **A-04.**

### 2.5 Colour roles and interaction tints

**Astryx.** Semantic surface layering (`body → surface/card → popover → inverted`), text and icon tokens paired 1:1, and — the part worth stealing outright — **interaction expressed as two alpha tints, not colours**: `--overlay-hover` at ~5% ink and `--overlay-pressed` at ~10%, composited over whatever surface the control sits on via a `background-image` gradient. Selection is a third achromatic wash. Status uses translucent fills at ~22% with tinted ink, never solid, with exactly one exception (the persistent error toast). Categorical hues follow one formula — hue at 24% alpha for the fill, saturated hue for the text — instead of hand-picked pairs.

**Today.** Every family hand-authors `--sidebar-hover` and `--sidebar-active` as literal hexes with no documented relationship to the surface they sit on; `.br-tab:hover` uses a bespoke `color-mix(--background-default 62%)`; list rows use 42% and settings rows 38% of the same mix for the identical gesture; and the audit flagged exactly this class of hand-authored derived value as the one that historically drifts.

**Proposal.** Add five interaction tokens per family, *derived* by the generator from the family's ink, and route every hover/press/selection through them:

```
--overlay-hover     ink @  5%     hover on any surface
--overlay-pressed   ink @ 10%     :active, with scale(.98)
--overlay-selected  ink @ 14%     selected rows, active nav, active tab fill
--accent-muted      accent @ 8%   focus rings (inset), selected accents
--border-emphasized derived       inputs and other interactive borders
```

The hand-authored `--sidebar-hover`/`-active` hexes are deleted and become the overlay tints over `--sidebar`. Status washes are regenerated from one formula at 22%. This shrinks what a new theme family must author *and* removes the drift class in one move. **A-05.**

### 2.6 Selection — wash and weight, with Biorouter's rail kept

**Astryx.** Active is `--overlay-selected` behind the whole row plus a 400 → 500 weight bump. No left bar, no accent colour: "selection is achromatic so theme accents stay reserved for CTAs." Its one indicator is a 2px rounded pill that *moves* (the tab underline sized to the label, the TOC thumb sliding on a track).

**Biorouter today** marks active with an accent rail (sidebar), an accent underline (settings tabs), an accent stripe (chat tabs), an accent-tinted glyph (recents), and an accent-filled Button variant used to encode *state* in workflow rows — five accent dialects, one of which (the variant flip) reads as a call to action when it means "scheduled".

**Proposal.** Adopt wash + weight as the base layer of every selected state, and keep exactly one accent mark per surface class — the 2px straight rail (sidebar), the 2px underline (nav tabs), the 2px bottom stripe (document tabs). Delete accent-tinted glyphs, delete state-encoding-by-variant-flip (state becomes a chip). The rails were straightened in this worktree already; this proposal keeps that geometry and adds the wash underneath, so selection reads even where colour is not the signal. **A-06.**

### 2.7 Elevation

**Astryx.** Three shadow levels, each a tight contact shadow plus a soft ambient, both mode-aware; **flat cards** (elevation is reserved for things that genuinely float); a 1px *inset ring* instead of a border on floating surfaces, whose alpha grows with elevation; and 2px inset rings for selection and validation states, which compose better than outlines because they are inside the box.

**Proposal.** Keep Biorouter's four-token elevation policy and its per-family tinting, and change three things: floating surfaces gain the 1px inset ring so their edges stay crisp in dark families; cards lose all shadow and hover by *border brightening* instead; and the two hardcoded tab-pill shadows — which currently paint Parchment's warm ink under Alma Mater and Roche Limit — become a fifth micro-elevation token. The modal surface's 44%-diluted hairline goes to full strength per D-18. **A-07.**

---

## 3. Elements

Each element below states the target spec. Where Biorouter already agrees, §6 says so instead of repeating it.

### 3.1 Buttons

Four variants — primary (coral fill), secondary (ink wash at 6%), ghost, destructive (**tinted** danger fill with deep danger ink, not a solid red block). Heights 28/32/36 with 32 default; padding `8px 12px`; radius `--radius-element`; text 14/500; no border on filled variants; gap 8px to a 16px icon.

The state stack is variant-agnostic and shared by every control in the system: hover = `--overlay-hover` gradient over the existing fill; press = `--overlay-pressed` + `scale(.98)`; disabled = `opacity .5; cursor: not-allowed` with **no colour change**; focus-visible = instant 2px accent outline at 2px offset. Solid fills brighten by `color-mix(accent, tint 15%)` rather than taking an overlay.

This deletes: the `shape: 'pill'`/`'round'` misnomer (both already render 8px — the prop dies), `destructive`'s unique `hover:opacity-90` (which lets the modal scrim ghost through), and the 103 raw `<button>` elements' excuse for existing.

### 3.2 Inputs, textareas, selects

**Chrome moves to the wrapper.** The container div carries height, padding, border and radius; the native input inside is naked. This is what makes leading icons, trailing buttons, chips-inside-input and input groups all work without per-case CSS.

Container: 32px (md), padding `4px 8px`, radius `--radius-element`, 1px `--border-emphasized`, 8px internal gap, 16px icons. Hover (unfocused): `box-shadow: inset 0 0 0 2px color-mix(--border-emphasized 30%, transparent)` — a whisper, no colour jump. Focus-within: border → accent plus `inset 0 0 0 2px var(--accent-muted)`. An **inset** halo, so nothing shifts layout and D-15's "focus is a surface shift, never a ring" is honoured more literally than today's outline.

Field anatomy: label 14/500 muted → 4px gap → control → 12px status line. A 56px field, adopted as the Settings and forms rhythm.

Select trigger is the input chrome exactly; its popover is `--radius-container` with 4px padding and 32px option rows at `--radius-element` — the concentric rule in miniature.

### 3.3 Checkbox, radio, switch

22×22 visual box (radius `--radius-inner`) with a 24×24 invisible hit target and an 8px label gap; radio identical with a 10px inner dot; switch is a 40×24 track with a thumb that **grows 16 → 20px as it slides** — the one micro-delight Astryx allows itself, cheap and calm, and worth taking.

### 3.4 Chips, badges, status dots

Two tiers, and the distinction is enforced: a **chip** is squared (24px, radius `--radius-inner`, 12/500, optional 16px remove button) and carries a category or a filter; a **badge** is a 20px pill and carries status. Hue chips are generated from one formula — hue at 24% alpha behind saturated hue ink — instead of the four hand-rolled small-label recipes in the app today (`rounded` 4px chips, `rounded-md` pills, `Badge`, `BuiltInBadge`).

### 3.5 Cards

Flat: surface fill, 1px hairline, `--radius-container`, 12px padding, **no shadow**. Title 14/600, 8px gap, body in muted ink. Clickable cards hover by brightening the border only. Selectable cards flip the border to accent and add a 2px inset accent ring — zero layout shift.

Astryx's own guidance is the part to internalise: *"Cards are not the default layout tool — most content groups don't need a container at all."* Biorouter mostly agrees already; ScheduleDetailView does not (§4.5).

### 3.6 Dialogs

One shell. Three widths and one full-screen: **S 400** (confirm) · **M 480** (form) · **L 640** (palette, detail) · **full** (editors, radius 0). This replaces the fifteen distinct `max-w` values in use.

Surface `--radius-container`, no border, `--elev-modal` + inset ring. Sections own their padding (16px): header ~44px with title 20/600 and an optional 12px subtitle, body scrolling internally, footer ~56px right-aligned with 8px gap. Close is a 32px ghost icon button inset 8px. Scrim ~60% ink with a 2px blur. Enter: fade + 16px rise + `scale(.95)` at medium-max; **exit is instant** — an element you are leaving should not delay you.

Astryx's `purpose` axis is worth taking wholesale because Biorouter has the bug it prevents: `info` dismisses on backdrop click, `form` does not (protecting typed input), `required` must be answered.

The six hand-rolled modal shells are deleted into this one. Confirmations get one recipe: title is a verb phrase (never "Yes/No"), destructive confirm is the tinted danger variant, width S.

### 3.7 Toasts — sized to Biorouter's real content

This is the element where copying Astryx directly would break the app, so the census matters. Astryx's toast is one line, 400px, with an optional action and a close — its docs say "keep messages short: only a few words," and anything larger is a banner. Biorouter's toasts today carry:

- ~120 two-line confirmations (title + one sentence), the longest routine one ~95 characters — three lines at 420px;
- titles that are raw `provider/model` identifiers, 40+ characters;
- messages containing unbounded absolute file paths and stringified exceptions;
- sticky errors with a **two-button action row** ("Ask biorouter", "Copy error");
- and one maximal case: the grouped extension-load toast — a collapsible, scrollable, ~400px-tall panel with per-extension rows, each carrying *its own* pair of action buttons.

**Proposal — a three-tier notification model.**

| Tier | Content ceiling | Spec |
|---|---|---|
| **Toast** | title + ≤3 wrapped lines + ≤2 actions | 380–420px, `--radius-container`, 16px padding, status chip 28px, title 13/600, body 13/400 muted, actions in a `label`-size ghost row, close 20px in a reserved gutter. Auto-dismiss 5s; errors persist. |
| **Banner** (in-place) | title + description + one action | full-bleed `section` variant for app-level notices, `card` variant inline; translucent status wash at 22% with tinted ink, 20px icon, 12/16 padding. |
| **Notification centre** | anything with a list, a disclosure, or per-item actions | the grouped extension report moves here. A toast may *link* to it. |

Toasts stack top-right (Biorouter's corner — the composer and artifact panel own the bottom-right), 12px gap, newest nearest the corner, with **dedup by key as policy** rather than the single exception that exists today. `closeOnClick` is dropped whenever a toast carries actions — today an error toast dies if you click 2px off its button. The layer moves to `--z-toast`, which the vendor default currently overrides at 9999. **A-08** is the notification-centre split.

### 3.8 Tooltips, popovers, menus

One floating-surface recipe: popover-tier surface (one step off the app ground), `--radius-container`, no border, elevation + inset ring, 4px gap from the trigger, entry = fade + 8px directional slide + `scale(.95)` at fast-max with the origin at the anchor edge.

Menus: 4px container padding, 2px between items, 32px items at `--radius-element` with `6px 8px` padding and an 8px icon gap, hover = `--overlay-hover`, min-width = trigger. Biorouter's `DROPDOWN_ROW_CLASS_NAME` is already almost exactly this and becomes the shared row for Select too, ending the 13px-vs-14px menu-row fork.

Tooltips: inverse surface, 14/20 text, `4px 8px` padding, 4px offset, 165ms rise-and-fade, instant on keyboard focus, ~310ms hover-intent delay. The heatmap's bespoke light card is retired in favour of a *rich popover* (it is a data readout, not a tooltip) and leaves the toast layer it borrowed.

### 3.9 Progress, spinners, skeletons, empty states

- **Progress:** one primitive. 8px track, pill, accent fill, `width` transition at 250ms; indeterminate sweeps `−100% → 250%` on a 1.5s loop; `role="progressbar"` always. Replaces four vocabularies (16px/8px/8px/dot-matrix) and three fill techniques.
- **Spinner:** one primitive, `currentColor`, sizes 14/20/24, rotating at ~525ms/turn linear. Replaces five constructions across ~29 files, including the theme-blind `border-white` ones.
- **Skeleton:** solid fill (no shimmer gradient), content-shaped, opacity pulse 0.25↔1 — and the behaviour worth stealing: a **1s delay before showing**, so fast loads never flash. Policy: skeletons for lists and panels, spinner for buttons and inline actions. Replaces five loading dialects.
- **Empty state:** 24px icon in plain ink, 17/600 title, 14/20 muted line, one *subtle* (never primary) 32px action, 16px stack gap, `32px 24px` padding. The quietest thing on the screen. Adopted by the three views that hand-roll it today.

### 3.10 Tables and lists

Transparent — no card chrome, no outer border, no zebra. Headers are **muted 14/600** (quieter than the body they label), cells 14/400 ink, `8px 12px` padding, 36px rows, and the only lines are 1px bottom hairlines with the last row's omitted. Row hover is `--overlay-hover` at 125ms.

List items: 8px padding, `--radius-element`, 2px gaps, two-line construction (14/400 primary over 12/400 supporting).

**One row spec, enforced:** title `14/500`, metadata `supporting` in muted ink, at most **three visible actions** at 32px with 16px icons, everything else behind a `⋯` overflow. Destructive actions live only in the overflow. This ends: five row-title treatments, the 28-vs-32px delete-button fork, Scheduler's 14px icons, seven-action workflow rows, and primary-filled Launch/Play buttons that are invisible until hover.

---

## 4. Composition

### 4.1 The app shell — a more compact sidebar

The audit measured **~462px of fixed chrome above the first recent chat**: a 52px empty band, 8px padding, a 32px brand row, a 32px "MENU" header, nine 32px nav rows, two 18px divider blocks and a 32px Recents header. On a 720p window more than half the rail is spent before any history shows.

**Proposal.**

1. **Fold the wordmark into the chrome band**, right of the traffic lights, and drop the band to 44px. Recovers ~48px and removes the app's only entirely empty 52px strip.
2. **Delete the "MENU" header** (32px labelling something self-evident) and keep only the Recents header, which is doing real work.
3. **Group the nav.** Primary verbs (New Session, Home) stay top. "Applications" and "Apps" are adjacent near-synonyms and merge. Scheduler and Workflows — low-frequency destinations currently sitting above the daily surface — move into a collapsible "More" group, styled as an Astryx section row: a 32px row identical to an item with a trailing 14px chevron, no uppercase mini-label.
4. **Halve the divider blocks** and delete the upper one; the section rows now do the zoning.
5. **Recents rows** keep 32px, gain a trailing status dot for running sessions (replacing the 16px ring-and-glow, which stays in the chat where it belongs), and lose the one-off `color-mix` well surface and its 12px radius — the list sits directly on the rail like every other list in the app.
6. Tooltip-on-every-row gets a hover-intent delay so scrubbing the list stops flashing 224px cards.

Net: **~110–130px** of the rail returned to content, with the item count down from nine to seven. The sidebar keeps 240px of width.

The same 44px band is shared by the chat header and the artifact strip from one `--chrome-height` token — today it is 52px written three ways with two different hairline colours and three different grounds meeting at a seam the code comments claim is continuous.

### 4.2 Page headers

One recipe, everywhere: a full-bleed hairline; title `24/600`; optional description in `secondary` muted; **actions right-aligned on the title row**, primary at 36px and everything else at 32px. This replaces the three placements in use (inline-with-title, a button row below the description, none) and the column-capped hairline in Settings and Knowledge. Settings additionally loses its double rule (header border + tab-strip border a few pixels apart).

Breadcrumbs — 28px, 14/400, `/` separators, muted-vs-ink only — are added to drill-in surfaces (Knowledge page, schedule detail, session replay) where the app currently offers only a Back button.

### 4.3 Tabs

Two roles, never mixed. **Underline tabs** for view switching: 32px, 2px indicator sized to the *label* (not the tab), weight 400 → 600, colour muted → ink, 125ms, with the hidden-bold-duplicate trick so bolding never changes width. **Squared chips** for filtering, `--overlay-selected` when active. Document tabs (chat, artifact, dock) keep their pill-plus-stripe language, dropped to 32px and unified on an 8px icon gap.

### 4.4 The chat surface

Keep: the 760px column, the 16px row rhythm, quiet tool-call lines, the code-block card, the working ring-and-glow (one motif, one owner — the duplicate `TurnActivityIndicator`/`LoadingBioRouter` pair collapses).

Change:

- **Metadata drops to `supporting`** (12px). Timestamps are currently 14px — the same size as message content — repeated under every message; this is the surface's single biggest density cost. One `MessageMeta` primitive replaces three hand-copied animation stacks.
- **One muted ink.** `text-text-default/70` and `text-text-muted` are used interchangeably across the composer toolbar, panel chrome and gauges; the semantic token wins.
- **Composer**: radius 16 → 12, one padding grid (12px shell, 8px toolbar, 12px attachments — the current `p-4` attachments strip is the heaviest thing in the card), the dead focus ternary deleted, Send becomes a real primary Button instead of an `outline` repainted by className override, and the FLIP animation moves onto the motion tokens.
- **Composer toolbar chips**: one recipe — 16px icons (retiring the 18px outliers), ≥6px horizontal padding (today `px-0.5` gives ~20px hit zones), one metadata size in their popovers (today: 10, 11, 12 and 13px).
- **Tool-call interiors** get one inset grid, replacing five recipes, and arguments render in mono per the app's own mono-for-data rule. The disclosure chevron becomes visible at rest — today it appears on hover, so first-run users get no signal that rows expand.
- **Landing state** adopts Astryx's AI-chat template: the greeting, the composer card, and a row of ghost suggestion chips beneath it — optional, and the one place a "what can I ask?" affordance genuinely belongs.
- **User bubble** and the 896px replay column both move onto the standard radius and the 760px measure.

### 4.5 Primary views

Every list view converges on: standard header (§4.2) → optional filter band → hairline list with 36px rows → shared empty state → shared skeleton. This closes seventeen of the eighteen items in the cross-view ledger, including the four error dialects and the five loading dialects.

Two views need structural work rather than alignment:

- **ScheduleDetailView** predates the flat redesign wholesale: no `MainPanelLayout`, `h-screen` sizing (the exact anti-pattern the layout's own comment warns breaks embedded panes), a muted ground, `label:` colon sentences instead of definition rows, and per-semantic tinted button variants that exist nowhere else. It is rebuilt on the standard scaffold with Astryx's **definition-row** pattern: 36px hairline rows, muted label and value left, a text action right.
- **Settings** adopts Astryx's two-cell section grid — description left, controls right, 40px gap — which gives long tabs the scannable hierarchy their flat 11px labels currently deny them, and a sticky anchor rail (232px, one 2px thumb sliding on a 10% track at 130ms) instead of relying on `scrollIntoView` deep links.

**Knowledge** keeps its framed-panel workspace — it is a genuinely different kind of surface — but its panels move to `--radius-container`, its segmented switcher becomes the standard chip row, and its 360px fixed column becomes a `minmax` so the graph stops starving at narrow widths.

---

## 5. Motion

**Astryx** runs the whole system on **one easing** — `cubic-bezier(0.24, 1, 0.4, 1)`, a strong decelerate with no overshoot — and nine duration tokens (three tiers × min/default/max). Durations are themeable; the curve is not. The governing rule is legible in the data: *the bigger the element, the slower the motion* — menu item 95ms, button 125–175ms, popover 165ms, dialog 400–550ms. Entrances are staged; **exits are near-instant**. Nothing bounces.

**Biorouter today** has a correct three-token scale that *loses by default*: Tailwind's own 150ms/`(0.4,0,0.2,1)` was never re-pointed at the tokens, so every bare `transition-colors` runs a fourth duration and a fourth curve. The count is 49 tokened sites against ~74 literal durations and 150+ bare utilities. `--ease-in` is defined and never used; `--ease-g2` has zero consumers; `--ease-spring` directly contradicts `design.md`'s "no spring, no overshoot" while legislating its own exemption in a CSS comment.

**Proposal.**

1. **Re-point Tailwind's defaults** at the tokens in `@theme`. One change, ~120 per-site annotations deleted, and every future `transition-colors` is on-spec by default. This is the cheapest high-value fix in the document.
2. **One easing token** with Astryx's curve; `--ease-in` and `--ease-g2` are deleted. `--ease-spring` survives *only* for the tab-drag choreography — which the audit called the best-reasoned motion in the app — and `design.md` is amended to say so rather than leaving code and spec each claiming authority.
3. **Nine durations** (fast 95/125/175, medium 250/300/450, slow 525 for ambient loops), declared per family so Parchment can be calmer than a future lab-focused family.
4. **One overlay entry recipe:** fade + a small move toward the final position (8px for menus and tooltips, 16px for dialogs) + `scale(.95)`, `fill-mode: backwards`. Exits fade fast or not at all.
5. **Route changes stay instant** — and the eleven `page-transition` classes that decorate views while being defined in no stylesheet, plus the two dead `view-transition` markers, are deleted rather than implemented.
6. **Reduced motion means instant state change, not no state change** — with one nuance worth copying: spinners slow to ~3s/turn rather than freezing, so necessary feedback survives.
7. Six unanimated abrupt changes get the treatment they were specified for: the sidebar active rail (which snaps today), tool-call expand/collapse, react-select menus, `BaseModal`'s instant mount, the heatmap readout, and `Dot`'s never-implemented live pulse.

---

## 6. What Biorouter already gets right

Not everything needs changing, and the audits were explicit about it. These stand as-is and become the reference standard the rest is held to:

- **The theming pipeline** — one file per family, generate-and-refuse-on-contrast-failure, discovered families, ordered blocks, per-family terminal ground, 252 assertions. Its measured cost for a fourth family is one file, which means a redesigned palette can be trialled as a throwaway family before Parchment is touched.
- **The tab-drag choreography** — ghost lift, source dimmed in flow, deliberately unanimated drop tint, static insertion caret. Every value has a recorded reason.
- **The dropdown row recipe** (`DROPDOWN_ROW_CLASS_NAME`) — one exported class composed by item, checkbox, radio and sub-trigger. This is exactly the pattern §3 generalises.
- **The code/terminal palette** — one generated palette feeding chat markdown, the artifact panel and xterm so they cannot drift, at 13/20 in both.
- **The tool-call line** (D-17) and the collapsed-row label grammar.
- **`NotificationSurface`** — one geometry, two densities, a reserved close gutter. The toast spec in §3.7 is an extension of it, not a replacement.
- **The usage heatmap's fit-to-box engine** — server-side quartile shading, keyboard-focusable cells, and the measure-and-fit sizing shipped in this worktree.
- **Focus as a surface shift** (D-15), which Astryx shows is compatible with an instant keyboard-only outline rather than opposed to it.

---

## 7. The deletion list

A redesign that only adds is a redesign that fails. These are removed as part of the work, each verified dead or duplicative by the audits:

**Tokens:** `--background-card` (byte-identical to `--background-default` in all six family/mode combinations), `--sidebar-accent(-foreground)`, `--sidebar-ring`, `--sidebar-primary(-foreground)` (zero consumers, and Roche's value would fail AA if anything ever used it), `--border-default`, `--color-block-teal`/`-orange` (a "teal" holding a coral, 8 live call sites), `--ease-g2`, `--ease-in`, `--font-serif`, `--shadow-modal-chrome-top/bottom`.

**CSS:** `shimmer` + `.animate-shimmer`, `breathe-pulse`, `.sidebar-item` and its three `!important` pointer-events guards, `.biorouter-diagnostics-*`, the duplicate `.biorouter-settings-row` (it differs from `.biorouter-list-row` by four percentage points of hover mix).

**Classes in TSX:** `page-transition` × 11, `biorouter-composer-view-transition` × 2, `animate-[wind_…]` × 2 (the keyframes were deleted), the dead `shadow-sm`/`shadow-md` in `DocumentPreview`.

**Components:** ~300 lines of unreachable shadcn sidebar scaffolding, the six hand-rolled modal shells, the legacy `SessionViewComponents` renderer with its pre-token `bg-bgSecondary`, two `window.confirm` calls, the `shape` prop on Button, and the duplicate `tailwindcss-animate` dependency.

---

## 8. Execution

Ten phases, each a commit on this worktree, each independently revertible, each gated on the full frontend suite (1,654 tests), `lint:check`, the theme `--check`, and the 252 contrast assertions. Phases 1–3 are pure infrastructure and change no pixels by themselves — they are what make 4–10 small.

| # | Phase | What lands | Risk |
|---|---|---|---|
| 1 | **Motion root** | Re-point Tailwind defaults at tokens; one easing; nine durations; delete dead motion code | Low — visible only as consistency |
| 2 | **Type tokens** | The §2.2 scale as `@theme` entries; codemod ~180 arbitrary classes | Low, mechanical, high line count |
| 3 | **Radius + interaction tints** | §2.4 ladder, §2.5 overlays; generator emits both per family | Medium — touches every family |
| 4 | **Typeface** | Bundle Figtree, switch `--font-sans`, re-check contrast at the new metrics | Medium — the most visible single change |
| 5 | **Controls** | Button, input/textarea/select, checkbox/radio/switch, chips/badges on the new specs | Medium |
| 6 | **Overlays** | One dialog shell + size scale, six shells migrated, one close affordance, one toast tier model, notification centre | High — most files, most behaviour |
| 7 | **Shell** | 44px band, sidebar compaction (§4.1), tabs at 32px, one icon-label gap | Medium — the compaction you asked for |
| 8 | **Views** | Header recipe, row spec, list/table, empty/loading/error convergence, ScheduleDetailView and Settings rebuilt | High — broadest surface |
| 9 | **Chat** | Metadata sizing, `MessageMeta`, composer grid and radius, toolbar chips, tool-call interiors, landing chips | Medium |
| 10 | **Sweep** | The §7 deletions, doc reconciliation (`design.md` amended, not appended), gallery regenerated | Low |

Verification is not "it compiles". Each phase ships with: the existing unit tests green, the contrast script green, and **screenshots at 1440×900, 1120×800 and 800×600 in light and dark for every family** — driven through the same Vite + agent-browser harness used to verify the heatmap work in this worktree, because jsdom cannot catch a Tailwind class collision and Electron cannot be launched from an agent shell. Phases 6 and 8 additionally get a manual pass in the real dev GUI.

---

## 9. Decisions for you

Everything above is a recommendation. These ten are the ones where a different answer changes the work, listed with what I would do and why.

| # | Decision | Recommendation |
|---|---|---|
| **A-01** | **Typeface** — bundle Figtree as `--font-sans`, or keep the native stack? | **Bundle Figtree.** You asked for the advertised font; Inter is already bundled for the wordmark, so the mechanism and the licence question are both settled. Cost: ~35KB and a re-run of the contrast script at new metrics. |
| **A-02** | **Type scale** — adopt the 14-base geometric scale with semantic roles? | **Yes.** It is the largest cheap win in the audit: ~180 arbitrary classes deleted and the first time the documented scale is enforceable. |
| **A-03** | **Content row height** 40 → 36px, contradicting D-12·B | **Yes, tighten.** D-12's principle (one rhythm, no compact mode) survives; only the value moves, and the app gains ~10% more rows per screen. |
| **A-04** | **Radius** — composer and modals 16 → 12px; `full` restricted to dots, knob, avatars | **Yes.** This is what makes the app read as one system. Keeping 16px anywhere but the artifact sheet reintroduces the fork. |
| **A-05** | **Interaction tints** replace hand-authored `--sidebar-hover`/`-active` per family | **Yes.** Removes the exact class of derived-value drift the theming doc warns about, and shrinks what a new family must author. |
| **A-06** | **Selection** — achromatic wash + weight as the base, accent kept as one 2px mark per surface | **Adopt the wash, keep the rail.** Astryx would drop the coral entirely; you asked to keep the colour, and the straightened rail already shipped here. |
| **A-07** | **Toast tiering** — a notification centre for compound reports | **Yes.** The grouped extension report is a 400px interactive panel living in a transient layer; nothing else in the census needs more than three lines and two buttons. |
| **A-08** | **Sidebar compaction** — wordmark into the band, "MENU" deleted, Scheduler/Workflows into a "More" group, Applications and Apps merged | **Yes** on all four; the nav grouping is the one users will notice, so it is the most worth iterating on if you disagree. |
| **A-09** | **Focus** — keep the surface shift, add an instant 2px accent outline for `:focus-visible` only | **Yes.** These are separable; today keyboard focus is nearly invisible on several controls. |
| **A-10** | **Scope** — all ten phases, or a subset? | **All ten, in order.** Phases 1–3 are what make the rest small; stopping after 5 would leave two systems running at once, which is the state the app is already in. |

Tell me which of these you want changed and I will revise the document before any code moves. If you approve as written, phase 1 begins with the motion root.

---

## Related documentation

- [Biorouter Design System](../../../design.md) — the current source of truth, cited throughout by its `D-nn` decisions; this document proposes amendments to §3.2, §3.4, §3.6 and §4.
- [Theme system architecture](../theming/theme-system-architecture.md) — the pipeline this redesign treats as fixed infrastructure and extends.
- [UI cohesion redesign](../ui-overhaul/ui-cohesion-redesign.md) — the 2026-07 pass whose open items (z-ladder, close affordance, toast layer) this document closes.
- [Astryx design system](https://astryx.atmeta.com/) — the external reference. All measurements in this document were read from live computed styles on 2026-07-27.
