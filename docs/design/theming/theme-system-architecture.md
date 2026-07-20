# Theme system — how it works today, and how to make adding themes cheap

> **What this is.** The architecture of BioRouter's theme system: where a theme actually lives,
> what is generated from what, the token contract, and what a fourth family costs.
> **Status:** Current. Implemented 2026-07-18; §5 is the shipped architecture, and §1–§4 are
> kept as the diagnosis that motivated it. §7 names what is still open.
> **Audience:** developers adding a theme family, or touching any of the generated regions in
> `main.css`, `themes.generated.ts` and `index.html`.

Three families ship — Parchment, Alma Mater and Roche Limit — the team expects more, and themes
stay baked into the app rather than being user-installable. Sections are numbered and cited by
number from the per-family token references, so the numbering is a stable reference scheme.

> §1–§4 below are the diagnosis that motivated the work and are kept as the record of what was
> measured. **§5 is the shipped architecture.** The staged plan it originally proposed was compressed:
> the guard work, the token contract and the generator all landed together, because extracting the
> definitions turned out to be the only safe way to prove the generator faithful.

---

## 1 · Where a theme actually lives today

**Nowhere central.** A theme is not a configuration object, a file, or a plugin. It is a *pattern of
edits* spread across nine files, and it is 100% compile-time — there is no loading, no registry, no
theme artifact. The split is by **rendering technology**, not by concern:

| Layer | Lives in | Why it can't just be CSS |
|---|---|---|
| Semantic UI tokens | `main.css` — `:root[data-theme='x']` + `.dark[data-theme='x']` | — |
| Tailwind utility mapping | `main.css` — `@theme inline` | — |
| Syntax colours | `codeTheme.ts` (TS objects) | `react-syntax-highlighter` takes a JS style object |
| Terminal ANSI-16 | `InAppTerminalDock.tsx` (TS objects) | xterm paints to canvas; cannot read `var()` |
| Boot splash | `index.html` (literal hexes) | paints before the app |
| Family registry | `ThemeContext.tsx` | — |
| Family registry *again* | `index.html` pre-hydration script | can't import TS |
| Label + swatch | `ThemeFamilySelector.tsx` | — |
| Contrast scopes | `scripts/check-contrast.mjs` | — |

### The real cost of a 4th family

| Measure | Count |
|---|---|
| Files touched | **9** |
| Discrete edit sites | **23** |
| Hand-authored colour values | **~220** |
| CSS declarations per family | **147** (87 light + 60 dark) |

Every family **redeclares every token**. The set difference between Alma's and Roche's light blocks
is empty in both directions; so is the difference between a family's own light and dark blocks. Only
17 non-colour tokens (radii, motion, z-index) are inherited.

The cascade rests on specificity `(0,2,0)` beating the bare `:root`/`.dark` `(0,1,0)` — **and on
source order**, because `:root[data-theme='x']` and `.dark[data-theme='x']` have *identical*
specificity. If someone writes the dark block above the light one, dark mode renders light tokens
**and the contrast guard still passes.**

---

## 2 · What is already good (do not break it)

Three things are better than they look and should be the model for everything else:

- **`boot-splash.test.ts` derives its expectations from the source.** It regex-extracts
  `THEME_FAMILIES` from `ThemeContext.tsx` and `FAMILIES` from `index.html` and asserts they match,
  then requires every family to have light + dark splash rules. **Two of the three "duplicated
  family lists" are therefore already CI-guarded** — only `ThemeFamilySelector.tsx` can silently
  drift. *(This corrects an earlier claim of mine that all three were unguarded.)*
- **`ThemeFamilySelector.test.tsx`** is likewise registry-driven.
- **`@theme inline` is load-bearing and correct.** It emits `.bg-sidebar { background-color:
  var(--sidebar) }` and deliberately does **not** emit `--color-sidebar` into the cascade (verified:
  0 occurrences in compiled output). Utilities are late-bound to the semantic token. Reverting to
  plain `@theme` would still work today by accident and would silently break scoped theming, with no
  test failing.

---

## 3 · What is actually broken

### Three live inconsistencies, already shipping at N=3

1. **Roche Limit wears Parchment's scrim.** The modal/diagnostics overlays are hardcoded
   `rgba(32,25,15,0.18)` (warm brown) outside the token layer. Only Alma-light gets a retint
   (`main.css:1533`). No test.
