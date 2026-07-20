# Spec 003 — Pathway Séance

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-003-pathway-seance`, a graph workbench whose third authoring round was blocked
> by a provider outage.
> **Status:** Historical record — a July 2026 run that ended un-accepted because the
> UCSF Azure IP-allowlist 403 outage cut off its final refinement. The campaign has
> since moved on to remediation, so no further rounds will be credited to this file.
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
- **KB** means knowledge base — a BioRouter knowledge store the app can be granted access to. `kb_get_graph` and `kb_list_bases` are the knowledge tools the runtime called.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-003-pathway-seance` |
| Authoring rounds | 2 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL (browser round 1; refinement built)

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Dense graph workbench with seed rail, canvas, inspector, dossier, and bottom transport |
| Layout matches (5.3) | ⚠️ | Regions are present, but at 1280x720 the left rail placed **Expand lasso** near y=936 |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | Loading seeds, toggling literature, and changing layout updated the surface immediately |
| Agent-driven loop (5.6) | ⚠️ | Refined turn reached KB discovery, all workers, and many app calls, but inserted repeated `ui_describe` between phases and did not finish |
| Multi-agent ran (5.7) | ✅ | Cartographer, Bridger, Skeptic, and Narrator all completed on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | Subscription succeeded; local legend gesture updated presence, but the bounded turn was stopped before a clean signal-driven close |

## Aesthetic verdict: PARTIAL

- The `midnight` pack, luminous graph staging, evidence legend, and three-column visual hierarchy matched the brief.
- The seed/legend/lasso rail improved, but **Expand lasso** still started around y=766 in the 720p acceptance viewport.

> **Note.** This record gives two different vertical positions for the same **Expand
> lasso** control — `y=936` in the layout row above and `y=766` here — and does not state
> which round each measurement came from or which supersedes the other. Both figures are
> preserved as written.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filenames below are references only.

- `spec-003-initial.png`
- `spec-003-refined.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-003-pathway-seance-static.json`](../authoring-logs/spec-003-pathway-seance-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- `skills.loadSkill("pathway-analysis")` failed because the generated skill does not exist.
- `kb_get_graph` failed with `-32602` (the JSON-RPC *invalid params* code) because no KB id or active KB was supplied; `kb_list_bases` later found only `soul`.
- The model recovered to three successful worker consults, but then repeated `ui_describe` after the unchanged-surface warning.
- Round 2 removed both deterministic tool failures and correctly searched the real `soul` KB id, transparently handling zero hits.
- The clean retest completed all four worker consults and successful node/link actions. It still repeated `ui_describe` between action phases; Edges/Ghosts counters and dossier remained blank while Nodes reached 12.
- A third refinement was attempted but UCSF Azure rejected it with the IP-allowlist 403; no change was credited.

## Related documentation

- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the provider blocker that stopped this app's third round.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [Cumulative findings register](../audit-findings-register.md) — where the nonexistent-skill and repeated-`ui_describe` findings are written up in full, this app included.
- [Remediation results](../remediation-results.md) — what was built in response to those findings.
- [Spec 004 — Trial Regia](spec-004-trial-regia.md) — the next app in the run, blocked by the same outage.
