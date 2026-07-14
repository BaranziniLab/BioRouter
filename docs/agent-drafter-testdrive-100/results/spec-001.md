# Spec 001 — Variant Tribunal
- **App id:** `spec-001-variant-tribunal`
- **Authoring rounds:** 6   **Reached acceptance:** partial
- **Channel:** CLI authoring + in-app browser verification
- **Archetype chosen by the agent:** canvas
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI), verified in authoring, main runtime, and worker session rows

## Functional verdict: PARTIAL
| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Genome evidence workbench is primary; only a 340px footer note composer is secondary. |
| Layout matches (5.3) | ✅ | 260px rail, 678px center, 340px verdict inspector, 64px transport, floating presence; eight named regions. |
| Declared surface (5.4) | ✅ | All 6 actions, 4 signals, 3 custom components, state schema, and 4 profiles declared and wired. |
| Client reactivity (5.5) | ✅ | Clicking PVS1 immediately lit the criterion, added it to the verdict card, and changed the presence text before the model completed. |
| Agent-driven loop (5.6) | ⚠️ | One full loop reached `ui_describe → consult×4 → app_call×multiple → ui_notify`, patched evidence tracks, and changed VUS confidence 42%→62%. A later second instruction entered a runaway repeated-`ui_describe` failure, so repeatability failed. |
| Multi-agent ran (5.7) | ✅ | Separate UCSF sessions and attributed consults ran for `prosecutor`, `defense`, `clerk`, and `chief_justice`. |
| Signals round-trip (5.8) | ❌ | First gesture surfaced `signal "criterion_clicked" is not subscribed`; explicit `ui_subscribe` was added and completed on the next turn, but that turn ran away before a post-subscription gesture could be verified. |

## Aesthetic verdict: ALIGNED
- `clinical` pack applied in light mode; coherent warm clinical palette and system typography.
- Dense but readable three-column courtroom layout, correct transport placement, restrained black/amber/green/red semantics, and an ambient presence chip.
- No page overflow at 1280×720. Named-region geometry matches the spec closely.

## Screenshots
- `../shots/spec-001-pre-runtime-fix.png` — complete authored workbench before runtime refinement.
- `../shots/spec-001-agent-loop.png` was attempted after the completed loop, but CDP screenshot capture timed out twice; final dynamic state is preserved in the DOM/session trace and authoring logs.

## Friction encountered (see `FINDINGS.md`)
- `[ERGONOMICS][high]` Agent Drafter store bypassed `BIOROUTER_PATH_ROOT`; the incomplete draft was quarantined and XDG isolation added.
- `[AUTHORING-INEFFICIENCY][med]` six rejected nested manifest/orchestration shapes before convergence.
- `[SPEC-GAP][high]` invented invalid KB id `br.kb`.
- `[FUNCTIONAL-BUG][high]` profile display-name/key mismatch plus UI-enabled worker stalled the first loop.
- `[FUNCTIONAL-BUG][high]` signal was not subscribed until a later refinement.
- `[SECURITY/ROBUSTNESS][high]` main agent repeatedly retried `ui_describe` after the tool result explicitly said the user declined and ordered it not to retry; this made the second instruction runaway.
