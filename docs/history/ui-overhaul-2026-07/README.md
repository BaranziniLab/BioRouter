# UI overhaul, July 2026 — view redesigns

This folder holds the two **view-level** redesigns specified during BioRouter's July 2026
desktop UI overhaul: the Home page and the Knowledge view. **Both happened and both are
still in force** — each was signed off (2026-07-08 and 2026-07-10), implemented, and
verified in the running app, so the desktop you use today looks the way these documents
specify. They are archived rather than live because their work is finished: nothing in
either is an outstanding plan, and the values they fix are restatements of the design
system, not an independent source of truth.

Read them to recover *why* a Home or Knowledge surface is shaped the way it is — what the
token and session counters on Home actually measured, why the usage heatmap replaced the
tiles, which radius a Knowledge control gets and on what rule — or to decode an `H-NN` or
`K-NN` decision cited in a commit. Do not read them as current specifications: the
authority on every value is the root
[BioRouter design system](../../../design.md), which both documents cite rather than
restate, and if the app has since drifted, the code and `design.md` win. The **app-wide**
half of the same overhaul — the cohesion specification for the shell and its still-open
status record — was not archived and remains in
[`docs/design/ui-overhaul/`](../../design/ui-overhaul/README.md).

> **Identifier key.** `H-01`…`H-08` are the eight sign-off decisions in the Home screen
> redesign; `K-01`…`K-08` are the eight in the Knowledge view redesign. Each document
> defines its own set. `D-NN` and `DR-NN` identifiers cited in either belong to the design
> system's decision and drift registers in [`design.md`](../../../design.md).

## Documents

| Document | What it covers |
|---|---|
| [Home screen redesign](home-screen-redesign.md) | Why the Home column was realigned to the chat column, what the token and session numbers on Home actually meant, and the eight decisions (`H-01`…`H-08`) behind the usage heatmap that replaced the flat tiles. Historical record — signed off 2026-07-08; all seven implementation steps done and shipped, including a `token_events` table (schema v10) whose migration was verified against the real session database. |
| [Knowledge view redesign](knowledge-view-redesign.md) | The three defects diagnosed in the Knowledge view — radii rounded by guess, two columns speaking different surface languages, depth faked with tinted glass — and the radius, surface and component specifications that correct them, plus the eight sign-off decisions `K-01`…`K-08`. Historical record — signed off 2026-07-10 with every decision accepted as option A; all eight execution steps shipped across 14 front-end files, verified in both themes and by adversarial review. |

## Rendered pages

Each document above is the written companion to an HTML page beside it. **These pages must
be opened in a browser to be useful** — they carry the pixels, and Markdown cannot
reproduce them. The Markdown companions carry the reasoning and the exact values, so anyone
working without a browser should read those instead.

- `home-screen-redesign.html` — live before/after mockups of the Home page, the interactive
  heatmap with hover and keyboard tooltips, the intensity-formula histograms, the
  width-comparison bars, and theme-switchable colour swatches.
- `knowledge-view-redesign.html` — before/after mockups built live from the app's own colour
  tokens in both light and dark themes, and the radius ladder shown as real swatches.

## Related documentation

- [UI overhaul](../../design/ui-overhaul/README.md) — the app-wide half of the same July 2026 overhaul: the shell cohesion specification and the execution status record that still carries open items.
- [BioRouter design system](../../../design.md) — the Parchment palette, the radius ladder, and the `D-NN` and `DR-NN` registers that both documents here cite as their source of values.
- [Design](../../design/README.md) — the live visual design specifications these two redesigns were written alongside: brand marks, theme families and the design-system gallery.
- [Historical records](../README.md) — the rest of BioRouter's archive, and how to check any archived document's standing.
