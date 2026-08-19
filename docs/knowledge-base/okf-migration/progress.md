# OKF migration — progress

> **What this is.** The live tracker for the OKF/BioOKF migration. One row per stage; updated as
> each lands. A stage is DONE only when its gate in [`stages.md`](stages.md) has actually been run.
> **Status:** Current (in progress).
> **Audience:** Contributors working on the Knowledge subsystem.

**Branch:** `feature/okf-knowledge` · **Worktree:** `~/Desktop/BioRouter-okf`

| Stage | What | State | Gate run? |
| --- | --- | --- | --- |
| — | Research (OKF v0.2, LLM Wiki, BioOKF v0.5, BioRouter audit) | DONE | n/a |
| — | Design records DR-1…DR-22 | DONE | committee run; REQUEST_CHANGES folded in |
| 0 | `okf` core module (Rust) | **DONE** — 6 files, 2,574 lines, 97 tests | **PASSED** |
| 1 | BioOKF profile module (28 types / 35 predicates) | **DONE** — 6 files, 3,576 lines, 66 tests | **PASSED** |
| 1.5 | Seams (7 behaviour-preserving mitigations) | **DONE** — 473 tests | **PASSED** (adversarial, mutation-tested) |
| 2 | Graph derivation: typed nodes + edges, three link forms | IN PROGRESS | — |
| 3 | Store, manifest, service, legacy migration | IN PROGRESS | — |
| 4 | MCP tool surface + skills | NOT STARTED | — |
| 5 | Sub-agent macros (profile-aware) | NOT STARTED | — |
| 6 | HTTP routes + OpenAPI + TS client | NOT STARTED | — |
| 7 | Desktop UI design pass (spec + Slice A) | Spec revised after committee; Slice A IN PROGRESS | — |
| 8 | Verification in the real app | NOT STARTED | — |

## Measured baselines

Captured before any change, so a later "pre + N" assertion has something true to stand on.

| Suite | Count | Measured |
| --- | --- | --- |
| `cargo test -p biorouter-mcp --lib knowledge::` (baseline, before any OKF work) | 283 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stages 0+1) | **446** = 283 + 97 + 66 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 1.5) | **473** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::okf` | 97 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::biookf` | 66 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib` (whole crate lib) | 1217 passed, 7 pre-existing ignores | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_routes` | 46 | 2026-08-19 |
| Desktop `it()` under `components/knowledge` | 94 in 14 files | 2026-08-19 |

## Log

### 2026-08-19
- Worktree `~/Desktop/BioRouter-okf` created on `feature/okf-knowledge` off `main` (ea34ef57).
- 16-agent research sweep completed: OKF v0.2 spec digest, LLM Wiki lineage, BioOKF v0.5
  (layout, vocabulary, provenance, lint, core crate, MCP surface, Studio renderer), and a
  six-part audit of BioRouter's own knowledge subsystem.
- Design records DR-1…DR-11 written.

### 2026-08-19 (later)
- **Stage 0 landed.** `knowledge/okf/`: `frontmatter.rs` (the line-based split, fallible by
  design), `model.rs` (`ConceptDoc` with `#[serde(flatten)] extra` so unknown producer keys
  round-trip), `trust.rs` (bare-`verified` normalization, derived trust tier, `is_stale`),
  `links.rs` (all three link grammars behind one entry point, plus footnote refs),
  `conformance.rs` (the three producer rules, honouring the five consumer tolerances), and nine
  real fixtures including BioOKF's own Tocilizumab worked example. 97 tests.
- **Stage 1 landed.** `knowledge/biookf/`: both vocabularies declared by a macro over a single
  table each, so the enum, `ALL`, `as_str`, `parse` and the family functions cannot drift; the 11
  negatives are derived from a `negatable()` predicate rather than listed; domain/range with
  `not_<X>` inheriting its base; the deprecated alias table; and the lint rule set. 66 tests.
- **Gate PASSED**, adversarially: the gate agent noticed the clippy command it was handed returned
  in 0.77s — a cache hit that would have replayed clean over dirty code — and forced a real
  re-check by touching all 13 new files, then ran `./scripts/clippy-lint.sh` workspace-wide as the
  authoritative check. `clippy-baselines/` untouched, so no long function was smuggled in behind a
  new baseline entry.

### 2026-08-19 (Stage 1.5 + reviews)
- **Stage 1.5 landed and passed its gate.** Seven behaviour-preserving seams; `knowledge::`
  446 → 473. `openapi.json` regenerates byte-identical, so the published contract is untouched.
- **The gate was adversarial and found two real defects**, both since fixed: the new
  link-equivalence test was half inert (proved by mutating lint's resolver — the test stayed
  green), and the `valid_page` fixture sweep had missed nine call sites. The test now fails under
  each of five separate mutations, including one in the *shared* parser that no consumer-site
  mutation can reach.
- **A design error caught by the same gate:** DR-6 said bump `schema_version` to 2, but S-g had
  already taken 2 for the cross-reference-rules generation that every base on disk carries.
  Bumping OKF to 2 would have declared every existing base already-migrated and skipped its
  migration silently. The OKF schema is generation **3**.
- **The Knowledge UI design pass ran and its committee returned 10 blocking + 19 major findings.**
  The palette failed colour-vision analysis it had never been given: under protanopia in light
  mode two of the 28 type colours measure ΔE00 0.35 — the same colour — and the proposed guard
  tested only normal trichromacy, so it would have shipped green. Node type also had no redundant
  channel, with ~82% of nodes unlabelled at the default zoom. Fixed with seven family *shapes*
  plus CVD simulation in the guard.
