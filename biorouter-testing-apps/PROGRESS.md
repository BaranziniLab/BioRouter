# BioRouter Build-100 — Progress Tracker

Driver: `biorouter run --no-session` (headless) + periodic interactive TUI via
tmux. Model: **xiaomi_mimo / mimo-v2.5-pro**. Extensions: developer + todo.

| # | App | Lang | Status | Commits | Files | LOC | Notes |
|---|-----|------|--------|---------|-------|-----|-------|
| 1 | algo-pathfinding-rs | Rust | ☑ built + refined | 6 | 17 | ~1630 src | build OK; **54 tests pass**; 6 algos; refine added compare+ANSI colors. lib-only (no bin) |
| 2 | algo-sorting-visualizer-py | Python | ☑ built + refined | 6 | 23 | 3020 | **184 tests pass** (was 156); refine added argparse CLI + --seed + 28 CLI tests; clean incremental commits |
| 4 | algo-graph-toolkit-rs | Rust | ☑ built + fixed | 3 | 17 | 3842 | **92 tests pass** after 2 fix turns (SCC, Prim-forest, Floyd-Warshall id-remap); 13 modules, real binary |
| 5 | algo-string-matching-py | Python | ☑ built + fixed | 4 | 23 | 1750 | **199 tests pass** out-of-the-box after fix turn added `pythonpath=["src"]`; 11 algorithms |
| 3 | algo-bst-avl-redblack-cpp | C++ | ☑ fixed via interaction | 2 | 13 | 2073 | initial build BROKEN (0 compiles); fix turn → builds + **47 tests pass**; ctest not registered |

| 6 | algo-dynamic-programming-cpp | C++ | ☑ built + fixed | 5 | 36 | 1374 | **79 tests pass** — but cost 5 turns (rate-limit + 3 cmake/DP-bug fixes); 11 solvers |
| 7 | algo-hash-table-impl-rs | Rust | ☑ built | 3 | 13 | 1986 | **94 tests pass** (chaining/linear/robinhood); clean one-shot |
| 8 | algo-compression-lz77-huffman-py | Python | ☑ resumed + done | 4 | 16 | 1586 | **98 tests pass** out-of-box (clean venv); LZ77+Huffman+codec |
| 9 | algo-bignum-arbitrary-precision-cpp | C++ | ⚠️ partial (74/76) | 2 | 22 | 2143 | builds clean; fix turn fixed gcd but Karatsuba + division edge cases persist — MiMo weak on subtle C++ arithmetic |
| 10 | algo-bloom-cuckoo-filters-rs | Rust | ☑ built | 4 | 11 | 1590 | **50 tests pass**; bloom/counting/cuckoo/scalable; clean one-shot |

| 11 | bio-seq-alignment-py | Python | ☑ built + fixed | 3 | 30 | 2347 | NW/SW/Gotoh/BLOSUM62/MSA; fix turn converging affine-gap bugs |
| 12 | bio-fasta-fastq-toolkit-rs | Rust | ☑ built | 3 | 16 | 1709 | **68 tests pass**; FASTA/FASTQ parse+stats+quality+convert; clean one-shot |

### Round-2 checkpoint (apps 6-10): ISSUES/round-2-report.md + improvement (deeper rate-limit retry budget) shipped, committed (4abb47d), CLI rebuilt.

### ⏸ PAUSED after app 11 per user request. See FINAL_REPORT.md.

**⚠️ Concurrency lowered to ≤2 builds after MiMo rate-limit (429) truncated apps 6 & 8.**

## Cadence
- Build apps in small parallel batches via the headless harness.
- After every 5 apps: write a consolidated issue/feature report in `ISSUES/` and
  apply a concrete BioRouter improvement (commit on a branch in the BioRouter repo).
- Running UX/failure notes in `FAILURE_LOG.md`.

## Milestones
- [x] Foundation: checklist (100), testing dir, harness, MiMo smoke test passed
- [ ] Apps 1–5 + improvement round 1
- [ ] Apps 6–10 + improvement round 2
- [ ] … through 100
| 18 | bio-blast-lite-rs | Rust | ☑ built + fixed | 3 | 13 | 2326 | **60 tests pass** (51 unit+9 integration); seed-extend BLAST; 1 integration fix turn |
| 19 | bio-genome-assembly-py | Python | ☑ built | 3 | 17 | 3020 | **70 tests pass** out-of-box (OLC+deBruijn assembler, N50); recovered after binary-delete |
| 20 | bio-motif-finder-py | Python | ☑ built (94/97) | 5 | 20 | 3362 | **94 tests pass** (Gibbs/MEME/PWM); 3 CLI-integration tests need pkg install (exit 127) |
| 24 | med-clinical-trial-sim-py | Python | ☑ built (126/128) | 3 | 23 | 3102 | **126 tests pass** (group-sequential, alpha-spending, MC OC); 2 fail on numpy SeedSequence fixture |
| 25 | med-drug-interaction-graph-rs | Rust | ☑ built (1-shot) | 4 | 16 | 2660 | **115 tests pass** (DDI graph, severity, paths, centrality, suggest); clean Rust one-shot |
| 26 | med-risk-score-calculator-py | Python | ☑ built (resumed+fixed) | 3 | 18 | 3839 | **200 tests pass** (8 clinical scores); premature stop→resume created tests→validation fix |
| 27 | med-cohort-builder-sql-py | Python | ☑ built | 3 | 17 | 4040 | **60 tests pass** out-of-box (synthetic EHR + SQL cohort compiler); clean one-shot |
| 28 | med-biomarker-discovery-r | R | ☑ built (1-shot) | 3 | 29 | 2450 | **65 R tests pass** (LASSO/RFE/stability sel, BH-FDR, CV); 3rd clean R one-shot |
| 31 | stat-bayesian-mcmc-py | Python | ☑ built | 5 | 26 | 4051 | **108 tests pass** out-of-box (MH/Gibbs/HMC/slice, R-hat/ESS/HPD); clean, no premature stop |
| 32 | stat-glm-from-scratch-r | R | ☑ built (resumed+fixed) | 5 | 19 | 910 | tests pass on clean **R CMD INSTALL** (IRLS, gaussian/binomial/poisson); premature stop→resume→NAMESPACE fix |
| 33 | stat-timeseries-arima-py | Python | ☑ built | 2 | 29 | 2701 | **70 tests pass** out-of-box (AR/MA/ARIMA/SARIMA/Holt-Winters, ACF/PACF, auto-order); clean |
| 34 | stat-hypothesis-testing-suite-r | R | ☑ built (1-shot) | 2 | 24 | 3028 | **111 R tests pass** (parametric/nonparam/categorical/normality + corrections); installs clean |
| 35 | stat-bootstrap-resampling-py | Python | ☑ built (undeclared dep) | 4 | 24 | 4345 | **90 tests pass** (w/ scipy) (BCa/block/jackknife/permutation); scipy used but NOT declared in pyproject |
