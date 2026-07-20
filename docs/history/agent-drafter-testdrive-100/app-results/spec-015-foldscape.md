# Spec 015 — FoldScape

> **What this is.** The per-app rubric verdict for `spec-015-foldscape`, the FoldScape protein
> energy-landscape explorer that Agent Drafter authored and a reviewer then drove in a browser
> during the 100-app test drive. It isolates the state-identity defect in which the UI and every
> worker agent reasoned about different residues.
> **Status:** Historical record — three authoring rounds, one browser review, closed. The
> state-identity defect this record names as most serious is part of the audit that drove the
> canonical-state remediation reported in [remediation-results.md](../remediation-results.md). The
> ledger carries no per-app timestamp, and the run's only dated event is the 2026-07-12
> provider-outage resolution recorded in
> [azure-403-outage-incident.md](../azure-403-outage-incident.md).
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

`spec-015` is the fifteenth brief in the 100-idea corpus at
[hundred-app-test-specs.md](../../../agent-drafter/testing/hundred-app-test-specs.md#15-foldscape);
the app id Agent Drafter was given is `spec-015-foldscape`.

The check ids `5.2`–`5.8` below are section numbers from the
[app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md), which defines
each check and rules that an app is functionally PASS only when 5.2, 5.5, 5.6 and 5.7 all hold and
the layout check 5.3 substantially matches. The defects named here are recorded once,
de-duplicated, in the [audit findings register](../audit-findings-register.md).

> **Domain shorthand.** `φ`/`ψ` are the two backbone dihedral angles of a protein residue.
> `L34→A` is a mutation of the leucine at residue 34 to alanine; `M1` is the methionine at residue 1.
> `RMSD` is root-mean-square deviation, the structural-distance axis of the energy funnel. A
> *Ramachandran* figure plots φ against ψ; a *contact map* plots which residue pairs are in contact.
> All four come from the app's brief in the corpus.

## Run metadata

- **App id:** `spec-015-foldscape`
- **Authoring rounds:** 3 real rounds. One further retry is excluded because it was
  harness-induced — the reviewer had misread the omitted default theme — and was interrupted once
  source/readback proved the invariant. The equivalent exclusion for provider outages is defined in
  [azure-403-outage-incident.md](../azure-403-outage-incident.md).
- **Reached acceptance:** static yes; runtime partial
- **Channel:** CLI (named resumable BioRouter session)
- **Provider/model:** `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI)

## Functional verdict

**Recorded verdict: PARTIAL.**

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Structure canvas, energy funnel, residue/dihedral controls, scientific figures, and mutation verdict dominate. |
| Layout matches (5.3) | ⚠️ | Required regions exist, but the header slider overlays title/KPIs and the lower transport/progress clips at 1280×720. |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ⚠️ | Residue selection and φ/ψ controls updated locally, but Energy/RMSD/inspector bindings became blank after mutation. |
| Agent-driven loop (5.6) | ❌ | Workers analyzed stale L34→A while the visible selection was M1/φ=-70°/ψ=4°; main rendered pending minimization but invoked no mutation/minimization action. |
| Multi-agent ran (5.7) | ✅ | Folder, Mutagenesis Critic, and Validator consults all completed on the UCSF model. |
| Signals round-trip (5.8) | ❌ | `residue_selected` and `mutation_chosen` both reported not subscribed; mutation emitted the latter twice. |

> **Most serious finding — state identity.** Check 5.6 above is the defect this app is cited for: the
> UI and all workers reasoned about different residues. The
> [audit findings register](../audit-findings-register.md) records it as *Shared agent state and
> client control state diverge between turns*, first seen in
> [Spec 004 — Trial Regia](spec-004-trial-regia.md) and reproduced here, in the layout probes, and in
> [Spec 016 — AeroCanvas](spec-016-aerocanvas.md).

## Aesthetic verdict

**Recorded verdict: PARTIAL.**

The default `biorouter` pack resolves correctly and the protein ribbon/funnel/Ramachandran/
contact-map visuals are distinctive, but overlay collisions and below-fold transport materially harm
the 720p composition.

## Screenshot evidence

Captured during the run but not preserved in this repository — the `shots/` directory lived inside
the ephemeral `.br-testdrive/runtime` sandbox, which was not checked in. These paths record what was
captured; they are not live links.

- `shots/spec-015/baseline.png`
- `shots/spec-015/mutation-state-split.png`

## Friction encountered

- **Static review.** Clean, after correcting the harness's default-theme interpretation.
- **Authoring.** Three authoring rounds plus a futile reviewer-induced retry exposed that default
  `biorouter` is intentionally omitted during serialization; the retry was interrupted once
  source/readback proved the invariant. The same harness misreading is diagnosed at length in
  [Spec 010 — Diagnosis Odyssey](spec-010-diagnosis-odyssey.md), where `ThemeConfig::is_default` is
  named as the reason the base pack does not appear in a manifest.
- **Defect — state identity.** The most serious runtime issue: the UI and all workers reasoned about
  different residues.
- **Defect — signals and actions.** First-use signals were lost; the agent rendered advice instead of
  applying declared actions and remained `AI · updating data`.

## Related documentation

- [Spec 010 — Diagnosis Odyssey](spec-010-diagnosis-odyssey.md) — the fuller account of the
  default-theme misreading that also cost this app a retry.
- [Spec 016 — AeroCanvas](spec-016-aerocanvas.md) — the next app in the run, which reproduces the
  same state-identity defect through a slider reset.
- [Audit findings register](../audit-findings-register.md) — the de-duplicated entry for the
  state-identity defect, with repro steps and the suggested SDK fix.
- [Remediation results](../remediation-results.md) — what was actually built to close the
  canonical-state defect this record isolates.
- [App test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — the
  definition of every `5.x` check and the pass rule applied here.
