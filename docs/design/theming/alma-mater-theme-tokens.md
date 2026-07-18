# Alma Mater — a UCSF theme for BioRouter

**Status:** ✅ **Approved (Eggplant accent) & implemented** · **Date:** 2026-07-10 · **Owner:** Baranzini Lab, UCSF
**Companion mockup:** [`docs/design/alma-mater-theme.html`](alma-mater-theme.html) — renders this palette on real BioRouter chrome, in **light + dark**, with a live accent picker so you can compare the "elegant twist" options before we build anything.

> This document is **step 1** of the theme work: it names the palette, maps every
> token, and proves the contrast. **Nothing in the codebase changes until you approve it.**
> Step 2 (after sign-off) abstracts the theme selection into a **theme-family toggle**
> and wires Alma Mater into the Appearance settings.

---

## 1 · What this is

Two named themes, selected independently of light/dark:

| Theme | What it is | Soul |
|---|---|---|
| **Parchment** | The current BioRouter look — warm paper-and-bone neutrals, one terracotta-coral accent. | Warm, editorial, low-chroma. |
| **Alma Mater** | A new theme that dresses BioRouter in the **official UCSF brand palette**. | Cool, institutional, navy-anchored — with a **UCSF Violet / eggplant** accent as the twist. |

**Alma Mater is a re-colouring, not a re-layout.** It changes colour tokens only. It keeps
BioRouter's entire structure: flat surfaces, hairline borders, dense rows, the two-tone
canvas, the type scale, the motion, and — per your instruction — **the exact same fonts.**

### Constraints honoured (your four asks)

1. **Font unchanged.** `--font-sans` and `--font-mono` stay the current native stacks. UCSF's brand typefaces (Helvetica Neue / Granjon) are **not** adopted. Type scale, weights, tracking — untouched.
2. **Real light *and* real dark.** Alma Mater ships a genuine light variant and a genuine dark variant — four token sets total (Parchment already has two). Not a filter over one mode.
3. **UCSF palette as the source.** Every colour below is drawn from `identity.ucsf.edu/brand-guide` (verified against the live brand guide by the research pass), used with restraint — one accent, colour-as-evidence, never decoration.
4. **Same calm, minimal soul.** The instrument stays quiet. UCSF is the *twist*, not a new instrument.

---

## 2 · The design thesis

> **Alma Mater = UCSF Navy as the structural foundation + UCSF Violet/eggplant as the single brand accent, on cool institutional neutrals.**

Three ideas do the work:

