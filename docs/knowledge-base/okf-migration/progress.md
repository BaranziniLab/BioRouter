# OKF migration — progress

> **What this is.** The live tracker for the OKF/BioOKF migration. One row per stage; a stage is
> DONE only when its gate in [`stages.md`](stages.md) has actually been run.
> **Status:** Current.
> **Audience:** Contributors working on the Knowledge subsystem.

**Branch:** `feature/okf-knowledge` · **Worktree:** `~/Desktop/BioRouter-okf`

## Stages

| Stage | What | State | Gate |
| --- | --- | --- | --- |
| — | Research (OKF v0.2, LLM Wiki, BioOKF v0.5, BioRouter audit) | DONE | n/a |
| — | Design records DR-1…DR-28 | DONE | 3-reviewer committee, all findings folded in |
| 0 | `okf` core module | DONE — 2,574 lines, 97 tests | **PASSED** |
| 1 | BioOKF profile — 28 types / 24+11 predicates | DONE — 3,576 lines, 66 tests | **PASSED** |
| 1.5 | Seams (7 behaviour-preserving mitigations) | DONE | **PASSED** — mutation-tested |
| 2 | Typed graph derivation | DONE | FAILED then fixed (DR-26…28) |
| 3 | Store, manifest, format, OKF scaffolds | DONE | FAILED then fixed |
| 3.5 | Wire-type corrections the gate forced | DONE | — |
| 4 | MCP tools + skills | DONE | **PASSED** |
| 5 | Profile-aware macros, source-node materialization | DONE | **PASSED** |
| 6 | HTTP routes, OpenAPI, generated TS client | DONE | **PASSED** |
| 7 | Desktop UI design pass (spec + Slice A + Slice B) | DONE | **PASSED** — browser-verified |
| 8 | Verification in the real app | DONE — 11 of 12 scenarios | **PASSED** |

## Measured counts

Measured in this worktree, never estimated. Re-measure rather than trusting these.

| Suite | Baseline | Now |
| --- | --- | --- |
| `cargo test -p biorouter-mcp --lib knowledge::` | 283 | **587** |
| `cargo test -p biorouter-mcp` (whole crate) | — | **1425** |
| `cargo test -p biorouter-server --test knowledge_routes` | 46 | **55** |
| Desktop `npm run test:run` | 2698 | **2841** in 289 files |
| Contrast assertions in `lint:check` | 330 | **332** |

## Verification in the real app

Run against a fully sandboxed config: 27,216 files fingerprinted before and after with an identical
manifest SHA, both sandbox seams (`XDG_CONFIG_HOME` **and** `BIOROUTER_PATH_ROOT`) confirmed present
on every spawned process via `ps eww`, and `lsof` showing zero open files under the real config.

| Scenario | Result |
| --- | --- |
| Knowledge section opens and renders | PASS |
| Create an OKF base through the UI | PASS |
| Create a BioOKF base through the UI | PASS |
| Ingest into OKF through the ingest panel (live SSE) | PASS |
| Ingest biomedical into BioOKF through the panel | PASS — 11 typed pages, 17 typed edges |
| BioOKF graph: colour by type, shape by family, typed edges, legend | PASS |
| Facet rail filtering | PASS |
| **OKF *and legacy* graphs render** | **PASS** — the back-compat property |
| Node inspector | PASS (after the fix in `eee061ce`) |
| Lint from the UI | was BLOCKED — no affordance existed; added in `eee061ce` |
| Theme family + light/dark with the graph open | PASS — follows live, no reload |
| `.brkb` export and re-import | PASS — lossless, format fields preserved |

A legacy base renders 10 pages / 57 links with `node_type: null` throughout, its legacy `kind`
vocabulary intact, the legend suppressed, and **0 lint diagnostics** — a legacy base is not scolded
for not being OKF, matching DR-26.

## What is deliberately not in this release

Each keeps its decision record so the reasoning is not lost.

- **Migrating an existing base to OKF** (DR-26). The automatic schema ladder stops one generation
  below OKF and `kb_migrate_format` is not built, because rewriting every page of an existing base is
  the fifth privacy write choke point DR-17 identified — one that in its eager form has no caller
  identity at all. New bases are OKF or BioOKF; existing bases keep working untouched.
- **`br_page_id` stamping** (DR-3, deferred by DR-22). The resolution ladder ships; the stamp does
  not, because it is the only change that would rewrite frontmatter on existing pages.
- **A plain-OKF bundle export** (DR-21). It would be an ungated second transfer door bypassing the
  `.brkb` provenance sidecar. `.brkb` remains the only door — and now contains a conformant bundle.
- **Attested Computations** (OKF §10). A good fit for reproducible analysis, entirely separable.
- **A renderer swap** (DR-9). Gated on measured graph sizes; `@cosmograph/cosmos` is also
  CC-BY-NC-4.0 and unusable here.

## Known gaps

- The Radix missing-`Description` warning on the knowledge drawers (pre-existing on
  `ChangeLogDrawer`; the new lint drawer matches the house pattern). Worth one pass across all of them.
- §5.12's keyboard model for the graph canvas.

## Log

### 2026-08-19
- 16-agent research sweep; design records DR-1…DR-28, each surviving a committee.
- Stages 0 → 8 landed in order, every stage gated by an adversarial reviewer that had to state, per
  claim, whether it verified by *running* something or only by reading.
- Notable catches, all by gates rather than by authors: a `Manifest` field addition that would have
  **persisted** a cleared `.active-kb`; a graph cache that would have 404'd every existing base
  forever; a link-equivalence test that stayed green under the mutation it was written to catch; a
  palette whose own guard passed while two of its colours were identical under protanopia; a
  schema-generation number that would have declared every existing base already-migrated; and a
  read-only lint that could never run on a private base — a pre-existing issue-#56 bug this work
  inherited and fixed.
