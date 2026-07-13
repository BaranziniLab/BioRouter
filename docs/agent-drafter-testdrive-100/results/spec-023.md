# Spec 023 — Longitude
- **App id:** spec-023-longitude
- **Authoring rounds:** 2   **Reached acceptance:** pending browser verification
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PASS (static; browser pending)
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | pending | Browser verification required |
| Layout matches (5.3) | pending | Static regions: agent-notes, era-ruler, figure, inspector, lane-rail, timeline, transport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | pending | Browser state/binding drive required |
| Agent-driven loop (5.6) | pending | Two live instructions required |
| Multi-agent ran (5.7) | pending | Profiles declared: annotator, archivist, genealogist |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

## Aesthetic verdict: PENDING
- Expected pack `lab-notebook`; manifest pack `lab-notebook`.

## Platform integration
- **Requested:** `br.kb` search/graph for citations; fast route for card placement, deep for lineage reasoning; `figure` plots a citations-per-year sparkline in the inspector.
- **Configured:** extensions=['autovisualiser', 'knowledge']; skills=none; knowledge_base=none; grants=none; routes=['deep', 'fast']; workflows=['branch_here', 'place_card_fast', 'trace_lineage']
- **Available in isolated runtime:** built-in extensions=['autovisualiser', 'knowledge']; external connectors=none; skills=none; KBs=none.
- **Exercised:** pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds.
- **Missing/blocked:** requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures.

## Screenshots
- `../shots/spec-023-*.png` (pending)

## Friction encountered
- None in static review.
