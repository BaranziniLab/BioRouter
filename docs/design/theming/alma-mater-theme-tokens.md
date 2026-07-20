# Alma Mater theme tokens

> **What this is.** The colour-token reference for Alma Mater, BioRouter's UCSF-brand theme
> family: the UCSF palette it draws from, the accent decision, a complete token-by-token light
> and dark mapping, and the verified WCAG contrast ratios behind every value.
> **Status:** Current. Approved with the UCSF Teal accent, implemented, and reconciled against
> the shipped code on 2026-07-18 (light-theme revision the same day; originally implemented
> 2026-07-10). §9 records what was actually built.
> **Audience:** developers adding a component that reads colour tokens, and anyone proposing a
> new theme family.

Sections are numbered and cited by number from the other theme documents, so the numbering is a
stable reference scheme rather than decoration.

**Owner:** Baranzini Lab, UCSF.
**Companion mockups:** [`alma-mater-light-theme-studio.html`](alma-mater-light-theme-studio.html) (the 2026-07-18 revision — live contrast validation + exportable setup) · [`alma-mater-theme-studio.html`](alma-mater-theme-studio.html) — renders this palette on real BioRouter chrome, in **light + dark**, with a live accent picker that was used to compare the "elegant twist" options before the build. Both are kept as the decision record; the shipped values are in the theme file, not in them.

> This document names the palette, maps every token, and proves the contrast.
> It was written as the pre-build spec and has since been **reconciled against the
> shipped code** — every value below was re-read from the source of truth and every
> ratio recomputed from the hex. The theme-family toggle it proposed is built and
> wired into Appearance settings; see **§9 · as built**.
>
> **Where the theme actually lives now:** one file — [`ui/desktop/themes/alma-mater.theme.mjs`](../../../ui/desktop/themes/alma-mater.theme.mjs).
> The CSS token blocks, the syntax and terminal palettes, the picker entry and the
> boot-splash CSS are all *generated* from it by `npm run themes`, into
> marker-delimited regions of `main.css`, `themes.generated.ts` and `index.html`.
> Do not hand-edit those regions. The re-architecture is described in
> [`theme-system-architecture.md`](theme-system-architecture.md).

---

## 1 · The theme families

Named theme families, selected independently of light/dark. Three ship today:

| Theme | What it is | Soul |
|---|---|---|
| **Parchment** | The base BioRouter look — warm paper-and-bone neutrals, one terracotta-coral accent. Still the default, and the only family that declares the 17 structural tokens. | Warm, editorial, low-chroma. |
| **Alma Mater** | This document: BioRouter in the **official UCSF brand palette**. | Cool, institutional, navy-anchored — with the **UCSF Teal** C column as the accent. |
| **Roche Limit** | A later family, specified in [`roche-limit-theme.md`](roche-limit-theme.md). | Neutral-warm, orange accent. |

**Alma Mater is a re-colouring, not a re-layout.** It changes colour tokens only. It keeps
BioRouter's entire structure: flat surfaces, hairline borders, dense rows, the two-tone
canvas, the type scale, the motion, and — as the brief required — **the exact same fonts.**

### Constraints honoured

1. **Font unchanged.** `--font-sans` and `--font-mono` stay the current native stacks. UCSF's brand typefaces (Helvetica Neue / Granjon) are **not** adopted. Type scale, weights, tracking — untouched.
2. **Real light *and* real dark.** Alma Mater ships a genuine light variant and a genuine dark variant — four token sets total (Parchment already has two). Not a filter over one mode.
3. **UCSF palette as the source.** Every colour below is drawn from `identity.ucsf.edu/brand-guide` (verified against the live brand guide by the research pass), used with restraint — one accent, colour-as-evidence, never decoration.
4. **Same calm, minimal soul.** The instrument stays quiet. UCSF is the *twist*, not a new instrument.

---

## 2 · The design thesis

> **Alma Mater = UCSF Navy as the structural foundation + the UCSF Teal C column as the single brand accent, on neutral institutional greys.**

Three ideas do the work:

- **Navy is the ground.** `#052049` (UCSF's "base color") anchors ink in light mode and every dark surface in dark mode. Where Parchment darkens toward warm umber, Alma Mater darkens toward deep navy. This one move makes the theme read unmistakably as UCSF.
- **Neutrals go cool.** Parchment's bone/cream ramp is replaced by UCSF's cool grays (`#F2F3F4` → `#D1D3D3` → `#506380`) with a faint blue cast — the deliberate opposite bias to Parchment's warmth.
- **Teal is the accent (revised 2026-07-18).** The single brand accent is UCSF's **Teal "C" column**, stepped by job: **C3 `#16A0AC`** (the named "UCSF Teal") for rails and dots, **C2 `#14828C`** for the CTA fill and links, **C1 `#0E5258`** for hover — lifting to **C4 `#60D0DA`** in dark so it glows on navy. It plays exactly the role terracotta-coral plays in Parchment: primary CTAs, the active-nav rail, live-status dots, **the sidebar nav icons** — and nothing else.

  > The original build used the Violet **G** column (`#6C247C` eggplant → `#C45ED8` orchid), chosen to avoid the corporate-blue cliché. It was replaced because it cost the theme its recognisability: nobody pictures plum when they picture UCSF. Note the honest caveat from the brand research — teal is **not** co-iconic with navy. Navy is the single base colour and UCSF's body ink; teal is one of six peer primary hue ranges and, measured on ucsf.edu, a micro-accent (≈164k px² against navy's 1.8M). Confining it to icons and actions rather than to whole surfaces is the more literal reading of UCSF's one stated colour rule.

