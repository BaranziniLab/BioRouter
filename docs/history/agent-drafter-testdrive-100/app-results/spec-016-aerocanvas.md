# Spec 016 — AeroCanvas
- **App id:** spec-016-aerocanvas
- **Authoring rounds:** 1   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Wind-tunnel canvas, airfoil controls, polar/Cp figures, and transport dominate. |
| Layout matches (5.3) | ✅ | Required condition rail, fluid canvas, inspector, and transport are present with a concept-specific terminal/HUD treatment. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | AoA fill to 15° immediately reset to 4°; Play did not become Pause; major first-load bindings were blank. |
| Agent-driven loop (5.6) | ⚠️ | Optimize ran consult/notify/app_call and changed narration, but repeated describe/tool sequences and never completed. |
| Multi-agent ran (5.7) | ⚠️ | Two consults ran (Aerodynamicist/Skeptic path); declared Instrumenter was not evidenced. |
| Signals round-trip (5.8) | ❌ | First AoA manipulation reported `aoa_slider` not subscribed. |

## Aesthetic verdict: PARTIAL
- Baseline strongly matches the `terminal` pack and cyan→amber CFD HUD, but the live agent turn covered most of the rail/canvas/inspector with opaque black regions, making the app unusable during optimization.

## Screenshots
- `../shots/spec-016/baseline.png`
- `../shots/spec-016/optimize-loop.png`

## Friction encountered
- Drafter built cleanly in one round.
- Initial shared bindings (velocity family, Reynolds-derived values, L/D, separation, solve steps) were blank.
- Direct AoA manipulation reset from 15° to stale 4°, reproducing the canonical-state defect.
- Runtime theme/render corruption was the most severe yet: almost the whole scientific surface became opaque black.
