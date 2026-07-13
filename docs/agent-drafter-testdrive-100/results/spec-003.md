# Spec 003 — Pathway Séance
- **App id:** spec-003-pathway-seance
- **Authoring rounds:** 2 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL (browser round 1; refinement built)
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Dense graph workbench with seed rail, canvas, inspector, dossier, and bottom transport |
| Layout matches (5.3) | ⚠️ | Regions are present, but at 1280x720 the left rail placed **Expand lasso** near y=936 |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Loading seeds, toggling literature, and changing layout updated the surface immediately |
| Agent-driven loop (5.6) | ⚠️ | Refined turn reached KB discovery, all workers, and many app calls, but inserted repeated `ui_describe` between phases and did not finish |
| Multi-agent ran (5.7) | ✅ | Cartographer, Bridger, Skeptic, and Narrator all completed on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | Subscription succeeded; local legend gesture updated presence, but the bounded turn was stopped before a clean signal-driven close |

## Aesthetic verdict: PARTIAL
- The `midnight` pack, luminous graph staging, evidence legend, and three-column visual hierarchy match the brief.
- The seed/legend/lasso rail improved, but **Expand lasso** still starts around y=766 in the 720p acceptance viewport.

## Screenshots
- [`../shots/spec-003-initial.png`](../shots/spec-003-initial.png)
- [`../shots/spec-003-refined.png`](../shots/spec-003-refined.png)

## Friction encountered
- `skills.loadSkill("pathway-analysis")` failed because the generated skill does not exist.
- `kb_get_graph` failed with `-32602` because no KB id or active KB was supplied; `kb_list_bases` later found only `soul`.
- The model recovered to three successful worker consults, but then repeated `ui_describe` after the unchanged-surface warning.
- Round 2 removed both deterministic tool failures and correctly searched the real `soul` KB id, transparently handling zero hits.
- The clean retest completed all four worker consults and successful node/link actions. It still repeated `ui_describe` between action phases; Edges/Ghosts counters and dossier remained blank while Nodes reached 12.
- A third refinement was attempted but UCSF Azure rejected it with the IP-allowlist 403; no change was credited.
