# Theme system — how it works today, and how to make adding themes cheap

**Status:** proposal, awaiting decision · **Date:** 2026-07-18 · **Context:** three families now ship
(Parchment, Alma Mater, Roche Limit); the team expects more.

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

## 5 · Recommendation

**Adopt "one authored source per theme, everything else derived" — but ship it in stages, and reject
runtime/marketplace-installable themes for now.**

Do **not** start with codegen. Start with guard work: ~1 day, closes the only defect class that has
actually shipped, and makes every later stage safe.

### Stage 1 — Assert the duplications, auto-scale the guard · ~1 day

Pure additions. Nothing generated, nothing deleted.

1. Export the resolved cascade maps `check-contrast.mjs` already builds.
2. Assert the cross-file duplications, per family × mode:
   `CODE_BG_*[mode] === resolve('--background-code')`, and the terminal ground against **the token
   that family actually uses** — recorded per family, *not* unified. Parchment-dark uses
   `--background-code`; the others use `--background-muted`.
3. Add the missing assertion families: every syntax stop against its real code ground; every
   non-dim ANSI slot against its terminal ground, with the two documented dim slots exempted by an
   explicit entry carrying a reason.
4. **Replace the six hardcoded scopes with a regex sweep for `\[data-theme='([^']+)'\]`.** A new
   family becomes auto-audited with *zero* edits to this file — the single biggest win of the
   codegen proposals, obtained without a generator.
5. Re-key `ThemeFamilySelector`'s `FAMILIES` as `Record<ThemeFamily, …>` so a missing entry is a
   compile error. Kills the last unguarded list, ~6 lines.

### Stage 2 — One manifest, three consumers · ~0.5 day

Move `label` and `swatch` into `THEME_FAMILIES`. The picker and the boot script derive from it;
`boot-splash.test.ts` keeps cross-checking.

### Stage 3 — Write the token contract down · ~1 day

Produce the actual list (147 declarations, not the "~45" everyone including me assumed) with each
token's role, its required contrast partners, and which are structural vs family-varying. Fix the
three live inconsistencies in §3 while doing it. **Do this before any generator** — both codegen
proposals mispriced themselves 3× by guessing at this number.

### Stage 4 — Codegen · 4–6 days · **conditional**

One `<family>.theme.ts` per theme; a generator emits the CSS blocks, both TS palettes, the picker
manifest, the pre-hydration list and the splash CSS, with contrast validation as a precondition of
emission. **Only build this once a concrete 5th theme exists.** Keep a browser-truth contrast path:
a guard that reads generator JSON instead of the CSS the browser receives makes emitter bugs
invisible — precisely the failure class it exists to catch.

### Rejected for now — runtime / BAAM-installable themes

Technically viable (§4 proves the mechanism), but:

- **The obvious implementation is a no-op.** Injecting into `@layer user-theme` loses to the
  existing tokens at *every* value, because `main.css`'s token blocks are **unlayered** (first
  `@layer` is at line 845, all token blocks are above it) and unlayered beats every layer. Fixing it
  means restratifying a 2,300-line stylesheet.
- **Six tokens per family are inexpressible** in a safe value grammar: the `--shadow-*` set is
  multi-layer compound strings plus the bare keyword `none`. Admitting raw CSS strings reopens the
  validation surface the safety argument depends on.
- **Zero demand.** The BAAM registry carries 37 extensions and 129 skills and no theme requests.
  Break-even on 4–5 weeks of installer/CSP/IPC work is somewhere past theme #7.

Revisit when someone asks for a theme we won't ship ourselves, or two more first-party families
land. At that point Stage 4's output *is* the installable payload.

**Do take one cheap piece now:** a scoped `<div data-theme={candidate}>` live preview in Appearance
settings. Works today because utilities resolve at the use site — but only if the selector drops
`:root` (which matches `<html>` only). ~20 lines.

---

## 6 · Developer experience, before and after

| | Add a 4th theme |
|---|---|
| **Today** | 9 files, 23 edit sites, ~220 hand-authored values, 3 lists to sync (1 unguarded), terminal palette untested, half the tokens unasserted |
| **After Stage 1** | Same authoring, but the contrast guard picks the family up automatically, every duplication is asserted, and the picker list is a compile error if missed |
| **After Stage 3** | Same, plus a written contract saying exactly which values are needed and what each must contrast against |
| **After Stage 4** | One file + one command |

---

## 7 · Open questions

1. **Is there a concrete 4th and 5th theme?** This single input decides whether Stage 4 happens.
2. **Terminal ground: unify or codify?** Unifying means retuning up to three ANSI palettes; codifying
   means a per-family `groundToken` forever. Lean **codify** — it's honest and assertable.
3. **Theme files as TS or JSON?** TS gives compile-time completeness; JSON is version-proof and
   marketplace-ready. If Stage 4 might ever feed a runtime path, JSON wins.
4. **Do shadows and radii stay hand-authored?** Recommend shadows stay raw strings and out of the
   contrast set; radii/motion/z-index leave the per-family blocks entirely.
5. **Should the Alma modal-overlay override become a token?** It is the last per-family component
   rule. Recommend yes, in Stage 3.
6. **Should the brand mark become family-aware?** It is a live bug today (§3.2).
