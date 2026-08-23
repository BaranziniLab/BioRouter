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
| `cargo test -p biorouter-mcp --lib knowledge::` | 283 | **631** |
| `cargo test -p biorouter-mcp` (whole crate) | — | **1471** |
| `cargo test -p biorouter-server --test knowledge_routes` | 46 | **57** |
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
| BioOKF graph: colour by type, solid vs hollow by top-level class, typed edges, legend | PASS — the shape-by-family channel was removed after this run; see `../knowledge-ui-redesign/redesign-spec.md` R-04 |
| Facet rail filtering | PASS |
| **OKF *and legacy* graphs render** | **PASS** — the back-compat property |
| Node inspector | PASS (after the fix in `eee061ce`) |
| Lint from the UI | was BLOCKED — no affordance existed; added in `eee061ce` |
| Theme family + light/dark with the graph open | PASS — follows live, no reload |
| `.brkb` export and re-import | PASS — lossless, format fields preserved |

A legacy base renders 10 pages / 57 links with `node_type: null` throughout, its legacy `kind`
vocabulary intact, the legend suppressed, and **0 lint diagnostics** — a legacy base is not scolded
for not being OKF, matching DR-26.

## KB-to-KB merge (post-Stage-8)

The one capability with a user-visible dead end: `.brkb` import always mints a fresh id, so a
collaborator's archive lands beside your base with no path to one graph. The **deterministic** half
of a merge now closes it — DR-29, DR-30 and DR-31 in [`design.md`](design.md).

| What | Where |
| --- | --- |
| The mechanics: raw dedup by content hash, rename on collision, reference rewriting, the pre/post canonical check, one transaction | `crates/biorouter-mcp/src/knowledge/merge.rs` |
| Barrier over both ids + the `max`/union classification fold | `KnowledgeService::merge_bases` / `absorb_classification` |
| Model surface | `kb_merge_preview` (gated, does not ratchet) and `kb_merge` (gated, ratchets) |
| User surface | `POST /knowledge/bases/{id}/merge`, behind the user-action proof; `dry_run` defaults to **true** |
| Tests | `cargo test -p biorouter-mcp --lib knowledge::merge` (21), `--test privacy_toggle_merge` (2), `-p biorouter-server --test knowledge_routes` merge rows (2). Re-measure rather than trusting these. |

**Not built, deliberately:** the judgement half — semantic candidate matching, true-match collapse,
prose and subtype harmonisation. An identifier present in both bases is renamed on the incoming side
and every reference to it repointed, never collapsed, because a wrong collapse destroys a curated
page silently and a wrong rename leaves two pages and a record. That half is a macro and belongs on
this foundation. There is also **no UI**; the surfaces above are the whole of it.

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

## Log

### 2026-08-22
- **§5.12's keyboard model is built**, closing a WCAG 2.1.1 Level A failure that predates this
  migration: the canvas §3.5 calls "the reason the view exists" had no tab stop, no focus model and
  no traversal, so the primary content of the section could not be reached without a mouse. One tab
  stop with a `focus-visible` ring, ±60° cone arrow traversal with a half-plane fallback, a
  degree-ordered `Tab` walk, `Home` to the highest-degree node, and an `aria-live` region that speaks
  `<identifier>, <type>, <family>`. Pure logic in `graph/graphKeyboard.ts` (18 tests); verified end
  to end in a real browser, since jsdom cannot render the canvas at all — force-graph calls
  `canvas.getContext('2d')`.
- **The node shape channel was removed** by operator decision, making that live region the section's
  only redundant channel rather than a second one. Rationale, what it cost and what it replaced:
  `../knowledge-ui-redesign/redesign-spec.md` R-04 (amended).
- **Two container-query overflow seams closed**, both found by sweeping the pane in 5px steps from
  720 to 1800 rather than checking the four canonical sizes. Both lived exactly *at* a threshold,
  where one step's promotion races another step's narrowing, so every canonical size rendered
  correctly and the suite was green. `--knowledge-pane-full-filters` moved 1060 → 1140 and
  `Predicate` folded into `More`. `styles/knowledgeLadder.test.ts` guards what jsdom can guard,
  which is the declarations rather than the layout.
- The graph's `zoomToFit` padding was found to be a regression, not a force-parameter problem:
  after a fit the cluster occupies exactly `viewport - 2 × padding` on the binding axis, a quantity
  no charge or link-distance value can move. Binding-axis fill 68% → 85%.

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
