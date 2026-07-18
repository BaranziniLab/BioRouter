# Spec 006 — Ward Board
- **App id:** spec-006-ward-board
- **Authoring rounds:** 1 successful + 1 provider-blocked   **Reached acceptance:** no
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Dense problem-oriented board with cards, evidence inspector, sparklines, and bottom transport |
| Layout matches (5.3) | ✅ | Exact 260px rail / center cards / 340px inspector / 64px transport composition at 1280x720 |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | AKI selection changed only its tag; inspector content stayed Hypoxemia; acuity KPI was blank |
| Agent-driven loop (5.6) | ⚠️ | Worker-like calls completed, then repeated describe prevented app actions/note and second instruction |
| Multi-agent ran (5.7) | ⚠️ | Generic `subagent` calls ran three names, not verifiable declared-profile `consult` sessions |
| Signals round-trip (5.8) | ⚠️ | Initial `card_selected` emitted before subscription |

## Aesthetic verdict: ALIGNED
- The restrained clinical palette, urgency coral, sparkline strip, card density, fixed transport, and clinician-review treatment closely match the brief.
- The whole composition fits 1280x720 with no page or panel overflow.

## Screenshots
- [`../shots/spec-006-initial.png`](../shots/spec-006-initial.png)

## Friction encountered
- Manifest declared all four workers explicitly on the UCSF model, but runtime used generic `subagent` rather than profile `consult`, so separate worker-session routing could not be verified.
- Main called `ui_describe` twice after the worker outputs and reached no required app action or note patch.
- First-selection signal was lost; selected-problem local rendering split between an updated tag and stale evidence panel.
- Manifest lists nonexistent/unverified `clinical-databases` skill.
- The queued refinement hit the UCSF IP-allowlist 403 in 5.4s and made no app change; local retest reproduced the stale inspector, blank KPI, and first-signal loss.
