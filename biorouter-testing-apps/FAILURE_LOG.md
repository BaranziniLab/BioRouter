# BioRouter Build-100 — Failure / UX / Gotcha Log

Running log of every failure, hiccup, rough edge, and developer-experience note
observed while driving the BioRouter CLI to build the 100 apps. Consolidated into
actionable issues every 5 apps (see `ISSUES/`).

## Foundation phase
- ✅ `biorouter run --no-session -t "…"` works headless with xiaomi_mimo / mimo-v2.5-pro.
- ✅ developer extension `text_editor` (write) confirmed working.
- ⚠️ UX: no portable `timeout(1)` on macOS; long agent runs can hang a harness with
  no built-in wall-clock cap on `biorouter run`. Worked around with a perl `alarm`
  wrapper. **Candidate improvement:** a `--max-runtime`/`--max-turns` flag on `run`.
- Note: `biorouter run` prints a session banner (provider/model/workdir/knowledge)
  then streams tool calls — good for log-based failure analysis.

## Per-app observations

### App 3 — algo-bst-avl-redblack-cpp (C++) — HIGH-VALUE FAILURE
- 🐛🐛 **Agent claimed completion but left a non-building project.** It wrote 5
  headers + a `CMakeLists.txt` that references `tests`/`benchmark` targets whose
  `.cpp` sources were **never created** (`No SOURCES given to target: tests`), and
  made only **1 commit** (the harness catch-all). The build.log shows **zero**
  `cmake`/`make`/`clang++`/`ctest` invocations — i.e. MiMo **never attempted to
  compile**, despite the spec explicitly saying "build/compile and run the tests…
  fix errors until it builds." **Severity: HIGH** — silent false "done."
  - Contrast with the Rust app, which *did* build+test itself. Hypothesis: the
    agent treats C++/cmake as higher-friction and skips verification, OR ran out
    of its per-run turn budget after writing headers. Either way the user is left
    with a broken repo and no signal that it's broken.
  - **Candidate BioRouter improvements:** (a) a "self-verify before declaring
    done" guard/hook that runs the project's build/test command and refuses to
    finish on red; (b) surface remaining-turn-budget so early termination is
    visible; (c) a recipe/skill that enforces build-green for known toolchains.
  - Used as the first interactive **fix** turn (style C) — see whether the agent
    recovers when handed the exact cmake error.

### Cross-cutting — SYSTEMATIC C++ verification failure (HIGH, confirmed 2×)
- 🐛🐛 Both C++ apps (3 and 6) exhibited the **identical** failure: MiMo writes
  headers + a `CMakeLists.txt` that references benchmark/CLI/test targets whose
  `.cpp` sources are **never created**, then stops **without ever running cmake**.
  `No SOURCES given to target: <x>` on first independent build, every time. By
  contrast Rust (1,4,7) and Python (2,5) builds *do* self-compile/test.
- This is now clearly **language-specific**: the agent's "verify before done"
  discipline holds for `cargo`/`pytest` but collapses for `cmake`. Likely the
  multi-step cmake configure→build→ctest flow exceeds what MiMo reliably drives
  unprompted. **Round-3 improvement candidate:** a C++-aware build-verify
  helper/skill (auto cmake+build+run) the agent is steered to use.
- Both recovered fully when handed the exact cmake error (app3 → 47 tests; app6
  fix turn running). Reinforces: **precise failure → reliable repair.**
