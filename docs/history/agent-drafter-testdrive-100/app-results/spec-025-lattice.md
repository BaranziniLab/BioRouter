# Spec 025 — Lattice
- **App id:** spec-025-lattice
- **Authoring rounds:** 1   **Reached acceptance:** pending browser verification
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PASS (static; browser pending)
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | pending | Browser verification required |
| Layout matches (5.3) | pending | Static regions: figure, inspector, lattice, progress, seed, warnings |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | pending | Browser state/binding drive required |
| Agent-driven loop (5.6) | pending | Two live instructions required |
| Multi-agent ran (5.7) | pending | Profiles declared: falsifier, generator, planner |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

## Aesthetic verdict: PENDING
- Expected pack `midnight`; manifest pack `midnight`.

## Platform integration
- **Requested:** `br.kb` for grounding + prior-art; `single-cell`/`crispr-screens` skills for assay plans; deep route for generation, fast for scoring; `figure` sketches an expected-effect plot per hypothesis.
- **Configured:** extensions=['autovisualiser', 'knowledge']; skills=none; knowledge_base=none; grants=none; routes=['deep', 'deep_generation', 'fast', 'fast_scoring']; workflows=['design_experiment', 'generate_children', 'prune_dead_ends', 'rank_testability']
- **Available in isolated runtime:** built-in extensions=['autovisualiser', 'knowledge']; external connectors=none; skills=none; KBs=none.
- **Exercised:** pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds.
- **Missing/blocked:** requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures.

## Screenshots
- `../shots/spec-025-*.png` (pending)

## Friction encountered
- None in static review.
