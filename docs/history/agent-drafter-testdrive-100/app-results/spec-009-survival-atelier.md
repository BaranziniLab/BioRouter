# Spec 009 — Survival Atelier
- **App id:** spec-009-survival-atelier
- **Authoring rounds:** 1 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: FAIL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Survival-analysis studio with covariate rail, KM canvas/risk table, forest dock, inspector, and transport |
| Layout matches (5.3) | ✅ | Required rail/canvas/inspector/fixed transport are present; below-fold rail/table content scrolls |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | CUA drag could not add a stratum; slider reset; failed gestures blanked initialized bindings |
| Agent-driven loop (5.6) | ❌ | Core stratum prerequisite was unreachable, so no valid turn or second instruction could run |
| Multi-agent ran (5.7) | ❌ | No valid stratum turn; no worker profile executed |
| Signals round-trip (5.8) | ❌ | No stratum/cutpoint signal was successfully emitted |

## Aesthetic verdict: ALIGNED
- Elegant serif `journal` treatment, large KM canvas, muted geometry, floating C-index, forest dock, and fixed transport match the brief.
- Body scroll height is 1008px, but the primary transport remains visible at 720p and the composition is coherent.

## Screenshots
- [`../shots/spec-009-initial.png`](../shots/spec-009-initial.png)

## Friction encountered
- HTML5 drag was the only stratum-creation path and did not respond to two real CUA drags; no accessible fallback existed.
- Failed gestures caused initialized binding values to disappear; keyboard slider changes reset to 65.
- Guarded **Fit Cox** correctly requested a stratum but misleadingly entered an AI-updating status with no session message.
- Manifest declares unavailable/unverified `clinical-biostatistics` skill.
- The queued refinement hit the UCSF IP-allowlist 403 in 5.7s and made no app change; local retest confirmed covariates remain drag-only and inaccessible.
