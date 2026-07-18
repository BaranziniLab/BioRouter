# Layout probe — Radial Canvas

- **Archetype:** `canvas`
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24`
- **Static verdict:** pass after one Agent Drafter theme refinement
- **Layout-diversity verdict:** pass
- **Functional verdict:** partial
- **Aesthetic verdict:** aligned after refinement

## Evidence

- The app is a genuine full-bleed radial composition with five orbital tool
  petals, concentric motion rings, one floating control island, and a transient
  modal bottom sheet. It has no persistent sidebar or inspector.
- The slider updated 52→71, changed ring emphasis locally, and emitted
  `probe_adjusted`; the first signal was unsubscribed.
- The Audit petal opened a dismissible dialog sheet. Activation changed the
  local state to Active and updated narration immediately.
- Both inherited UCSF auditors completed, but the main agent then repeated
  `ui_describe` and never completed its turn.
- The initial build omitted `theme.pack` and used default light mode. An exact
  Agent Drafter refinement persisted `theme.pack="midnight"` and
  `createApp(... theme:"dark")`; the verified result has the intended deep-space
  black ground and luminous orbit hierarchy.

## Screenshots

- `../shots/layout-probe-radial-canvas/baseline.png`
- `../shots/layout-probe-radial-canvas/active.png`
- `../shots/layout-probe-radial-canvas/refined-dark.png`
