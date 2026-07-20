# 100-app Agent Drafter test drive

This folder is the archived evidence set for a test drive that pointed Agent Drafter — BioRouter's
app-authoring MCP extension — at a 100-app specification corpus, audited every app it produced, and
then remediated the platform defects the audit exposed. It holds the per-app rubrics, the
machine-readable authoring ledger, three cross-cutting audits, the six-wave remediation plan, and the
completion report that closed the campaign.

> **Status:** Historical record — the run is over and the remediation shipped. Authoring stopped at
> spec 025 of 100; `app-results/` holds spec-001 through spec-025, and 18 of those carry full
> browser verdicts in [authored-app-verdict-index.md](authored-app-verdict-index.md). The campaign
> closed with the fixes reported in [remediation-results.md](remediation-results.md), built on branch
> `feat/apps-sdk-v2` in commits `ae8987a6`, `7527f848`, and `d8cf95cc`.
> **Audience:** maintainers of Agent Drafter and the Apps SDK.

The test drive's thesis, and the reason the remediation exists, is recorded in the plan: Agent
Drafter reliably built a correct static shell for every app, and then failed the agentic contract it
had just declared, because that contract was enforced only as prose in a system prompt. Everything
here is evidence for or against that claim.

## Reading order

1. [pre-run-baseline-gates.md](pre-run-baseline-gates.md) — the green state the run started from.
2. [authored-app-verdict-index.md](authored-app-verdict-index.md) — one row per authored app, with
   its static, functional, and aesthetic verdict.
3. [audit-findings-register.md](audit-findings-register.md) — the de-duplicated defect register that
   the per-app rubrics fed.
4. [remediation-plan.md](remediation-plan.md) — each finding mapped to a specific code fix.
5. [remediation-results.md](remediation-results.md) — what was actually built, and what it caught.

## Files in this folder

| Path | What it holds |
|---|---|
| [pre-run-baseline-gates.md](pre-run-baseline-gates.md) | Worktree, branch, baseline commit, pinned model, and the five test gates that passed before any app was authored. |
| [authored-app-verdict-index.md](authored-app-verdict-index.md) | Authored-app, verdict, and screenshot index for specs 001–018. |
| [audit-findings-register.md](audit-findings-register.md) | Cumulative, de-duplicated findings register — 22 findings with symptom, repro, root cause, impact, and suggested fix. |
| [azure-403-outage-incident.md](azure-403-outage-incident.md) | The UCSF Azure 403 / IP-allowlist outage that halted the run, its resolution, and the fail-fast harness correction it forced. |
| [layout-diversity-audit.md](layout-diversity-audit.md) | Corpus-level layout audit plus five controlled no-sidebar probe apps built to test structural range. |
| [platform-integration-audit.md](platform-integration-audit.md) | Requested / configured / available / exercised audit for extensions, connectors, skills, knowledge bases, routes, workflows, figures, and exports. |
| [remediation-plan.md](remediation-plan.md) | Six-wave engineering plan mapping each finding to a code fix, with `file:line` citations, effort, risk, and gates. |
| [remediation-results.md](remediation-results.md) | Completion report: what shipped per wave, corpus re-lint results, test counts, the self-repair proof, and the not-done ledger. |
| `app-results/` | Per-app functional and aesthetic rubrics, `spec-001-variant-tribunal.md` through `spec-025-lattice.md`. |
| `layout-probes/` | Browser rubrics for the five no-sidebar archetype probes: `kpi-mosaic.md`, `centered-wizard.md`, `radial-canvas.md`, `tabletop-workbench.md`, `constellation.md`. |
| `authoring-logs/` | Per-app static audits (`spec-NNN-<slug>-static.json`) and one captured browser-issue log. |
| `data/ledger.json` | Machine-readable authoring rounds, timings, and audit state. |
| `data/platform-integrations.json` | Machine-readable output of the platform-integration audit. |
| `data/layout-probe-static-audit.json` | Static evidence for the five layout probes. |

## Conventions used across these files

- **`spec-NNN`** identifies an idea by its position in the 100-app corpus at
  [hundred-app-test-specs.md](../../agent-drafter/testing/hundred-app-test-specs.md). The app id
  Agent Drafter was given is `spec-NNN-<slug>`, and the same id names the rubric in `app-results/`
  and the static audit in `authoring-logs/`.
- **Findings are numbered 1–22** in the order they appear in
  [audit-findings-register.md](audit-findings-register.md); the remediation plan's map table cites
  them by that number.
- **Waves 0–5** are the remediation plan's dependency-ordered work stages, each gated on the
  previous one. [remediation-results.md](remediation-results.md) also reports a Wave 6 that the plan
  did not contain.
- **Model pin.** Authoring and runtime models were pinned to the UCSF Azure OpenAI deployment
  `versa_azure/gpt-5.5-2026-04-24`; the audits rejected any other model reference.

> **Note.** The run executed inside an ephemeral, worktree-local sandbox rooted at
> `.br-testdrive/runtime`, with app stores under
> `.br-testdrive/runtime/config/biorouter/agent_drafter/<app-id>/`. That sandbox was not checked in,
> so paths beginning `.br-testdrive/` do not resolve in this repository. The same applies to the
> `shots/` screenshot directory referenced throughout these documents: the screenshots were not
> preserved, and every `shots/…` path cited below is a record of what was captured, not a live link.

## Running the harness regression suite

The driver that executed the run is `scripts/agent-drafter-testdrive/`. Its regression suite still
runs:

```bash
python3 -m unittest scripts/agent-drafter-testdrive/test_run.py -v
```

## Related documentation

- [App test-drive runbook](../../agent-drafter/testing/app-test-drive-runbook.md) — the procedure
  this run was required to follow, including the per-app rubric.
- [Hundred-app test specs](../../agent-drafter/testing/hundred-app-test-specs.md) — the 100-idea
  corpus every `spec-NNN` here refers to.
- [Agent Drafter apps platform design](../../agent-drafter/apps-platform-design.md) — how the
  platform under test is meant to work.
- [Apps SDK v2 design](../../apps-sdk/v2-design.md) — the contract whose enforcement gaps this
  campaign found and closed.
- [Agent Drafter stress test](../agent-drafter-stress-test/README.md) — the earlier, smaller
  stress campaign against the same extension.
