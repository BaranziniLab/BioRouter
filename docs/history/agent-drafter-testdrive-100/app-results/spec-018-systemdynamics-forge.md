# Spec 018 — SystemDynamics Forge
- **App id:** spec-018-systemdynamics-forge
- **Authoring rounds:** 1   **Reached acceptance:** static under original reviewer; runtime/integration partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Stock-flow canvas, equation/element tools, trajectory strip, loop figures, parameters, and transport dominate. |
| Layout matches (5.3) | ✅ | Required rail/canvas/plot/inspector/transport are present with distinctive paper-and-pipe visual grammar. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | contact_rate fill 0.18→0.30 reset to 0.18 on Run; initial/run bindings were blank and no tuned parameter reached the model. |
| Agent-driven loop (5.6) | ⚠️ | Run and Auto-wire both rendered planning/presence, state/figure frames, and specialist starts, but repeated describes and never reached visible model/action completion. |
| Multi-agent ran (5.7) | ⚠️ | Run used Loop Analyst/Calibrator; Auto-wire started Architect plus both others, but completion was not reached in the bounded observation. |
| Signals round-trip (5.8) | ❌ | Parameter editing produced no observable `parameter_tuned` delivery and reset to stale state. |

## Aesthetic verdict: PARTIAL
- Baseline is polished and aligned with `biorouter`; live agent rendering created large opaque black blocks over every major region, making both tested turns unreadable.

## Platform integration
- **Requested:** system-archetype KB, fast/deep model routes, loop/sensitivity figures, policy-lever workflow.
- **Configured:** real `knowledge` + `autovisualiser`; workflow `policy_lever_sweep`; invented `system-dynamics-archetypes` KB/grant; no routes.
- **Available:** built-in extensions and workflow structure only; isolated runtime has no KB/skill/connector payload.
- **Exercised:** UI figures and workers ran; no KB tool, route, or workflow execution was evidenced.
- **Missing/blocked:** KB unavailable/invented and required fast/deep routes absent; stricter integration audit fails these fields.

## Screenshots
- `../shots/spec-018/baseline.png`
- `../shots/spec-018/autowire-run.png`

## Friction encountered
- The original reviewer passed one clean Drafter round; the stricter catalog/route audit now finds four integration gaps.
- The visible brief looked populated but was only a placeholder; first Auto-wire click did nothing useful until explicitly filled.
- Direct parameter state reset, duplicated progress mounted into the inspector, and runtime theme corruption obscured the app.
