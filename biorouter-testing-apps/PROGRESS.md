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
