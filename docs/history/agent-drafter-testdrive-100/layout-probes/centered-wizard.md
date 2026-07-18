# Layout probe — Centered Wizard

- **Archetype:** `wizard`
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24`
- **Static verdict:** pass
- **Layout-diversity verdict:** pass
- **Functional verdict:** partial
- **Aesthetic verdict:** aligned

## Evidence

- The page is a single centered vertical progression with generous paper field,
  a three-stage stepper, progressive disclosure, and no persistent sidebars.
- The `journal` pack and oversized bookish heading are visually and structurally
  distinct from both the numbered three-panel apps and the KPI mosaic probe.
- The mode→intensity→review flow works and intensity updated to 78 locally.
- The selected mode became blank on the review step, demonstrating local/shared
  step-state loss; `probe_adjusted` was not subscribed on first use.
- The primary button invoked an undeclared `activate_probe_review` turn rather
  than the declared `activate_probe` action. The runtime repeated `ui_describe`
  until declined, then reached only one auditor before the test was stopped; the
  button remained disabled and the completion canvas never appeared.
- At 1280×720 the first-step CTA begins below the fold because the hero/header
  consumes too much vertical space, despite the otherwise coherent layout.

## Screenshot

- `../shots/layout-probe-centered-wizard/baseline.png`
