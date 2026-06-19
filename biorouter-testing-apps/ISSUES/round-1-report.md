# BioRouter QA — Round 1 Issues Report (apps 1–5)

Consolidated from driving the BioRouter CLI (Xiaomi MiMo / `mimo-v2.5-pro`,
developer + todo) to interactively build + refine apps 1–5. Each app is a real
multi-file project (1.7k–3.8k LOC) in its own git repo.

## Outcome summary

| # | App | Lang | LOC | Tests (independently verified) | Path to green |
|---|-----|------|-----|-------------------------------|---------------|
| 1 | pathfinding | Rust | ~1.6k | 54 pass | one-shot ✓ + refined |
| 2 | sorting-visualizer | Python | ~3.0k | 184 pass | one-shot ✓ + refined (CLI) |
| 3 | bst-avl-redblack | C++ | ~2.1k | 47 pass | **broken→fixed** (1 interactive turn) |
| 4 | graph-toolkit | Rust | ~3.8k | 70/71 → fixing last | **3 red→fixed** (2 turns) |
| 5 | string-matching | Python | ~1.7k | 199 pass | **clean-checkout broken→fixed** |

All five reached working, tested states. Every defect was recoverable through
interactive fix turns — the headline positive.

## Functional findings

**F1 — Agent declares "done" on a non-building / failing project (HIGH).**
- C++ (app 3): wrote headers + a CMakeLists referencing nonexistent sources;
  **never invoked the compiler** (0 cmake/clang calls); 1 commit. Broken on arrival.
- Rust (app 4): **ran `cargo test` 6× but shipped with 3 red tests** — saw red,
  finished anyway.
- Root issue: no "build/test must be green before finishing" guard. Verification
  discipline is also **language-dependent** (rigorous for Python/Rust compilation,
  absent for C++/cmake).

**F2 — "Works in my session, broken on clean checkout" (HIGH).**
- Python (app 5): src-layout package, no `pythonpath`/editable config → fresh
  `pytest` fails collection (`ModuleNotFoundError`). Tests are fine *after* `pip
  install -e .` (199 pass), but the committed repo isn't runnable as documented.
- Inconsistent **git commits**: apps 1,2 made clean multi-commit history; apps
  3,4 made only the harness catch-all commit despite "make ≥3 commits."

**F3 — Tool-call parameter malformation `-32602` (MEDIUM).**
- MiMo intermittently emits a `text_editor`/`str_replace` call **missing the
  required `path` field**, which serde rejects pre-handler with an opaque
  `-32602: failed to deserialize parameters: missing field 'path'`. Agent
  self-recovers but burns a turn. The error gives the model no constructive hint.

**F4 — `--resume` on a missing/`--no-session` session is a hard error (MEDIUM).**
- `run --resume --name X` exits 1 (`No session found with name X`) instead of
  offering to start fresh or listing existing names. `--no-session` builds are
  silently non-resumable with no build-time warning.

**F5 — Spec/scaffold mismatch (LOW).**
- "Build a CLI" + `cargo init --lib` → library-only crate, no binary. The agent
  doesn't reconcile stated intent with its own scaffolding choice.

**F6 — Partial interactive fix (LOW/INFO).**
- App 4's first fix turn resolved 2 of 3 failing tests but left one (a genuine
  Floyd-Warshall node-id-vs-matrix-index bug); needed a second, more specific turn.
  Precision of the failure description strongly correlates with fix success.

## Cosmetic / clarity / UX findings

**C1 — Over-aggressive path abbreviation** in tool-call headers
(`path: ~/D/b/a/s/algorithms/bfs.rs`). Saves width but obscures which file is
edited. Suggest showing the in-project path in full.

**C2 — No remaining-turn / budget signal.** When the agent stops early (app 3),
there's no indication whether it *finished* or *ran out of turns*. Surfacing a
budget/turn indicator would disambiguate "done" from "gave up."

**C3 — `--no-session` vs `--name` is an easy, silent foot-gun** (see F4); the two
modes aren't distinguished at a glance and the iteration consequence is invisible.

Positives worth recording: clear startup banner (provider/model/session/workdir);
legible `▸ tool call <tool> · <ext>` headers; **excellent iterative-repair
ability** — every defect above was fixed by a targeted follow-up turn with retained
or reconstructed context.

## Improvement applied this round
See `IMPROVEMENTS.md` — round 1 implements a fix for **F3** (descriptive
missing-`path` error so the model self-corrects in one step) on a branch in the
BioRouter repo. F1 (build-verify guard) is the highest-value item and is queued as
a larger change for a later round.
