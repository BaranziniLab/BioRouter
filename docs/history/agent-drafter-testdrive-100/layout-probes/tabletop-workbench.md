# Layout probe — Tabletop Workbench

> **What this is.** The per-probe browser rubric for `layout-probe-tabletop-workbench`, one of five
> purpose-built probe apps authored to test whether Agent Drafter can produce structurally different
> app layouts — here a `workbench` app built as a full-width tabletop with no left rail or right
> inspector.
> **Status:** Historical record — the probe was authored, statically audited and driven in a browser,
> and the investigation it belongs to is closed. The two defects it reproduced (the shared/local
> binding split and the runtime theme corruption) were addressed by remediation Waves 3.2 and 4.5,
> reported in [remediation-results.md](../remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A **layout probe** is not one of the 100 numbered apps in the test drive. The five probes were
commissioned by the [layout diversity audit](../layout-diversity-audit.md) to establish whether the
structural sameness of the numbered apps came from the test corpus — which prescribes a left rail /
center stage / right inspector / bottom transport skeleton in all 100 briefs — or from Agent Drafter
itself. Each probe was given an archetype and a layout constraint that forbids the usual sidebars,
then driven in a browser against the same rubric the numbered apps use. This one was required to be a
`workbench` app: a full-width table under a top filter ribbon, with an expandable bottom detail
drawer.

No probe-level timestamp was recorded. The run's only dated event is the 2026-07-12 resolution of the
provider outage described in [azure-403-outage-incident.md](../azure-403-outage-incident.md), so this
probe sits in the July 2026 campaign without a date of its own.

> **Identifier key.** `B-03` is a row id in the probe app's own synthetic table fixture; it is quoted
> below only to show that the drawer kept the selected id while losing every value bound to it.
> `probe_adjusted` is the app's own declared signal, named in its manifest. `ui_describe` is one of
> the Apps SDK control-plane tools an agent calls to read the declared surface before acting on it;
> see the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

## Run metadata

| Field | Value |
|---|---|
| App id | `layout-probe-tabletop-workbench` |
| Archetype | `workbench` |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` |

## Verdicts

The verdict vocabulary is the test-drive rubric's, defined in the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md): functional checks
are PASS / PARTIAL / FAIL (§5) and aesthetic alignment is ALIGNED / PARTIAL / OFF (§6). Static and
layout-diversity verdicts are pass/fail against the probe's authored constraint.

The aesthetic axis carries a qualifier: the clean baseline and the state during an agent turn were
assessed differently, and only the agent-turn state is degraded.

| Axis | Verdict | Qualifier |
|---|---|---|
| Static | pass | — |
| Layout diversity | pass | — |
| Functional | partial | — |
| Aesthetic | partial | baseline is clean; the degradation appears during the agent turn |

## Layout and composition

- The page uses a full-width ruled data tabletop under a horizontal filter
  ribbon; selected detail expands from the bottom edge. There is no persistent
  left rail or right inspector.
- This is materially distinct from the [KPI mosaic](kpi-mosaic.md),
  [centered wizard](centered-wizard.md), [radial canvas](radial-canvas.md), and all numbered
  three-column shells — the left rail / center stage / right inspector shape that every one of the
  100 corpus briefs prescribes.

## Direct manipulation

- Intensity updated 45→73, and row selection expanded the drawer. The drawer's
  selected-row content, mode, and last action became blank while the selected id
  showed `B-03`, reproducing the shared/local binding split.

## Agent turn and runtime defects

- First-use `probe_adjusted` was not subscribed. Both UCSF auditors completed,
  but the main turn repeated `ui_describe` before reaching them and did not
  complete visibly.

## Aesthetic observations

The two states were assessed separately, and they disagree:

- **Clean baseline.** The lab-notebook baseline is polished and dense.
- **During the agent turn.** Large opaque black regions covered the heading, ribbon, table, and
  drawer, reproducing the runtime theme corruption first recorded against
  [spec-012 Contagion Studio](../app-results/spec-012-contagion-studio.md) and
  [spec-013 Orbital Sandbox](../app-results/spec-013-orbital-sandbox.md).

## Screenshots captured

Two screenshots were taken during the run, one per aesthetic state. They were not preserved in this
repository, so the paths below are a record of what was captured rather than live links.

- `shots/layout-probe-tabletop-workbench/baseline.png`
- `shots/layout-probe-tabletop-workbench/interaction.png`

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned this
  probe and compares its verdicts against the other four.
- [Audit findings register](../audit-findings-register.md) — where the shared/local state split and
  the runtime theme-mutation defects are recorded once, de-duplicated across every app that hit them.
- [Constellation probe](constellation.md) — the other probe that reproduced the runtime theme
  corruption, in a milder form confined to the command palette.
- [Layout probe static audit data](../data/layout-probe-static-audit.json) — the machine-readable
  static evidence, including this probe's `lab-notebook` theme and declared regions.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the
  functional (§5) and aesthetic (§6) checks these verdicts come from.
