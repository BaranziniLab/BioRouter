# BioRouter QA — Round 5 Issues Report (apps 21–25)

First half of the biomedical-informatics batch. Apps 21–25 = FHIR parser,
survival analysis (R), terminology mapper, clinical-trial simulator, DDI graph.

## Outcome

| # | App | Lang | Tests (independently verified) | Note |
|---|-----|------|-------------------------------|------|
| 21 | ehr-fhir-parser | Python | 253 pass | premature stop → resumed to a large complete toolkit |
| 22 | survival-analysis | **R** | 78 pass | clean 1-shot (KM/Cox/log-rank) |
| 23 | icd-snomed-mapper | Python | code+data complete, **no tests** | partial — agent won't write test_*.py (cf app17) |
| 24 | clinical-trial-sim | Python | 126/128 | group-sequential/alpha-spending/MC; 2 numpy-seed fixture fails |
| 25 | drug-interaction-graph | Rust | 115 pass | clean 1-shot (graph/severity/centrality/suggest) |

4 of 5 substantially green. Cumulative **~25 apps, ~2,500 passing tests** across
Rust / Python / C++ / R.

## Findings

**Premature stream stop is the dominant failure of this batch (HIGH, 3×: 17, 21,
23).** All three cut off mid-stream (rc=0, no error) at a transition to a *new
large block* (the test suite or sample-data files). ~3 of the last ~8 builds.
**→ Round-5 improvement target: continue-on-truncation in the agent loop.**

**"Writes everything but the tests" (MEDIUM, 2×: 17, 23).** Even with explicit,
file-by-file test requests, MiMo sometimes produces only `conftest.py` /
`__init__.py` and treats `pyproject testpaths` as "tests handled". The lone
sub-class the interactive loop does NOT reliably repair. Both accepted as partials
(code+data complete, untested).

**Language reliability ranking is now clear (n≈25):**
- **R** — excellent: 2/2 near-perfect one-shots, idiomatic packages, self-verifies
  with Rscript (validates the analyzer R addition).
- **Rust** — excellent: consistently builds + self-tests; occasional single
  edge-case failure fixed in one turn.
- **Python** — strong, but the recurring src-layout / CLI-subprocess / skipped-test
  reproducibility issues all live here.
- **C++** — most improved: after the early 4–5-turn cmake disasters (apps 3,6,9),
  app 15 was a clean one-shot; still the highest-variance toolchain.

**Infra resilience (positive):** keychain-lock and deleted-binary disruptions both
auto-recovered (rebuild + re-sign + re-run).

## Improvement this round
Implementing **continue-on-truncation** (see IMPROVEMENTS.md): when a streamed
assistant turn ends with no tool call, no final output, and no natural stop
(i.e. truncated mid-task), the agent auto-continues (bounded) instead of returning
control — directly attacking the #1 throughput drag, analogous to how the round-2
retry budget handles transient 429s.