**Bonus:** teal keeps its distance from the blue **info** status (`#0F388A`) — the collision the first synthesis had when the accent was CTA-Blue. See §4 for the measured hue separation.

---

## 3 · The UCSF palette we draw from

UCSF codes every colour as **hue-letter + value-number** (letter = hue family, number = value, `1` darkest → `5/6` palest). Row 3 is the "named" core; rows 1–2 are the rich/deep tier; rows 4–5 are the light tints. This *is* UCSF's sanctioned tint ladder — you step the column, you don't invent tints.

The columns Alma Mater actually uses:

| Family | Deep (1–2) | Core (3) | Light (4–5) | Alma Mater uses it for |
|---|---|---|---|---|
| **A Navy / Blue** | **A1 `#052049`** (Navy), A2 `#0F388A` | A3 `#006BE9` (CTA), B3 `#178CCB` | B5 `#B8E6FA`, B6 `#E2F4FC` | Ink, chrome, all dark surfaces, **info** |
| **G Violet** | G1 `#461850`, G2 `#6C247C` | G3 `#A238BA` | G4 `#C45ED8`, G5 `#EACCF0` | Syntax `function` (was the accent, to 2026-07-18) |
| **H Magenta** | H1 `#561038`, H2 `#821A56` | H3 `#C42882` | H4 `#E266AE`, H5 `#F2C2DE` | (alt accent option) |
| **F Purple/Periwinkle** | F1 `#2E2872`, F2 `#443E8C` | F3 `#6C62D0` | F4 `#8A8CE3` | (alt accent option) |
| **C Teal** ⭐ | **C1 `#0E5258`**, **C2 `#14828C`** | **C3 `#16A0AC`** | **C4 `#60D0DA`**, C5 `#B4E2E8`, C6 `#E8F6F8` | **The brand accent** — fill, links, rails, nav icons, heatmap |
| **D Green** | D1 `#00483A`, **D2 `#007242`** | D3 `#32A03E` | E3 `#84C234` | **Success**, syntax `string` |
| **L/M/N Alerts** | — | **L3 `#FEB80A`** (Yellow), M3 `#FA6E1E` (Orange), **N3 `#E61048`** (Red) | — | **Warning / Danger** |
| **I/J/K Grays** | I3 `#506380` (Blue-Gray) | J2 `#878D96`, J3 `#B4B9BF` | K3 `#D1D3D3`, J5 `#E1E3E5`, I6 `#F2F3F4` | **Neutral ramp**, muted text |

UCSF also publishes **digital-adjusted** variants for on-screen accessibility (e.g. Interactive Teal `#058488`, hyperlink `#0071AD`). We honour the spirit — every text colour below clears WCAG AA — but the published teal ladder already passes at the steps we use (C2 `#14828C` at 4.56:1 on white, C1 `#0E5258` at 8.87:1), so no digital-adjusted substitute was needed.

> Note: UCSF ships **no dark-theme palette**. Every Alma Mater *dark* value is a contrast-safe extrapolation along UCSF's own hue columns, marked **derived** below and verified to AA.

---

## 4 · The accent decision (settled)

**Decided 2026-07-18: the UCSF Teal C column.** The options below were previewed live on real
chrome before the call; they are kept as the record of what was weighed.

| Option | Light accent | Dark accent | Character | Verdict |
|---|---|---|---|---|
| **⭐ UCSF Teal (chosen)** | C2 `#14828C` (fill) / C3 `#16A0AC` (rail) | C4 `#60D0DA` | The colour people actually picture beside UCSF navy | white-on-fill **4.56:1** |
| Deep Teal | C1 `#0E5258` | C4 `#60D0DA` | Same column, quieter and far more legible | white-on-fill 8.87:1 |
| CTA Blue | A3 `#006BE9` | derived `#7FC4EE` | UCSF's own documented digital button/link colour — the most literally compliant, and the most generic | white-on-fill 4.91:1 |
| Eggplant *(the original build)* | G2 `#6C247C` | G4 `#C45ED8` | Sophisticated, avoids the blue cliché — but unplaceable as UCSF | white-on-fill 9.63:1 |