- ⚠️ **Deeper escalation (app 6):** even after an *explicit* instruction to create the
  missing `dp_bench`/`dp_cli` sources and run cmake, MiMo expanded the project to 37
  files / 1.3k LOC but **left the identical broken targets** and STILL shipped a
  non-building tree (ran cmake 5× but didn't resolve it). Took a **3rd, dead-simple
  "just delete those two targets and run these exact commands" turn** to converge.
  Finding: for cmake specifically, *general* repair prompts underperform; only a
  mechanical, copy-pasteable instruction reliably lands. This is the clearest
  evidence yet for a **deterministic C++ build-verify helper** over prompt-only repair.

### Cross-cutting — MiMo rate limit + NO auto-retry (HIGH, round-2 improvement target)
- 🐛🐛 Running **3 concurrent `biorouter run` sessions** reliably triggers
  `Rate limit exceeded: Too many requests` from the MiMo API. Apps 6 (C++, died at
  6 files/122 LOC) and 8 (Python, died at 2 files/161 LOC) were both truncated
  mid-build.
- 🐛 **BioRouter does not auto-retry on rate-limit/429** — it surfaces "Please retry
  if you think this is a transient or recoverable error" and **aborts the whole
  run**, leaving a half-built repo. For a known-transient 429 this is the wrong
  default; a real user loses all in-flight progress. **Candidate round-2
  improvement:** exponential-backoff auto-retry on rate-limit / 5xx in the provider
  request path (with a cap + jitter), so transient throttling doesn't kill a run.
- ✅ **Mitigations:** (a) drop concurrency to ≤2 builds; (b) named sessions make
  recovery trivial — `run --resume` continues the truncated build from where it
  stopped. Both 6 and 8 resumed to completion (79 / 98 tests). Nice test of
  session-resume robustness under failure.
- 🔬 **Precise root cause (code-level):** retry IS wired — `utils.rs` maps HTTP 429 →
  `ProviderError::RateLimitExceeded`, `retry.rs::should_retry` retries it, and
  `xiaomi_mimo.rs` wraps both `post` and `stream` in `with_retry`. BUT
  `DEFAULT_MAX_RETRIES = 3` with 1s→2s→4s backoff = only ~7s of total retrying.
  Under ≥3 concurrent sessions the throttle outlasts that, retries exhaust, and
  `agents/agent.rs:1672` surfaces it as a **turn-ending** "Ran into this error…
  Please retry…" message. **Round-2 fix (scoped):** give `RateLimitExceeded` a
  deeper, dedicated retry budget (it's always transient) — e.g. ~6–8 attempts with
  the existing 30s cap — instead of the generic 3. Low-risk, high-value.

### App 4 — algo-graph-toolkit-rs (Rust) — shipped with RED tests
- 🐛 **Declared done with 3 failing tests** (68 passed, 3 failed): Kosaraju SCC on a
  complex graph, Prim on a disconnected graph (should yield a spanning forest), and
  Floyd-Warshall on a disconnected graph. Unlike the C++ app, MiMo **did** run
  `cargo test` (6×) during the build — but tolerated red and finished anyway. So the
  failure mode isn't "never tested," it's "tested, saw red, shipped regardless."
- 🐛 Only **1 commit** again (catch-all), despite "make ≥3 logical commits." Git
  discipline is inconsistent across runs (apps 1,2 committed well; 3,4 didn't).
- Driving an interactive fix turn (style C) with the exact failures.

### App 5 — algo-string-matching-py (Python) — passes-for-agent, broken-for-user
- 🐛 **Clean-checkout `pytest` fails collection** with `ModuleNotFoundError: No module
  named 'strmatch'`. The agent used a **src-layout** (`src/strmatch/`) but never added
  `pythonpath`/editable-install config, so tests only pass if you `pip install -e .`
  first (which I confirmed → **199 tests pass**). The committed repo isn't runnable
  out-of-the-box. **UX impact: high** — a user cloning the repo and running the
  documented `pytest` hits an immediate error. Classic gotcha the agent should know.
  Fix turn launched (add `[tool.pytest.ini_options] pythonpath=["src"]`).

### Cross-cutting — session resume
- 🐛 **`run --resume --name X` is a hard error when session X doesn't exist**
  (`Error: No session found with name 'algo-pathfinding-rs'`, rc=1). A real user
  who fat-fingers a session name, or whose session was created with `--no-session`,
  gets a dead end. **Candidate improvement:** either (a) fall back to creating the
  session with a warning, or (b) print `biorouter session list`-style hints of
  existing names. Worked around in `interact.sh` with a resume→seed fallback.
- 🐛 **`--no-session` builds are silently non-resumable** — there is no warning at
  build time that you won't be able to iterate on that session later. The two are
  easy to conflate. Documenting so users know to use `--name` when they intend to
  iterate.

### App 1 — algo-pathfinding-rs (Rust) — calibration
- 🐛 **Harness bug (mine, fixed):** spec file passed as a relative path was `cat`-ed
  *after* `cd` into the app dir, so the detailed spec never reached the agent — it
  built a reasonable graph/pathfinding lib purely from the folder name. Fixed by
  resolving the spec to an absolute path before `cd`. Lesson logged because it
  mirrors a real user gotcha: **BioRouter happily runs with a thin prompt and
  improvises** rather than flagging that the instruction looked truncated/empty.
- 🔁 **Interactivity gap (methodology):** initial build used `--no-session`, which
  is NOT resumable — so follow-up refinement turns can't continue the conversation.
  Switched the harness to **named sessions** (`run --name <app>` + `--resume`) so
  the Claude harness can iterate with retained context, mimicking real use.
- ✅ Good: agent immediately used `todo_write` with a sensible 10-step plan, then
  `cargo init` via shell — clean, legible tool sequencing in the log.
- UX/clarity (early read): banner (provider/model/session/workdir/knowledge) is
  clear; tool calls render with a `▸ tool call <tool> · <ext>` header — easy to
  scan. Full scoring pending build completion.
- 🐛 **BioRouter/MiMo bug — `-32602: failed to deserialize parameters: missing
  field 'path'`** (1× of ~15 `text_editor` calls). MiMo intermittently emits a
  `str_replace` call without the required `path` field; the developer extension
  rejects it with a JSON-RPC invalid-params error. Agent self-recovered (retried),
  but it wastes a turn. **Severity: medium** (self-healing, but a stricter/more
  forgiving param coercion — or echoing the offending args back to the model —
  would help). Candidate fix: in the text_editor handler, return a *descriptive*
  error naming the missing field + the other params received, so the model can
  correct in one step instead of re-deriving the whole call.
- 🎨 **Cosmetic/clarity — over-aggressive path abbreviation** in tool-call headers:
  edits show `path: ~/D/b/a/s/algorithms/bfs.rs`. Collapsing `Desktop→D`,
  `src→s` saves width but makes it hard to tell which file/dir is touched at a
  glance. Suggest abbreviating only the *prefix* up to the working dir and showing
  the in-project path (`…/algo-pathfinding-rs/src/algorithms/bfs.rs`) in full.
- ⚠️ **Spec-vs-scaffold mismatch:** spec asked for a *CLI binary*, but MiMo ran
  `cargo init --lib`, yielding a library-only crate; the "CLI" ended up as library
  functions with no `src/main.rs`/`[[bin]]`. The agent doesn't reconcile "build a
  CLI" with its own scaffolding choice. Caught it during refinement; good candidate
  for a follow-up "make it a real runnable binary" interaction turn.
- ✅ **Interactive resume→seed fallback works:** after the `--resume` hard error,
  the harness seeded a fresh named session; the agent inspected existing files and
  correctly extended them (compare subcommand + ANSI colors) with tests still
  green and coherent incremental commits. Iteration fidelity good despite no prior
  chat history — MiMo reorients from the codebase well.

### Cross-cutting — Keychain/keyring transient failure (dev-workflow gotcha)
- 🐛 Apps 14 & 15 failed instantly with `Configuration value not found:
  XIAOMI_MIMO_API_KEY` (keyring read). Root: macOS **locks the keychain** after
  inactivity, and rebuilding the CLI mid-loop (`cargo build`, ad-hoc signature)
  can also invalidate the "Always Allow" ACL. A subsequent read then fails with no
  GUI prompt to answer in headless mode → the whole build aborts at turn 0.
- ✅ It recovered on its own once the keychain was accessible again (smoke test
  passed). **Lessons:** (a) after any CLI rebuild, re-sign with the stable
  Developer ID (`just sign-dev-binaries debug` / `just copy-binary`) so the grant
  survives — CLAUDE.md documents this; (b) a headless keyring-read failure should
  ideally degrade more gracefully (clear one-line cause + which env var to set),
  and (c) it argues for `XIAOMI_MIMO_API_KEY` via env for long unattended runs.

### App 17 — premature stream stop (reliability)
- 🐛 Build ended mid-sentence ("Now let me create the core PDB parser module:") with
  only the package scaffold written (4 files, ~9 LOC), rc=0, NO error / rate-limit /
  max-turns message. Looks like a clean stream truncation that ended the turn as if
  complete. Indistinguishable from success without inspecting content — reinforces
  the C2 "no done-vs-stopped signal" finding. Recovered via --resume.
- ✅ C1 fix confirmed live: tool-call paths now render the in-project tail in full
  (`path: ~/…/bio-protein-structure-py/src/bio_protein_structure/__init__.py`).

### App 17 — interactive fix did NOT fully converge (test suite)
- 🐛 After the initial build (premature stop), a resume completed the 1775-LOC
  protein modules, but TWO explicit "create the pytest suite" turns produced only
  tests/__init__.py — never actual test_*.py with assertions. pytest reports
  "no tests collected". A rare case where the precise-failure→repair pattern did
  NOT land: the agent kept acknowledging the request but not writing tests.
  Accepted as partial (code complete, untested) to avoid starving other apps.
  Hypothesis: something about this app's prompt/context made MiMo treat "tests
  exist" as satisfied by the package __init__ + the pyproject testpaths config.

### Cross-cutting — CLI binary disappeared mid-loop (environmental)
- 🐛 Apps 19 & 20 failed with empty logs / 0 files: `target/debug/biorouter` (the
  symlink target for ~/.local/bin/biorouter) was deleted between app18 and app19
  — most likely a concurrent `cargo clean`/rebuild in the BioRouter workspace.
  build_app.sh's `biorouter run` hit a dangling symlink and produced nothing.
- ✅ Recovered: rebuilt + re-signed the binary, re-ran the two apps. Reinforces
  that long unattended loops should pin a stable, installed CLI (or set
  XIAOMI_MIMO_API_KEY via env + a copied binary) rather than a dev-target symlink
  that shared workspace activity can invalidate.

### App 20 — CLI integration tests assume install (variant of src-layout gotcha)
- 🐛 3 of 97 tests fail with `assert 32512 == 0` (32512 = exit 127, command not
  found): the CLI integration tests shell out to the CLI entry-point as a
  subprocess, which isn't on PATH in a clean venv (no `pip install -e .`). The 94
  algorithm/unit tests pass. The agent writes CLI tests that aren't runnable from a
  clean checkout — the CLI analog of the app-5 src-layout issue. One fix turn did
  not resolve it (it should invoke `python -m <pkg>` with pythonpath, or call the
  CLI function directly, instead of a bare command name). Accepted at 94/97.

### Cross-cutting — premature stream stop RECURRING (apps 17, 21)
- 🐛 2nd occurrence: app21 (FHIR) stopped mid-sentence ("Now let me create the
  synthetic FHIR bundle generator for tests:") after 10 tool calls, rc=0, no error
  — same signature as app17. Both stops happen at the **transition from
  implementing modules to writing the test suite**, suggesting either a stream
  truncation or the model emitting a soft stop before the (large) test-writing
  step. Both resumable. Watch frequency; if it keeps clustering at the
  code→tests boundary it may be a MiMo response-length/stop-token issue worth a
  provider-side mitigation (e.g. continue-on-truncation for non-final responses).

### ESCALATION — premature stream stop is now the dominant batch failure (apps 17, 21, 23 — HIGH)
- 3rd occurrence in the med/bio batch: app23 wrote all 7 modules then cut off at
  "Now let me create the sample data files...". Pattern is consistent: rc=0, no
  error, stops at a transition to a *new large block* (tests or data files).
- Frequency (~3 of last 7 builds) makes this the #1 throughput drag of the batch.
- **Strong round-5 improvement candidate:** provider-side continue-on-truncation —
  if a streamed assistant turn ends without a stop reason indicating natural
  completion (e.g. length/truncation, or ends mid-plan with pending tool intent),
  automatically continue the turn instead of returning control. Mirrors how the
  retry budget handles transient 429s. Would remove a whole class of resume turns.

### App 23 — reinforces "scaffolding but no test functions" (cf app 17)
- After a resume (modules+data complete) and an EXPLICIT file-by-file test request
  (test_mapping.py, test_hierarchy.py, ...), the agent still produced no test_*.py
  (the explicit turn also errored out, exit 1, cause unclear — binary OK). 2nd app
  (with app17) where MiMo reliably writes everything EXCEPT the test suite. Pattern:
  it treats tests/conftest.py + pyproject testpaths as "tests handled". Accepted as
  partial (code+data complete, untested).

### Premature stop — 4th occurrence (app 26) + harness mitigation
- app26 cut off at "Now let me create comprehensive tests. First, the validation
  tests:" — identical signature. 4 of last ~10 builds. The truncation lands on the
  big end-of-build test-writing block.
- ZERO-RISK harness mitigation applied: build_app.sh now instructs "write tests
  INCREMENTALLY ... do NOT defer the entire test suite to the end", to shrink the
  large code→tests transition where the stream truncates. (The provider-side
  continue-on-truncation remains the proper fix; the Plan-B Stop hook is the safe
  in-product mitigation.)
