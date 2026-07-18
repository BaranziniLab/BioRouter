# Spec 010 — Diagnosis Odyssey
- **App id:** spec-010-diagnosis-odyssey
- **Authoring rounds:** 4   **Reached acceptance:** no (browser failures)
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: FAIL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Interactive phenotype→syndrome→gene reasoning graph with dossier and path chronicle |
| Layout matches (5.3) | ⚠️ | Regions exist, but primary transport starts near y=788 in a 720px viewport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | Fabry selection left core dossier/yield/chronicle bindings blank or stale |
| Agent-driven loop (5.6) | ❌ | Two 120s worker timeouts; main completed without actions/UI update; no second instruction |
| Multi-agent ran (5.7) | ❌ | Pathfinder and Test Recommender timed out; Refuter/Chronicler never ran |
| Signals round-trip (5.8) | ⚠️ | Click started a turn, but ambient status reported `node_clicked` unsubscribed |

## Aesthetic verdict: PARTIAL
- The custom black/ivory map, gold confirmed-node treatment, rare-disease rail, and candidate dossier strongly express the intended BioRouter aesthetic.
- The absent theme block correctly resolves to the default `biorouter` pack; primary controls remain below the 720p viewport.

## Screenshots
- [`../shots/spec-010-initial.png`](../shots/spec-010-initial.png)

## Friction encountered
- The original audit misread default-theme canonicalization and caused unnecessary refinement rounds: `ThemeConfig::is_default` intentionally omits the base `biorouter` block on serialization. The corrected reviewer resolves absence to `biorouter`.
- Manifest invents unavailable skills (`rare-disease`, `clinical-databases`) and KB id `hpo-omim-gene-disease`; search returned no evidence.
- Pathfinder and Test Recommender each hit the 120-second timeout; main performed no required action and silently completed.
- Fabry selection exposed blank/stale bound values and first-signal loss.