**Why Teal over Eggplant:** eggplant was distinctive but not *recognisable*; it made the theme
look like a considered palette rather than like UCSF. Teal is the one saturated hue people
associate with the institution beside navy, and it is verifiably in use on ucsf.edu
(`#16A0AC` as a section band, `#14828C` as text).

**Why Teal over CTA Blue:** A3 is the more literal reading of UCSF's digital guidance and would
be the safer institutional answer. It was passed over because it reads as generic web-blue in a
product whose entire point is that it is *UCSF's* research instrument — and because it collides
with the `info` status hue. Measured: A3 `#006BE9` is **212°**, info A2 `#0F388A` is **220°** —
**8° apart**, indistinguishable as a hue signal. Teal C2 `#14828C` is **185°**, i.e. **35°** off
info. (An earlier draft of this doc claimed 57°; recomputing from the hexes gives 35°. The
conclusion survives — teal is 4× further from info than CTA-Blue is — but the figure was wrong.)

**Where teal is allowed:** primary CTAs, the active-nav rail, live-status dots, links, the
sidebar nav icons, the usage heatmap, and the syntax `type` stop. Nowhere else. The structure
stays neutral grey — which is also what every peer academic-medical design system does.

## 5 · Token-by-token mapping

All values below are **verified to WCAG AA** (computed from the hex; see §6). Geometry, radius,
motion, z-index and shadow *shapes* are **shared with Parchment** — Alma Mater overrides colour only.
Source column: a UCSF code means it's a published swatch; **derived** means a contrast-safe
extrapolation along a UCSF hue column.

### 5a · Two-tone canvas (surfaces)

| Token | Alma Mater Light | src | Alma Mater Dark | src |
|---|---|---|---|---|
| `--background-canvas` (main panel) | `#FFFFFF` | White | `#0D2A50` | derived |
| `--background-app` (window ground) | `#FFFFFF` | White | `#04142E` | derived navy |
| `--background-default` / `--card` | `#FFFFFF` | White | `#08213F` | derived navy |
| `--background-muted` (panel/terminal ground) | `#F2F3F4` | I6 | `#0D2A50` | derived |
| `--background-code` (code ground) | `#F2F3F4` | I6 | `#08213F` | derived navy |
| `--background-medium` (hover fill) | `#E1E3E5` | J5 | `#143563` | derived |
| `--background-strong` (pressed) | `#D1D3D3` | K3 | `#1E477F` | derived |
| `--background-inverse` | `#052049` | Navy | `#F2F3F4` | I6 |
| `--background-focus` (surface shift) | `#D3D6D9` | derived (one step past hover, neutralised) | `#163C74` | derived |

> **`--background-canvas` is not `--background-app`.** `--background-app` is the
> *window* ground — what `body` paints, behind everything. `--background-canvas` is
> what the main panel actually paints: the conversation, the hub and every
> top-level view. Light mode is the two-tone canvas of the mockups — a **white**
> panel against the `#EEF0F0` sidebar. (Before this token existed the panel painted
> `--background-muted` and the whole canvas read grey, which is the bug this
> separation fixes.) Dark deliberately **inverts the ladder**: the canvas `#0D2A50`
> sits *above* the cards `#08213F`, so the navy page lifts and cards recess into
> it. That is the shipped, preferred appearance — it is not a drift from
> `--background-app` waiting to be "corrected".

> **`--background-code` is not always `--background-muted`.** In light they coincide
> (`#F2F3F4`); in dark, code sits on the *card* navy `#08213F` while the page ground is a
> step lighter at `#0D2A50`. The syntax palette in §5g is measured against `--background-code`;
> the terminal in §5j is grounded on `--background-muted` in **both** modes — Alma's
> declared `terminalGround`. The two are genuinely different surfaces and the theme
> definition records that rather than assuming they agree.

### 5b · Sidebar (the two-tone signature)

Cool-gray, one calm step deeper than the canvas — same restraint as Parchment's sidebar (a *slightly deeper surface*, never a dark slab). The colour on the sidebar is carried by the **accent rail on the active item**, not by tinting the whole surface.

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--sidebar` | `#EEF0F0` | derived neutral grey | `#071B3A` | derived |
| `--sidebar-hover` | `#E3E5E5` | derived | `#0E2A50` | derived |
| `--sidebar-active` | `#D8D9DA` | derived | `#163864` | derived |
| `--sidebar-icon` | `#14828C` | C2 | `#60D0DA` | C4 |
| `--sidebar-border` | `#E1E3E5` | J5 | `#1A3A66` | derived |
| `--sidebar-foreground` | `#052049` | =text-default | `#F2F3F4` | =text-default |
| `--sidebar-primary` | =`--background-accent` | | =`--background-accent` | |

> The alternative — a **navy sidebar even in light mode**, UCSF's most recognizable move — was previewed in the mockup and **not taken** (§7.2): most on-brand, but the biggest departure from Parchment's layout feel.

