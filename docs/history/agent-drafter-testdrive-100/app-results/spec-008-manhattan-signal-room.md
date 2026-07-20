# Spec 008 — Manhattan Signal Room

> **What this is.** The per-app functional and aesthetic rubric verdict for
> `spec-008-manhattan-signal-room`, a GWAS workbench, and the record of a
> fabricated-statistics incident by its main agent.
> **Status:** Historical record — a closed July 2026 run (one successful round, one
> provider-blocked) from the completed audit phase of the campaign; see the
> [cumulative findings register](../audit-findings-register.md) and the
> [remediation results](../remediation-results.md).
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

The app is a genome-wide association study (GWAS) workbench, so its verdict rows carry
statistical-genetics shorthand:

| Term | Meaning in this record |
|---|---|
| `GWAS` | Genome-wide association study — the analysis the app visualizes as a Manhattan plot. |
| `SNP`, `rs123` | A single-nucleotide polymorphism and its dbSNP-style rsID; `rs123` is the variant the tester selected. |
| `PIP` | Posterior inclusion probability — the fine-mapping quantity the agent fabricated. |
| `λ=1.00` | The genomic inflation factor reported by the app after an action. |
| Prospector, Fine Mapper, Colocalizer, Interpreter | The app's four declared worker profiles, written here as display names. |

> **Note on geometry figures.** This record mixes approximate (`y≈1111`) and exact
> (`1173px tall`, `overflows horizontally by 16px`) measurements without stating how each
> was taken. §4.2 of the runbook prescribes reading geometry from the live page with
> `browser_evaluate`. All figures are preserved as written.

## Run metadata

| Field | Value |
|---|---|
| App id | `spec-008-manhattan-signal-room` |
| Authoring rounds | 1 successful + 1 provider-blocked |
| Reached acceptance | no |
| Channel | CLI (named resumable BioRouter session) |
| Provider/model | `versa_azure/gpt-5.5-2026-04-24` (UCSF Azure OpenAI) |

## Functional verdict: PARTIAL

| Check | Result | Notes |
|---|---|---|
| Not a chatbot (5.2) | ✅ | Wide interactive Manhattan/locus workbench with peak rail, inspector, tissues, and transport |
| Layout matches (5.3) | ⚠️ | Regions exist, but transport is at y≈1111 and document is 1173px tall at 720p |
| Declared surface (5.4) | ✅ | Static manifest/source cross-check |
| Client reactivity (5.5) | ✅ | rs123 selection immediately updated both plots and the inspector; actions later rendered 5 SNPs and λ=1.00 |
| Agent-driven loop (5.6) | ⚠️ | Action batches ran, but repeated describe prevented locus brief, remaining workers, and second instruction |
| Multi-agent ran (5.7) | ⚠️ | Prospector and Fine Mapper ran on UCSF; Colocalizer and Interpreter were not reached |
| Signals round-trip (5.8) | ⚠️ | Gesture started a turn but ambient status still reported `peak_clicked` unsubscribed |

## Aesthetic verdict: PARTIAL

- The dark violet Manhattan plot, ranked peak rail, locus panel, inspector cards, and floating KPI strongly match `midnight`.
- The primary transport is far below the acceptance viewport and the page overflows horizontally by 16px.

## Screenshot evidence

> **Note.** The run's `shots/` directory was branch-local to the test worktree and is not
> part of this documentation repository, so the filename below is a reference only.

- `spec-008-initial.png`

The machine-readable static audit survives at
[`../authoring-logs/spec-008-manhattan-signal-room-static.json`](../authoring-logs/spec-008-manhattan-signal-room-static.json).

## Friction encountered

Each item below is rolled up in the
[cumulative findings register](../audit-findings-register.md).

### Fabricated statistics after a worker refused

This is the most serious finding from this app and is written up in the register as
`[FUNCTIONAL-BUG][SEV: high] Main fabricates quantitative scientific output after worker
reports insufficient data`.

- Fine Mapper responsibly refused defensible PIPs from insufficient inputs, but main then invented a normalized five-SNP PIP vector and rendered it.

### Other friction

- The generated `statistical-genetics` skill failed to load.
- Prospector/Fine Mapper sessions were verified on the UCSF model; Colocalizer/Interpreter were never reached.
- Main made four extra `ui_describe` calls between action phases; locus story stayed blank.
- The queued refinement hit the UCSF IP-allowlist 403 in 4.2s and made no app change; local retest still showed below-fold transport, first-signal loss, and split/stale selection state.

## Related documentation

- [Cumulative findings register](../audit-findings-register.md) — the full write-up of the fabricated-PIP incident, with the exact invented values and the suggested provenance guard.
- [Agent Drafter 100-app test-drive runbook](../../../agent-drafter/testing/app-test-drive-runbook.md) — defines the 5.2–5.8 rubric checks and the geometry-measurement procedure.
- [Remediation results](../remediation-results.md) — what was built to move contract clauses out of the system prompt and into enforced checks.
- [UCSF Azure 403 outage incident](../azure-403-outage-incident.md) — the provider blocker that voided this app's queued refinement.
- [Platform integration audit](../platform-integration-audit.md) — the accounting behind the failed `statistical-genetics` skill load.
