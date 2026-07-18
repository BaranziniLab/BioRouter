# Spec 024 — Quorum
- **App id:** spec-024-quorum
- **Authoring rounds:** 1   **Reached acceptance:** pending browser verification
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PASS (static; browser pending)
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | pending | Browser verification required |
| Layout matches (5.3) | pending | Static regions: audit, board, figure, inspector, prisma, progress, protocol, transport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | pending | Browser state/binding drive required |
| Agent-driven loop (5.6) | pending | Two live instructions required |
| Multi-agent ran (5.7) | pending | Profiles declared: appraiser, extractor, screener |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

## Aesthetic verdict: PENDING
- Expected pack `clinical`; manifest pack `clinical`.

## Platform integration
- **Requested:** `br.kb` search + `page` for full-text; `systematic-review`/`clinical-biostatistics` skills; deep route for appraisal; `figure` renders the PRISMA flow diagram on export.
- **Configured:** extensions=['autovisualiser', 'knowledge']; skills=none; knowledge_base=none; grants=none; routes=['deep_appraisal', 'fast_screening']; workflows=['export_prisma', 'extract_fields', 'grade_bias', 'screen_batch']
- **Available in isolated runtime:** built-in extensions=['autovisualiser', 'knowledge']; external connectors=none; skills=none; KBs=none.
- **Exercised:** pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds.
- **Missing/blocked:** requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures.

## Screenshots
- `../shots/spec-024-*.png` (pending)

## Friction encountered
- None in static review.
