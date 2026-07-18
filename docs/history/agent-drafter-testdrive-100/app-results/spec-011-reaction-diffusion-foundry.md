# Spec 011 — Reaction-Diffusion Foundry

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-011-reaction-diffusion-foundry`, a live simulation app, and the first result in
> the run to report static acceptance with runtime still only partial.
> **Status:** Historical record — a closed July 2026 run from the completed audit. Its
> repeated-`ui_describe` and late-subscription findings are among those the
> [remediation results](../remediation-results.md) explicitly target.
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
- **Reached acceptance** is recorded here as a split verdict — `static yes; runtime partial` — separating the static manifest/source review from the live browser run. Specs 001–010 use the single-value form (`yes` / `no` / `partial`); only the PASS rule above is formally defined in the runbook.
- **Excluded rounds.** This app's provider-blocked retries are counted separately and excluded from the round total. The exclusion rule is not stated in this file; the register's `[SECURITY/ROBUSTNESS]` entry on zero-exit-code provider failures records that the harness detects the 403 marker, records `rc 75`/provider-blocked, and excludes such rounds from round budgets.
- **`Feed F` / `F/K regime`** are the feed and kill rate parameters of the simulated reaction-diffusion system. **`target_chosen`** and **`param_dragged`** are two of the app's declared UI signals.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-011-reaction-diffusion-foundry` |
| Authoring rounds | 1 real round (plus 6 provider-blocked retries, excluded) |
| Reached acceptance | static yes; runtime partial |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Full-bleed live simulation canvas, parameter rail, inspector, and bottom transport dominate; composer is secondary. |
| Layout matches (5.3) | ✅ | 240px reagent rail, central canvas, 340px inspector, phase/spectral cards, and 64px transport are all visible at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Feed F changed 0.036→0.055 and immediately changed canvas regime/score; Step and Capture frame produced a timeline capture. |
| Agent-driven loop (5.6) | ⚠️ | Target selection produced notes, theme/render/highlight frames, but the turn never completed and never staged a new F/K regime. |
| Multi-agent ran (5.7) | ✅ | Cartographer, Morphologist, and Perturbationist consults all started/completed; their combined recommendation appeared in Timeline. |
| Signals round-trip (5.8) | ❌ | First target click and subsequent parameter drag reported `target_chosen` / `param_dragged` not subscribed. |

## Aesthetic verdict: ALIGNED

- Expected and actual pack are `lab-notebook`. The ink-on-cream simulator is polished and legible at 1280×720; the agent's live theme mutation also rendered coherently in dark mode.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filenames below are references only.
> From this spec onward the run also switched to a per-spec subdirectory naming form
> (`spec-011/baseline.png`) instead of the flat `spec-NNN-initial.png` used by specs
> 001–010.

- `spec-011/baseline.png`
- `spec-011/agent-loop.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-011-reaction-diffusion-foundry-static.json`](../authoring-logs/spec-011-reaction-diffusion-foundry-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Initial Drafter pass hit three schema errors (`capabilities.name`, theme object shape, and tagged workflow step) before self-correcting and building cleanly in 324.8 seconds.
- Runtime signal subscription is too late for the gesture that starts the turn.
- After one successful describe/subscribe/three-consult/render sequence, the main agent repeated essentially the entire sequence, including multiple unchanged `ui_describe` calls. The browser remained `AI · updating data` after more than two minutes.
- The phase/spectral card values became blank during the agent turn even though local Feed/Kill state remained visible.

## Related documentation

- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the outage behind this app's six excluded retries, and the evidence that its single real round completed after VPN restoration.
- [Cumulative findings register](../audit-findings-register.md) — where the late-subscription and repeated-`ui_describe` findings are written up in full, this app included.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [Spec 012 — Contagion Studio](spec-012-contagion-studio.md) — the next app in the run, which shares the split static/runtime acceptance vocabulary.
- [Remediation results](../remediation-results.md) — what was built in response to these findings.
