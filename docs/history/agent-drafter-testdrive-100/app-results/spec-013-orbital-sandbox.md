# Spec 013 — Orbital Sandbox

> **What this is.** The per-app rubric verdict for `spec-013-orbital-sandbox`, the Orbital Sandbox
> gravitational N-body console that Agent Drafter authored and a reviewer then drove in a browser
> during the 100-app test drive.
> **Status:** Historical record — one authoring round, one browser review, closed. The campaign this
> belongs to ended and its defects were remediated; see
> [remediation-results.md](../remediation-results.md). The ledger carries no per-app timestamp, and
> the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-013` is the thirteenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#13-orbital-sandbox);
the app id Agent Drafter was given is `spec-013-orbital-sandbox`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

> **Domain shorthand.** `L4` is the fourth Lagrange point — one of the equilibrium positions in a
> three-body configuration, which the brief's Navigator profile is meant to target.

## Run metadata

- **App id:** `spec-013-orbital-sandbox`
- **Authoring rounds:** 1 real round. Three provider-blocked retries preceded it and are excluded
  from the round count: the harness records a UCSF Azure 403 as `kind=provider-blocked` / rc 75 and
  does not credit it as authoring work — see
  [azure-403-outage-incident.md](../azure-403-outage-incident.md).
- **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | N-body canvas, body editor, elements table, phase-space card, and transport dominate. |
| Layout matches (5.3) | ⚠️ | Required rail/canvas/inspector/transport exist, but the left body list and bottom transport/composer clip at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Add body inserted a fourth body; Step changed positions, velocities, and orbital-element rows. |
| Agent-driven loop (5.6) | ⚠️ | Stabilize produced an L4 plan, specialist consults, notify/highlight frames, but no stabilize/app action and the turn remained in a repeated-describe loop. |
| Multi-agent ran (5.7) | ⚠️ | Navigator and Chaos Auditor consults completed; declared Ephemeris Scribe did not run. |
| Signals round-trip (5.8) | ❌ | First Add body gesture reported `body_added` not subscribed. |

## Aesthetic verdict

**Recorded verdict: PARTIAL.**

The baseline uses the expected `midnight` pack with precise violet orbit styling, but viewport
clipping is substantial and the live run reproduced the opaque-black-region theme corruption seen in
[Spec 012 — Contagion Studio](spec-012-contagion-studio.md). That corruption is registered as its
own defect in the [audit findings register](../audit-findings-register.md).

## Screenshot evidence

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-013/baseline.png`
- `shots/spec-013/stabilize-loop.png`

## Friction encountered

- **Authoring.** One real Drafter round built cleanly in 263.5 seconds after three outage retries.
- **Defect — signal delivery.** The core local simulation works, but first-use signal delivery does
  not.
- **Agent behaviour.** The agent consulted two of three profiles and rendered a plan, then made
  repeated unchanged `ui_describe` calls instead of invoking a declared action.
- **Runtime rendering.** Runtime styling obscured large portions of the rail and inspector and left
  the bottom controls crowded/clipped.

## Related documentation

- [Spec 012 — Contagion Studio](spec-012-contagion-studio.md) — where the runtime theme corruption
  this app reproduced was first seen.
- [Spec 014 — Serengeti Engine](spec-014-serengeti-engine.md) — the next app in the run, showing the
  same plan-without-action failure in the agent loop.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated defect entries behind
  every ⚠️ and ❌ above.
- [Azure 403 outage incident](../azure-403-outage-incident.md) — why three retries for this app are
  excluded from the round count.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check and the pass rule applied here.
