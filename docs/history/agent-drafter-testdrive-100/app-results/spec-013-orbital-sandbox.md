# Spec 013 — Orbital Sandbox
- **App id:** spec-013-orbital-sandbox
- **Authoring rounds:** 1 real round (plus 3 provider-blocked retries, excluded)   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | N-body canvas, body editor, elements table, phase-space card, and transport dominate. |
| Layout matches (5.3) | ⚠️ | Required rail/canvas/inspector/transport exist, but the left body list and bottom transport/composer clip at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Add body inserted a fourth body; Step changed positions, velocities, and orbital-element rows. |
| Agent-driven loop (5.6) | ⚠️ | Stabilize produced an L4 plan, specialist consults, notify/highlight frames, but no stabilize/app action and the turn remained in a repeated-describe loop. |
| Multi-agent ran (5.7) | ⚠️ | Navigator and Chaos Auditor consults completed; declared Ephemeris Scribe did not run. |
| Signals round-trip (5.8) | ❌ | First Add body gesture reported `body_added` not subscribed. |

## Aesthetic verdict: PARTIAL
- The baseline uses the expected `midnight` pack with precise violet orbit styling, but viewport clipping is substantial and the live run reproduced the opaque-black-region theme corruption seen in Spec 012.

## Screenshots
- `../shots/spec-013/baseline.png`
- `../shots/spec-013/stabilize-loop.png`

## Friction encountered
- One real Drafter round built cleanly in 263.5 seconds after three outage retries.
- The core local simulation works, but first-use signal delivery does not.
- The agent consulted two of three profiles and rendered a plan, then made repeated unchanged `ui_describe` calls instead of invoking a declared action.
- Runtime styling obscured large portions of the rail and inspector and left the bottom controls crowded/clipped.
