# Spec 002 — Cohort Funnel Foundry

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-002-cohort-funnel-foundry`, recording a partial functional pass and the
> repeated-`ui_describe` engine defect first seen in spec 001.
> **Status:** Historical record — a completed browser-verified run from the July 2026
> test drive. The engine-loop and signal-subscription defects it reports were
> subsequently addressed in the [remediation results](../remediation-results.md), so
> this file is frozen evidence rather than living reference.
> **Audience:** developers working on Agent Drafter and the Apps SDK.

The 100-app test drive asked Agent Drafter to author 100 different scientific apps from
written briefs, then drove each finished app in a real browser to check whether it
behaved as it declared. A *verdict* is the score one app earned against the runbook's
rubric — a functional verdict (does it work as an agent-driven surface?) and an
aesthetic verdict (does it look the way the brief asked?). This file records that
verdict for one app.

## How to read this record

- **`spec-NNN`** identifies a numbered brief in [the 100 agentic app test specs](../../../agent-drafter/testing/hundred-app-test-specs.md); app ids follow `spec-NNN-<slug>`. The campaign-wide roll-up is [the authored-app verdict index](../authored-app-verdict-index.md).
- **Check IDs `5.2`–`5.8`** are rubric sections defined in [the test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) (§5). An app is a functional **PASS** only if 5.2, 5.5, 5.6 and 5.7 all hold and the layout (5.3) substantially matches; §6 scores the aesthetic verdict independently.
- **Reached acceptance** records whether the app cleared that bar. This result set uses `yes`, `no` and `partial`; only the PASS rule above is formally defined in the runbook.
- **`br.run` and `br.call`** are two Apps SDK client entry points for asking the app's agent to do work; see the [Apps SDK reference](../../../apps-sdk/sdk-reference.md). `br.call` returns a typed, structured turn result; `br.run` streams a markdown reply into a target element.
- **CUA** is the computer-use browser-automation harness used to drive the finished app (Playwright MCP or equivalent, per §4.2 of the runbook).

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-002-cohort-funnel-foundry` |
| Authoring rounds | 2 |
| Reached acceptance | partial |
| Channel | CLI authoring + in-app browser verification |
| Archetype chosen by the agent | canvas |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

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

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-002-initial.png`

Surviving machine-readable evidence for this app:
[`../authoring-logs/spec-002-cohort-funnel-foundry-static.json`](../authoring-logs/spec-002-cohort-funnel-foundry-static.json)
and
[`../authoring-logs/spec-002-cohort-funnel-foundry-browser-issues.txt`](../authoring-logs/spec-002-cohort-funnel-foundry-browser-issues.txt).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Initial typed capability rejected because a data source omitted required `name`.
- HTML5 DataTransfer drag could not be verified through CUA, so Agent Drafter added a keyboard/click fallback without removing drag.
- Both **Ask architect** and **Send** handlers ran but their `br.run` calls delivered no app message; in-session refinement replaced them with the proven `br.call` path.
- Repeated `ui_describe`/`ui_subscribe` after all four consults reproduced the [Spec 001](spec-001-variant-tribunal.md) engine-loop defect despite the explicit one-call prompt.

## Related documentation

- [Spec 001 — Variant Tribunal](spec-001-variant-tribunal.md) — the app where the repeated-`ui_describe` engine-loop defect reproduced here was first isolated.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [Apps SDK reference](../../../apps-sdk/sdk-reference.md) — the `br.run` / `br.call` signatures at issue in this run.
- [Cumulative findings register](../audit-findings-register.md) — where the `br.run`-delivers-no-turn and signal-before-subscribe findings are written up in full.
- [Remediation results](../remediation-results.md) — what was built in response.
