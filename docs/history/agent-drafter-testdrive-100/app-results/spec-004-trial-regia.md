# Spec 004 — Trial Regia

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-004-trial-regia`, a clinical-trial design workbench, documenting a
> stale-shared-state defect between the UI and the worker agents.
> **Status:** Historical record — a completed and then provider-blocked run from the
> July 2026 test drive. It has been superseded by the remediation work rather than
> still being in flight; see the [remediation results](../remediation-results.md).
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

Because the app under test is a clinical-trial designer, its verdict rows carry
trial-statistics shorthand:

| Term | Meaning in this record |
|---|---|
| `MDE` | Minimum detectable effect — the smallest treatment effect the design can detect. |
| `SoA` | Schedule of activities — the per-visit assessment grid rendered beside the Gantt chart. |
| `KM` | Kaplan–Meier survival curve. |
| `n=248` / `n=784` | Sample size. The two values are the contradictory ones this run reports. |
| `power .82`, `alpha .05` | Statistical power and significance level of the designed trial. |

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-004-trial-regia` |
| Authoring rounds | 3 |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Rich Gantt-first trial workbench; the ask box is secondary |
| Layout matches (5.3) | ✅ | Left arm/endpoint rail, central Gantt+SoA, right power/KM/flags inspector, fixed transport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | First-load KPI bindings were blank; endpoint signal was lost pre-subscription; MDE keyboard input reset |
| Agent-driven loop (5.6) | ⚠️ | Power turn completed and patched state; feasibility turn consumed stale n=248 while UI showed n=784 and later repeated describe |
| Multi-agent ran (5.7) | ✅ | Designer, Biostatistician, Regulatory Critic, and Operationalizer ran on UCSF Azure GPT-5.5 |
| Signals round-trip (5.8) | ⚠️ | `endpoint_selected` emitted before subscription; later structured button turns did reach the agent |

## Aesthetic verdict: ALIGNED

- The `journal` pack, serif typography, ruled grid, restrained ivory/ink palette, compact badges, and protocol-density match the spec.
- The central schedule intentionally scrolls horizontally; the visible viewport remains coherent and the bottom transport stays reachable.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-004-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-004-trial-regia-static.json`](../authoring-logs/spec-004-trial-regia-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

### Authoring friction

- Initial authoring needed one static repair and added an extra `protocol_exporter` profile beyond the four requested.
- The generated runtime attempted nonexistent `clinical-biostatistics` skill loading.

### Runtime behaviour and defects

- First Power turn successfully called the four requested workers and actions, rendering power .82 / n=784 / MDE .35 / alpha .05 plus KM and flags.
- **Contradictory sample size across a turn boundary:** the next control serialized stale sample size 248 while rendered/shared state held 784, leading workers to reason from contradictory inputs.
- The second turn eventually repeated `ui_describe` after all worker consults instead of proceeding directly to state/action updates; the daemon was stopped.
- The real refinement added the missing forest-figure region and removed the extra profile, but local retest still showed blank initial bindings, slider reset, and first-signal loss. Its live retest was then blocked by UCSF HTTP 403 before reasoning.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — carries the full write-up of the diverging shared/client state defect this app isolated, plus its repro.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the provider blocker that stopped this app's live retest.
- [Remediation results](../remediation-results.md) — what was built to make one canonical state source the enforced path.
- [Spec 009 — Survival Atelier](spec-009-survival-atelier.md) — the other clinical-statistics app in the run, and one of two outright functional FAILs.