### 5c · Accent — one hue (UCSF Teal)

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--background-accent` (button fill) | `#14828C` | C2 | `#60D0DA` | C4 |
| `--background-accent-hover` | `#0E5258` | C1 | `#8AE0E8` | derived (→C4 light) |
| `--text-on-accent` (ink on fill) | `#FFFFFF` | White | `#052049` | Navy |
| `--border-accent` | `#14828C` | C2 | `#60D0DA` | C4 |
| `--text-accent` (links, coloured labels) | `#14828C` | C2 | `#60D0DA` | C4 |
| `--accent-bar` (rails / dots / underline) | `#16A0AC` | C3 "UCSF Teal" | `#60D0DA` | C4 |
| `--sidebar-icon` (nav icons) | `#14828C` | C2 | `#60D0DA` | C4 |

> **Why the ladder splits three ways.** C3 is the *named* UCSF Teal but is only **3.15:1 on white**, so it can never be text or a filled button — it is rails and dots. C2 is the lightest official teal that clears AA with white ink (**4.56:1**), so it takes the fill and the links. C1 (**8.87:1**) takes hover. UCSF quotes C3 at 5.08:1 — and it does compute to **5.09:1**, but **against navy**, not white. Copying that figure across grounds would have shipped an inaccessible link colour.

### 5d · Text hierarchy

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--text-default` | `#052049` | Navy | `#F2F3F4` | I6 |
| `--text-muted` | `#506380` | I3 Blue-Gray | `#B4B9BF` | J3 |
| `--text-subtle` | `#586780` | derived (kept ≥5:1) | `#909AA6` | derived |
| `--text-inverse` | `#FFFFFF` | White | `#052049` | Navy |

### 5e · Borders

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--border-subtle` (hairline default) | `#E1E3E5` | J5 | `#17386A` | derived |
| `--border-strong` (hover / emphasis) | `#CFD3D8` | derived | `#24487F` | derived |
| `--border-input` (control edge) | `#D1D3D3` | K3 | `#24487F` | derived |
| `--ring` / focus (neutral, high-contrast mode only) | `#506380` | I3 | `#B4B9BF` | J3 |
| `--border-focus` | `#586780` | derived | `#909AA6` | derived |

> **Focus stays a neutral surface shift, never the accent** — exactly as Parchment does it (design.md D-15). A saturated teal outline on every focused field would read as an alarm on a calm surface. `--background-focus` deepens the control's own fill by one cool step.
>
> House rule on the focus surface: **`text-default` must clear AA on it, `text-subtle` need not.**
> Alma light is `text-default` **10.99:1** on `#D3D6D9` but `text-subtle` only **3.92:1**, and the
> contrast guard deliberately does not assert the second pair — forcing it would mean moving the
> hover fill and flattening the muted/medium ramp.

### 5f · Status (fill vs text/icon, split per Parchment)

Light mode uses UCSF's accessible dark stops; dark mode uses lighter tints with navy ink — mirroring Parchment's authored-twice discipline. Ink on fills = `--text-on-status` (white in light, navy `#052049` in dark).

| Role | Light fill | Light text/icon | Dark fill | Dark text/icon | src |
|---|---|---|---|---|---|
| **Danger** | `#E61048` | `#C40D3E` | `#F5768A` | `#F5768A` | N3 Red / derived |
| **Success** | `#007242` | `#007242` | `#5FBF74` | `#5FBF74` | D2 Green / derived |
| **Warning** | `#8A5A00` | `#8A5A00` | `#FEB80A` (navy ink) | `#FEB80A` | derived amber / L3 |
| **Info** | `#0F388A` | `#0F388A` | `#7FB3E6` | `#7FB3E6` | A2 Blue / derived |

Each role also carries a **border** token, so a status card's edge is not a generic hairline.
In Alma these track the text/icon stop exactly — the border is the same ink as the label:

| Token | Light | Dark |
|---|---|---|
| `--border-danger` | `#C40D3E` (=text-danger) | `#F5768A` |
| `--border-success` | `#007242` (=text-success) | `#5FBF74` |
| `--border-warning` | `#8A5A00` (=text-warning) | `#FEB80A` |
| `--border-info` | `#0F388A` (=text-info) | `#7FB3E6` |

> Note the light **warning fill is `#8A5A00`, not `#FEB80A`.** UCSF's L3 yellow cannot carry
> white ink at any size, so light mode uses the derived dark amber for the fill *and* the text,
> and only dark mode gets to use L3 itself (with navy ink). `--text-on-status` is white in
> light, navy `#052049` in dark.

### 5g · Code syntax palette (generated into `themes.generated.ts`, consumed by `codeTheme.ts`)

