# Spec 016 — AeroCanvas

> **What this is.** The per-app rubric verdict for `spec-016-aerocanvas`, the AeroCanvas 2D
> computational-fluid wind tunnel that Agent Drafter authored and a reviewer then drove in a browser
> during the 100-app test drive. It records the most severe runtime theme corruption in the corpus.
> **Status:** Historical record — one authoring round, one browser review, closed. The campaign this
> belongs to ended and its defects were remediated; see
> [remediation-results.md](../remediation-results.md). The ledger carries no per-app timestamp, and
> the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-016` is the sixteenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#16-aerocanvas);
the app id Agent Drafter was given is `spec-016-aerocanvas`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

> **Methodology boundary.** This spec is where the campaign's authoring prompts changed: the
> [audit findings register](../audit-findings-register.md) records that the numbered prompts carry an
> anti-template clause **from Spec 016 onward**, added in response to the layout-diversity finding.
> Earlier specs were not re-authored under that clause. See the
> [layout diversity audit](../layout-diversity-audit.md) for the finding and the five controlled
> probes that mitigated it.

> **Domain shorthand.** `AoA` is angle of attack; `Cp` is the pressure coefficient plotted around the
> airfoil surface; `L/D` is the lift-to-drag ratio; *Reynolds-derived* values follow from the
> Reynolds number set in the condition rail; a *polar* is the lift/drag-versus-AoA plot; `CFD HUD` is
> the computational-fluid-dynamics heads-up display the `terminal` theme pack renders. All are from
> the app's brief in the corpus.

## Run metadata

- **App id:** `spec-016-aerocanvas`
- **Authoring rounds:** 1
- **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Wind-tunnel canvas, airfoil controls, polar/Cp figures, and transport dominate. |
| Layout matches (5.3) | ✅ | Required condition rail, fluid canvas, inspector, and transport are present with a concept-specific terminal/HUD treatment. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | AoA fill to 15° immediately reset to 4°; Play did not become Pause; major first-load bindings were blank. |
| Agent-driven loop (5.6) | ⚠️ | Optimize ran consult/notify/app_call and changed narration, but repeated describe/tool sequences and never completed. |
| Multi-agent ran (5.7) | ⚠️ | Two consults ran (Aerodynamicist/Skeptic path); declared Instrumenter was not evidenced. |
| Signals round-trip (5.8) | ❌ | First AoA manipulation reported `aoa_slider` not subscribed. |

## Aesthetic verdict

**Recorded verdict: PARTIAL.**

Baseline strongly matches the `terminal` pack and cyan→amber CFD HUD, but the live agent turn
covered most of the rail/canvas/inspector with opaque black regions, making the app unusable during
optimization.

## Screenshot evidence

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-016/baseline.png`
- `shots/spec-016/optimize-loop.png`

## Friction encountered

- **Authoring.** Drafter built cleanly in one round.
- **Defect — blank first-load bindings.** Initial shared bindings (velocity family, Reynolds-derived
  values, L/D, separation, solve steps) were blank.
- **Defect — canonical state.** Direct AoA manipulation reset from 15° to stale 4°, reproducing the
  canonical-state defect: the client's local state object and the SDK's shared state document
  disagree between turns. The [audit findings register](../audit-findings-register.md) records it as
  *Shared agent state and client control state diverge between turns*, first seen in
  [Spec 004 — Trial Regia](spec-004-trial-regia.md) and also reproduced in
  [Spec 015 — FoldScape](spec-015-foldscape.md).
- **Defect — runtime theme corruption.** The most severe instance in the corpus: almost the whole
  scientific surface became opaque black. The same defect appears in
  [Spec 012 — Contagion Studio](spec-012-contagion-studio.md),
  [Spec 013 — Orbital Sandbox](spec-013-orbital-sandbox.md) and
  [Spec 018 — SystemDynamics Forge](spec-018-systemdynamics-forge.md), against which "most severe" is
  judged.

## Related documentation

- [Spec 015 — FoldScape](spec-015-foldscape.md) — the preceding app, which isolates the same
  canonical-state defect from the state-identity side.
- [Spec 017 — Automata Loom](spec-017-automata-loom.md) — the next app, and the first result to carry
  a platform-integration audit.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated entries for the theme
  corruption and canonical-state defects seen here.
- [Layout diversity audit](../layout-diversity-audit.md) — the finding that added the anti-template
  clause applying from this spec onward.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check and the pass rule applied here.
