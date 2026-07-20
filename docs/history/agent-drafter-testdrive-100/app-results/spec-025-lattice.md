# Spec 025 — Lattice

> **What this is.** The static-audit and platform-integration record for
> `spec-025-lattice`, a hypothesis-generation app authored during the Agent Drafter
> Apps SDK v2 100-app test drive. Every browser check in it is unverified.
> **Status:** Historical record — the last app the July 2026 test drive authored. Authoring
> stopped at spec 025 of 100, so this file was frozen at "pending browser verification" and
> never completed; the campaign moved on to the remediation reported in
> [remediation results](../remediation-results.md). Specs 026–100 were never authored and
> have no rubric here.
> **Audience:** developers working on Agent Drafter and the Apps SDK.

The 100-app test drive asked Agent Drafter to author 100 scientific apps from written
briefs, then drive each finished app in a real browser to check whether it behaved as it
declared. This app cleared the static half of that process — its manifest and source were
cross-checked against the brief — and the run ended before the browser half could be
performed. Read it as evidence of what Agent Drafter *built*, not of what the app *does*.

## How to read this record

- **`spec-NNN`** identifies a numbered brief in
  [the 100 agentic app test specs](../../../agent-drafter/testing/hundred-app-test-specs.md);
  app ids follow `spec-NNN-<slug>`. The campaign-wide roll-up is
  [the authored-app verdict index](../authored-app-verdict-index.md), which covers only
  specs 001–018 — the apps that reached a full browser verdict.
- **Check IDs `5.2`–`5.8`** are rubric sections defined in §5 of
  [the test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md)
  (`5.2` = "It is not a chatbot", through `5.8` = "Signals round-trip"). An app is a
  functional **PASS** only if 5.2, 5.5, 5.6 and 5.7 all hold and the layout (5.3)
  substantially matches; §6 scores the aesthetic verdict independently.
- **`pending`** means the check was never run, not that it was run and was inconclusive.
- **This is the terminal file of the run.** Spec 025 is the highest-numbered app authored;
  see [the test-drive README](../README.md) for why the run stopped here.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-025-lattice` |
| Authoring rounds | 1 |
| Reached acceptance | pending browser verification |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PASS (static; browser pending)

> **Note.** The PASS applies only to check 5.4, the static manifest/source cross-check. The
> other six checks require a browser and were never run, so this row of the corpus carries
> no functional evidence either way.

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | pending | Browser verification required |
| Layout matches (5.3) | pending | Static regions: figure, inspector, lattice, progress, seed, warnings |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | pending | Browser state/binding drive required |
| Agent-driven loop (5.6) | pending | Two live instructions required |
| Multi-agent ran (5.7) | pending | Profiles declared: falsifier, generator, planner |
| Signals round-trip (5.8) | pending | Gesture + agent reaction required |

## Aesthetic verdict: PENDING

- Expected pack `midnight`; manifest pack `midnight`.

## Platform integration

- **Requested:** `br.kb` for grounding + prior-art; `single-cell`/`crispr-screens` skills for assay plans; deep route for generation, fast for scoring; `figure` sketches an expected-effect plot per hypothesis.
- **Configured:** extensions=['autovisualiser', 'knowledge']; skills=none; knowledge_base=none; grants=none; routes=['deep', 'deep_generation', 'fast', 'fast_scoring']; workflows=['design_experiment', 'generate_children', 'prune_dead_ends', 'rank_testability']
- **Available in isolated runtime:** built-in extensions=['autovisualiser', 'knowledge']; external connectors=none; skills=none; KBs=none.
- **Exercised:** pending browser/session verification. A configured name is not credited until a real runtime tool/route/workflow succeeds.
- **Missing/blocked:** requested skills, KB payloads, and external connectors are unavailable unless the catalog changes; invented ids are static failures.

Two details in the lines above are worth reading deliberately.

- **The requested skills are absent by design.** `single-cell` and `crispr-screens` are not
  in the isolated runtime's skill catalog, and from spec 021 onward the corrective protocol
  in the [platform integration audit](../platform-integration-audit.md) required Agent
  Drafter to leave unavailable ids unset rather than invent them. `skills=none` is therefore
  the protocol working — but the brief's assay-plan requirement went unmet, and no
  substitute was configured.
- **Four route names cover two requested roles.** The brief asks for a deep route for
  generation and a fast route for scoring; the manifest declares `deep`,
  `deep_generation`, `fast`, and `fast_scoring`. The static review recorded no issue against
  this (the app's entry in [`../data/ledger.json`](../data/ledger.json) has an empty
  `issues` list), and no browser turn ran to show which of the four a real route selection
  would use.

## Screenshot evidence

> **Note.** The run's `shots/` directory was local to the test worktree and is not part of
> this documentation repository. No screenshot was captured for this app in any case — the
> planned `spec-025-*.png` captures were still pending when the run stopped.

The machine-readable static audit for this app survives at
[`../authoring-logs/spec-025-lattice-static.json`](../authoring-logs/spec-025-lattice-static.json).

## Friction encountered

None in static review.

## Related documentation

- [Test-drive README](../README.md) — the campaign index, and the record of why authoring stopped at this spec.
- [Agent Drafter app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the browser procedure this app never received.
- [Platform integration audit](../platform-integration-audit.md) — the requested/configured/available/exercised framework used above, and the spec-021-onward protocol that produced `skills=none`.
- [Spec 024 — Quorum](spec-024-quorum.md) — the preceding app in the run, frozen in the same static-only state and sharing the same boilerplate availability findings.
- [Remediation results](../remediation-results.md) — what the campaign built instead of finishing the remaining 75 apps.
