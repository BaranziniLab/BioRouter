# Spec 010 — Diagnosis Odyssey

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-010-diagnosis-odyssey`, a diagnostic reasoning graph — a functional FAIL caused
> by worker timeouts — plus a correction to the test harness's own theme-audit logic.
> **Status:** Historical record — a closed four-round July 2026 run. The harness bug it
> describes (default-theme canonicalization) has already been corrected, so this file is
> purely retrospective.
> **Audience:** developers working on Agent Drafter and the Apps SDK.

The 100-app test drive asked Agent Drafter to author 100 different scientific apps from
written briefs, then drove each finished app in a real browser to check whether it
behaved as it declared. A *verdict* is the score one app earned against the runbook's
rubric — a functional verdict (does it work as an agent-driven surface?) and an
aesthetic verdict (does it look the way the brief asked?). This file records that
verdict for one app, and one platform-level correction the run produced along the way.

## How to read this record

- **`spec-NNN`** identifies a numbered brief in [the 100 agentic app test specs](../../../agent-drafter/testing/hundred-app-test-specs.md); app ids follow `spec-NNN-<slug>`. The campaign-wide roll-up is [the authored-app verdict index](../authored-app-verdict-index.md).
- **Check IDs `5.2`–`5.8`** are rubric sections defined in [the test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) (§5). An app is a functional **PASS** only if 5.2, 5.5, 5.6 and 5.7 all hold and the layout (5.3) substantially matches; §6 scores the aesthetic verdict independently.
- **Reached acceptance** records whether the app cleared that bar. This result set uses `yes`, `no` and `partial`; only the PASS rule above is formally defined in the runbook.
- **Pathfinder, Test Recommender, Refuter and Chronicler** are the app's four declared worker profiles, written here as display names.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-010-diagnosis-odyssey` |
| Authoring rounds | 4 |
| Reached acceptance | no — browser failures |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: FAIL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Interactive phenotype→syndrome→gene reasoning graph with dossier and path chronicle |
| Layout matches (5.3) | ⚠️ | Regions exist, but primary transport starts near y=788 in a 720px viewport |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ❌ | Fabry selection left core dossier/yield/chronicle bindings blank or stale |
| Agent-driven loop (5.6) | ❌ | Two 120s worker timeouts; main completed without actions/UI update; no second instruction |
| Multi-agent ran (5.7) | ❌ | Pathfinder and Test Recommender timed out; Refuter/Chronicler never ran |
| Signals round-trip (5.8) | ⚠️ | Click started a turn, but ambient status reported `node_clicked` unsubscribed |

## Aesthetic verdict: PARTIAL

- The custom black/ivory map, gold confirmed-node treatment, rare-disease rail, and candidate dossier strongly express the intended BioRouter aesthetic.
- The absent theme block correctly resolves to the default `biorouter` pack; primary controls remain below the 720p viewport.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-010-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-010-diagnosis-odyssey-static.json`](../authoring-logs/spec-010-diagnosis-odyssey-static.json).

## Harness correction: an omitted default theme block is not a missing theme

This finding is about the reviewer harness, not about this app, and it generalizes to
every app in the run. It is registered as `[HARNESS-BUG][SEV: high][RESOLVED]` in the
[cumulative findings register](../audit-findings-register.md).

- The original audit misread default-theme canonicalization and caused unnecessary refinement rounds: `ThemeConfig::is_default` intentionally omits the base `biorouter` block on serialization. The corrected reviewer resolves absence to `biorouter`.

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

- Manifest invents capabilities that did not exist in the test runtime: two unavailable **skills** (`rare-disease`, `clinical-databases`) and one **knowledge base id** (`hpo-omim-gene-disease`); search returned no evidence.
- Pathfinder and Test Recommender each hit the 120-second timeout; main performed no required action and silently completed.
- Fabry selection exposed blank/stale bound values and first-signal loss.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — carries the resolved harness-bug entry above and the 120-second worker-timeout finding, each with a repro.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the aesthetic review in §6.
- [Platform integration audit](../platform-integration-audit.md) — the requested/configured/available/exercised accounting behind the invented skill and KB ids.
- [Spec 006 — Ward Board](spec-006-ward-board.md) — the other app that declared the nonexistent `clinical-databases` skill.
- [Remediation results](../remediation-results.md) — what was built in response to these findings.
