# BioRouter branding

This folder holds the BioRouter brand identity: the two-colour `BioRouter` wordmark with its
split navy/coral rule, the `BR` monogram derived from it for the square app icon, the
interactive studios used to tune both, and the exported icon files. It covers the geometry,
colour tokens, lockups and typeface licensing behind the marks — what the logo *is*, and what
constrains it.

Come here when you are regenerating, exporting or reviewing a BioRouter logo asset, or when you
need to know why a mark is shaped the way it is before changing it. Go elsewhere for two
neighbouring concerns that are easy to confuse with branding. Colour **tokens** — the palette the
app themes read, including the `--color-coral-700` value this spec deliberately pins as a literal
hex — live in [`../theming/`](../theming/). The **rollout** of these marks into the app, landing
site and platform icon files is tracked in
[`../ui-overhaul/execution-status.md`](../ui-overhaul/execution-status.md), which is also
authoritative on the typeface decision that postdates the spec below.

## Documents

| Document | What it covers |
|---|---|
| [logo-and-wordmark-spec.md](logo-and-wordmark-spec.md) | The normative specification for the wordmark and the `BR` monogram — geometry, colour tokens, lockups, deliverables, the files a glyph replacement touches, and the licence blocker that ruled out SF Pro. Current for geometry, colour and lockups (approved 2026-07-17); its typeface section is superseded — Inter (SIL Open Font License) was chosen and shipped on 2026-07-18, recorded in [`../ui-overhaul/execution-status.md`](../ui-overhaul/execution-status.md). |

## Subdirectories

- [`assets/`](assets/) — the exported `BR` mark files, in beige and transparent variants
  (`br-icon-beige.svg` / `.png`, `br-icon-transparent.svg` / `.png`) plus a `br-icon-review.png`
  review render. Images only; the folder carries no index of its own. The SVGs are set in Inter
  at weight 800 with the letters as paths, and record their approved studio dials in a comment
  header. These are the same assets that shipped to
  `ui/desktop/src/images/br-icon-{beige,transparent}.{svg,png}`. The PNGs are browser-rendered at
  1024×1024 rather than `sips`-rendered, because `sips` flattens font weight.

## Interactive studios

Two self-contained HTML studios sit beside the spec. **Both need a browser to be useful** — they
are live tuning tools with dials, not documents, and reading their source tells you nothing.

- `logo-wordmark-studio.html` — the wordmark studio, for tuning the horizontal `BioRouter`
  wordmark's weight, tracking, underline gap and vertical position.
- `logo-icon-studio.html` — the icon centring studio, for the `BR` mark's size and placement
  inside the square icon plate.

## Related documentation

- [UI overhaul execution status](../ui-overhaul/execution-status.md) — records how the typeface
  blocker was resolved, which values were re-tuned in Inter, and how the marks shipped; treat it
  as authoritative wherever it disagrees with the spec here.
- [Alma Mater theme tokens](../theming/alma-mater-theme-tokens.md) — the UCSF-brand theme family
  that re-defines `--color-coral-700`, which is why the branding spec pins a literal hex instead
  of referencing the token.
- [UI cohesion redesign](../ui-overhaul/ui-cohesion-redesign.md) — the app-wide visual
  specification the brand marks were rolled into, for the surrounding UI context.