Recoloured to UCSF families; every stop ≥4.5:1 on its code ground (light `#F2F3F4`, dark `#08213F`). **`type/class` is the accent hue** — C1 Teal `#0E5258` in light, `#5CC6D0` in dark — tying code to the accent. The **eggplant survives as `function`** (G2 `#6C247C`), the one place the original violet accent still earns its keep. Diff rows reuse status danger/success, as Parchment does.

| Prism role | Light | src | Dark | src |
|---|---|---|---|---|
| plain (var / prop) | `#052049` | Navy | `#E1E3E5` | J5 |
| comment | `#586780` | derived | `#8A93A6` | derived |
| keyword (600) | `#0F388A` | A2 Blue | `#7FB3E6` | derived |
| string | `#007242` | D2 Green | `#6FC084` | derived |
| number / boolean | `#8A5A00` | derived amber | `#E0A44A` | derived |
| function | `#6C247C` | G2 Violet | `#C58AD6` | derived |
| type / class (600) | `#0E5258` | C1 Teal | `#5CC6D0` | derived |
| operator / punctuation | `#506380` | I3 | `#B4B9BF` | J3 |
| deleted (diff −) | `#C40D3E` | derived | `#F5768A` | derived |
| inserted (diff +) | `#007242` | D2 | `#5FBF74` | derived |

### 5h · Usage heatmap (sequential)

Parchment uses a warm terracotta ramp; Alma Mater uses UCSF's own teal ladder — monotonic in luminance so the five steps read in order even in greyscale.

| Level | Light | Dark |
|---|---|---|
| 0 (idle) | `#E8F6F8` (C6) | `#10223F` |
| 1 | `#B4E2E8` (C5) | `#0E5258` |
| 2 | `#60D0DA` (C4) | `#14828C` |
| 3 | `#16A0AC` (C3) | `#16A0AC` |
| 4 | `#0E5258` (C1) | `#60D0DA` |

### 5i · Shadows and the scrim

Same four elevation *shapes* as Parchment; only the tint changes — light-mode shadows shift from warm brown `rgba(32,25,15,…)` to **navy `rgba(5,32,73,…)`**; dark-mode shadows stay black. Flat-by-default rule is unchanged. `--shadow-modal-chrome-bottom` / `-top` are `none` in both modes.

| Token | Light | Dark |
|---|---|---|
| `--scrim` (modal / diagnostics overlay) | `rgba(5, 32, 73, 0.2)` — navy | `rgba(0, 0, 0, 0.48)` |

> **`--scrim` is a token now, and that is new.** The modal and diagnostics overlays used to be a
> hardcoded `rgba(32,25,15,0.18)` — Parchment's warm brown — sitting *outside* the token layer,
> with a single hand-written rule retinting Alma Mater light. Roche Limit, added afterwards,
> therefore silently wore Parchment's brown scrim over its own neutral-warm surfaces. Promoting
> the value to a token is what stops the next family repeating that; `main.css` now reads
> `background: var(--scrim)` at the one use site.

### 5j · Terminal ANSI-16 (`InAppTerminalDock`)

xterm paints to a canvas and cannot read `var()`, so the terminal needs its own literal palette.
Alma's `terminalGround` is **`--background-muted` in both modes** (light `#F2F3F4`, dark `#0D2A50`) —
recorded per family rather than assumed, because Parchment-dark grounds on `--background-code`
instead. `background` and `cursorAccent` are **derived** from that ground and must not be authored.

Ratios below are against each mode's own ground, recomputed from the hexes.

| Slot | Light | | Dark | |
|---|---|---|---|---|
| `foreground` | `#052049` | 14.44 | `#E1E3E5` | 11.15 |
| `cursor` | `#0E5258` | 7.99 | `#8AE0E8` | 9.50 |
| `selectionBackground` | `#D8D9DA` | 1.27 † | `#163864` | 1.22 † |
| `black` | `#052049` | 14.44 | `#1E477F` | 1.55 † |
| `red` | `#C40D3E` | 5.44 | `#F5768A` | 5.36 |
| `green` | `#007242` | 5.42 | `#5FBF74` | 6.28 |
| `yellow` | `#8A5A00` | 5.33 | `#FEB80A` | 8.25 |
| `blue` | `#0F388A` | 9.68 | `#7FB3E6` | 6.48 |
| `magenta` | `#6C247C` | 8.67 | `#C58AD6` | 5.45 |
| `cyan` | `#0E5258` | 7.99 | `#5CC6D0` | 7.13 |
| `white` | `#506380` | 5.50 | `#B4B9BF` | 7.26 |
| `brightBlack` | `#586780` | 5.15 | `#909AA6` | 5.03 |
| `brightRed` | `#D0143F` | 4.90 | `#FF8FA0` | 6.62 |
| `brightGreen` | `#1F7A3D` | 4.83 | `#7FD08F` | 7.73 |
| `brightYellow` | `#8A5A00` | 5.33 | `#FFCA4A` | 9.42 |
| `brightBlue` | `#255FB5` | 5.59 | `#A3C9F0` | 8.31 |
| `brightMagenta` | `#8A1FA0` | 6.79 | `#D7A5E8` | 7.13 |
| `brightCyan` | `#106A72` | 5.68 | `#7FD8E0` | 8.74 |
| `brightWhite` | `#052049` | 14.44 | `#F2F3F4` | 12.91 |

