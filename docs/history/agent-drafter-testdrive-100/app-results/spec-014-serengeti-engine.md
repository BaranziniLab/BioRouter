# Spec 014 — Serengeti Engine

> **What this is.** The per-app rubric verdict for `spec-014-serengeti-engine`, the Serengeti Engine
> spatial ecosystem simulator that Agent Drafter authored and a reviewer then drove in a browser
> during the 100-app test drive. Its headline finding is an agent that narrated a scientific
> intervention it never actually applied.
> **Status:** Historical record — one authoring round, one browser review, closed. The campaign this
> belongs to ended and its defects were remediated; see
> [remediation-results.md](../remediation-results.md). The ledger carries no per-app timestamp, and
> the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-014` is the fourteenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#14-serengeti-engine);
the app id Agent Drafter was given is `spec-014-serengeti-engine`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

## Run metadata

- **App id:** `spec-014-serengeti-engine`
- **Authoring rounds:** 1
- **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | A spatial ecosystem canvas, habitat/species controls, scientific plots/table, and transport dominate. |
| Layout matches (5.3) | ✅ | The requested 250px species rail, full-bleed grid, 340px inspector, and bottom transport are present and coherent. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Brush selection, canvas painting, seed random, and Play→Pause controls responded; simulation/field-note UI changed locally. |
| Agent-driven loop (5.6) | ⚠️ | Balance rendered a plan, highlights, and field-note patch, but did not invoke habitat/rate/simulation actions; lion vision stayed 0.68 despite the announced 0.52 plan. |
| Multi-agent ran (5.7) | ✅ | Ranger, Invasive Species, and Naturalist consults all completed. |
| Signals round-trip (5.8) | ❌ | First terrain-paint gesture reported `terrain_painted` not subscribed. |

> **Headline finding.** Check 5.6 above is the one this app is cited for: the agent announced a lion
> vision change to 0.52 while the underlying value stayed at 0.68. The
> [audit findings register](../audit-findings-register.md) records it as *Agent renders an
> intervention plan without invoking the declared action*, alongside the same failure in
> [Spec 011](spec-011-reaction-diffusion-foundry.md) and
> [Spec 013](spec-013-orbital-sandbox.md).

## Aesthetic verdict

**Recorded verdict: ALIGNED.**

The `journal` pack is strongly expressed: warm parchment, serif captions, muted terrain, organic
motes, and a field-note intervention card. Unlike
[Spec 012 — Contagion Studio](spec-012-contagion-studio.md) and
[Spec 013 — Orbital Sandbox](spec-013-orbital-sandbox.md), live rendering remained legible.

## Screenshots

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-014/baseline.png`
- `shots/spec-014/balance-run.png`

## Friction encountered

- **Authoring.** The first Drafter round built cleanly but took 367.2 seconds. Build durations are
  copied from the run ledger at [data/ledger.json](../data/ledger.json), which records them to a
  tenth of a second.
- **Defect — narrative-only intervention.** All named workers ran and the main agent patched
  explanatory UI, yet the promised scientific intervention was narrative-only: no declared action
  ran, and lion vision stayed at 0.68 rather than the announced 0.52.
- **Agent behaviour.** One unchanged `ui_describe` occurred after the completed consult/patch
  sequence while the page remained `AI · updating data`.

## Related documentation

- [Spec 013 — Orbital Sandbox](spec-013-orbital-sandbox.md) — the preceding app, which failed the
  agent loop the same way and additionally showed theme corruption.
- [Spec 015 — FoldScape](spec-015-foldscape.md) — the next app in the run, where the same loop
  failure is compounded by a state-identity defect.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated defect entry for the
  plan-without-action failure and for the lost first signal.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check and the pass rule applied here.
- [Authored-app verdict index](../authored-app-verdict-index.md) — this verdict in the context of
  every other app the run produced.
