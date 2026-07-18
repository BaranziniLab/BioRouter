# Alma Mater theme tokens

> **What this is.** The colour-token reference for **Alma Mater**, BioRouter's UCSF-brand
> theme family: the UCSF palette it draws from, the accent decision, a complete
> token-by-token light/dark mapping, and the verified WCAG contrast ratios behind every
> value.
> **Status:** Current. The theme was approved (Eggplant accent) and implemented on
> 2026-07-10 and has shipped — `main.css`, `codeTheme.ts`, `ThemeContext.tsx` and
> `ThemeFamilySelector.tsx` all carry `alma-mater`. The tables below remain the
> authoritative source for the values.
> **Audience:** developers working on desktop theming and on any component that reads
> colour tokens.
> **Identifier key:** `D-NN` refers to a numbered design decision in the root
> [Biorouter Design System](../../../design.md); `A1`/`G2`/`I6`-style codes are UCSF's
> own hue-letter + value-number palette codes, explained under
> [The UCSF palette this theme draws from](#the-ucsf-palette-this-theme-draws-from).

BioRouter ships two named themes selected independently of light/dark mode. Parchment is
the default warm look; Alma Mater re-colours the same interface in UCSF's brand palette.
This document specifies every colour token Alma Mater overrides, names the UCSF swatch or
derivation behind each one, and records the contrast ratio that was computed for it. It
does not cover layout, typography or motion — Alma Mater changes none of those.

> **Legacy section numbers.** This document was originally written with numbered sections
> (`§1`–`§8`), and source comments cite them — `codeTheme.ts` points at `§5g`, and
> `main.css` cites the document as a whole. The mapping: §1 What the two themes are ·
> §2 The design thesis · §3 The UCSF palette this theme draws from · §4 The accent
> decision · §5 Token-by-token mapping (§5a–§5i are its subsections, in order) ·
> §6 Accessibility · §7 Decisions and how they were settled · §8 How it was implemented.

**Companion mockups.** [Alma Mater theme studio](alma-mater-theme-studio.html) renders this
palette on real BioRouter chrome in light and dark, and carries the accent picker used to
compare options while the accent was being chosen; [Alma Mater light theme
studio](alma-mater-light-theme-studio.html) explores the light variant in more depth.

## What the two themes are

Two named themes, selected independently of light/dark:

| Theme | What it is | Soul |
|---|---|---|
| **Parchment** | The current BioRouter look — warm paper-and-bone neutrals, one terracotta-coral accent. | Warm, editorial, low-chroma. |
| **Alma Mater** | A theme that dresses BioRouter in the **official UCSF brand palette**. | Cool, institutional, navy-anchored — with a **UCSF Violet / eggplant** accent as the twist. |

**Alma Mater is a re-colouring, not a re-layout.** It changes colour tokens only. It keeps
BioRouter's entire structure: flat surfaces, hairline borders, dense rows, the two-tone
canvas, the type scale, the motion, and the exact same fonts.

### Constraints the theme honours

1. **Font unchanged.** `--font-sans` and `--font-mono` stay the current native stacks. UCSF's brand typefaces (Helvetica Neue / Granjon) are **not** adopted. Type scale, weights, tracking — untouched.
2. **Real light *and* real dark.** Alma Mater ships a genuine light variant and a genuine dark variant — four token sets total (Parchment already has two). Not a filter over one mode.
3. **UCSF palette as the source.** Every colour below is drawn from `identity.ucsf.edu/brand-guide` (verified against the live brand guide by the research pass), used with restraint — one accent, colour-as-evidence, never decoration.
4. **Same calm, minimal soul.** The instrument stays quiet. UCSF is the *twist*, not a new instrument.

## The design thesis

> **Alma Mater = UCSF Navy as the structural foundation + UCSF Violet/eggplant as the single brand accent, on cool institutional neutrals.**

Three ideas do the work:

- **Navy is the ground.** `#052049` (UCSF's "base color") anchors ink in light mode and every dark surface in dark mode. Where Parchment darkens toward warm umber, Alma Mater darkens toward deep navy. This one move makes the theme read unmistakably as UCSF.
- **Neutrals go cool.** Parchment's bone/cream ramp is replaced by UCSF's cool grays (`#F2F3F4` → `#D1D3D3` → `#506380`) with a faint blue cast — the deliberate opposite bias to Parchment's warmth.
- **Eggplant is the accent — the elegant twist.** Instead of the obvious navy/teal, the single brand accent is UCSF's **Violet "G" column** (`#6C247C` eggplant in light, lifting to `#C45ED8` orchid in dark). It plays exactly the role terracotta-coral plays in Parchment: primary CTAs, the active-nav rail, live-status dots — and nothing else. It is sophisticated rather than loud, and it is distinctly UCSF without being the corporate-blue cliché.

**Bonus:** because the accent is violet, the blue **info** status (`#0F388A`) no longer collides with the accent hue — a problem the first synthesis had when the accent was CTA-Blue.

## The UCSF palette this theme draws from

UCSF codes every colour as **hue-letter + value-number** (letter = hue family, number = value, `1` darkest → `5/6` palest). Row 3 is the "named" core; rows 1–2 are the rich/deep tier; rows 4–5 are the light tints. This *is* UCSF's sanctioned tint ladder — you step the column, you don't invent tints.

The columns Alma Mater actually uses:

| Family | Deep (1–2) | Core (3) | Light (4–5) | Alma Mater uses it for |
|---|---|---|---|---|
| **A Navy / Blue** | **A1 `#052049`** (Navy), A2 `#0F388A` | A3 `#006BE9` (CTA), B3 `#178CCB` | B5 `#B8E6FA`, B6 `#E2F4FC` | Ink, chrome, all dark surfaces, **info** |
| **G Violet** ⭐ | G1 `#461850`, **G2 `#6C247C`** | G3 `#A238BA` | **G4 `#C45ED8`**, G5 `#EACCF0` | **The brand accent** |
| **H Magenta** | H1 `#561038`, H2 `#821A56` | H3 `#C42882` | H4 `#E266AE`, H5 `#F2C2DE` | (alt accent option) |
| **F Purple/Periwinkle** | F1 `#2E2872`, F2 `#443E8C` | F3 `#6C62D0` | F4 `#8A8CE3` | (alt accent option) |
| **C Teal** | C1 `#0E5258`, C2 `#14828C` | C3 `#16A0AC` | C4 `#60D0DA` | Syntax `func`, viz |
| **D Green** | D1 `#00483A`, **D2 `#007242`** | D3 `#32A03E` | E3 `#84C234` | **Success**, syntax `string` |
| **L/M/N Alerts** | — | **L3 `#FEB80A`** (Yellow), M3 `#FA6E1E` (Orange), **N3 `#E61048`** (Red) | — | **Warning / Danger** |
| **I/J/K Grays** | I3 `#506380` (Blue-Gray) | J2 `#878D96`, J3 `#B4B9BF` | K3 `#D1D3D3`, J5 `#E1E3E5`, I6 `#F2F3F4` | **Neutral ramp**, muted text |

UCSF also publishes **digital-adjusted** variants for on-screen accessibility (e.g. Interactive Teal `#058488`, hyperlink `#0071AD`). Alma Mater honours the spirit — every text colour below clears WCAG AA — but the eggplant accent already passes without needing the digital blue set.

> **Note.** UCSF ships **no dark-theme palette**. Every Alma Mater *dark* value is a contrast-safe extrapolation along UCSF's own hue columns, marked **derived** below and verified to AA.

## The accent decision

This was the one genuine choice. **Eggplant was recommended and adopted**; the studio mockup
previews all five live. Each keeps navy as the foundation and only swaps the accent hue.

| Option | Light accent | Dark accent | Character | Verdict |
|---|---|---|---|---|
| **⭐ Eggplant (adopted)** | G2 `#6C247C` | G4 `#C45ED8` | Rich, sophisticated, unmistakably UCSF, avoids the blue cliché | white-on-fill **9.63:1** |
| Violet | G3 `#A238BA` | G4 `#C45ED8` | Brighter, more energetic jewel tone | white-on-fill 5.51:1 |
| Magenta | H2 `#821A56` | H4 `#E266AE` | Bold, warm-leaning berry | white-on-fill 9.41:1 |
| Periwinkle | F2 `#443E8C` | F4 `#8A8CE3` | Quietest, serene lavender-blue (closest to "calm") | white-on-fill 9.07:1 |
| CTA Blue | A3 `#006BE9` | A3 `#006BE9` | Most literal UCSF digital spec; but it's the expected blue | white-on-fill 4.9:1 |

**Why Eggplant:** it is the most distinctive *and* the most restrained of the saturated options — a deep plum reads as considered, not attention-seeking, which fits the "calm instrument" thesis. In dark mode it lifts to orchid `#C45ED8` so it glows on the navy canvas instead of muddying (the same move Parchment makes lifting coral-600 → coral-400).

## Token-by-token mapping

All values below are **verified to WCAG AA** (computed from the hex; see
[Accessibility](#accessibility--verified-not-asserted)). Geometry, radius, motion, z-index
and shadow *shapes* are **shared with Parchment** — Alma Mater overrides colour only.
The source column gives a UCSF code where the value is a published swatch; **derived**
means a contrast-safe extrapolation along a UCSF hue column.

### Two-tone canvas (surfaces)

| Token | Alma Mater Light | src | Alma Mater Dark | src |
|---|---|---|---|---|
| `--background-app` (canvas) | `#FFFFFF` | White | `#04142E` | derived navy |
| `--background-default` / `--card` | `#FFFFFF` | White | `#08213F` | derived navy |
| `--background-muted` (page ground) | `#F2F3F4` | I6 | `#0D2A50` | derived |
| `--background-medium` (hover fill) | `#E1E3E5` | J5 | `#143563` | derived |
| `--background-strong` (pressed) | `#D1D3D3` | K3 | `#1E477F` | derived |
| `--background-inverse` | `#052049` | Navy | `#F2F3F4` | I6 |
| `--background-focus` (surface shift) | `#D0D6DE` | derived (one step past hover) | `#163C74` | derived |

### Sidebar (the two-tone signature)

Cool-gray, one calm step deeper than the canvas — same restraint as Parchment's sidebar (a *slightly deeper surface*, never a dark slab). The colour on the sidebar is carried by the **accent rail on the active item**, not by tinting the whole surface.

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--sidebar` | `#ECEEF1` | derived cool gray | `#071B3A` | derived |
| `--sidebar-hover` | `#E3E6EA` | derived | `#0E2A50` | derived |
| `--sidebar-active` | `#D7DBE0` | derived | `#163864` | derived |
| `--sidebar-border` | `#E1E3E5` | J5 | `#1A3A66` | derived |
| `--sidebar-foreground` | `#052049` | =text-default | `#F2F3F4` | =text-default |
| `--sidebar-primary` | =`--background-accent` | | =`--background-accent` | |

> **Note.** The alternative considered here — a **navy sidebar even in light mode**, UCSF's
> most recognizable move — is previewable in the studio mockup. It is the most on-brand but
> the biggest departure from Parchment's layout feel, and it was not adopted; see
> [Decisions and how they were settled](#decisions-and-how-they-were-settled).

### Accent — one hue (UCSF Violet)

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--background-accent` (button fill) | `#6C247C` | G2 Eggplant | `#C45ED8` | G4 Orchid |
| `--background-accent-hover` | `#571C66` | derived (→G1) | `#D07EE0` | derived |
| `--text-on-accent` (ink on fill) | `#FFFFFF` | White | `#1A0A24` | derived plum-black |
| `--border-accent` | `#6C247C` | G2 | `#C45ED8` | G4 |
| `--text-accent` (links, coloured labels) | `#6C247C` | G2 | `#D7A5E8` | derived light orchid |
| `--accent-bar` (rails / dots / underline) | `#A238BA` | G3 Violet | `#C45ED8` | G4 |

### Text hierarchy

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--text-default` | `#052049` | Navy | `#F2F3F4` | I6 |
| `--text-muted` | `#506380` | I3 Blue-Gray | `#B4B9BF` | J3 |
| `--text-subtle` | `#586780` | derived (kept ≥5:1) | `#909AA6` | derived |
| `--text-inverse` | `#FFFFFF` | White | `#052049` | Navy |

### Borders

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--border-subtle` (hairline default) | `#E1E3E5` | J5 | `#17386A` | derived |
| `--border-strong` (hover / emphasis) | `#CFD3D8` | derived | `#24487F` | derived |
| `--border-input` (control edge) | `#D1D3D3` | K3 | `#24487F` | derived |
| `--ring` / focus (neutral, high-contrast mode only) | `#506380` | I3 | `#B4B9BF` | J3 |
| `--border-focus` | `#586780` | derived | `#909AA6` | derived |

> **Why.** **Focus stays a neutral surface shift, never the accent** — exactly as Parchment
> does it (decision D-15 in the [design system](../../../design.md)). An eggplant outline on
> every focused field would read as an alarm on a calm surface. `--background-focus` deepens
> the control's own fill by one cool step.

### Status (fill vs text/icon, split per Parchment)

Light mode uses UCSF's accessible dark stops; dark mode uses lighter tints with navy ink — mirroring Parchment's authored-twice discipline. Ink on fills = `--text-on-status` (white in light, navy `#052049` in dark).

| Role | Light fill | Light text/icon | Dark fill | Dark text/icon | src |
|---|---|---|---|---|---|
| **Danger** | `#E61048` | `#C40D3E` | `#F5768A` | `#F5768A` | N3 Red / derived |
| **Success** | `#007242` | `#007242` | `#5FBF74` | `#5FBF74` | D2 Green / derived |
| **Warning** | `#8A5A00` | `#8A5A00` | `#FEB80A` (navy ink) | `#FEB80A` | derived amber / L3 |
| **Info** | `#0F388A` | `#0F388A` | `#7FB3E6` | `#7FB3E6` | A2 Blue / derived |

### Code syntax palette (`codeTheme.ts`)

Recoloured to UCSF families; every stop ≥4.5:1 on its code ground (light `#F2F3F4`, dark `#08213F`). `type/class` is the eggplant, tying code to the accent. Diff rows reuse status danger/success, as Parchment does.

| Prism role | Light | src | Dark | src |
|---|---|---|---|---|
| plain (var / prop) | `#052049` | Navy | `#E1E3E5` | J5 |
| comment | `#586780` | derived | `#8A93A6` | derived |
| keyword (600) | `#0F388A` | A2 Blue | `#7FB3E6` | derived |
| string | `#007242` | D2 Green | `#6FC084` | derived |
| number / boolean | `#8A5A00` | derived amber | `#E0A44A` | derived |
| function | `#0E5258` | C1 Teal | `#5CC6D0` | derived |
| type / class (600) | `#6C247C` | G2 Violet | `#C58AD6` | derived |
| operator / punctuation | `#506380` | I3 | `#B4B9BF` | J3 |
| deleted (diff −) | `#C40D3E` | derived | `#F5768A` | derived |
| inserted (diff +) | `#007242` | D2 | `#5FBF74` | derived |

### Usage heatmap (sequential)

Parchment uses a warm terracotta ramp; Alma Mater uses UCSF's own violet ladder — monotonic in luminance so the five steps read in order even in greyscale.

| Level | Light | Dark |
|---|---|---|
| 0 (idle) | `#ECE6F1` | `#10223F` |
| 1 | `#D9C2E8` | `#3A1E52` |
| 2 | `#C48ED6` | `#6C247C` |
| 3 | `#9E44B4` | `#A238BA` |
| 4 | `#6C247C` | `#C45ED8` |

### Shadows

Same four elevation *shapes* as Parchment; only the tint changes — light-mode shadows shift from warm brown `rgba(32,25,15,…)` to **navy `rgba(5,32,73,…)`**; dark-mode shadows stay black. Flat-by-default rule is unchanged.

## Accessibility — verified, not asserted

Every text-carrying pair was computed from the hex. **AA floors: body ≥4.5:1, large/UI ≥3:1.** All pairs pass; the tightest are flagged.

**Light mode**

| Foreground | Background | Ratio | |
|---|---|---|---|
| text-default `#052049` | canvas `#FFFFFF` | 16.04 | ✅ AAA |
| text-default `#052049` | sidebar `#ECEEF1` | 13.80 | ✅ AAA |
| text-muted `#506380` | ground `#F2F3F4` | 5.50 | ✅ AA |
| text-subtle `#586780` | ground `#F2F3F4` | 5.15 | ✅ AA |
| white ink | accent fill `#6C247C` | 9.63 | ✅ AAA |
| text-accent `#6C247C` | ground `#F2F3F4` | 8.67 | ✅ AAA |
| accent-bar `#A238BA` | white | 5.51 | ✅ (UI 3:1) |
| danger-text `#C40D3E` | white | 6.04 | ✅ AA |
| white ink | danger fill `#E61048` | 4.63 | ✅ AA (≥14px) |
| success `#007242` | white | 6.02 | ✅ AA |
| warning-text `#8A5A00` | white | 5.93 | ✅ AA |
| navy ink | warning fill `#FEB80A` | 9.22 | ✅ AAA |
| info `#0F388A` | white | 10.75 | ✅ AAA |

**Dark mode** (navy card ground `#08213F`)

| Foreground | Background | Ratio | |
|---|---|---|---|
| text-default `#F2F3F4` | canvas `#04142E` | 16.51 | ✅ AAA |
| text-muted `#B4B9BF` | card `#08213F` | 8.18 | ✅ AAA |
| text-subtle `#909AA6` | card `#08213F` | 5.66 | ✅ AA |
| plum ink `#1A0A24` | accent fill `#C45ED8` | 5.34 | ✅ AA |
| accent-bar `#C45ED8` | canvas `#04142E` | 5.19 | ✅ (UI 3:1) |
| text-accent `#D7A5E8` | card `#08213F` | 8.03 | ✅ AAA |
| danger `#F5768A` | card | 6.03 | ✅ AA |
| success `#5FBF74` | card | 7.07 | ✅ AA |
| warning `#FEB80A` | card | 9.29 | ✅ AAA |
| info `#7FB3E6` | card | 7.30 | ✅ AAA |

Code-syntax stops all clear AA on their ground (lowest: light `comment` 4.65, dark `comment` 5.24).

## Decisions and how they were settled

These five points were open when the theme was proposed. All were settled as recommended,
and the shipped tokens above reflect the settled answers — none of them is still open.

| Decision | Recommendation | Outcome |
|---|---|---|
| **Accent hue** | Eggplant, over Violet / Magenta / Periwinkle / CTA-Blue | **Eggplant adopted** (`--background-accent: #6C247C`). |
| **Sidebar treatment** | Cool-gray two-tone (calm, matches Parchment discipline), over a navy light-mode sidebar | **Cool-gray two-tone adopted** (`--sidebar: #ECEEF1`). |
| **Dark-surface base** | Navy-based dark surfaces (calm), over plum-tinted dark | **Navy adopted** (`--background-app: #04142E` in dark). |
| **How far the extended spectrum reaches** | Teal/green/magenta appear **only** in data-viz and the syntax palette — never as general UI chrome — to protect the one-accent rule | **Adopted**; the extended hues appear only in the syntax and status tables above. |
| **Default theme** | Parchment stays the default; Alma Mater is opt-in | **Confirmed**; the theme family resolves to `parchment` unless the user selects otherwise. |

## How it was implemented

The theme system is fully token-driven, so Alma Mater was a **near-free add** with *zero
component changes*:

- **`ui/desktop/src/styles/main.css`** — adds `:root[data-theme='alma-mater']` (light) and `.dark[data-theme='alma-mater']` (dark) blocks that re-declare **only the colour tokens** above. Specificity `(0,2,0)` beats bare `:root`/`.dark` without touching them. `--color-alma-*` primitives sit beside `--color-coral-*`.
- **`ui/desktop/src/styles/codeTheme.ts`** — adds an Alma Mater light/dark syntax set, selected by theme family.
- **`ui/desktop/src/contexts/ThemeContext.tsx`** — adds a `themeFamily: 'parchment' | 'alma-mater'` axis (localStorage-persisted), writes `data-theme` on `<html>` alongside the existing `.dark`/`.light` class, and broadcasts across windows like the mode already is.
- **`ui/desktop/index.html`** — mirrors the family in the pre-hydration script so it does not flash on load.
- **Appearance settings** — a small **theme-family selector** (Parchment | Alma Mater) mounts beside the existing light/dark control in `ui/desktop/src/components/settings/app/AppSettingsSection.tsx`. The existing `ui/desktop/src/components/BioRouterSidebar/ThemeSelector.tsx` is now paired with `ui/desktop/src/components/BioRouterSidebar/ThemeFamilySelector.tsx`.
- **Contrast guard** — `ui/desktop/scripts/check-contrast.mjs` asserts Alma Mater's pairs too, so a future edit cannot regress them.

Files touched are colour and config only — no layout, no className churn, no new components
beyond the one selector.

## Related documentation

- [Biorouter Design System](../../../design.md) — the parent design system, including decisions D-06 (typeface) and D-15 (focus as a surface shift) cited above.
- [Alma Mater theme studio](alma-mater-theme-studio.html) — renders these tokens on real BioRouter chrome in light and dark, with the accent picker used to choose Eggplant.
- [Alma Mater light theme studio](alma-mater-light-theme-studio.html) — a deeper exploration of the light variant.
- [Theme system explorer](theme-system-explorer.html) — how the token layers and theme axes fit together across both families.
- [UI overhaul execution status](../ui-overhaul/execution-status.md) — records the sweep that verified light + dark × parchment + alma-mater through the real persistence path.
