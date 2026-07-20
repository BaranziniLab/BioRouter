# Spec 018 — SystemDynamics Forge

> **What this is.** The per-app rubric verdict for `spec-018-systemdynamics-forge`, the
> SystemDynamics Forge stock-and-flow modeller that Agent Drafter authored and a reviewer then drove
> in a browser during the 100-app test drive. Its platform-integration audit records four
> configuration gaps.
> **Status:** Historical record — one authoring round, one browser review, closed. This is the last
> browser-verified app in the corpus: specs 019 onward hold static audits only, because the run
> stopped at 25 authored apps and pivoted to the remediation reported in
> [remediation-results.md](../remediation-results.md). The ledger carries no per-app timestamp, and
> the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-018` is the eighteenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#18-systemdynamics-forge);
the app id Agent Drafter was given is `spec-018-systemdynamics-forge`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

> **Two review generations.** "The original reviewer" is the static reviewer as it stood when this
> app was built; it passed the build. "The stricter catalog/route audit" is the later
> identifier-aware audit defined in the
> [platform integration audit](../platform-integration-audit.md), which refuses to credit a manifest
> string as a working capability. The acceptance line below therefore splits: static passed under the
> first, while runtime and integration are partial under the second. The same schema appears in
> [Spec 017 — Automata Loom](spec-017-automata-loom.md); the constant Available and Exercised
> definitions live in the audit, not in each per-app record.

## Run metadata

- **App id:** `spec-018-systemdynamics-forge`
- **Authoring rounds:** 1
- **Reached acceptance:** static under original reviewer; runtime/integration partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Stock-flow canvas, equation/element tools, trajectory strip, loop figures, parameters, and transport dominate. |
| Layout matches (5.3) | ✅ | Required rail/canvas/plot/inspector/transport are present with distinctive paper-and-pipe visual grammar. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | contact_rate fill 0.18→0.30 reset to 0.18 on Run; initial/run bindings were blank and no tuned parameter reached the model. |
| Agent-driven loop (5.6) | ⚠️ | Run and Auto-wire both rendered planning/presence, state/figure frames, and specialist starts, but repeated describes and never reached visible model/action completion. |
| Multi-agent ran (5.7) | ⚠️ | Run used Loop Analyst/Calibrator; Auto-wire started Architect plus both others, but completion was not reached in the bounded observation. |
| Signals round-trip (5.8) | ❌ | Parameter editing produced no observable `parameter_tuned` delivery and reset to stale state. |

## Aesthetic verdict

**Recorded verdict: PARTIAL.**

Baseline is polished and aligned with `biorouter`; live agent rendering created large opaque black
blocks over every major region, making both tested turns unreadable.

## Platform integration

| Field | Finding |
|---|---|
| Requested | System-archetype KB, fast/deep model routes, loop/sensitivity figures, policy-lever workflow. |
| Configured | Real `knowledge` + `autovisualiser`; workflow `policy_lever_sweep`; invented `system-dynamics-archetypes` KB/grant; no routes. |
| Available | Built-in extensions and workflow structure only; the isolated runtime has no KB/skill/connector payload. |
| Exercised | UI figures and workers ran; no KB tool, route, or workflow execution was evidenced. |
| Missing/blocked | KB unavailable/invented and required fast/deep routes absent; the stricter integration audit fails these fields. |

The four gaps the stricter audit counts are recorded individually in
[data/platform-integrations.json](../data/platform-integrations.json):

1. Requested KB capability unavailable in the isolated catalog.
2. Unavailable `knowledge_base`: `system-dynamics-archetypes`.
3. Unavailable KB grants: `system-dynamics-archetypes`.
4. Requested model routes are not configured.

## Screenshot evidence

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-018/baseline.png`
- `shots/spec-018/autowire-run.png`

## Friction encountered

- **Authoring and audit.** The original reviewer passed one clean Drafter round; the stricter
  catalog/route audit now finds the four integration gaps listed above.
- **Defect — placeholder brief.** The visible brief looked populated but was only a placeholder; the
  first Auto-wire click did nothing useful until the brief was explicitly filled.
- **Defects — state, progress, and theme.** Direct parameter state reset, duplicated progress mounted
  into the inspector, and runtime theme corruption obscured the app.

## Related documentation

- [Spec 017 — Automata Loom](spec-017-automata-loom.md) — the preceding app, which introduced this
  platform-integration schema and fails one field rather than four.
- [Spec 019 — Circuit Bench](spec-019-circuit-bench.md) — the first app after the live-verification
  phase ended, and therefore a static audit only.
- [Platform integration audit](../platform-integration-audit.md) — where the audit schema and the
  stricter identifier rules are defined.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated entries for the
  canonical-state, duplicated-progress, and theme-corruption defects seen here.
- [Authored-app verdict index](../authored-app-verdict-index.md) — this verdict, and its integration
  re-audit note, alongside every other app the run produced.