- **Navy is the ground.** `#052049` (UCSF's "base color") anchors ink in light mode and every dark surface in dark mode. Where Parchment darkens toward warm umber, Alma Mater darkens toward deep navy. This one move makes the theme read unmistakably as UCSF.
- **Neutrals go cool.** Parchment's bone/cream ramp is replaced by UCSF's cool grays (`#F2F3F4` → `#D1D3D3` → `#506380`) with a faint blue cast — the deliberate opposite bias to Parchment's warmth.
- **Eggplant is the accent — the elegant twist.** Instead of the obvious navy/teal, the single brand accent is UCSF's **Violet "G" column** (`#6C247C` eggplant in light, lifting to `#C45ED8` orchid in dark). It plays exactly the role terracotta-coral plays in Parchment: primary CTAs, the active-nav rail, live-status dots — and nothing else. It is sophisticated rather than loud, and it is distinctly UCSF without being the corporate-blue cliché.

**Bonus:** because the accent is now violet, the blue **info** status (`#0F388A`) no longer collides with the accent hue — a problem the first synthesis had when the accent was CTA-Blue.

---

## 3 · The UCSF palette we draw from

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

UCSF also publishes **digital-adjusted** variants for on-screen accessibility (e.g. Interactive Teal `#058488`, hyperlink `#0071AD`). We honour the spirit — every text colour below clears WCAG AA — but the eggplant accent already passes without needing the digital blue set.

> Note: UCSF ships **no dark-theme palette**. Every Alma Mater *dark* value is a contrast-safe extrapolation along UCSF's own hue columns, marked **derived** below and verified to AA.

---

## 4 · The accent decision (your call)

This is the one genuine choice. The doc **recommends Eggplant**, but the mockup lets you preview all five live. Each keeps navy as the foundation and only swaps the accent hue.

| Option | Light accent | Dark accent | Character | Verdict |
|---|---|---|---|---|
| **⭐ Eggplant (recommended)** | G2 `#6C247C` | G4 `#C45ED8` | Rich, sophisticated, unmistakably UCSF, avoids the blue cliché | white-on-fill **9.63:1** |
| Violet | G3 `#A238BA` | G4 `#C45ED8` | Brighter, more energetic jewel tone | white-on-fill 5.51:1 |
| Magenta | H2 `#821A56` | H4 `#E266AE` | Bold, warm-leaning berry | white-on-fill 9.41:1 |
| Periwinkle | F2 `#443E8C` | F4 `#8A8CE3` | Quietest, serene lavender-blue (closest to "calm") | white-on-fill 9.07:1 |
| CTA Blue | A3 `#006BE9` | A3 `#006BE9` | Most literal UCSF digital spec; but it's the expected blue | white-on-fill 4.9:1 |

**Why Eggplant:** it is the most distinctive *and* the most restrained of the saturated options — a deep plum reads as considered, not attention-seeking, which fits the "calm instrument" thesis. In dark mode it lifts to orchid `#C45ED8` so it glows on the navy canvas instead of muddying (the same move Parchment makes lifting coral-600 → coral-400).

---

## 5 · Token-by-token mapping

All values below are **verified to WCAG AA** (computed from the hex; see §6). Geometry, radius,
motion, z-index and shadow *shapes* are **shared with Parchment** — Alma Mater overrides colour only.
Source column: a UCSF code means it's a published swatch; **derived** means a contrast-safe
extrapolation along a UCSF hue column.

### 5a · Two-tone canvas (surfaces)

| Token | Alma Mater Light | src | Alma Mater Dark | src |
|---|---|---|---|---|
| `--background-app` (canvas) | `#FFFFFF` | White | `#04142E` | derived navy |
| `--background-default` / `--card` | `#FFFFFF` | White | `#08213F` | derived navy |
| `--background-muted` (page ground) | `#F2F3F4` | I6 | `#0D2A50` | derived |
| `--background-medium` (hover fill) | `#E1E3E5` | J5 | `#143563` | derived |
| `--background-strong` (pressed) | `#D1D3D3` | K3 | `#1E477F` | derived |
| `--background-inverse` | `#052049` | Navy | `#F2F3F4` | I6 |
| `--background-focus` (surface shift) | `#D0D6DE` | derived (one step past hover) | `#163C74` | derived |

### 5b · Sidebar (the two-tone signature)

Cool-gray, one calm step deeper than the canvas — same restraint as Parchment's sidebar (a *slightly deeper surface*, never a dark slab). The colour on the sidebar is carried by the **accent rail on the active item**, not by tinting the whole surface.

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--sidebar` | `#ECEEF1` | derived cool gray | `#071B3A` | derived |
| `--sidebar-hover` | `#E3E6EA` | derived | `#0E2A50` | derived |
| `--sidebar-active` | `#D7DBE0` | derived | `#163864` | derived |
| `--sidebar-border` | `#E1E3E5` | J5 | `#1A3A66` | derived |
| `--sidebar-foreground` | `#052049` | =text-default | `#F2F3F4` | =text-default |
| `--sidebar-primary` | =`--background-accent` | | =`--background-accent` | |

> Alternative (see §7): a **navy sidebar even in light mode** — UCSF's most recognizable move — is previewable in the mockup. It's the most on-brand but the biggest departure from Parchment's layout feel.

### 5c · Accent — one hue (UCSF Violet)

| Token | Light | src | Dark | src |
|---|---|---|---|---|
| `--background-accent` (button fill) | `#6C247C` | G2 Eggplant | `#C45ED8` | G4 Orchid |
| `--background-accent-hover` | `#571C66` | derived (→G1) | `#D07EE0` | derived |
| `--text-on-accent` (ink on fill) | `#FFFFFF` | White | `#1A0A24` | derived plum-black |
| `--border-accent` | `#6C247C` | G2 | `#C45ED8` | G4 |
| `--text-accent` (links, coloured labels) | `#6C247C` | G2 | `#D7A5E8` | derived light orchid |
| `--accent-bar` (rails / dots / underline) | `#A238BA` | G3 Violet | `#C45ED8` | G4 |

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

> **Focus stays a neutral surface shift, never the accent** — exactly as Parchment does it (design.md D-15). An eggplant outline on every focused field would read as an alarm on a calm surface. `--background-focus` deepens the control's own fill by one cool step.

### 5f · Status (fill vs text/icon, split per Parchment)

Light mode uses UCSF's accessible dark stops; dark mode uses lighter tints with navy ink — mirroring Parchment's authored-twice discipline. Ink on fills = `--text-on-status` (white in light, navy `#052049` in dark).

| Role | Light fill | Light text/icon | Dark fill | Dark text/icon | src |
|---|---|---|---|---|---|
| **Danger** | `#E61048` | `#C40D3E` | `#F5768A` | `#F5768A` | N3 Red / derived |
| **Success** | `#007242` | `#007242` | `#5FBF74` | `#5FBF74` | D2 Green / derived |
| **Warning** | `#8A5A00` | `#8A5A00` | `#FEB80A` (navy ink) | `#FEB80A` | derived amber / L3 |
| **Info** | `#0F388A` | `#0F388A` | `#7FB3E6` | `#7FB3E6` | A2 Blue / derived |

### 5g · Code syntax palette (`codeTheme.ts`)

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

### 5h · Usage heatmap (sequential)

Parchment uses a warm terracotta ramp; Alma Mater uses UCSF's own violet ladder — monotonic in luminance so the five steps read in order even in greyscale.

| Level | Light | Dark |
|---|---|---|
| 0 (idle) | `#ECE6F1` | `#10223F` |
| 1 | `#D9C2E8` | `#3A1E52` |
| 2 | `#C48ED6` | `#6C247C` |
| 3 | `#9E44B4` | `#A238BA` |
| 4 | `#6C247C` | `#C45ED8` |

### 5i · Shadows

Same four elevation *shapes* as Parchment; only the tint changes — light-mode shadows shift from warm brown `rgba(32,25,15,…)` to **navy `rgba(5,32,73,…)`**; dark-mode shadows stay black. Flat-by-default rule is unchanged.

---

## 6 · Accessibility — verified, not asserted

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

---

## 7 · Open decisions for you

1. **Accent hue.** Recommended **Eggplant**; preview Violet / Magenta / Periwinkle / CTA-Blue in the mockup and pick.
2. **Sidebar treatment.** Recommended **cool-gray two-tone** (calm, matches Parchment discipline). Alternative: a **navy light-mode sidebar** (most on-brand UCSF, biggest visual change) — previewable via a toggle in the mockup.
3. **Dark-surface base.** Recommended **navy-based** dark surfaces (calm). Alternative: plum-tinted dark to lean harder into the accent (less calm). Recommend navy.
4. **How far the extended spectrum reaches.** Recommended: teal/green/magenta appear **only** in data-viz and the syntax palette — never as general UI chrome — to protect the one-accent rule.
5. **Default theme.** Parchment stays the default; Alma Mater is opt-in. Confirm.

---

## 8 · Implementation preview (step 2 — after approval)

The theme system is fully token-driven, so Alma Mater is a **near-free add** with *zero component changes*:

- **`main.css`** — add `:root[data-theme='alma-mater']` (light) and `.dark[data-theme='alma-mater']` (dark) blocks that re-declare **only the colour tokens** above. Specificity `(0,2,0)` beats bare `:root`/`.dark` without touching them. Add `--color-alma-*` primitives beside `--color-coral-*`.
- **`codeTheme.ts`** — add an Alma Mater light/dark syntax set; select it by theme family.
- **`ThemeContext.tsx`** — add a `themeFamily: 'parchment' | 'alma-mater'` axis (localStorage-persisted), write `data-theme` on `<html>` alongside the existing `.dark`/`.light` class, broadcast across windows like the mode already is.
- **`index.html`** — mirror the family in the pre-hydration script so it doesn't flash on load.
- **Appearance settings** — mount a small **theme-family selector** (Parchment | Alma Mater) beside the existing light/dark control in [`AppSettingsSection.tsx`](../../ui/desktop/src/components/settings/app/AppSettingsSection.tsx). The existing [`ThemeSelector`](../../ui/desktop/src/components/BioRouterSidebar/ThemeSelector.tsx) is generalised or paired with a new `ThemeFamilySelector`.
- **Contrast guard** — extend `scripts/check-contrast.mjs` to assert Alma Mater's pairs too, so a future edit can't regress it.

Files touched are colour/config only — no layout, no className churn, no new components beyond the one selector.
