# Layout probe — Radial Canvas

> **What this is.** The per-probe browser rubric for `layout-probe-radial-canvas`, one of five
> purpose-built probe apps authored to test whether Agent Drafter can produce structurally different
> app layouts — here a `canvas` app built as a full-bleed radial composition with no left or right
> columns, including the theme refinement round it needed to reach its intended dark look.
> **Status:** Historical record — the probe was authored, statically audited, refined once and driven
> in a browser, and the investigation it belongs to is closed. The theme-pack omission it records
> falls in the area addressed by remediation Wave 0.2, the `ThemeConfig::is_default()` manifest
> lossiness fixed by having `read_app` return a resolved view; see
> [remediation-plan.md](../remediation-plan.md) and
> [remediation-results.md](../remediation-results.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

A **layout probe** is not one of the 100 numbered apps in the test drive. The five probes were
commissioned by the [layout diversity audit](../layout-diversity-audit.md) to establish whether the
structural sameness of the numbered apps came from the test corpus — which prescribes a left rail /
center stage / right inspector / bottom transport skeleton in all 100 briefs — or from Agent Drafter
itself. Each probe was given an archetype and a layout constraint that forbids the usual sidebars,
then driven in a browser against the same rubric the numbered apps use. This one was required to be a
`canvas` app: a full-bleed radial canvas with floating tool petals and a modal sheet, and no
left/right columns.

No probe-level timestamp was recorded. The run's only dated event is the 2026-07-12 resolution of the
provider outage described in [azure-403-outage-incident.md](../azure-403-outage-incident.md), so this
probe sits in the July 2026 campaign without a date of its own.

> **Identifier key.** `theme.pack` is a field in the app's `manifest.json`, naming the theme pack
> Agent Drafter applies; `createApp(... theme:"dark")` is the Apps SDK call in the app's own source
> that selects light or dark mode. The two must agree for the intended look, and this probe's first
> build set neither. `probe_adjusted` is the app's own declared signal, named in the same manifest.
> See the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

## Run metadata

| Field | Value |
|---|---|
| App id | `layout-probe-radial-canvas` |
| Archetype | `canvas` |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` |

## Verdicts

The verdict vocabulary is the test-drive rubric's, defined in the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md): functional checks
are PASS / PARTIAL / FAIL (§5) and aesthetic alignment is ALIGNED / PARTIAL / OFF (§6). Static and
layout-diversity verdicts are pass/fail against the probe's authored constraint.

Two axes carry a qualifier: the static and aesthetic verdicts were both reached only after the
corrective theme round described below, not on the first build.

| Axis | Verdict | Qualifier |
|---|---|---|
| Static | pass | after one Agent Drafter theme refinement round |
| Layout diversity | pass | — |
| Functional | partial | — |
| Aesthetic | aligned | after the same refinement round |

## Layout and composition

- The app is a genuine full-bleed radial composition with five orbital tool
  petals, concentric motion rings, one floating control island, and a transient
  modal bottom sheet. It has no persistent sidebar or inspector.

## Direct manipulation

- The slider updated 52→71, changed ring emphasis locally, and emitted
  `probe_adjusted`; the first signal was unsubscribed.
- The Audit petal opened a dismissible dialog sheet. Activation changed the
  local state to Active and updated narration immediately.

## Agent turn and runtime defects

- Both inherited UCSF auditors completed, but the main agent then repeated
  `ui_describe` and never completed its turn.

## The theme refinement round

The refinement happened in three steps, in this order:

1. **Initial build.** The build omitted `theme.pack` and used default light mode.
2. **Refinement.** An exact Agent Drafter refinement persisted `theme.pack="midnight"` and
   `createApp(... theme:"dark")`.
3. **Verified result.** The refined build has the intended deep-space black ground and luminous orbit
   hierarchy.

## Screenshots captured

Three screenshots were taken during the run — the light first build, the active state, and the
refined dark result, which together carried the before/after argument. They were not preserved in
this repository, so the paths below are a record of what was captured rather than live links, and the
refinement above cannot be re-verified visually from this file alone.

- `shots/layout-probe-radial-canvas/baseline.png`
- `shots/layout-probe-radial-canvas/active.png`
- `shots/layout-probe-radial-canvas/refined-dark.png`

## Related documentation

- [Layout diversity audit](../layout-diversity-audit.md) — the investigation that commissioned this
  probe and compares its verdicts against the other four.
- [Remediation plan](../remediation-plan.md) — Wave 0.2 specifies the resolved-manifest fix for the
  default-theme lossiness in the same area as this probe's first build. Note that the register's
  Finding 19, the harness misread of an omitted default pack, is scoped to specs 010 and 015; this
  probe's omitted pack was the non-default `midnight`, and the refinement visibly changed the render.
- [Audit findings register](../audit-findings-register.md) — where the unsubscribed-signal and
  repeated-`ui_describe` defects are recorded once, de-duplicated across every app that hit them.
- [Layout probe static audit data](../data/layout-probe-static-audit.json) — the machine-readable
  static evidence, including this probe's `midnight` theme and declared regions.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the
  functional (§5) and aesthetic (§6) checks these verdicts come from.
