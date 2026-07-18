# BioRouter logo redesign — wordmark + BR monogram

**Date:** 2026-07-17
**Status:** Geometry, colour and lockups **approved** in the specimen.
**Blocked:** the typeface. SF Pro is **licence-prohibited for a logo** (§8.1, verified against
Apple's licence text) — a substitute must be picked before any SVG/PNG is generated. Nothing
else in this spec changes when it is.
**Specimen:** `logo-specimen.html` (interactive; served at `localhost:5177` during the session)

---

## 1. Summary

Retire the router glyph. The BioRouter identity becomes **typographic**: a two-colour
wordmark set in the app's own font, plus a **BR monogram** derived from it for the
square app icon.

| Asset | Form | Where it lives |
|---|---|---|
| **Wordmark** — beige ground | `BioRouter` + split rule | headers, landing site, docs, print |
| **Wordmark** — transparent | same, alpha background | in-app |
| **BR monogram** — beige ground | `BR` + split rule, square | dock, favicon, `.icns` / `.ico`, CLI |

The monogram is not a compromise on "just the text" — it *is* just the text, cropped to
the two letters that survive at 16px. It carries the same navy→orange rule as the
wordmark, which is what binds the two together.

---

## 2. Decisions and why

### 2.1 Typeface: bold, outlined — but **not SF Pro** (see §8.1)

> ⚠️ **Superseded in part.** The reasoning below (why the letters must be *outlined*) still
> holds and drives the design. The *choice of SF Pro* does not survive §8.1 — Apple's
> licence prohibits using the font in artwork or on non-Apple platforms. Read this section
> as "a bold grotesque, outlined"; the specific face is pending.

The app's `--font-sans` (`ui/desktop/src/styles/main.css:526`) is a **native stack** —
`ui-sans-serif, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, …` — and the block
is explicit that no webfont is fetched (D-06). That stack resolves to **SF Pro on macOS,
Segoe UI on Windows, Roboto on Linux**.

A logo set in live text would therefore be *a different logo on every OS*, and the PNG
exports would not match the SVG. So the letters are **converted to outlined paths**. The
logo then looks exactly like the app's font on macOS and renders identically everywhere,
with no font dependency and no `@font-face` — which also keeps D-06 intact.

Cost accepted: the text is no longer editable or selectable. Re-generating from source is
the only way to change it.

### 2.2 The square icon is a BR monogram, not the wordmark

This decision was made **from measurement, not taste.** The specimen renders each lockup
at true icon sizes and reads the cap height from the rasteriser
(`TextMetrics.actualBoundingBoxAscent`, measured ratio **0.7046em** for SF Pro Bold).

| Icon size | **BR monogram** | Stacked `Bio`/`Router` | One-line `BioRouter` |
|---|---|---|---|
| 128px | **62.0px cap** | 20.8px | 13.7px |
| 64px | **31.0px** | 10.4px | 6.9px |
| 48px | **23.3px** | 7.8px | 5.2px |
| 32px | **15.5px** | 5.2px | 3.4px |
| **16px (favicon)** | **7.8px** | **2.6px** | **1.7px** |

At 16px the monogram has **3× the stacked cap height and 4.6× the one-line's**. Six letters
cannot render into 1.7px of cap; two letters into 7.8px can, and the favicon mock confirms
`BR` is genuinely readable. The stacked and one-line squares are retained in the specimen
as the evidence for this decision, not as live options.

### 2.3 Colour

| Role | Token | Hex |
|---|---|---|
| `Bio`, and the rule's left half | UCSF Navy | `#052049` |
| `Router`, and the rule's right half | `--color-coral-700` | `#a94f2a` |
| Ground (opaque variant) | `--color-neutral-50` | `#faf8f3` |

`coral-700` was chosen over `coral-600` (`#b85a32`, the app's `--text-accent`/CTA fill)
after comparing all three stops side by side on the real beige ground.

Contrast on beige, live-computed in the specimen: coral-500 **3.34:1**, coral-600
**4.35:1**, coral-700 **5.15:1**. Note that **WCAG 1.4.3 explicitly exempts logotypes**
("text that is part of a logo or brand name has no minimum contrast requirement"), so
coral-600 would not have been a violation — but coral-700 is the only stop that would
clear AA if this were body text, and it is visibly sturdier at small sizes.

**Inverted (navy ground):** `Bio` and the rule's left half flip to beige `#faf8f3`;
`Router` and the right half stay `coral-700`. Without this the navy half of the rule
disappears into the ground and the underline reads as orange-only-starting-midway.

---

## 3. Wordmark specification

```
BioRouter
```

| Property | Value |
|---|---|
| Font | SF Pro Display **Bold** (weight 700), outlined |
| Tracking | **0** |
| `Bio` | `#052049` |
| `Router` | `#a94f2a` |
| Rule — start | left edge of the **`o`** in `Bio` |
| Rule — end | right edge of the **`R`** in `Router` |
| Rule — colour split | at the `Bio`\|`Router` advance boundary |
| Rule — weight | **0.15em** |
| Rule — gap below baseline | **0.21em** (see §3.1) |
| Rule — corner radius | **0** (square terminals) |

The rule's colour split lands on the same x as the letters' colour split, so the bar reads
as an echo of the word above it rather than an independent graphic.

### 3.1 On the 0.21em gap — a correction worth recording

The client approved this geometry with the specimen's **"Bar gap" reading `.00em`**. That
readout was **wrong**. `layoutBar()` offset the rule from `bioRect.bottom`, which under
`line-height:1` is the *font box* bottom — already **0.209em below the baseline** — not the
baseline. So the control's entire 0–0.22 travel actually spanned **0.209em–0.429em**, and
no setting could produce a tight underline.

The specimen now measures the **true baseline** (zero-height inline-block probe) and the
readout is honest. The approved value is recorded here as **0.21em** because that is what
the client actually saw and approved — not `0`.

**Open:** the client's own BR reference image has a gap of roughly **0.09em**, a
conventional underline distance the broken control could not reach. `0.21em` may have been
"as tight as the slider allowed" rather than a preference. **Re-confirm the gap against the
now-honest control before outlining.**

---

## 4. BR monogram specification

```
BR
```

| Property | Value |
|---|---|
| `B` | `#052049`, full size |
| `R` | `#a94f2a`, **72%** of `B`'s font size |
| Baseline | shared — `B` and `R` sit on the same baseline |
| Tracking | 0 |
| Rule | spans the **full lockup width**, splits at the `B`\|`R` boundary |
| Rule weight / gap / radius | as the wordmark: 0.15em / 0.21em / 0 |
| Icon ground | `#faf8f3`, corner radius **20%** (matches the current `icon.svg` `rx=205` on 1024) |
| Lockup width | ≈**76%** of the icon (font-size 352 in a 512 box → 704 in 1024) |

`R` at 72% is the specimen default and matches the client's reference (~68–72%). The
specimen exposes an **R size** control (55–100%) if this wants re-tuning.

---

## 5. Deliverables

**SVG** (letters as `<path>`, no font dependency):

1. `wordmark-beige.svg` — opaque `#faf8f3` ground
2. `wordmark-transparent.svg` — alpha ground
3. `wordmark-inverted.svg` — navy ground variant
4. `monogram-beige.svg` — square, 1024×1024, `rx=205`
5. `monogram-transparent.svg`

**PNG / platform icons**, regenerated from the monogram SVG: `icon.png`, `icon@2x.png`,
`icon.icns`, `icon-light.icns`, `icon.ico`, `iconTemplate*.png`, `landing/icon.png`,
`crates/biorouter-cli/static/img/logo_{dark,light}.png`.

---

## 6. Scope — files this touches

Replacing the glyph is **not** a two-file change. `scripts/check-brand-consistency.sh`
hard-asserts the glyph path `M 125 220` across 8 locations and will fail CI on day one.

**Canonical assets**
- `ui/desktop/src/images/icon.svg` ← canonical, `rect fill="#faf8f3" rx="205"`
- `ui/desktop/src/images/glyph.svg` ← canonical mark
- `ui/desktop/src/images/icon-light.svg`

**Copies the script byte-compares against the canonical**
- `landing/icon.svg`, `landing/video/icon.svg`, `landing/video/assets/icon.svg`, `landing/video/reel/icon.svg`
- `crates/biorouter-cli/static/img/logo_dark.png`, `logo_light.png` (compared to `landing/icon.png`)

**Hard-coded glyph assertions to rewrite**
- `scripts/check-brand-consistency.sh` — `grep -q 'M 125 220'` on `glyph.svg` + `icon.svg`,
  and on `docs/agentic-system.html`, `docs/design-system.html`, `docs/theme-system.html`
- `ui/desktop/src/components/icons/BioRouter.tsx` — script asserts it references `glyph.svg`

**Components**
- `ui/desktop/src/components/BioRouterLogo.tsx` — currently composes `BioRouter` + `Rain`
  with a hover reveal. The `Rain` overlay is glyph-era; decide whether it survives.
- `ui/desktop/src/components/WelcomeBioRouterLogo.tsx`

---

## 7. Verification

- `./scripts/check-brand-consistency.sh` must pass after the rewrite.
- Render the favicon at a true 16px and confirm `BR` is legible (not a smudge).
- Confirm the Intel/ARM `.icns` and the `.ico` regenerate and the dock icon is not blank.
- The specimen's own guard: cap heights come from the rasteriser, so the section's
  "measured, not estimated" claim is literally true.

---

## 8. Open risks

### 8.1 SF Pro **cannot be used for this logo** — licence blocker 🚩 VERIFIED

This was checked against Apple's licence text on 2026-07-17, not assumed.

**Apple San Francisco font licence** (the separate agreement referenced by §1.B of the
Design Resources licence, accepted on download from `developer.apple.com/fonts`):

> "The Apple San Francisco font is to be used **solely for creating mock-ups of user
> interfaces** to be used in software products running on Apple's iOS, OS X or tvOS
> operating systems."

> "You **may not use the Apple Font to create, develop, display or otherwise distribute
> any documentation, artwork, website content or any other work product**."

**Apple Design Resources Licence** (`Apple-Design-Resources-License-20230621`), §2.B
*Other Use Restrictions*, verbatim:

> "The grants set forth in this License do not permit you to, and you agree not to,
> install, use or run the Apple Design Resources for the purpose of creating mock-ups of
> user interfaces to be used in software products running on **any non-Apple operating
> system software**. You may not embed the Apple Design Resources in any software programs
> or other products. Except as expressly provided for herein, you may not use the Apple
> Design Resources to create, develop, display or otherwise distribute any documentation,
> artwork, website content or any other work product."

And §2.D *No Reverse Engineering; Limitations*:

> "You may not … **create derivative works** of the Apple Design Resources or any part
> thereof."

**Why every clause bites this specific project:**

| What we planned | Clause it hits |
|---|---|
| A **logo** — not a UI mock-up | "solely for creating mock-ups of user interfaces" |
| Ships on **Windows + Linux** | "running on any non-Apple operating system software" |
| Ships on the public **landing site** | "may not … distribute any … website content" |
| **Outlining the glyphs to paths** | "may not … create derivative works" |

The outlining step, which I proposed as the portability fix, is itself the clearest
violation. **SF Pro is out for the logo.** No legal review is needed to reach that
conclusion — it is unambiguous on the face of the licence.

**Important scope note — this does *not* affect the app's UI.** `--font-sans` resolving to
SF Pro on macOS via `-apple-system` is the OS supplying its own system font to a native
app, which is normal and unaffected. Only *baking SF outlines into a distributed brand
asset* is prohibited.

**The design survives; only the typeface changes.** The brief's intent — "the main font of
the app" — is about *matching the app*, and the app's font is a **stack**, not SF
specifically. On Windows that stack is already Segoe UI, on Linux Roboto; there has never
been one true BioRouter letterform. Substitutes that are SIL OFL (logo-safe) and sit
naturally beside SF Pro:

| Font | Note |
|---|---|
| **Inter** | Deliberately SF-adjacent; the closest match, licence-clean (OFL) |
| **Public Sans** | USWDS; a neutral, institutional grotesque |
| **Source Sans 3** | Adobe, OFL; slightly warmer |
| **IBM Plex Sans** | More character; reads less "system default" |

Everything else in this spec — the colour split, the rule geometry, the monogram, and
every measurement in §2.2 — carries over unchanged. **Next step: re-render the specimen
with these candidates and pick.**

### 8.2 `coral-700` is a theme token, not a brand constant

`--color-coral-700` is re-defined by the **Alma Mater** theme family
(`main.css:286`), which recolours Parchment's coral to UCSF Violet/Eggplant. The logo must
use the **literal hex `#a94f2a`**, not `var(--color-coral-700)`, or the wordmark will turn
purple under that theme.

### 8.3 The gap value

See §3.1 — re-confirm 0.21em vs ~0.09em against the fixed control.

---

## 9. Rejected alternatives

| Option | Why rejected |
|---|---|
| Keep the router glyph as the app icon | Client wants the glyph retired entirely. |
| Stacked `Bio`/`Router` square | 2.6px cap at 16px — illegible. Superseded by the monogram. |
| One-line `BioRouter` squeezed into the square | 1.7px cap at 16px — the worst option measured. |
| Live `<text>` in the system stack | A different logo per OS; PNG ≠ SVG on Windows. |
| An embedded webfont | Violates D-06 (no webfont), and would not be the app's font anyway. |
