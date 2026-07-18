# Spec 014 — Serengeti Engine
- **App id:** spec-014-serengeti-engine
- **Authoring rounds:** 1   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | A spatial ecosystem canvas, habitat/species controls, scientific plots/table, and transport dominate. |
| Layout matches (5.3) | ✅ | The requested 250px species rail, full-bleed grid, 340px inspector, and bottom transport are present and coherent. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Brush selection, canvas painting, seed random, and Play→Pause controls responded; simulation/field-note UI changed locally. |
| Agent-driven loop (5.6) | ⚠️ | Balance rendered a plan, highlights, and field-note patch, but did not invoke habitat/rate/simulation actions; lion vision stayed 0.68 despite the announced 0.52 plan. |
| Multi-agent ran (5.7) | ✅ | Ranger, Invasive Species, and Naturalist consults all completed. |
| Signals round-trip (5.8) | ❌ | First terrain-paint gesture reported `terrain_painted` not subscribed. |

## Aesthetic verdict: ALIGNED
- The `journal` pack is strongly expressed: warm parchment, serif captions, muted terrain, organic motes, and a field-note intervention card. Unlike Specs 012–013, live rendering remained legible.

## Screenshots
- `../shots/spec-014/baseline.png`
- `../shots/spec-014/balance-run.png`

## Friction encountered
- The first Drafter round built cleanly but took 367.2 seconds.
- All named workers ran and the main agent patched explanatory UI, yet the promised scientific intervention was narrative-only: no declared action ran and the underlying rate did not change.
- One unchanged `ui_describe` occurred after the completed consult/patch sequence while the page remained `AI · updating data`.
