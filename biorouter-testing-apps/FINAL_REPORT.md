# BioRouter CLI — Comprehensive QA Report (Build-N Apps via Xiaomi MiMo)

**Scope of this run:** drive the **BioRouter CLI** (Xiaomi MiMo `mimo-v2.5-pro`,
developer + todo extensions only) to interactively build real, multi-file software
projects — each in its own git repo — as an end-to-end test of the agent system.
Paused at the user's request after app 11 of a planned 100.

> Harness split (confirmed with user): **BioRouter authors 100% of the app code
> and all app bug-fixes** (`biorouter run` / `--resume`); the **Claude Code harness
> only orchestrates, independently verifies (cargo/pytest/cmake), and writes the
> next instruction**. The **two improvements to BioRouter's own source** were made
> by Claude Code directly (the agent doesn't modify its own core).

## 1. What was built (12 attempted, ~11 fully green)

| # | App | Lang | Files | LOC | Tests (independently verified) | Turns |
|---|-----|------|-------|-----|-------------------------------|-------|
| 1 | pathfinding | Rust | 17 | 1.6k | **54 pass** | build+refine |
| 2 | sorting-visualizer | Python | 23 | 3.0k | **184 pass** | build+refine |
| 3 | bst-avl-redblack | C++ | 13 | 2.1k | **47 pass** | build+fix |
| 4 | graph-toolkit | Rust | 17 | 3.8k | **92 pass** | build+2 fix |
| 5 | string-matching | Python | 23 | 1.7k | **199 pass** | build+fix |
| 6 | dynamic-programming | C++ | 36 | 1.4k | **79 pass** | build+resume+3 fix |
| 7 | hash-table | Rust | 13 | 2.0k | **94 pass** | 1-shot |
| 8 | compression (LZ77+Huffman) | Python | 16 | 1.6k | **98 pass** | build+resume |
| 9 | bignum (arbitrary precision) | C++ | 22 | 2.1k | 74/76 (2 numeric edge cases) | build+fix |
| 10 | bloom/cuckoo filters | Rust | 11 | 1.6k | **50 pass** | 1-shot |
| 11 | seq-alignment | Python | 30 | 2.3k | **110 pass** | build+fix |
| 12 | fasta/fastq-toolkit | Rust | 16 | 1.7k | **68 pass** | 1-shot |

**~1,149 tests passing** across **Rust, Python, C++** (R was app 16, not reached).
Every repo is a real git repository with tracked, logically-structured commits.

## 2. Headline findings

### Functional (root causes pinned down)
- **F1 / G2 — Systematic C++ / cmake verification failure (HIGH, 3×).** C++ apps
  (3, 6, 9) write a `CMakeLists.txt` referencing benchmark/CLI/test targets whose
  sources don't exist and **never run cmake**. Rust/Python apps self-verify
  reliably (`cargo test` / `pytest` run repeatedly); cmake does not. C++ apps cost
  **4–5 interactive turns** each vs **1** for Rust/Python. Even *explicit* "create
  these files and run cmake" prompts underperform — only mechanical, copy-pasteable
  instructions converge.
- **G1 — Transient rate-limit (429) aborts the whole run (HIGH).** ≥3 concurrent
  sessions trip MiMo's limit; 429 → `RateLimitExceeded` *is* retried, but
  `DEFAULT_MAX_RETRIES=3` (~7s) is exhausted under sustained throttling, then
  `agents/agent.rs:1672` surfaces a turn-ending error and truncates the build
  (apps 6, 8). **→ Fixed (round 2).**
- **F3 — `text_editor` tool-call malformation `-32602` (MEDIUM).** MiMo
  intermittently emits the param key as `file_path` instead of `path`; serde
  rejected it pre-handler with an opaque error, costing a turn. **→ Fixed (round 1).**
- **F2 — "Works in my session, broken on clean checkout" family.** missing commits
  (apps 3,4 made only 1); Python **src-layout** with no `pythonpath` → fresh
  `pytest` fails collection (app 5); Rust **shipped 3 red tests** (app 4) — i.e.
  ran tests, saw red, finished anyway. The agent optimizes for its transient
  session, not a reproducible repo.
- **F4 — `--resume` on a missing session is a hard error** (`No session found with
  name X`, rc=1) instead of a graceful fallback. `--no-session` builds are silently
  non-resumable. *(Documented; CLI-only fix recommended — see §4.)*
- **F5 — spec/scaffold mismatch:** "build a CLI" + `cargo init --lib` → library-only
  crate (app 1).

### Cosmetic / clarity / UX
- **C1 — Over-aggressive path abbreviation** in tool-call headers
  (`path: ~/D/b/a/s/algorithms/bfs.rs`) — hard to tell which file is edited.
- **C2 — No remaining-turn / budget signal** — can't distinguish "finished" from
  "ran out / rate-limited" without reading the log tail; the C++ early-stop and the
  429 truncation both looked like normal completion.
- **C3 — `--no-session` vs `--name` is a silent foot-gun** (iteration consequences
  invisible at build time).
- Positives: clear startup banner (provider/model/session/workdir/knowledge);
  legible `▸ tool call <tool> · <ext>` headers; **excellent iterative repair** —
  every defect recovered when handed a precise failure; **robust session resume**
  after mid-build rate-limit cutoffs.

### The strongest signal
**Precise failure → reliable repair.** Every broken state (no-compile C++, red
tests, broken cmake, src-layout, rate-limit truncation) was fixed through
`--resume` fix turns. BioRouter is highly effective at *interactive iteration*;
its weakness is **unprompted self-verification**, which is language-dependent
(good for cargo/pytest, absent for cmake).

## 3. Improvements shipped to BioRouter (apply to CLI **and** GUI)

Both live in **shared backend crates** that `biorouter-cli` **and** the GUI's
`biorouterd` (`biorouter-server`) compile in — so both surfaces benefit; no GUI
(TypeScript) change is applicable. Branch: `improve/ratelimit-retry-budget`
(stacks both commits).

| Round | Fix | File | Test |
|-------|-----|------|------|
| 1 | `#[serde(alias = "file_path")]` on `text_editor.path` — kills the `-32602` wasted-turn class | `biorouter-mcp/.../rmcp_developer.rs` | `test_text_editor_params_accepts_file_path_alias` ✓ |
| 2 | `RATE_LIMIT_MAX_RETRIES=8` + `effective_max_retries()` — transient 429s get ~2 min of retry vs ~7s; generic errors unchanged | `biorouter/src/providers/retry.rs` | 2 unit tests ✓ |

Each was committed with a detailed message, unit-tested, and the CLI was rebuilt
so subsequent app builds ran on the improved agent (the "make the agent better
every 5 tasks" loop).

## 4. Recommended next improvements (precise, queued)
1. **C++ build-verify helper (round-3 target, highest ROI):** a bundled
   skill/helper that auto-runs `cmake -S . -B build && cmake --build build && ctest`
   (or the test binary) and that the agent is steered to invoke before declaring
   done — directly kills the most expensive recurring failure (C++ 4–5 turns → 1).
2. **General "don't finish on red" guard (F1):** a Stop-hook that runs the detected
   project's build/test and blocks/ warns on failure. Backend → benefits CLI + GUI.
3. **`--resume` graceful fallback (F4):** when a named session is absent, warn and
   start fresh (or list candidates) instead of `rc=1`. CLI-only (`cli.rs:356/400`);
   note it also touches a lookup used where a hard error is correct, so scope to the
   resume call-site.
4. **Cosmetic (C1/C2):** show in-project paths in full; surface a turn/budget
   indicator so "done" vs "ran out" is unambiguous.

## 5. Methodology artifacts (this folder)
`CHECKLIST.md` (100-app plan + interaction protocol) · `PROGRESS.md` ·
`FAILURE_LOG.md` (running findings) · `UX_BENCHMARK.md` (1–5 scoring per app) ·
`ISSUES/round-1-report.md`, `round-2-report.md` · `IMPROVEMENTS.md` ·
`build_app.sh` / `interact.sh` (the harness) · `specs/` (per-app specs).
