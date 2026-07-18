# Layout probe — Tabletop Workbench

- **Archetype:** `workbench`
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24`
- **Static verdict:** pass
- **Layout-diversity verdict:** pass
- **Functional verdict:** partial
- **Aesthetic verdict:** partial at runtime

## Evidence

- The page uses a full-width ruled data tabletop under a horizontal filter
  ribbon; selected detail expands from the bottom edge. There is no persistent
  left rail or right inspector.
- This is materially distinct from the KPI mosaic, centered wizard, radial
  canvas, and all numbered three-column shells.
- Intensity updated 45→73, and row selection expanded the drawer. The drawer's
  selected-row content, mode, and last action became blank while the selected id
  showed B-03, reproducing the shared/local binding split.
- First-use `probe_adjusted` was not subscribed. Both UCSF auditors completed,
  but the main turn repeated `ui_describe` before reaching them and did not
  complete visibly.
- The clean lab-notebook baseline is polished and dense. During the agent turn,
  large opaque black regions covered the heading, ribbon, table, and drawer,
  reproducing the runtime theme corruption from Specs 012–013.

## Screenshots

- `../shots/layout-probe-tabletop-workbench/baseline.png`
- `../shots/layout-probe-tabletop-workbench/interaction.png`
