# Spec 007 — Provenance Autopsy
- **App id:** spec-007-provenance-autopsy
- **Authoring rounds:** 1 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Chain-of-custody console with artifact rail, DAG, transform table, diff/log inspector, and transport |
| Layout matches (5.3) | ✅ | Full terminal layout fits 1280x720 with fixed controls; dense DAG/table scroll is intentional |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | s7 selection updated artifact, DAG label, selected diff, and logs immediately |
| Agent-driven loop (5.6) | ⚠️ | Turn started from the gesture but repeated describe after consults and never reached actions/findings |
| Multi-agent ran (5.7) | ⚠️ | Tracer and Diff Hunter ran in separate UCSF sessions; Bisector and Reporter were not reached |
| Signals round-trip (5.8) | ✅ | DAG selection started the structured agent turn without an unsubscribed-signal error |

## Aesthetic verdict: ALIGNED
- The black/green terminal palette, monospace hierarchy, chain-of-custody table, compact DAG, fixed transport, and suspicion controls match the spec closely.
- Minor node/table clipping appears within intentionally dense scrollable regions.

## Screenshots
- [`../shots/spec-007-initial.png`](../shots/spec-007-initial.png)

## Friction encountered
- Direct gesture and local reactivity were the cleanest so far; no first-signal loss was observed.
- Tracer and Diff Hunter were verified on the required UCSF model.
- Main then made two extra `ui_describe` calls and stopped before required app actions, Bisector/Reporter, KPI, or findings.
- Tool frames were duplicated into both semantic evidence and dedicated progress regions.
- Manifest declares unverified `reproducibility` skill.
- The queued refinement hit the UCSF IP-allowlist 403 in 3.6s and made no app change. A clean direct-gesture retest still worked locally, but the agent turn itself immediately received the same 403.
