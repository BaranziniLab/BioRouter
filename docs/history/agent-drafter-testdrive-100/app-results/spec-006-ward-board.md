# Spec 006 — Ward Board

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-006-ward-board`, a clinical board app, notable for a runtime that used generic
> subagents instead of the worker profiles the manifest declared.
> **Status:** Historical record — a closed July 2026 run (one successful round, one
> provider-blocked). The declared-profile-bypass defect it found is part of the
> completed audit; see the [cumulative findings register](../audit-findings-register.md).
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

> **Why `subagent` versus `consult` is the whole finding.** `consult` is the Apps SDK v2
> tool that asks a *declared worker profile* — a named entry in the app manifest's
> `orchestration.agents`, with its own session, capabilities and model route — to answer
> a sub-question. It is armed only when the app declares at least one valid profile.
> A generic `subagent` call delegates to an unnamed helper instead, so none of the
> declared profile's isolation, capabilities or model routing can be proven from the
> session record. Check 5.7 asks for the former; this app's runtime did the latter. See
> the [Apps SDK reference](../../../apps-sdk/sdk-reference.md) for both surfaces.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-006-ward-board` |
| Authoring rounds | 1 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

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

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-006-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-006-ward-board-static.json`](../authoring-logs/spec-006-ward-board-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Manifest declared all four workers explicitly on the UCSF model, but runtime used generic `subagent` rather than profile `consult`, so separate worker-session routing could not be verified.
- Main called `ui_describe` twice after the worker outputs and reached no required app action or note patch.
- First-selection signal was lost; selected-problem local rendering split between an updated tag and stale evidence panel.
- Manifest lists nonexistent/unverified `clinical-databases` skill. The isolated test runtime had no installed skills at all — the [platform integration audit](../platform-integration-audit.md) records what was requested versus what was actually available.
- The queued refinement hit the UCSF IP-allowlist 403 in 5.4s and made no app change; local retest reproduced the stale inspector, blank KPI, and first-signal loss.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — carries the full write-up of the declared-profile-bypass defect, including its repro and suggested fix.
- [Apps SDK reference](../../../apps-sdk/sdk-reference.md) — defines `consult`, worker profiles, and the `orchestration.agents` manifest block.
- [Platform integration audit](../platform-integration-audit.md) — the requested/configured/available/exercised accounting that shows the `clinical-databases` skill was never installed.
- [Spec 012 — Contagion Studio](spec-012-contagion-studio.md) — the app that reproduced this same generic-subagent bypass.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks.
