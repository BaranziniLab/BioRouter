# Spec 005 — Omics Loom
- **App id:** spec-005-omics-loom
- **Authoring rounds:** 1 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Four-column omics workbench with interactive volcano, heatmap, network, and inspector |
| Layout matches (5.3) | ⚠️ | All regions exist, but the primary transport starts near y=986 in a 720px viewport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Feature click immediately changed selected feature and inspector; KPI changed 0.74→0.88 after action |
| Agent-driven loop (5.6) | ⚠️ | Workers and actions ran, but repeated describe calls prevented final synthesis/pins and second instruction |
| Multi-agent ran (5.7) | ✅ | Aligner, Correlator, Contrarian, and Weaver all ran on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | Initial `point_clicked` was emitted before subscription |

## Aesthetic verdict: PARTIAL
- The ruled-paper background, taped cards, monochrome charts, compact typography, and dense multi-panel hierarchy strongly match `lab-notebook`.
- Primary transport is below the acceptance viewport and a few heatmap labels clip horizontally.

## Screenshots
- [`../shots/spec-005-initial.png`](../shots/spec-005-initial.png)
- [`../shots/spec-005-integrated.png`](../shots/spec-005-integrated.png)

## Friction encountered
- Direct STAT3 selection updated locally but its signal was not yet subscribed.
- All four workers completed and all sessions were verified on `versa_azure/gpt-5.5-2026-04-24`.
- Successful actions included link brush, feature focus, discordance boxes, concordance KPI 0.879, and eight cross-layer edges.
- The model repeatedly re-described the unchanged surface between phases; final synthesis and Contrarian pins never rendered before the bounded stop.
- Progress frames were duplicated into both the dedicated status area and inspector/synthesis region.
- The queued refinement hit the UCSF IP-allowlist 403 in 2.7s and made no app change; local retest confirmed the transport and first-signal defects remain.
