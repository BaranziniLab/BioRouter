# Spec 011 — Reaction-Diffusion Foundry
- **App id:** spec-011-reaction-diffusion-foundry
- **Authoring rounds:** 1 real round (plus 6 provider-blocked retries, excluded)   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Full-bleed live simulation canvas, parameter rail, inspector, and bottom transport dominate; composer is secondary. |
| Layout matches (5.3) | ✅ | 240px reagent rail, central canvas, 340px inspector, phase/spectral cards, and 64px transport are all visible at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Feed F changed 0.036→0.055 and immediately changed canvas regime/score; Step and Capture frame produced a timeline capture. |
| Agent-driven loop (5.6) | ⚠️ | Target selection produced notes, theme/render/highlight frames, but the turn never completed and never staged a new F/K regime. |
| Multi-agent ran (5.7) | ✅ | Cartographer, Morphologist, and Perturbationist consults all started/completed; their combined recommendation appeared in Timeline. |
| Signals round-trip (5.8) | ❌ | First target click and subsequent parameter drag reported `target_chosen` / `param_dragged` not subscribed. |

## Aesthetic verdict: ALIGNED
- Expected and actual pack are `lab-notebook`. The ink-on-cream simulator is polished and legible at 1280×720; the agent's live theme mutation also rendered coherently in dark mode.

## Screenshots
- `../shots/spec-011/baseline.png`
- `../shots/spec-011/agent-loop.png`

## Friction encountered
- Initial Drafter pass hit three schema errors (`capabilities.name`, theme object shape, and tagged workflow step) before self-correcting and building cleanly in 324.8 seconds.
- Runtime signal subscription is too late for the gesture that starts the turn.
- After one successful describe/subscribe/three-consult/render sequence, the main agent repeated essentially the entire sequence, including multiple unchanged `ui_describe` calls. The browser remained `AI · updating data` after more than two minutes.
- The phase/spectral card values became blank during the agent turn even though local Feed/Kill state remained visible.
