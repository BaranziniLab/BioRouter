# Layout probe — KPI Mosaic

- **Archetype:** `dashboard`
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24`
- **Static verdict:** pass
- **Layout-diversity verdict:** pass
- **Functional verdict:** partial
- **Aesthetic verdict:** aligned

## Evidence

- The page uses an asymmetric full-width KPI mosaic, a top command ribbon, a
  horizontal story band, and an overlay bottom narrative drawer.
- No persistent left or right sidebar/rail/inspector is present.
- The clinical pack is visually distinct from the numbered app shells: large
  offset tile spans, generous field, one coral/blue active accent, and drawer
  interaction over the canvas.
- The intensity slider updated 64→80 locally and opened the drawer; initial KPI
  bindings were blank until that first local update.
- The first `probe_adjusted` signal was not subscribed.
- Both inherited UCSF sub-agents (`layout_critic`, `interaction_auditor`) returned
  audit results. The main agent then entered the repeated-`ui_describe` failure
  mode and never called `activate_probe`, so the runtime action criterion is only
  partial.

## Screenshots

- `../shots/layout-probe-kpi-mosaic/baseline.png`
- `../shots/layout-probe-kpi-mosaic/interaction.png`
