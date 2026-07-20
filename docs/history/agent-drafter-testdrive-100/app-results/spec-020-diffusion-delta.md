# Spec 020 — Diffusion Delta

> **What this is.** The static-audit record for `spec-020-diffusion-delta`, the Diffusion Delta
> dispersion-modelling app Agent Drafter authored during the 100-app test drive. Only the manifest
> and region cross-check ran; every browser-driven rubric check is unverified.
> **Status:** Historical record — permanently frozen at "pending browser verification". The run
> stopped at 25 authored apps and pivoted to the remediation reported in
> [remediation-results.md](../remediation-results.md), so these checks were never completed. The
> ledger carries no per-app timestamp, and the run's only dated event is the 2026-07-12
> provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-020` is the twentieth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#20-diffusion-delta);
the app id Agent Drafter was given is `spec-020-diffusion-delta`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. None of those four checks was ever run for this app.

This record and [Spec 019 — Circuit Bench](spec-019-circuit-bench.md) share the same shape: both
were authored in the batch that was in progress when the run stopped, and both differ only in app
name, region list, declared profiles, and theme pack.

## Run metadata

- **App id:** `spec-020-diffusion-delta`
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
| Multi-agent ran (5.7) | pending | Profiles declared: dispersion_modeler, meteorologist, risk_assessor |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

### Static regions found

Present in the built source; their position and size were never verified in a browser.

- `dosage-kpi`
- `field-rail`
- `inspector`
- `map`
- `monitor-table`
- `notes`
- `plot`
- `presence`
- `progress`
- `site-monitors`
- `transport`

## Aesthetic verdict

**Recorded verdict: PENDING.** Expected pack `midnight`; manifest pack `midnight`.

## Screenshot evidence

None. No browser session ran, so no screenshot was captured. The `shots/` directory referenced by
browser-verified records in this folder lived inside the ephemeral `.br-testdrive/runtime` sandbox
and was not checked in.

## Platform integration

This record has no platform-integration audit. The corrective protocol that added the
Requested / Configured / Available / Exercised / Missing-blocked schema to per-app records applies
from spec 021 onward, as documented in the
[platform integration audit](../platform-integration-audit.md); the corpus-level integration
evidence for this app is in that audit and in
[data/platform-integrations.json](../data/platform-integrations.json).

## Friction encountered

None in static review — but nothing that could produce runtime friction was exercised. No browser
turn, no agent loop, and no signal round-trip ran, so this line is not a clean bill of health.

## Related documentation

- [Spec 019 — Circuit Bench](spec-019-circuit-bench.md) — the other static-only record with no
  platform-integration section, authored in the same batch.
- [Spec 021 — Radiant](spec-021-radiant.md) — the first record written under the corrective
  protocol, and so the first static-only record that does carry an integration audit.
- [Authored-app verdict index](../authored-app-verdict-index.md) — which explains that specs 019–020
  were the batch in progress when the run stopped.
- [Platform integration audit](../platform-integration-audit.md) — the corrective protocol that
  changed per-app reporting from spec 021 onward.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check that was left pending here.
