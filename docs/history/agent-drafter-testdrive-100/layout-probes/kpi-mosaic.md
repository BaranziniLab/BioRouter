# Layout probe — KPI Mosaic

> **What this is.** The per-probe browser rubric for `layout-probe-kpi-mosaic`, one of five
> purpose-built probe apps authored to test whether Agent Drafter can produce structurally different
> app layouts — here a `dashboard` app with no persistent sidebars.
> **Status:** Historical record — the probe was authored, statically audited and driven in a browser,
> and the investigation it belongs to is closed. The two runtime defects it records (the unsubscribed
> first signal and the repeated `ui_describe` loop) were fixed by remediation Waves 3.1 and 4.1, as
> reported in [remediation-results.md](../remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A **layout probe** is not one of the 100 numbered apps in the test drive. The five probes were
commissioned by the [layout diversity audit](../layout-diversity-audit.md) to answer a single
question: is the sameness of the numbered apps caused by the test corpus, which prescribes a
left rail / center stage / right inspector / bottom transport skeleton in all 100 briefs, or by
Agent Drafter itself? Each probe was given an archetype and a layout constraint that forbids the
usual sidebars, then driven in a browser against the same rubric the numbered apps use. This one was
required to be a `dashboard` app built as an asymmetric KPI mosaic with a top command ribbon and a
bottom narrative drawer, and no persistent sidebars.

No probe-level timestamp was recorded. The run's only dated event is the 2026-07-12 resolution of the
provider outage described in [azure-403-outage-incident.md](../azure-403-outage-incident.md), so this
probe sits in the July 2026 campaign without a date of its own.

> **Identifier key.** `activate_probe` and `probe_adjusted` are the probe app's own declared surface —
> an action and a signal named in its manifest. `ui_describe` is one of the Apps SDK control-plane
> tools an agent calls to read that surface before acting on it; see the
> [Apps SDK reference](../../../apps-sdk/sdk-reference.md). `layout_critic` and `interaction_auditor`
> are the two inherited UCSF worker profiles the probe apps were configured with.

## Run metadata

| Field | Value |
|---|---|
| App id | `layout-probe-kpi-mosaic` |
| Archetype | `dashboard` |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` |

## Verdicts

The verdict vocabulary is the test-drive rubric's, defined in the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md): functional checks
are PASS / PARTIAL / FAIL (§5) and aesthetic alignment is ALIGNED / PARTIAL / OFF (§6). Static and
layout-diversity verdicts are pass/fail against the probe's authored constraint.

| Axis | Verdict |
|---|---|
| Static | pass |
| Layout diversity | pass |
| Functional | partial |
| Aesthetic | aligned |

## Layout and composition

- The page uses an asymmetric full-width KPI mosaic, a top command ribbon, a
  horizontal story band, and an overlay bottom narrative drawer.
- No persistent left or right sidebar/rail/inspector is present.
- The clinical pack is visually distinct from the numbered app shells: large
  offset tile spans, generous field, one coral/blue active accent, and drawer
  interaction over the canvas.

## Direct manipulation

- The intensity slider updated 64→80 locally and opened the drawer; initial KPI
  bindings were blank until that first local update.

## Agent turn and runtime defects

- The first `probe_adjusted` signal was not subscribed.
- Both inherited UCSF sub-agents (`layout_critic`, `interaction_auditor`) returned
  audit results. The main agent then entered the repeated-`ui_describe` failure
  mode — reissuing the same control-plane call instead of progressing — and never called
  `activate_probe`, so the runtime action criterion is only partial.

## Screenshots captured

Two screenshots were taken during the run. They were not preserved in this repository, so the paths
below are a record of what was captured rather than live links.

- `shots/layout-probe-kpi-mosaic/baseline.png`
- `shots/layout-probe-kpi-mosaic/interaction.png`

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned this
  probe and compares its verdicts against the other four.
- [Audit findings register](../audit-findings-register.md) — where the unsubscribed-signal and
  repeated-control-plane-call defects are recorded once, de-duplicated across every app that hit them.
- [Layout probe static audit data](../data/layout-probe-static-audit.json) — the machine-readable
  static evidence, including this probe's `clinical` theme and declared regions.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the
  functional (§5) and aesthetic (§6) checks these verdicts come from.
- [Constellation probe](constellation.md) — the `explorer` probe, the only one that repeated
  `ui_subscribe` rather than `ui_describe`.
