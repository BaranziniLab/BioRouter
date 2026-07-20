# Layout probe — Centered Wizard

> **What this is.** The per-probe browser rubric for `layout-probe-centered-wizard`, one of five
> purpose-built probe apps authored to test whether Agent Drafter can produce structurally different
> app layouts — here a `wizard` app built as a single centered stepper with no sidebars.
> **Status:** Historical record — the probe was authored, statically audited and driven in a browser,
> and the investigation it belongs to is closed. The defects it exposed (split local/shared step
> state, an unsubscribed first signal, and a call to an undeclared action) were folded into the
> [audit findings register](../audit-findings-register.md) and fixed by remediation Waves 3.1–3.3,
> reported in [remediation-results.md](../remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A **layout probe** is not one of the 100 numbered apps in the test drive. The five probes were
commissioned by the [layout diversity audit](../layout-diversity-audit.md) to establish whether the
structural sameness of the numbered apps came from the test corpus — which prescribes a left rail /
center stage / right inspector / bottom transport skeleton in all 100 briefs — or from Agent Drafter
itself. Each probe was given an archetype and a layout constraint that forbids the usual sidebars,
then driven in a browser against the same rubric the numbered apps use. This one was required to be a
`wizard` app: a single centered stepper with progressive disclosure and a full-width completion
canvas, and no sidebars.

No probe-level timestamp was recorded. The run's only dated event is the 2026-07-12 resolution of the
provider outage described in [azure-403-outage-incident.md](../azure-403-outage-incident.md), so this
probe sits in the July 2026 campaign without a date of its own.

> **Identifier key.** `activate_probe` and `probe_adjusted` are the probe app's own declared surface —
> an action and a signal named in its manifest, which the agent is contractually limited to. An
> undeclared name such as `activate_probe_review` is therefore a contract violation, not an
> alternative spelling. `ui_describe` is one of the Apps SDK control-plane tools an agent calls to
> read that surface before acting on it; see the
> [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

## Run metadata

| Field | Value |
|---|---|
| App id | `layout-probe-centered-wizard` |
| Archetype | `wizard` |
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

- The page is a single centered vertical progression with generous paper field,
  a three-stage stepper, progressive disclosure, and no persistent sidebars.
- The `journal` pack and oversized bookish heading are visually and structurally
  distinct from both the numbered three-panel apps and the KPI mosaic probe.

## Direct manipulation

- The mode→intensity→review flow works and intensity updated to 78 locally.

## Agent turn and runtime defects

- The selected mode became blank on the review step, demonstrating local/shared
  step-state loss; `probe_adjusted` was not subscribed on first use.
- The primary button invoked an undeclared `activate_probe_review` turn rather
  than the declared `activate_probe` action.
- The runtime repeated `ui_describe` until declined, then reached only one auditor before the probe
  run was stopped; the button remained disabled and the completion canvas never appeared. The record
  does not say who stopped the run or against what criterion.

## Aesthetic observations

- At 1280×720 the first-step CTA begins below the fold because the hero/header
  consumes too much vertical space, despite the otherwise coherent layout. This is a composition
  defect rather than a functional one, and did not change the aligned aesthetic verdict.

## Screenshots captured

One screenshot was taken during the run. It was not preserved in this repository, so the path below
is a record of what was captured rather than a live link.

- `shots/layout-probe-centered-wizard/baseline.png`

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned this
  probe and compares its verdicts against the other four.
- [Audit findings register](../audit-findings-register.md) — where the unsubscribed-signal, split
  state and undeclared-action defects are recorded once, de-duplicated across every app that hit them.
- [Layout probe static audit data](../data/layout-probe-static-audit.json) — the machine-readable
  static evidence, including this probe's `journal` theme and declared regions.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the
  functional (§5) and aesthetic (§6) checks these verdicts come from.
- [KPI mosaic probe](kpi-mosaic.md) — the `dashboard` probe this one is contrasted against for
  structural distinctness.
