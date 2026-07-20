# Spec 017 — Automata Loom

> **What this is.** The per-app rubric verdict for `spec-017-automata-loom`, the Automata Loom
> cellular-automata rule-space explorer that Agent Drafter authored and a reviewer then drove in a
> browser during the 100-app test drive. It is the first result in the corpus to carry a
> platform-integration audit of what the app requested versus what it actually exercised.
> **Status:** Historical record — one authoring round, one browser review, closed. The stricter
> integration review it introduces was later applied corpus-wide, and the campaign's defects were
> remediated; see [remediation-results.md](../remediation-results.md). The ledger carries no per-app
> timestamp, and the run's only dated event is the 2026-07-12 provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-017` is the seventeenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#17-automata-loom);
the app id Agent Drafter was given is `spec-017-automata-loom`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

> **Two review generations.** "The original reviewer" is the static reviewer as it stood when this
> app was built. "The stricter integration review" is the later catalog- and identifier-aware audit
> defined in the [platform integration audit](../platform-integration-audit.md), which refuses to
> credit a manifest string as a working capability and re-audits specs 001–020. This is the first
> per-app record to include its Requested / Configured / Available / Exercised / Missing-blocked
> schema; earlier records predate it and were not rewritten against it.

> **Domain shorthand.** `B23/S23` is birth/survival rule notation — a cell is born on 2 or 3 live
> neighbours and survives on 2 or 3. The `λ-entropy` plot is the Langton's-λ and entropy figure the
> brief asks for to classify a rule. A *glider* is a small pattern that translates across the grid;
> the app's Taxonomist profile is meant to detect and log one.

## Run metadata

- **App id:** `spec-017-automata-loom`
- **Authoring rounds:** 1
- **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Dense cellular grid, rule bits, entropy plot, discovery log/table, and transport dominate. |
| Layout matches (5.3) | ✅ | Required rail/canvas/inspector/transport exist with an unusually dense retro-scientific terminal treatment. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | Rule changed to B23/S23 and canvas advanced, but the B2 pressed state and multiple bindings became blank/stale after Step. |
| Agent-driven loop (5.6) | ⚠️ | Hunt consulted all workers, notified/highlighted, invoked an app action, and advanced the grid, but then repeated describe/notify and never completed the three-trial hunt. |
| Multi-agent ran (5.7) | ✅ | Explorer, Taxonomist, and Archivist consults all completed. |
| Signals round-trip (5.8) | ❌ | First rule-bit gesture reported `rule_bit_toggled` not subscribed. |

## Aesthetic verdict

**Recorded verdict: PARTIAL.**

Baseline is an excellent phosphor terminal CA loom. During the agent run, duplicated progress streams
and opaque black regions obscured the rail/inspector and reduced scientific readability.

## Platform integration

| Field | Finding |
|---|---|
| Requested | Life-lexicon KB, fast/deep routes, scientific λ-entropy figure, overnight batch workflow. |
| Configured | Real `knowledge` + `autovisualiser`; UCSF routes `fast_stepping` / `deep_classification`; workflow `batch_scan_rule_space`; invented `ca-rules-lexicon` KB/grant. |
| Available | Both built-in extensions and the UCSF routes/workflow structure; no KB payload, skills, external extension, or connector exists in the isolated catalog. |
| Exercised | Worker/action UI loop ran; no real KB, route selection, or workflow execution was evidenced. |
| Missing/blocked | Requested KB grounding is unavailable; the concrete KB id/grant is invalid environmental configuration and is queued for Agent Drafter removal. |

The machine-readable form of this audit, including the per-app issue list, is in
[data/platform-integrations.json](../data/platform-integrations.json).

## Screenshot evidence

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-017/baseline.png`
- `shots/spec-017/glider-run.png`

## Friction encountered

- **Authoring and audit.** The first Drafter round built cleanly under the original reviewer; the
  stricter integration review flags the invented KB/grant.
- **Defect — canonical state.** Direct-state bindings diverged after a rule toggle/step.
- **Defect — duplicated progress.** The timeline was mounted into both the canvas and inspector,
  duplicating every progress frame.
- **Agent behaviour.** The agent's action advanced the automaton, but the loop remained active
  instead of completing and logging the promised hunt.

## Related documentation

- [Spec 018 — SystemDynamics Forge](spec-018-systemdynamics-forge.md) — the next app, which repeats
  this platform-integration schema and fails four of its fields rather than one.
- [Platform integration audit](../platform-integration-audit.md) — where the Requested / Configured /
  Available / Exercised / Missing-blocked schema is defined and why it was introduced.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated entries for the
  invented KB identifier, the duplicated progress stream, and the diverging state.
- [Spec 016 — AeroCanvas](spec-016-aerocanvas.md) — the preceding app, and the point from which the
  authoring prompts carried an anti-template clause.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check and the pass rule applied here.