† Exempt by contract, with the reason recorded: `selectionBackground` is a surface, not a
foreground (floor 1.0); ANSI `black` is a dim slot — a lifted ground by convention, not text
(floor 1.0). Bright slots carry a documented **3:1** floor rather than 4.5, because on a light
ground "bright" (lighter than its base) and "AA" are in direct conflict. **Alma happens to clear
4.5:1 on every non-exempt slot in both modes anyway** — the tightest are light `brightGreen`
4.83 and dark `brightBlack` 5.03 — so it never actually spends that relaxation.

### 5k · Raw palette remap

Beyond the 62 semantic tokens, the light block re-declares **27 raw palette primitives**
(`--color-coral-*`, `--color-neutral-*`, `--color-red/blue/green/yellow-*`). These are
**mode-independent by design** — declared once in the light block and inherited by dark — and
they exist only to catch components that reach *past* the semantic layer and use a
`neutral-500` / `coral-600` utility directly. Without them, a stray direct usage would keep
Parchment's warm bias inside an otherwise cool theme. The semantic layer stays authoritative.

The coral ramp is remapped straight onto the teal ladder: `coral-400` → `#60D0DA` (C4),
`coral-500` → `#16A0AC` (C3), `coral-600` → `#14828C` (C2), `coral-700` → `#0E5258` (C1).
The neutral ramp runs cool grey at the light end (`#F7F8F9` → `#D1D3D3`) and turns navy at
the dark end (`#3A4A66` → `#1F3152` → `#0F2545` → `#04142E`).

### 5l · Boot splash

The pre-React splash paints before any of this exists, so `index.html` carries its own
per-family block (`html[data-theme='alma-mater'] #br-boot`). It is generated from the theme
file's `splash` values; `--br-bg` is **derived** from `--background-muted`.

| Splash var | Light | Dark |
|---|---|---|
| `--br-bg` | `#F2F3F4` (derived from `--background-muted`) | `#0D2A50` (derived) |
| `--br-navy` | `#052049` | `#052049` |
| `--br-coral` | `#16A0AC` | `#16A0AC` |
| `--br-track` | `#E8E1D2` | `#E8E1D2` |

> **The splash coral was wrong until this pass.** Alma Mater inherited Parchment's terracotta
> `#B85A32`, so the boot mark flashed warm orange for the whole backend-startup window and then
> jumped to teal on hydration. It is now the accent bar's own `#16A0AC` — 2.84:1 on the light
> splash ground and 4.55:1 on the dark one. The light figure sits just under the 3:1 that SC
> 1.4.11 asks of non-text UI, which is acceptable here only because the mark is a **logotype**,
> explicitly exempted by that success criterion — it is brand artwork, not an interface control,
> and nothing is actionable during the splash. `--br-track` is still the shared Parchment bone.

---

## 6 · Accessibility — verified, not asserted

Every text-carrying pair was computed from the hex. **AA floors: body ≥4.5:1, large/UI ≥3:1.** All pairs pass; the tightest are flagged.

**Light mode**

| Foreground | Background | Ratio | |
|---|---|---|---|
| text-default `#052049` | canvas `#FFFFFF` | 16.04 | ✅ AAA |
| text-default `#052049` | sidebar `#EEF0F0` | 14.02 | ✅ AAA |
| text-muted `#506380` | ground `#F2F3F4` | 5.50 | ✅ AA |
| text-subtle `#586780` | ground `#F2F3F4` | 5.15 | ✅ AA |
| white ink | accent fill `#14828C` | 4.56 | ✅ AA |
| text-accent `#14828C` | canvas `#FFFFFF` | 4.56 | ✅ AA |
| accent-bar `#16A0AC` | white | 3.15 | ✅ (UI 3:1) |
| sidebar-icon `#14828C` | sidebar `#EEF0F0` | 3.99 | ✅ (SC 1.4.11 3:1) |
| sidebar-icon `#14828C` | sidebar-hover `#E3E5E5` | 3.61 | ✅ (SC 1.4.11 3:1) |
| sidebar-icon `#14828C` | sidebar-active `#D8D9DA` | 3.23 | ✅ (SC 1.4.11 3:1) |
| text-default `#052049` | background-focus `#D3D6D9` | 10.99 | ✅ AAA |
| danger-text `#C40D3E` | white | 6.04 | ✅ AA |
| white ink | danger fill `#E61048` | 4.63 | ✅ AA (≥14px) |
| success `#007242` | white | 6.02 | ✅ AA |
| warning-text `#8A5A00` | white | 5.93 | ✅ AA |
| white ink | warning fill `#8A5A00` | 5.93 | ✅ AA |
| info `#0F388A` | white | 10.75 | ✅ AAA |

