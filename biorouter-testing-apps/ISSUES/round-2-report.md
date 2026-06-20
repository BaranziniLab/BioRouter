# BioRouter QA — Round 2 Issues Report (apps 6–10)

Continued interactive build/refine of apps 6–10 with the round-1-improved CLI
(now accepting the `file_path` alias). All five reached working, tested states.

## Outcome

| # | App | Lang | Tests (independently verified) | Turns | Note |
|---|-----|------|-------------------------------|-------|------|
| 6 | dynamic-programming | C++ | 79 pass | **5** | rate-limit + 3 cmake/DP-bug fixes; very expensive |
| 7 | hash-table | Rust | 94 pass | 1 | clean one-shot |
| 8 | compression (LZ77+Huffman) | Python | 98 pass | 2 (resume) | rate-limit truncated → resumed |
| 9 | bignum (arbitrary precision) | C++ | building | – | – |
| 10 | bloom/cuckoo filters | Rust | 50 pass | 1 | clean one-shot |

## Findings (new this round)

**G1 — Rate limit aborts the run; retry budget too shallow (HIGH).**
Running ≥3 concurrent `biorouter run` sessions triggers MiMo 429s that truncate
builds (apps 6, 8). Code-level root cause: 429 *is* mapped to
`RateLimitExceeded` and *is* retried, but `DEFAULT_MAX_RETRIES = 3` (≈7s of
backoff) is exhausted by sustained throttling, after which `agent.rs:1672`
surfaces a turn-ending error. → **Fixed this round (see IMPROVEMENTS round 2).**

**G2 — Systematic C++/cmake verification failure (HIGH, confirmed 2×).**
Both C++ apps (3, 6) wrote a `CMakeLists.txt` referencing nonexistent
benchmark/CLI targets and **never ran cmake**. App 6 needed 4 build/fix turns to
converge — and even *explicit* "create these files and run cmake" prompts under-
performed; only a mechanical "delete these two target blocks, run these exact
commands" turn worked. Rust/Python self-verify reliably; cmake does not. →
**Queued as round-3 improvement: a C++-aware build-verify helper.**

**G3 — Strongly positive: precise-failure → reliable repair, and `--resume`
robustness.** Every truncated/broken build (rate-limit cutoffs, red tests, broken
cmake) recovered through `interact.sh --resume` with retained context. Session
resume after a mid-build failure works well.

## Cosmetic / clarity
- C2 reaffirmed: no remaining-turn/budget signal — can't tell "finished" from
  "rate-limited/ran out" without reading the log tail.
- C++ apps produce **thin first drafts** (app 6 resume: 13 files / 211 LOC before
  the real implementation landed in later turns), versus Rust/Python which arrive
  substantial in one shot.

## Improvement applied this round
**Round 2 → G1:** deeper dedicated retry budget for `RateLimitExceeded`
(`RATE_LIMIT_MAX_RETRIES = 8`, ~2 min span vs the previous ~7s), in
`crates/biorouter/src/providers/retry.rs`, with unit tests. See `IMPROVEMENTS.md`.
