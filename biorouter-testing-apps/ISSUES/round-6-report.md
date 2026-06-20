# BioRouter QA — Round 6 Issues Report (apps 26–30)

Closes the biomedical-informatics batch (apps 21–30). Apps 26–30 = clinical risk
scores, SQL cohort builder, R biomarker discovery, SEIR epidemic model, DICOM tool.

## Outcome

| # | App | Lang | Tests | Note |
|---|-----|------|-------|------|
| 26 | risk-score-calculator | Python | 200 pass | premature stop → resume created tests → validation fix |
| 27 | cohort-builder-sql | Python | 60 pass | clean 1-shot (synthetic EHR + SQL compiler) |
| 28 | biomarker-discovery | **R** | 65 pass | clean 1-shot (LASSO/RFE/stability, BH-FDR, CV) |
| 29 | seir-model | Python | 82 pass | clean 1-shot (SIR/SEIR/SEIRD, RK4, Gillespie, fit) |
| 30 | dicom-image-tool | Python | 124 pass | clean 1-shot (from-scratch DICOM binary parser) |

**31 apps fully verified + ~5 partials; ~3,220 passing tests** across Rust /
Python / C++ / R.

## Findings

**The incremental-test harness mitigation appears to WORK (positive).** After
4 premature stops at the code→tests transition (apps 17, 21, 23, 26), I added a
zero-risk one-line instruction to the build prompt ("write tests INCREMENTALLY …
do NOT defer the entire test suite to the end"). The next three builds (apps 27,
28, 29) — and app 30 — all completed **without a premature stop** and with tests
present. n is small, but the targeted prompt change moved the metric, which is the
QA loop closing its own feedback cycle (observe → cheap fix → measure).

**R is the most reliable toolchain (now 3/3 clean one-shots: apps 16, 22, 28).**
Idiomatic packages, diligent `Rscript`/testthat self-verification, at most one
fix turn. Strong validation of the R support added to the `analyze` tool.

**Language reliability (n≈31):** R ≈ Rust > Python > C++ (variance), with C++
much improved since the early cmake disasters. Python carries the recurring
reproducibility issues (src-layout, CLI-needs-install, occasional skipped tests).

**Substantial-artifact confirmation.** This batch produced genuinely non-trivial
software: a 253-test FHIR R4 toolkit, an adaptive clinical-trial simulator
(alpha-spending + Monte-Carlo OC), a 4k-LOC SQL cohort compiler over a synthetic
EHR, a DDI graph engine, and a **from-scratch DICOM binary parser** (no pydicom) —
all multi-file, multi-thousand-LOC, tested, and git-tracked.

## Improvement this round
Zero-risk harness mitigation (incremental-test prompting) — shipped and apparently
effective. The provider-side continue-on-truncation remains the documented proper
fix (deferred to protect the running loop); the Plan-B Stop hook is the safe
in-product version.