**Dark mode** (navy card ground `#08213F`)

| Foreground | Background | Ratio | |
|---|---|---|---|
| text-default `#F2F3F4` | canvas `#04142E` | 16.51 | ✅ AAA |
| text-muted `#B4B9BF` | card `#08213F` | 8.18 | ✅ AAA |
| text-subtle `#909AA6` | card `#08213F` | 5.66 | ✅ AA |
| navy ink `#052049` | accent fill `#60D0DA` | 8.81 | ✅ AAA |
| accent-bar `#60D0DA` | canvas `#04142E` | 10.07 | ✅ (UI 3:1) |
| text-accent `#60D0DA` | card `#08213F` | 8.87 | ✅ AAA |
| sidebar-icon `#60D0DA` | sidebar `#071B3A` | 9.40 | ✅ (SC 1.4.11 3:1) |
| sidebar-icon `#60D0DA` | sidebar-active `#163864` | 6.46 | ✅ (SC 1.4.11 3:1) |
| danger `#F5768A` | card | 6.03 | ✅ AA |
| success `#5FBF74` | card | 7.07 | ✅ AA |
| warning `#FEB80A` | card | 9.29 | ✅ AAA |
| info `#7FB3E6` | card | 7.30 | ✅ AAA |
| navy ink `#052049` | warning fill `#FEB80A` | 9.22 | ✅ AAA |
| navy ink `#052049` | danger fill `#F5768A` | 5.99 | ✅ AA |

Code-syntax stops all clear AA on their ground (lowest: light `comment` **5.15**, dark `comment` **5.24**).

> Two corrections made when these were recomputed against the shipped hexes rather than
> carried forward: text-default on the sidebar is **14.02**, not 14.09; and the light
> `comment` floor is **5.15**, not the 4.65 an earlier draft claimed. The `navy ink on
> #FEB80A` row had also drifted into the *light* table — `#FEB80A` is the **dark**-mode
> warning fill (§5f); light mode fills with `#8A5A00` under white ink at 5.93. All three
> were wrong in the doc only; no shipped value changed.

---

## 7 · The five decisions — settled

These were the open questions when this doc was step 1. All five are now decided and shipped;
the alternatives are kept as the record of what was weighed.

1. **Accent hue → UCSF Teal.** The original recommendation here was **Eggplant** (G2 `#6C247C`),
   with Magenta / Periwinkle / CTA-Blue as the other live options. Overturned on 2026-07-18 for
   the reasons in §4: eggplant was distinctive but not *recognisable* as UCSF. The eggplant is
   retained in exactly one place — the syntax `function` stop (§5g).
2. **Sidebar treatment → cool-gray two-tone.** Calm, matches Parchment's discipline. The
   navy light-mode sidebar (most on-brand, biggest departure) was previewed and passed over.
3. **Dark-surface base → navy.** A plum-tinted dark was the alternative when the accent was
   eggplant; with a teal accent it was moot. Every dark surface is a navy step (§5a).
4. **How far the extended spectrum reaches → data-viz and syntax only.** Teal/green/magenta
   never become general UI chrome, which is what protects the one-accent rule.
5. **Default theme → Parchment.** Alma Mater is opt-in via the Appearance theme-family selector.
   Parchment also remains the **base** family in the generator's sense: it alone supplies the
   bare `:root` / `.dark` blocks and the 17 structural tokens (radii, motion, z-index) that no
   theme may vary.

---

## 8 · The original implementation plan (kept for reference)

This was the step-2 plan. It held up — the theme system was already token-driven, so Alma Mater
landed with *zero component changes* — but the file list is now historical: several of these
sites are generated rather than hand-edited. See §9 for what the code looks like today.

- **`main.css`** — add `:root[data-theme='alma-mater']` (light) and `.dark[data-theme='alma-mater']` (dark) blocks that re-declare **only the colour tokens** above. Specificity `(0,2,0)` beats bare `:root`/`.dark` without touching them. Add `--color-alma-*` primitives beside `--color-coral-*`.
- **`codeTheme.ts`** — add an Alma Mater light/dark syntax set; select it by theme family.
- **`ThemeContext.tsx`** — add a `themeFamily: 'parchment' | 'alma-mater'` axis (localStorage-persisted), write `data-theme` on `<html>` alongside the existing `.dark`/`.light` class, broadcast across windows like the mode already is.
- **`index.html`** — mirror the family in the pre-hydration script so it doesn't flash on load.
- **Appearance settings** — mount a small **theme-family selector** (Parchment | Alma Mater) beside the existing light/dark control in [`AppSettingsSection.tsx`](../../../ui/desktop/src/components/settings/app/AppSettingsSection.tsx). The existing [`ThemeSelector`](../../../ui/desktop/src/components/BioRouterSidebar/ThemeSelector.tsx) is generalised or paired with a new `ThemeFamilySelector`.
- **Contrast guard** — extend `scripts/check-contrast.mjs` to assert Alma Mater's pairs too, so a future edit can't regress it.

