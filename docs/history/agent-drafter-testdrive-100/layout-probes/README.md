# Layout probes

This folder holds the five per-probe browser rubrics from the layout diversity investigation that ran
inside the 100-app Agent Drafter test drive. The probes were real: all five apps were authored
through named Agent Drafter sessions, statically audited, and driven in a desktop browser against the
same rubric the numbered apps use. The investigation is closed and its conclusion still stands —
Agent Drafter can generate materially different layouts when the prompt asks for them, so the
structural sameness of the numbered apps was a test-corpus constraint, not a platform limitation. The
runtime defects the probes exposed no longer describe current behaviour: every one of them was folded
into the [audit findings register](../audit-findings-register.md) and fixed by the remediation waves
reported in [remediation-results.md](../remediation-results.md). Read these files as evidence for
what was observed during the run, not as guidance on how Agent Drafter behaves today.

Come here when you want the detailed browser evidence for one specific probe — its layout and
composition, its verdicts, the defect it reproduced. If you want the argument the probes were built
to settle, or the summary table comparing all five, read
[layout-diversity-audit.md](../layout-diversity-audit.md) instead; this folder is its appendix. If
you want rubrics for the numbered corpus apps, those are in `../app-results/` — a layout probe is
**not** one of the 100 numbered apps. For current Agent Drafter and Apps SDK behaviour, go to
[the Apps SDK reference](../../../apps-sdk/sdk-reference.md), not here. No probe carries a timestamp
of its own; the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
[azure-403-outage-incident.md](../azure-403-outage-incident.md), so all five sit in the July 2026
campaign.

## Documents in this folder

| Document | What it covers |
|---|---|
| [centered-wizard.md](centered-wizard.md) | The browser rubric for `layout-probe-centered-wizard`, a `wizard` app built as a single centered stepper with no sidebars; it exposed split local/shared step state, an unsubscribed first signal, and a call to an undeclared action. |
| [constellation.md](constellation.md) | The browser rubric for `layout-probe-constellation`, an `explorer` app built as a full-bleed network with an overlay command palette instead of a persistent inspector; it uniquely exposed a repeated `ui_subscribe` control-plane call, fixed by the Wave 4.1 turn guard. |
| [kpi-mosaic.md](kpi-mosaic.md) | The browser rubric for `layout-probe-kpi-mosaic`, a `dashboard` app with no persistent sidebars; it records two runtime defects — the unsubscribed first signal and a repeated `ui_describe` loop — fixed by remediation Waves 3.1 and 4.1. |
| [radial-canvas.md](radial-canvas.md) | The browser rubric for `layout-probe-radial-canvas`, a `canvas` app built as a full-bleed radial composition with no left or right columns, including the theme refinement round it needed to reach its intended dark look; the theme-pack omission falls in the area addressed by Wave 0.2. |
| [tabletop-workbench.md](tabletop-workbench.md) | The browser rubric for `layout-probe-tabletop-workbench`, a `workbench` app built as a full-width tabletop with no left rail or right inspector; it reproduced the shared/local binding split and runtime theme corruption, addressed by Waves 3.2 and 4.5. |

Every probe passed its static and layout-diversity checks and scored `partial` on the functional
axis. Each file states its own verdicts, including the qualifiers where a bare verdict would misstate
the result.

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned these
  five probes, with the corpus finding and the comparison table across all of them.
- [Test drive README](../README.md) — the index and reading order for the whole campaign these probes
  belong to.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated defect register these
  rubrics fed; the runtime defects the probes reproduced are findings 6, 8, 12, and 13.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the rubric
  whose PASS / PARTIAL / FAIL and ALIGNED / PARTIAL / OFF vocabulary every probe verdict uses.
