# Spec 023 — Longitude

> **What this is.** The static-audit and platform-integration record for `spec-023-longitude`, the
> Longitude citation-timeline app Agent Drafter authored during the 100-app test drive. The
> manifest, region and integration cross-checks ran; every browser-driven rubric check is
> unverified.
> **Status:** Historical record — permanently frozen at "pending browser verification" after two
> authoring rounds. The run ended before verification, pivoting to the remediation reported in
> [remediation-results.md](../remediation-results.md). The ledger carries no per-app timestamp, and
> the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-023` is the twenty-third brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#23-longitude);
the app id Agent Drafter was given is `spec-023-longitude`. `br.kb` is the Apps SDK v2 knowledge-base
API, documented in the [Apps SDK reference](../../../apps-sdk/sdk-reference.md).

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. None of those four checks was ever run for this app.

> **Schema note.** The integration fields below follow the corrective protocol in the
> [platform integration audit](../platform-integration-audit.md), which applies from spec 021
> onward. Its Available, Exercised and Missing/blocked rows are constants of that protocol and
> repeat verbatim across specs 021–025.

## Run metadata

- **App id:** `spec-023-longitude`
- **Authoring rounds:** 2. This record does not say what the second round changed. The run ledger at
  [data/ledger.json](../data/ledger.json) records the first round as a 324.1-second `build` and the
  second as a 25.9-second `static-fix`, so the second was a correction rather than a rebuild.
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
| Multi-agent ran (5.7) | pending | Profiles declared: annotator, archivist, genealogist |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

### Static regions found

Present in the built source; their position and size were never verified in a browser.

- `agent-notes`
- `era-ruler`
- `figure`
- `inspector`
- `lane-rail`
- `timeline`
- `transport`

## Aesthetic verdict

**Recorded verdict: PENDING.** Expected pack `lab-notebook`; manifest pack `lab-notebook`.

## Platform integration

| Field | Finding |
|---|---|
| Requested | `br.kb` search/graph for citations; fast route for card placement, deep for lineage reasoning; `figure` plots a citations-per-year sparkline in the inspector. |
| Configured | Extensions `autovisualiser`, `knowledge`; skills none; knowledge base none; grants none; routes `deep`, `fast`; workflows `branch_here`, `place_card_fast`, `trace_lineage`. |
| Available in isolated runtime | Built-in extensions `autovisualiser`, `knowledge`; external connectors none; skills none; knowledge bases none. |
| Exercised | Pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds. |
| Missing/blocked | Requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures. |

The machine-readable form of this audit is in
[data/platform-integrations.json](../data/platform-integrations.json).

## Screenshot evidence

None. No browser session ran, so no screenshot was captured. The `shots/` directory referenced by
browser-verified records in this folder lived inside the ephemeral `.br-testdrive/runtime` sandbox
and was not checked in.

## Friction encountered

None in static review — but nothing that could produce runtime friction was exercised. No browser
turn, no agent loop, and no signal round-trip ran, so this line is not a clean bill of health.

## Related documentation

- [Spec 022 — Crossfire](spec-022-crossfire.md) — the preceding static-only record under the same
  corrective protocol.
- [Spec 024 — Quorum](spec-024-quorum.md) — the next static-only record, where requested skills are
  configured as none without being flagged.
- [Spec 021 — Radiant](spec-021-radiant.md) — the first record under the corrective protocol, with
  the schema note in full.
- [Platform integration audit](../platform-integration-audit.md) — the corrective protocol that
  defines these fields and the rule that a configured name earns no credit.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check that was left pending here.