Two things the plan got wrong, both found by running the real gate rather than reasoning about
the token layer: the primitives were **not** added as a parallel `--color-alma-*` set — they
**remap the existing `--color-coral-*` / `--color-neutral-*` names in place** (§5k), which is
what actually catches components reaching past the semantic layer. And the plan had no line for
the boot splash, which is why Alma flashed Parchment terracotta for months (§5l).

---

## 9 · Implementation — as built

Shipped, then **re-architected on 2026-07-18**. The theme is no longer a pattern of edits spread
across nine files; it is **one file plus one command**.

### Where Alma Mater lives now

| | |
|---|---|
| **Source of truth** | [`ui/desktop/themes/alma-mater.theme.mjs`](../../../ui/desktop/themes/alma-mater.theme.mjs) — 62 semantic tokens × 2 modes, 27 raw remaps, 10 syntax stops × 2, 19 terminal stops × 2, 3 splash values, plus `id` / `label` / `swatch` / `terminalGround`. |
| **Command** | `npm run themes` regenerates; `npm run themes -- --check` fails CI if anything is stale. It runs inside `npm run lint:check`. |
| **Contract** | `ui/desktop/scripts/lib/theme-contract.mjs` — the written-down answer to "what does a theme have to define?". A missing token means the generator **refuses to emit**. |

### What is generated from it

| Target | Region | Content |
|---|---|---|
| `src/styles/main.css` | marker-delimited | `:root[data-theme='alma-mater']` — **89 declarations** (62 semantic + 27 raw); `.dark[data-theme='alma-mater']` — **62**. Dark must follow light: the two tie at specificity `(0,2,0)` and dark wins on source order alone. |
| `src/styles/themes.generated.ts` | whole file | Syntax palette, terminal palette, `codeGround`, `terminalGround`, brand-mark colours, and the family manifest (`THEME_FAMILY_IDS`, label, swatch). |
| `index.html` | marker-delimited | The pre-hydration `FAMILIES` allow-list and the two boot-splash blocks. |

`ThemeContext.tsx`, `ThemeFamilySelector.tsx`, `codeTheme.ts` and `InAppTerminalDock.tsx` now all
**import** from `themes.generated.ts` instead of holding their own copies. `THEME_FAMILIES` is
literally `THEME_FAMILY_IDS`. The three duplicated family lists the old architecture carried are
gone.

### Values that are DERIVED and must not be authored

The generator computes these, because they are exactly the ones that used to live in two-to-four
places and silently drift:

| Derived value | From |
|---|---|
| `terminal.background`, `terminal.cursorAccent` | the family's own `terminalGround` token — Alma: `--background-muted`, both modes |
| code ground (`CODE_BG`) | `--background-code` |
| splash `--br-bg` | `--background-muted` |

Authoring `terminal.background` by hand is a validation error, not a silent override. This matters
because the three families **genuinely disagree** about which token the terminal dock paints —
Parchment-dark grounds on `--background-code`, Alma and Roche on `--background-muted`. A generator
that hardcoded one answer would have re-grounded two terminals under ANSI palettes tuned for a
different surface.

### Corrections this pass folded in

- **`--scrim` promoted to a token** (§5i). The overlay was a hardcoded warm-brown `rgba()` outside
  the token layer with a one-off Alma override; Roche Limit consequently wore Parchment's scrim.
- **Alma's boot-splash coral fixed** from Parchment's terracotta `#B85A32` to `#16A0AC` (§5l).
- **`--background-code` and `--sidebar-icon` written down here for the first time.** Both shipped
  long before this doc mentioned them; `--sidebar-icon` is the token that actually carries "teal
  nav icons", which §5c had been describing without naming.

### Verification performed

- `node scripts/check-contrast.mjs` — **228/228 assertions pass**, of which **78 are Alma Mater**.
  The guard discovers scopes by sweeping `main.css` for `[data-theme='…']`, so a new family is
  audited with zero edits to it.
- `node scripts/generate-themes.mjs --check` — generated artifacts current across all 3 themes.
- Every ratio in §5j and §6 recomputed from the shipped hexes with the WCAG relative-luminance
  formula, against the token each value is actually painted on.

## Related documentation

- [Theming](README.md) — the folder index, and the other theme families' token references.
- [Theme system architecture](theme-system-architecture.md) — how a theme is defined once and generated into every consumer; the authority over any hex quoted here.
- [Roche Limit theme tokens](roche-limit-theme.md) — the sibling token reference for the JupyterLab-inspired family.
- [Biorouter Design System](../../../design.md) — the parent design system and the register of the numbered `D-NN` decisions cited above.