2. **Roche Limit's brand mark contradicts its own splash.** `index.html` rebrands the BR monogram
   for Roche (`--br-coral: #ee6c1a`), but `BioRouterMark.tsx` holds `const NAVY/CORAL/TEAL` as fixed
   module constants and never reads the family. The splash paints orange, React hydrates coral/navy.
3. **The three families disagree about which token the terminal paints** — verified by resolving the
   cascade:

   | Family / mode | Terminal bg | `--background-muted` | `--background-code` | Actually equals |
   |---|---|---|---|---|
   | parchment dark | `#16120c` | `#282217` | `#16120c` | **code** |
   | alma dark | `#0d2a50` | `#0d2a50` | `#08213f` | muted |
   | roche dark | `#232320` | `#232320` | `#1b1b19` | muted |

   `InAppTerminalDock.tsx:75` states *"The ground is --background-muted"* — false for Parchment.
   **This is the trap a naive generator walks into**: any codegen that emits
   `terminal.background = ref('--background-code')` silently re-grounds two terminals under ANSI
   palettes tuned for a different surface. That is the same defect class as the 4.15:1 bug.

### The largest untested surface

**126 hand-tuned terminal hexes across three families, with documented per-stop ratios and not one
assertion.** `InAppTerminalDock.test.tsx` contains zero colour tests.

Roughly half the token vocabulary is never contrast-asserted at all, including `--accent-bar` —
whose own code comment argues at length that it must clear 3:1 on every light ground.

### The guard's own blind spots

- It matches selectors by **exact string equality**. A block written with double quotes or an extra
  space yields `{}`, the scope silently becomes pure Parchment, and ~40 assertions pass while
  measuring the wrong theme.
- It models the cascade **by construction** (`Object.assign({}, LIGHT, DARK, X_L, X_D)`), so it
  cannot see the source-order hazard above.
- The 4.15:1 incident was **a missing assertion, not a cascade bug** — `--background-code` simply had
  no check. Worth stating plainly, because it changes what the fix is.

---

## 4 · Empirical findings (tested, not assumed)

Four experiments in a real browser against the compiled stylesheet:

| Question | Result |
|---|---|
| Can a theme the Tailwind build never saw be added at **runtime**? | **Yes.** Injecting `:root[data-theme='kelp-forest']{--sidebar:…}` at runtime recoloured `bg-sidebar`, `text-sidebar-icon` and `bg-background-accent` correctly. |
| Does a **partial** theme break the UI? | **No.** Omitted tokens fall back to Parchment's `:root` defaults, not to black. A 5-token theme still yields a usable app. |
| Do **dark variants** work at runtime? | **Yes**, via `.dark[data-theme='x']`. |
| Do tokens resolve in a plain `<style>` block? | **Yes** — `var(--background-muted, magenta)` computed `#f2f3f4`. |

That last one **contradicts a comment in `index.html:86-92`**, which asserts that `@theme inline`
"compiles those tokens away — they do not exist as runtime custom properties" and instructs future
readers not to "simplify this back to tokens." The magenta observation behind it was real, but the
cause is almost certainly **stylesheet timing at boot** (Vite injects CSS via JS in dev), not
compilation. The tokens demonstrably exist. This matters: that incorrect belief is what forces every
new family to hand-copy splash hexes into `index.html`.

---

## 5 · The shipped architecture

**One definition per family; everything else generated. Compile-time only — themes are baked into
the app and are not user-installable, by decision.**

```text
ui/desktop/themes/<id>.theme.mjs      the ONE file you write
npm run themes                        emits everything below
npm run themes -- --check             CI gate: fails if generated output is stale
```

### What is generated

| Artifact | What lands there |
|---|---|
| `src/styles/main.css` | the `:root[data-theme=X]` / `.dark[data-theme=X]` token blocks, inside a marker region |
| `src/styles/themes.generated.ts` | syntax palettes, terminal ANSI palettes, brand-mark inks, family manifest, `THEME_FAMILY_IDS` |
| `index.html` | the pre-hydration family list and the per-family boot-splash CSS |

Regions, not whole files: `main.css` and `index.html` carry hand-written reasoning that is not
derivable from a palette, so the generator owns only a delimited span. Parchment's `:root`/`.dark`
blocks stay hand-written — it is the base layer and also carries the 17 structural tokens no theme
may vary.

### What is derived, never authored

These are exactly the values that used to be typed in two-to-four places and drift:

| Derived | From |
|---|---|
| `terminal.background`, `terminal.cursorAccent` | the family's own `terminalGround` token |
| code ground (`CODE_BG*`) | `--background-code` |
| boot-splash `--br-bg` | `--background-muted` |
| picker label + swatch, family list | the definition's `label` / `swatch` / `id` |

