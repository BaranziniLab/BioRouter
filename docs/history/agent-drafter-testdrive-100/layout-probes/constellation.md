# Layout probe — Constellation

> **What this is.** The per-probe browser rubric for `layout-probe-constellation`, one of five
> purpose-built probe apps authored to test whether Agent Drafter can produce structurally different
> app layouts — here an `explorer` app built as a full-bleed network with an overlay command palette
> instead of a persistent inspector.
> **Status:** Historical record — the probe was authored, statically audited and driven in a browser,
> and the investigation it belongs to is closed. The repeated control-plane call it uniquely exposed
> (`ui_subscribe`, at least five times) was fixed by the Wave 4.1 turn guard, reported in
> [remediation-results.md](../remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A **layout probe** is not one of the 100 numbered apps in the test drive. The five probes were
commissioned by the [layout diversity audit](../layout-diversity-audit.md) to establish whether the
structural sameness of the numbered apps came from the test corpus — which prescribes a left rail /
center stage / right inspector / bottom transport skeleton in all 100 briefs — or from Agent Drafter
itself. Each probe was given an archetype and a layout constraint that forbids the usual sidebars,
then driven in a browser against the same rubric the numbered apps use. This one was required to be
an `explorer` app: a full-bleed network with a command-palette overlay and a transient popover
dossier rather than a persistent inspector.

No probe-level timestamp was recorded. The run's only dated event is the 2026-07-12 resolution of the
provider outage described in [azure-403-outage-incident.md](../azure-403-outage-incident.md), so this
probe sits in the July 2026 campaign without a date of its own.

> **Identifier key.** The probe's data model is a constellation of nodes; selecting one is meant to
> populate the selected/readout bindings, and activating one opens that node's dossier — "Nucleus"
> being the node activated during this run. `probe_adjusted` is the app's own declared signal, named
> in its manifest. `ui_subscribe` is one of the Apps SDK control-plane tools an agent calls to
> subscribe to that signal; see the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

## Run metadata

| Field | Value |
|---|---|
| App id | `layout-probe-constellation` |
| Archetype | `explorer` |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` |

## Verdicts

The verdict vocabulary is the test-drive rubric's, defined in the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md): functional checks
are PASS / PARTIAL / FAIL (§5) and aesthetic alignment is ALIGNED / PARTIAL / OFF (§6). Static and
layout-diversity verdicts are pass/fail against the probe's authored constraint.

Two axes carry a qualifier, because a bare verdict would misstate the result. The static verdict was
reached only after one corrective Agent Drafter round, and the aesthetic verdict differs between the
clean baseline and the degraded state during an agent turn.

| Axis | Verdict | Qualifier |
|---|---|---|
| Static | pass | after one Agent Drafter theme refinement round |
| Layout diversity | pass | — |
| Functional | partial | — |
| Aesthetic | aligned | baseline only; partial during the agent turn |

## Layout and composition

- The page is a full-bleed phosphor constellation with a centered translucent
  command palette, anchored transient node dossier, bottom-corner readout, and
  small narration module. It reserves no sidebar or inspector column.
- The `terminal` pack plus `theme:"dark"` refinement produces the intended black
  phosphor ground and is visually unlike every other probe.

## Direct manipulation

- Intensity updated 42→68. Node selection did not populate the selected/readout
  bindings; activation later opened the Nucleus dossier and changed local
  narration/state.

## Agent turn and runtime defects

- First-use `probe_adjusted` was unsubscribed. Both UCSF auditors completed, then
  the main turn repeated `ui_subscribe` at least five times and never completed. This is the only
  probe that looped on `ui_subscribe`; the other four looped on `ui_describe`.

## Aesthetic observations

- During the agent turn, opaque dark rectangles partially clipped the command
  palette text, another form of the runtime theme-region corruption first recorded against
  [spec-012 Contagion Studio](../app-results/spec-012-contagion-studio.md) and reproduced in
  [spec-013 Orbital Sandbox](../app-results/spec-013-orbital-sandbox.md) and the
  [tabletop workbench probe](tabletop-workbench.md).

## Screenshots captured

Two screenshots were taken during the run. They were not preserved in this repository, so the paths
below are a record of what was captured rather than live links.

- `shots/layout-probe-constellation/baseline.png`
- `shots/layout-probe-constellation/active.png`

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned this
  probe and compares its verdicts against the other four.
- [Audit findings register](../audit-findings-register.md) — where the repeated control-plane call
  and the runtime theme-mutation defects are recorded once, de-duplicated.
- [Tabletop workbench probe](tabletop-workbench.md) — the other probe that reproduced the runtime
  theme corruption, in a more severe form.
- [Layout probe static audit data](../data/layout-probe-static-audit.json) — the machine-readable
  static evidence, including this probe's `terminal` theme and declared regions.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the
  functional (§5) and aesthetic (§6) checks these verdicts come from.
