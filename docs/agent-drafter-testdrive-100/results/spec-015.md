# Spec 015 — FoldScape
- **App id:** spec-015-foldscape
- **Authoring rounds:** 3 real rounds (plus 1 interrupted harness-induced retry, excluded)   **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Structure canvas, energy funnel, residue/dihedral controls, scientific figures, and mutation verdict dominate. |
| Layout matches (5.3) | ⚠️ | Required regions exist, but the header slider overlays title/KPIs and the lower transport/progress clips at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | Residue selection and φ/ψ controls updated locally, but Energy/RMSD/inspector bindings became blank after mutation. |
| Agent-driven loop (5.6) | ❌ | Workers analyzed stale L34→A while the visible selection was M1/φ=-70°/ψ=4°; main rendered pending minimization but invoked no mutation/minimization action. |
| Multi-agent ran (5.7) | ✅ | Folder, Mutagenesis Critic, and Validator consults all completed on the UCSF model. |
| Signals round-trip (5.8) | ❌ | `residue_selected` and `mutation_chosen` both reported not subscribed; mutation emitted the latter twice. |

## Aesthetic verdict: PARTIAL
- The default `biorouter` pack resolves correctly and the protein ribbon/funnel/Ramachandran/contact-map visuals are distinctive, but overlay collisions and below-fold transport materially harm the 720p composition.

## Screenshots
- `../shots/spec-015/baseline.png`
- `../shots/spec-015/mutation-state-split.png`

## Friction encountered
- Static review is clean after correcting the harness's default-theme interpretation.
- Three authoring rounds plus a futile reviewer-induced retry exposed that default `biorouter` is intentionally omitted during serialization; the retry was interrupted once source/readback proved the invariant.
- The most serious runtime issue is state identity: the UI and all workers reasoned about different residues.
- First-use signals were lost; the agent rendered advice instead of applying declared actions and remained `AI · updating data`.
