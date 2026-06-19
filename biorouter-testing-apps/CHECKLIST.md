# BioRouter Build-100 Test Checklist

Goal: drive the **BioRouter CLI** (Xiaomi MiMo, `mimo-v2.5-pro`, developer + todo
only) to build 100 substantial software artifacts — each in its own git repo
under this directory — as a comprehensive end-to-end test of the agent system.

Scale target: each item should be a real artifact (multiple files, hundreds–
thousands of LOC), not a one-file script. Every repo is `git init`'d and commits
are tracked.

Status legend: ☐ todo · ◐ in progress · ☑ done · ✗ blocked (see FAILURE_LOG.md)

## Batch 1 — Algorithms & data structures (1–10)
1. ☐ `algo-pathfinding-rs` — A*/Dijkstra/BFS pathfinding lib + CLI maze solver (Rust)
2. ☐ `algo-sorting-visualizer-py` — sorting algorithms + animated terminal visualizer (Python)
3. ☐ `algo-bst-avl-redblack-cpp` — balanced BST family with tests (C++)
4. ☐ `algo-graph-toolkit-rs` — graph algorithms (SCC, MST, max-flow, topo) (Rust)
5. ☐ `algo-string-matching-py` — KMP/Boyer-Moore/Rabin-Karp/suffix-array (Python)
6. ☐ `algo-dynamic-programming-cpp` — classic DP problem set + benchmark harness (C++)
7. ☐ `algo-hash-table-impl-rs` — open-addressing + chaining hash maps w/ bench (Rust)
8. ☐ `algo-compression-lz77-huffman-py` — LZ77 + Huffman codec (Python)
9. ☐ `algo-bignum-arbitrary-precision-cpp` — arbitrary-precision integer library (C++)
10. ☐ `algo-bloom-cuckoo-filters-rs` — probabilistic filters with FPR analysis (Rust)

## Batch 2 — Bioinformatics (11–20)
11. ☐ `bio-seq-alignment-py` — Needleman-Wunsch + Smith-Waterman aligner (Python)
12. ☐ `bio-fasta-fastq-toolkit-rs` — FASTA/FASTQ parser, stats, QC tool (Rust)
13. ☐ `bio-phylo-tree-builder-py` — neighbor-joining / UPGMA phylogenetics (Python)
14. ☐ `bio-variant-caller-pipeline-py` — pileup → variant calling pipeline (Python)
15. ☐ `bio-kmer-counter-cpp` — k-mer counting + de Bruijn graph (C++)
16. ☐ `bio-gene-expression-r` — RNA-seq differential expression analysis (R)
17. ☐ `bio-protein-structure-py` — PDB parser + secondary-structure metrics (Python)
18. ☐ `bio-blast-lite-rs` — seed-and-extend local alignment search (Rust)
19. ☐ `bio-genome-assembly-py` — overlap-layout-consensus mini-assembler (Python)
20. ☐ `bio-motif-finder-py` — Gibbs sampling / MEME-style motif discovery (Python)

## Batch 3 — Biomedical informatics (21–30)
21. ☐ `med-ehr-fhir-parser-py` — FHIR resource parser + patient timeline (Python)
22. ☐ `med-icd-snomed-mapper-py` — clinical terminology crosswalk service (Python)
23. ☐ `med-survival-analysis-r` — Kaplan-Meier + Cox PH modeling (R)
24. ☐ `med-clinical-trial-sim-py` — adaptive trial design simulator (Python)
25. ☐ `med-drug-interaction-graph-rs` — drug-drug interaction graph engine (Rust)
26. ☐ `med-dicom-image-tool-py` — DICOM reader + windowing/segmentation (Python)
27. ☐ `med-risk-score-calculator-py` — composable clinical risk scores API (Python)
28. ☐ `med-cohort-builder-sql-py` — cohort query builder over synthetic EHR (Python)
29. ☐ `med-biomarker-discovery-r` — feature selection for biomarker panels (R)
30. ☐ `med-epidemic-seir-model-py` — SEIR/agent-based epidemic simulator (Python)