`terminalGround` is **per family on purpose**. Parchment dark paints `--background-code`; Alma Mater
and Roche Limit paint `--background-muted`. Assuming they agreed would silently re-ground two
terminals under ANSI palettes tuned for a different surface.

### The contract

`scripts/lib/theme-contract.mjs` is the written-down answer to "what must a theme define": 60
semantic tokens × 2 modes, 27 raw-palette remaps, 10 syntax stops, 19 terminal stops, 3 splash
values. A definition missing any of them **cannot be emitted** — the generator validates first, then
contrast-checks the result, and refuses to write on failure.

That refusal is not theoretical: it caught five Parchment light terminal stops sitting below the AA
floor their own comment claimed they cleared.

### What guards it

- **`check-contrast.mjs` discovers families** by sweeping the stylesheet for `[data-theme='…']`.
  A new family is audited with **zero** edits to the guard. It also asserts light-before-dark block
  order, which is load-bearing and was previously unchecked.
- **`npm run themes -- --check`** is wired into `lint:check`, so stale generated output fails CI.
- **Per-slot terminal floors** (`TERMINAL_FLOORS`) with a recorded reason for every relaxation.

### Cost of a 4th family — measured, not estimated

Verified by actually adding a throwaway family: **one file**, no other edits. The contrast guard
picked it up unprompted (228 → 304 assertions), the type system flagged the two maps that still
needed deriving, and the splash test caught it wearing another family's ground because the demo
copied its surfaces verbatim.

| | Before | After |
|---|---|---|
| Files touched | 9 | **1** |
| Edit sites | 23 | **1** |
| Hand-authored values | ~220 | ~200 (in one place, validated) |
| Hardcoded family lists | 3 | **0** |
| Guard edits | 5 | **0** |

### Migration was proven, not asserted

A one-shot extractor pulled the shipping values into definitions; resolved-token output was then
diffed against a pre-change baseline. **All 104 tokens per family identical; Parchment 77/77
untouched.** The only differences were the two intended ones.

That extractor has since been **deleted**, deliberately. It read the hand-written values out of
`main.css` / `codeTheme.ts` / `InAppTerminalDock.tsx` — which no longer hold them, because those
files are now generated or read from the generated module. Re-running it would have emptied all
three definitions and exited 0. The migration is recorded in commit `74a8fe01`; recovering the tool
means recovering it from there, with fresh eyes on what it reads.

---

## 6 · Decisions taken

1. **Runtime / user-installable themes: rejected.** Technically viable (§4 proves the mechanism
   works), but themes stay baked in by decision. The obvious implementation is also a trap: injecting
   into `@layer user-theme` loses to the existing tokens at every value, because `main.css`'s token
   blocks are unlayered and unlayered beats every layer.
2. **Terminal ground: codified, not unified.** Each family declares which token its terminal paints.
3. **Shadows stay raw strings**, outside the contrast set.
4. **Bright ANSI slots hold 3:1, base slots hold 4.5.** On a light ground "bright" (conventionally
   *lighter*) and AA are mutually exclusive; forcing 4.5 would collapse `brightCyan` into `cyan`.
5. **`--accent-bar` is deliberately NOT asserted.** On the rail's own ground (`--sidebar-active`)
   Parchment measures 2.53, Alma Mater 2.23 and Roche 3.19; on `--background-strong` all three fail
   (2.18 / 2.10 / 2.80). So two of three families have never met 3:1 and the rule has never been
   enforced — asserting it would fail the default theme on day one, not catch a regression. The rail
   reinforces a background change the active row already makes, so it is not the sole cue. Roche's
   doc claimed a guarantee nothing meets; the doc was corrected rather than the themes.

## 7 · Still open

- The `index.html` comment claiming tokens "do not exist as runtime custom properties" is wrong
  (§4). The splash grounds are now generated from `--background-muted`, so the duplication is gone,
  but the comment's reasoning should be corrected.
- `--sidebar-icon` on a navy sidebar, and the scoped `<div data-theme>` live preview for settings,
  remain unbuilt.

## Related documentation

- [Theming](README.md) — the folder index, and the per-family token references this architecture generates.
- [Alma Mater theme tokens](alma-mater-theme-tokens.md) — the UCSF-brand family's token reference.
- [Roche Limit theme tokens](roche-limit-theme.md) — the JupyterLab-inspired family's token reference.
- [Biorouter Design System](../../../design.md) — the parent design system this architecture serves.
