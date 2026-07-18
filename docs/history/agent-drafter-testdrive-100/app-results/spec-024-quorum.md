# Spec 024 — Quorum

> **What this is.** The static-audit and platform-integration record for `spec-024-quorum`, the
> Quorum systematic-review board Agent Drafter authored during the 100-app test drive. The manifest,
> region and integration cross-checks ran; every browser-driven rubric check is unverified.
> **Status:** Historical record — permanently frozen at "pending browser verification". The 100-app
> run stopped at 25 authored apps and pivoted to the remediation reported in
> [remediation-results.md](../remediation-results.md), so these checks were never completed. The
> ledger carries no per-app timestamp, and the run's only dated event is the 2026-07-12
> provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-024` is the twenty-fourth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#24-quorum);
the app id Agent Drafter was given is `spec-024-quorum`. `br.kb` is the Apps SDK v2 knowledge-base
API, documented in the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. None of those four checks was ever run for this app.

> **Domain shorthand.** `PRISMA` is Preferred Reporting Items for Systematic Reviews and
> Meta-Analyses — the study-flow reporting standard whose counts the app's kanban tracks and whose
> flow diagram the `export_prisma` workflow renders. Risk-of-bias grading is the app's `grade_bias`
> workflow. Both come from the app's brief in the corpus.

> **Schema note.** The integration fields below follow the corrective protocol in the
> [platform integration audit](../platform-integration-audit.md), which applies from spec 021
> onward. Its Available, Exercised and Missing/blocked rows are constants of that protocol and
> repeat verbatim across specs 021–025.

## Run metadata

- **App id:** `spec-024-quorum`
- **Authoring rounds:** 1
- **Reached acceptance:** pending browser verification
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PASS (static; browser pending).**

> **Warning.** That PASS covers one check only — the static manifest/source cross-check, 5.4. Six of
> the seven rows below are `pending`, meaning the check was never executed. Nothing in this record
> establishes that the app works in a browser.

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | pending | Browser verification required |
| Layout matches (5.3) | pending | Static regions only, listed below |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | pending | Browser state/binding drive required |
| Agent-driven loop (5.6) | pending | Two live instructions required |
| Multi-agent ran (5.7) | pending | Profiles declared: appraiser, extractor, screener |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

### Static regions found

Present in the built source; their position and size were never verified in a browser.

- `audit`
- `board`
- `figure`
- `inspector`
- `prisma`
- `progress`
- `protocol`
- `transport`

## Aesthetic verdict

**Recorded verdict: PENDING.** Expected pack `clinical`; manifest pack `clinical`.

## Platform integration

| Field | Finding |
|---|---|
| Requested | `br.kb` search + `page` for full-text; `systematic-review` / `clinical-biostatistics` skills; deep route for appraisal; `figure` renders the PRISMA flow diagram on export. |
| Configured | Extensions `autovisualiser`, `knowledge`; skills none; knowledge base none; grants none; routes `deep_appraisal`, `fast_screening`; workflows `export_prisma`, `extract_fields`, `grade_bias`, `screen_batch`. |
| Available in isolated runtime | Built-in extensions `autovisualiser`, `knowledge`; external connectors none; skills none; knowledge bases none. |
| Exercised | Pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds. |
| Missing/blocked | Requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures. |

> **Unflagged skills gap.** Requested names the `systematic-review` and `clinical-biostatistics`
> skills; Configured records `skills=none`. This record does not list that gap under friction, even
> though the [audit findings register](../audit-findings-register.md) treats unavailable requested
> skills as a defect class in [Spec 006](spec-006-ward-board.md),
> [Spec 008](spec-008-manhattan-signal-room.md) and [Spec 009](spec-009-survival-atelier.md). Under
> the corrective protocol, leaving the ids unset is the prescribed behaviour when a requested skill
> is unavailable — but the unmet requirement is meant to be documented rather than left silent.

The machine-readable form of this audit is in
[data/platform-integrations.json](../data/platform-integrations.json).

## Screenshots

None. No browser session ran, so no screenshot was captured. The `shots/` directory referenced by
browser-verified records in this folder lived inside the ephemeral `.br-testdrive/runtime` sandbox
and was not checked in.

## Friction encountered

None in static review — but nothing that could produce runtime friction was exercised. No browser
turn, no agent loop, and no signal round-trip ran, so this line is not a clean bill of health. The
unmet skill requirement noted above is also absent from this section.

## Related documentation

- [Spec 023 — Longitude](spec-023-longitude.md) — the preceding static-only record under the same
  corrective protocol.
- [Spec 025 — Lattice](spec-025-lattice.md) — the last app the run authored, and the final record in
  this folder.
- [Platform integration audit](../platform-integration-audit.md) — the corrective protocol, and the
  rule that unavailable skills should be left unset and documented honestly.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated entry for Agent
  Drafter configuring nonexistent skills as runtime requirements.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check that was left pending here.
