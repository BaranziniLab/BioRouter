# Spec 012 — Contagion Studio
- **App id:** spec-012-contagion-studio
- **Authoring rounds:** 1 real round (plus 3 provider-blocked retries, excluded)   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Live S/E/I/R plot, rates rail, KPIs, scenario table, intervention track, and transport dominate. |
| Layout matches (5.3) | ⚠️ | Three-column control-room structure is present, but the lower case-data/composer area and transport labels clip at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | β manipulation repainted the curve/KPIs; Add intervention inserted a school-closure marker; Fit to data changed peak and attack-rate values. |
| Agent-driven loop (5.6) | ⚠️ | Fit to data invoked `app_call` and changed outputs, but both tested turns repeated the same orchestration sequence and never completed. |
| Multi-agent ran (5.7) | ❌ | Manifest declares Fitter, Adversary, Policy Analyst, and Reporter, but runtime used two generic `subagent` calls; no declared-profile consults were visible. |
| Signals round-trip (5.8) | ❌ | First intervention gesture reported `marker_dragged` not subscribed. |

## Aesthetic verdict: PARTIAL
- Baseline follows the `clinical` pack with crisp, dense white/steel/coral treatment, but the live agent theme/render pass produced large black, illegible blocks over plot/KPI/table content.

## Screenshots
- `../shots/spec-012/baseline.png`
- `../shots/spec-012/fit-loop.png`

## Friction encountered
- One real Drafter round built cleanly in 275.8 seconds after three outage retries.
- Runtime bypassed all four declared profiles with generic subagents.
- Both Add intervention and Fit to data remained `AI · updating data`; the describe/subscribe/notify/highlight/subagent sequence repeated.
- Runtime theming made major content areas visually unreadable, despite a good baseline.
