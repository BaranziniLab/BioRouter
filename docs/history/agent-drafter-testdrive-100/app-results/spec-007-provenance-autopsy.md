# Spec 007 — Provenance Autopsy

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-007-provenance-autopsy`, a chain-of-custody console, and the first app in the
> run to achieve a clean signal round-trip.
> **Status:** Historical record — a closed July 2026 run (one successful round, one
> provider-blocked). Its evidence is already rolled into the
> [cumulative findings register](../audit-findings-register.md) and the
> [remediation results](../remediation-results.md); no further rounds are pending.
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
- **Tracer, Diff Hunter, Bisector and Reporter** are this app's four declared worker profiles, written here as display names. Manifest profile keys are lowercase; [spec 001](spec-001-variant-tribunal.md) records that the mismatch between display name and manifest key was itself a defect.
- **DAG** means directed acyclic graph — the artifact-lineage diagram at the centre of the console.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-007-provenance-autopsy` |
| Authoring rounds | 1 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

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

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-007-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-007-provenance-autopsy-static.json`](../authoring-logs/spec-007-provenance-autopsy-static.json).

## What worked

- Direct gesture and local reactivity were the cleanest so far; no first-signal loss was observed.
- Tracer and Diff Hunter were verified on the required UCSF model.

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Main then made two extra `ui_describe` calls and stopped before required app actions, Bisector/Reporter, KPI, or findings.
- Tool frames were duplicated into both semantic evidence and dedicated progress regions.
- Manifest declares unverified `reproducibility` skill.
- The queued refinement hit the UCSF IP-allowlist 403 in 3.6s and made no app change. A clean direct-gesture retest still worked locally, but the agent turn itself immediately received the same 403.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — where the repeated-`ui_describe` and duplicated-progress-stream findings are written up in full.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser-driving procedure.
- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the provider blocker that voided this app's queued refinement.
- [Spec 001 — Variant Tribunal](spec-001-variant-tribunal.md) — the run's first app, and where the profile display-name/key mismatch was isolated.
- [Platform integration audit](../platform-integration-audit.md) — the accounting that shows the `reproducibility` skill was never installed in the test runtime.
