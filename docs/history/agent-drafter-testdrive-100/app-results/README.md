# Per-app rubric verdicts

This folder holds the per-app rubric records from the Agent Drafter Apps SDK v2 100-app test drive:
one Markdown file for each app Agent Drafter authored, `spec-001-variant-tribunal.md` through
`spec-025-lattice.md`. The run really happened — it was a July 2026 campaign that pointed Agent
Drafter at a 100-app specification corpus and drove each finished app in a real browser — but it is
**over**, and every file here is frozen evidence rather than current guidance. Authoring stopped at
spec 025 of 100; specs 026–100 were never authored and have no rubric here. The defects these
rubrics found were de-duplicated into the [audit findings register](../audit-findings-register.md)
and then fixed, so the current truth about the platform lives in
[remediation-results.md](../remediation-results.md) and the
[Apps SDK v2 design](../../../apps-sdk/v2-design.md), not in these files.

Come here when you want the per-check detail behind one app's verdict — what a specific rubric
check saw in the browser, and which defect it isolated. Go elsewhere for anything broader: the
one-row-per-app summary is [authored-app-verdict-index.md](../authored-app-verdict-index.md), the
defects themselves are recorded once in the
[audit findings register](../audit-findings-register.md), the five controlled no-sidebar archetype
probes live in `../layout-probes/`, and the machine-readable static audits
(`spec-NNN-<slug>-static.json`) live in `../authoring-logs/`.

The corpus splits at spec 018. Specs 001–018 were driven in a browser and carry full functional and
aesthetic verdicts; spec 018 is the last browser-verified app. Specs 019–025 hold static audits
only and are permanently frozen at "pending browser verification", because the run stopped and
pivoted to remediation before returning to them. The ledger carries no per-app timestamp; the run's
only dated event is the 2026-07-12 provider-outage resolution recorded in
[azure-403-outage-incident.md](../azure-403-outage-incident.md).

## Documents

| Document | What it covers |
|---|---|
| [spec-001-variant-tribunal.md](spec-001-variant-tribunal.md) | The rubric verdict for `spec-001-variant-tribunal`, the first app authored and browser-verified in the run. |
| [spec-002-cohort-funnel-foundry.md](spec-002-cohort-funnel-foundry.md) | A partial functional pass, plus the repeated-`ui_describe` engine defect first seen in spec 001. |
| [spec-003-pathway-seance.md](spec-003-pathway-seance.md) | A graph workbench whose third authoring round was blocked by the provider outage, leaving the run un-accepted. |
| [spec-004-trial-regia.md](spec-004-trial-regia.md) | A clinical-trial design workbench, documenting a stale-shared-state defect between the UI and the worker agents. |
| [spec-005-omics-loom.md](spec-005-omics-loom.md) | A multi-omics workbench, recording a below-fold transport defect and the loss of the first user signal. |
| [spec-006-ward-board.md](spec-006-ward-board.md) | A clinical board app whose runtime used generic subagents instead of the worker profiles its manifest declared. |
| [spec-007-provenance-autopsy.md](spec-007-provenance-autopsy.md) | A chain-of-custody console, and the first app in the run to achieve a clean signal round-trip. |
| [spec-008-manhattan-signal-room.md](spec-008-manhattan-signal-room.md) | A GWAS workbench, and the record of a fabricated-statistics incident by its main agent. |
| [spec-009-survival-atelier.md](spec-009-survival-atelier.md) | A survival-analysis studio — one of only two outright functional FAILs, caused by a drag-only interaction with no accessible fallback. |
| [spec-010-diagnosis-odyssey.md](spec-010-diagnosis-odyssey.md) | A diagnostic reasoning graph that failed on worker timeouts, plus a correction to the test harness's own theme-audit logic. |
| [spec-011-reaction-diffusion-foundry.md](spec-011-reaction-diffusion-foundry.md) | A live simulation app, and the first result in the run to report static acceptance with runtime still only partial. |
| [spec-012-contagion-studio.md](spec-012-contagion-studio.md) | An epidemic-modelling app, and the first record of the runtime theme corruption that renders app content as opaque black blocks. |
| [spec-013-orbital-sandbox.md](spec-013-orbital-sandbox.md) | The Orbital Sandbox gravitational N-body console: one authoring round, one browser review. |
| [spec-014-serengeti-engine.md](spec-014-serengeti-engine.md) | The Serengeti Engine spatial ecosystem simulator, whose agent narrated a scientific intervention it never actually applied. |
| [spec-015-foldscape.md](spec-015-foldscape.md) | The FoldScape protein energy-landscape explorer, isolating the state-identity defect in which the UI and every worker agent reasoned about different residues. |
| [spec-016-aerocanvas.md](spec-016-aerocanvas.md) | The AeroCanvas 2D computational-fluid wind tunnel, recording the most severe runtime theme corruption in the corpus. |
| [spec-017-automata-loom.md](spec-017-automata-loom.md) | The Automata Loom cellular-automata rule-space explorer, the first result to carry a platform-integration audit of what the app requested versus what it exercised. |
| [spec-018-systemdynamics-forge.md](spec-018-systemdynamics-forge.md) | The SystemDynamics Forge stock-and-flow modeller, whose integration audit records four configuration gaps; the last browser-verified app in the corpus. |
| [spec-019-circuit-bench.md](spec-019-circuit-bench.md) | Static audit only for the Circuit Bench app: the manifest and region cross-check ran, every browser-driven check is unverified. |
| [spec-020-diffusion-delta.md](spec-020-diffusion-delta.md) | Static audit only for the Diffusion Delta dispersion-modelling app; the browser checks were never completed. |
| [spec-021-radiant.md](spec-021-radiant.md) | Static and platform-integration audit for the Radiant knowledge-map app, with Exercised also left pending. |
| [spec-022-crossfire.md](spec-022-crossfire.md) | Static and platform-integration audit for the Crossfire claim-adjudication app; the campaign closed without returning to verify it. |
| [spec-023-longitude.md](spec-023-longitude.md) | Static and platform-integration audit for the Longitude citation-timeline app, frozen after two authoring rounds. |
| [spec-024-quorum.md](spec-024-quorum.md) | Static and platform-integration audit for the Quorum systematic-review board; its browser checks were never completed. |
| [spec-025-lattice.md](spec-025-lattice.md) | Static and integration audit for the Lattice hypothesis-generation app — the last app the test drive authored, and the point at which the run stopped. |

## Related documentation

- [100-app Agent Drafter test drive](../README.md) — the campaign index: reading order, conventions,
  and the full inventory of audits and plans these rubrics feed.
- [Authored-app verdict index](../authored-app-verdict-index.md) — one row per app with its static,
  functional, and aesthetic verdict; start there to find which rubric you want.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated defects behind every
  `partial` and `fail` recorded in this folder, each with symptom, repro, and root cause.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the procedure
  and the rubric that every verdict here was scored against.
