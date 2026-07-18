# Spec 004 — Trial Regia
- **App id:** spec-004-trial-regia
- **Authoring rounds:** 3   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Rich Gantt-first trial workbench; the ask box is secondary |
| Layout matches (5.3) | ✅ | Left arm/endpoint rail, central Gantt+SoA, right power/KM/flags inspector, fixed transport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | First-load KPI bindings were blank; endpoint signal was lost pre-subscription; MDE keyboard input reset |
| Agent-driven loop (5.6) | ⚠️ | Power turn completed and patched state; feasibility turn consumed stale n=248 while UI showed n=784 and later repeated describe |
| Multi-agent ran (5.7) | ✅ | Designer, Biostatistician, Regulatory Critic, and Operationalizer ran on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | `endpoint_selected` emitted before subscription; later structured button turns did reach the agent |

## Aesthetic verdict: ALIGNED
- The `journal` pack, serif typography, ruled grid, restrained ivory/ink palette, compact badges, and protocol-density match the spec.
- The central schedule intentionally scrolls horizontally; the visible viewport remains coherent and the bottom transport stays reachable.

## Screenshots
- [`../shots/spec-004-initial.png`](../shots/spec-004-initial.png)

## Friction encountered
- Initial authoring needed one static repair and added an extra `protocol_exporter` profile beyond the four requested.
- The generated runtime attempted nonexistent `clinical-biostatistics` skill loading.
- First Power turn successfully called the four requested workers and actions, rendering power .82 / n=784 / MDE .35 / alpha .05 plus KM and flags.
- The next control serialized stale sample size 248 while rendered/shared state held 784, leading workers to reason from contradictory inputs.
- The second turn eventually repeated `ui_describe` after all worker consults instead of proceeding directly to state/action updates; the daemon was stopped.
- The real refinement added the missing forest-figure region and removed the extra profile, but local retest still showed blank initial bindings, slider reset, and first-signal loss. Its live retest was then blocked by UCSF HTTP 403 before reasoning.
