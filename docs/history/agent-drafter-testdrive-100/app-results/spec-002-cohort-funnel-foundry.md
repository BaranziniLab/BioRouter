# Spec 002 — Cohort Funnel Foundry
- **App id:** `spec-002-cohort-funnel-foundry`
- **Authoring rounds:** 2   **Reached acceptance:** partial
- **Channel:** CLI authoring + in-app browser verification
- **Archetype chosen by the agent:** canvas
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Drag/click criterion library, central funnel, inspector and transport dominate; composer is secondary. |
| Layout matches (5.3) | ✅ | 280px rail, center funnel/log, 340px inspector and 64px transport; all nine named regions present. |
| Declared surface (5.4) | ✅ | All 6 actions, 4 signals, 4 components, state schema and 4 profiles declared/wired. |
| Client reactivity (5.5) | ✅ | Accessible eGFR click fallback added a seventh stage immediately; N changed 2,291→1,168 and attrition 82%→91% before the agent finished. |
| Agent-driven loop (5.6) | ⚠️ | Refinement replaced non-delivering `br.run` controls with `br.call`; explicit `ui_subscribe` and all four consults completed, but the main agent then repeated `ui_describe`/`ui_subscribe` until stopped before its required app calls/UI patches. |
| Multi-agent ran (5.7) | ✅ | `architect`, `auditor`, `statistician`, and `scribe` all completed attributed UCSF worker turns. |
| Signals round-trip (5.8) | ❌ | First local selection emitted before subscription and showed `signal "stage_selected" is not subscribed`; subscription completed during the turn, but runaway control-plane calls prevented a clean subsequent-gesture proof. |

## Aesthetic verdict: PARTIAL
- Correct dark `terminal` pack, crisp mono typography, green live values and restrained coral power warning.
- Region sizes and information hierarchy align, but the initial screenshot leaves a large lower black field and the scroll-contained criterion rail hides several chips, undercutting the requested maximal density.

## Screenshots
- `../shots/spec-002-initial.png`

## Friction encountered
- Initial typed capability rejected because a data source omitted required `name`.
- HTML5 DataTransfer drag could not be verified through CUA, so Agent Drafter added a keyboard/click fallback without removing drag.
- Both **Ask architect** and **Send** handlers ran but their `br.run` calls delivered no app message; in-session refinement replaced them with the proven `br.call` path.
- Repeated `ui_describe`/`ui_subscribe` after all four consults reproduced the Spec 001 engine-loop defect despite the explicit one-call prompt.