## Batch 4 — Statistics & data analysis (31–45)
31. ☐ `stat-bayesian-mcmc-py` — Metropolis-Hastings / Gibbs sampler library (Python)
32. ☐ `stat-glm-from-scratch-r` — generalized linear models implementation (R)
33. ☐ `stat-timeseries-arima-py` — ARIMA/Holt-Winters forecasting toolkit (Python)
34. ☐ `stat-hypothesis-testing-suite-r` — comprehensive test battery + reporting (R)
35. ☐ `stat-bootstrap-resampling-py` — bootstrap/jackknife/permutation engine (Python)
36. ☐ `stat-pca-dimreduction-cpp` — PCA/t-SNE/UMAP-lite numerics (C++)
37. ☐ `data-etl-pipeline-py` — configurable ETL pipeline w/ validation (Python)
38. ☐ `data-csv-query-engine-rs` — columnar CSV query engine (Rust)
39. ☐ `data-dashboard-generator-py` — static analytics dashboard builder (Python)
40. ☐ `data-stream-aggregator-rs` — streaming windowed aggregations (Rust)
41. ☐ `stat-survival-power-r` — power analysis + sample size calculator (R)
42. ☐ `stat-mixed-models-r` — linear mixed-effects modeling (R)
43. ☐ `data-anomaly-detection-py` — multivariate anomaly detection toolkit (Python)
44. ☐ `data-feature-store-py` — feature engineering + store with lineage (Python)
45. ☐ `stat-causal-inference-py` — propensity scoring / IPW / DiD (Python)

## Batch 5 — Machine learning & numerical (46–55)
46. ☐ `ml-neural-net-from-scratch-py` — MLP w/ autograd, no frameworks (Python)
47. ☐ `ml-decision-tree-forest-rs` — decision tree + random forest (Rust)
48. ☐ `ml-linear-models-cpp` — linear/logistic regression w/ SGD (C++)
49. ☐ `ml-kmeans-clustering-py` — clustering suite (k-means/DBSCAN/hierarchical) (Python)
50. ☐ `ml-recommender-system-py` — collaborative filtering + matrix factorization (Python)
51. ☐ `ml-gradient-boosting-py` — gradient-boosted trees implementation (Python)
52. ☐ `ml-nlp-text-classifier-py` — TF-IDF + naive Bayes/SVM pipeline (Python)
53. ☐ `num-linear-algebra-rs` — matrix ops, LU/QR/SVD decompositions (Rust)
54. ☐ `num-ode-solver-cpp` — Runge-Kutta/adaptive ODE integrators (C++)
55. ☐ `num-fft-signal-py` — FFT + DSP filtering toolkit (Python)

## Batch 6 — Games (56–65)
56. ☐ `game-snake-rs` — terminal Snake with AI autoplayer (Rust)
57. ☐ `game-snake-py` — pygame Snake variant + level editor (Python)
58. ☐ `game-tetris-cpp` — terminal Tetris with scoring/levels (C++)
59. ☐ `game-2048-rs` — 2048 with solver + undo (Rust)
60. ☐ `game-conway-life-py` — Game of Life w/ patterns + RLE loader (Python)
61. ☐ `game-chess-engine-cpp` — chess engine w/ minimax + alpha-beta (C++)
62. ☐ `game-minesweeper-py` — Minesweeper w/ solver/probability hints (Python)
63. ☐ `game-roguelike-rs` — procedural dungeon roguelike (Rust)
64. ☐ `game-sudoku-solver-generator-py` — Sudoku generator + backtracking solver (Python)
65. ☐ `game-pong-ai-py` — Pong with reinforcement-learning paddle (Python)

## Batch 7 — Complex software engineering (66–80)
66. ☐ `swe-key-value-store-rs` — LSM-tree embedded KV store w/ WAL (Rust)
67. ☐ `swe-http-server-cpp` — epoll/kqueue HTTP/1.1 server (C++)
68. ☐ `swe-json-parser-rs` — spec-compliant JSON parser + serializer (Rust)
69. ☐ `swe-regex-engine-py` — NFA/DFA regex engine (Python)
70. ☐ `swe-task-queue-py` — distributed task queue w/ workers (Python)
71. ☐ `swe-mini-interpreter-rs` — Lox-like scripting language interpreter (Rust)
72. ☐ `swe-orm-lite-py` — lightweight ORM over SQLite (Python)
73. ☐ `swe-template-engine-py` — Jinja-like template engine (Python)
74. ☐ `swe-rpc-framework-rs` — length-prefixed RPC framework (Rust)
75. ☐ `swe-static-site-generator-py` — Markdown static site generator (Python)
76. ☐ `swe-bytecode-vm-cpp` — stack-based bytecode VM (C++)
77. ☐ `swe-graphql-server-py` — schema-driven GraphQL server (Python)
78. ☐ `swe-build-system-rs` — dependency-graph build tool (Rust)
79. ☐ `swe-container-runtime-py` — namespace/cgroup mini container runtime (Python)
80. ☐ `swe-distributed-kv-raft-rs` — Raft consensus KV cluster (Rust)

## Batch 8 — Large/multi-module projects (81–90)
81. ☐ `proj-markdown-ide-py` — full markdown editor TUI w/ plugins (Python)
82. ☐ `proj-data-viz-library-py` — plotting library w/ multiple backends (Python)
83. ☐ `proj-web-crawler-rs` — concurrent crawler + indexer (Rust)
84. ☐ `proj-time-series-db-rs` — embeddable time-series database (Rust)
85. ☐ `proj-spreadsheet-engine-cpp` — formula-evaluating spreadsheet engine (C++)
86. ☐ `proj-package-manager-py` — dependency resolver + package manager (Python)
87. ☐ `proj-ci-runner-py` — YAML-driven CI pipeline runner (Python)
88. ☐ `proj-genomics-workflow-py` — multi-stage genomics workflow engine (Python)
89. ☐ `proj-text-search-engine-rs` — inverted-index full-text search w/ BM25 (Rust)
90. ☐ `proj-trading-backtester-py` — event-driven strategy backtester (Python)

## Batch 9 — Mixed advanced / cross-domain (91–100)
91. ☐ `adv-image-processing-cpp` — convolution/edge/morphology image lib (C++)
92. ☐ `adv-ray-tracer-rs` — path-tracing renderer (Rust)
93. ☐ `adv-physics-engine-py` — 2D rigid-body physics engine (Python)
94. ☐ `adv-audio-synth-py` — modular audio synthesizer + sequencer (Python)
95. ☐ `adv-network-protocol-rs` — reliable protocol over UDP (Rust)
96. ☐ `adv-compiler-frontend-cpp` — lexer/parser/AST/typechecker for a C subset (C++)
97. ☐ `adv-blockchain-py` — proof-of-work blockchain + P2P mempool (Python)
98. ☐ `adv-graph-database-rs` — property graph DB w/ traversal query lang (Rust)
99. ☐ `adv-scientific-pipeline-r` — reproducible multi-stage analysis (R)
100. ☐ `adv-quantum-circuit-sim-py` — quantum circuit state-vector simulator (Python)

---
Languages covered: Rust (28), Python (52), C++ (14), R (8) — every batch mixes languages.
Each build is driven through `biorouter run`/`session` (Xiaomi MiMo) and committed to git.

## Interaction Protocol (each app is INTERACTIVE, not one-shot)

Every app goes through an **initial build** (`build_app.sh`, named resumable
session) followed by **2–4 follow-up refinement turns** (`interact.sh --resume`)
in which the Claude harness drives the BioRouter agent like a real user iterating
on their project. Each app draws its follow-ups from this menu (varied across the
100 so every interaction style is exercised):

- **A. Add a feature** — "now add <capability X> and wire it into the CLI/tests."
- **B. Change a requirement mid-stream** — "actually the input format should be Y, refactor accordingly."
- **C. Fix / debug** — "running `<cmd>` gives `<error>`; diagnose and fix it." (sometimes inject a real bug first)
- **D. Refactor / restructure** — "split module Z, extract a trait/interface, reduce duplication."
- **E. Improve output aesthetics** — "make the CLI output prettier: colors, aligned tables, a summary line."
- **F. Add tests / coverage** — "add edge-case tests for <component> and make them pass."
- **G. Add docs / examples** — "write a usage example and expand the README with a diagram."
- **H. Performance** — "benchmark and optimize the hot path; report before/after."
- **I. Productionize** — "add error handling, input validation, and a config file."
- **J. Explain & verify** — "summarize the architecture and prove the tests cover the main paths."

Each turn is committed separately so the iteration history is visible in git.
Both *functional* outcomes (did it work?) and *experiential* ones (how did the
CLI handle the request, call tools, and present results?) are scored in
`UX_BENCHMARK.md`.
