# OKF migration — the stepwise plan

> **What this is.** Nine implementation stages, each with an explicit gate, ordered so every stage
> leaves the tree building and the existing knowledge tests green.
> **Status:** Current (in progress).
> **Audience:** Contributors working on the Knowledge subsystem.

Each stage ends with a **committee gate**: independent reviewers (correctness, privacy/security,
back-compat, and — where the stage touches the renderer — a browser check) must clear it before the
next stage starts. A stage that fails its gate is fixed, not waived.

## Measured starting point

Counts below are measured in this worktree, not estimated. Re-measure rather than trusting them.

| Surface | Count |
| --- | --- |
| `biorouter-mcp --lib knowledge::` unit tests | 283, over 39 modules |
| `biorouter-server --test knowledge_routes` | 46 knowledge tests |
| Desktop `it()` cases under `components/knowledge` + the chat chip | 94, in 14 files |
| Rust tests that encode the page format directly | ~45 |
| UI tests that encode the page format directly | ~19 |
| Independent frontmatter parsers in-tree | 2 (Rust `split_frontmatter`, TS `splitFrontmatter`) |
| Copies of the `\[\[([^\]]+)\]\]` regex | 3 |

## Stage 0 — `okf` core module (Rust)

New `crates/biorouter-mcp/src/knowledge/okf/` holding the format itself, with no BioRouter
dependencies beyond serde:

- `frontmatter.rs` — the parse: line-based `---` delimiters, `{}` on absent frontmatter, error on
  unterminated, error on a non-mapping. Unknown keys **preserved** (`#[serde(flatten)]`).
- `model.rs` — `ConceptDoc`, `Source`, `Generated`, `Verified`, `Status`, `Actor`, `Edge`.
- `trust.rs` — `normalize_verified` (a bare mapping becomes a one-element list), `trust_tier`,
  `is_stale`.
- `links.rs` — the three link readers (DR-2) behind one `extract_edges` entry point.
- `conformance.rs` — the three OKF producer rules and the five consumer tolerances.

**Gate:** round-trips every fixture in `okf/fixtures/` byte-for-byte, including unknown keys.
Existing 283 tests untouched and green.

## Stage 1 — BioOKF profile module

`knowledge/biookf/` — the closed vocabulary as data, not prose:

- `vocabulary.rs` — the 28 `NodeType` and the 24 positive predicates; the 11 negatables derived, not
  listed, so `not_<X>` can never drift from `<X>`.
- `domain_range.rs` — the domain/range table, with `not_<X>` inheriting its base's table.
- `lint.rs` — the BioOKF rule set: invalid type/predicate, `identifier` uniqueness and
  human-readability, unresolved `object`, unresolved `primary_source`, missing provenance triplet,
  domain/range violation, unanchored source node, `<X>` and `not_<X>` contradiction.
- `aliases.rs` — the deprecated `type`/attribute aliases accepted on read (SPEC §14).

**Gate:** the vocabulary matches `SCHEMA.md` exactly — asserted by a test that parses the spec's own
tables out of a checked-in fixture, so a spec bump fails loudly instead of silently diverging.

## Stage 1.5 — Seams (must land before Stage 2)

The risk review found that several later stages trip a landmine that is cheap to defuse *first* and
expensive to diagnose *after*. Each item here is a small, **behaviour-preserving** change whose whole
purpose is to make the next stage safe. None of them changes the page format.

| # | Change | Defuses |
| --- | --- | --- |
| S-a | Graph-cache envelope `version`; `read_cache` returns `Ok(None)` on parse failure or version mismatch; retire the scaffold-node self-heal predicate | DR-13 — otherwise every existing base 404s its graph forever, or silently serves typeless nodes forever |
| S-b | `#[serde(default)]` on every `Manifest` field including `schema_version`; `list_bases` surfaces an unreadable manifest instead of dropping it | DR-12 — otherwise the first visibility toggle **persists** a cleared `.active-kb` |
| S-c | One shared `[[…]]` parser + resolver; `query` and `lint` call it; a test drives all three consumers over one corpus and asserts they agree | DR-14 — they already disagree today, and extending the grammar corrupts the other two |
| S-d | `complete` sentinel dispatches sibling tool calls before returning | DR-15 — batched typed writes would be silently discarded and misreported as "wrote no knowledge pages" |
| S-e | `valid_page(type, title, body)` fixture helper; existing fixtures moved onto it | DR-19 — otherwise a validating writer turns ~20 privacy tests red for reasons unrelated to privacy |
| S-f | `make_schema` emits real JSON Schema (enums, descriptions, nesting) | DR-16 — a closed vocabulary is otherwise unenforceable and un-declarable to the model |
| S-g | `schema.md` migration keyed off `Manifest.schema_version`, called from all three macros, error no longer swallowed | DR-12/DR-16 — the substring fingerprint reports "already migrated" for every base that exists |

**Gate:** every one of the 283 `knowledge::` tests still passes, the new equivalence test for S-c
passes, and a v1 `manifest.yaml` with no `format` key still loads. Nothing user-visible changed.

## Stage 2 — Graph derivation

Rewrite `graph.rs` to emit typed nodes and edges:

- `GraphNode` gains `node_type`, `subtype`, `identifier`, `status`, `stale`.
- `GraphEdge.relation` — the pre-cut socket — is finally populated, plus `knowledge_level`,
  `negated`, `primary_source`.
- Dangling links become **recorded**, not dropped (`graph.rs:82`'s `if let Some(to)` has no `else`
  today), so lint has a real check.
- Identity resolves through the DR-3 ladder.

**Gate:** every existing graph test still passes; new tests cover all three link forms, a dangling
edge, and a rename that does not break inbound edges.

## Stage 3 — Store, manifest, service

- `Manifest` gains `format`, `okf_version`, `biookf_version`; `schema_version` → **3**, not 2
  (DR-6 — 2 was already taken by the cross-reference-rules generation every base on disk carries,
  and numbering OKF 2 would declare every one of them already-migrated).
- `create_base_as` takes a `KbFormat` and scaffolds the matching tree and `schema.md`.
- `schema.md` carries `type: Schema` frontmatter (DR-23) and the deriver skips it as a scaffold
  page, exactly as it skips `index.md` and `log.md`.
- A legacy base keeps working untouched, read through its own generation's path. **`kb_migrate_format`
  is NOT built** — DR-22 defers it, and DR-17 explains why a migration path would be a fifth privacy
  write choke point bypassing all four that exist. The automatic schema ladder therefore stops at
  generation 2 (`AUTOMATIC_SCHEMA_CEILING`), one below the OKF generation, asserted at compile time.
- `index.md` and `log.md` are written in the OKF shapes (`# Section` + `* [Title](link) - desc`;
  `## YYYY-MM-DD` groups, newest first, the kind in the bullet) — both were silent conformance
  failures before this stage.

**Gate:** the three durability invariants hold — tmp+rename on every durable write, no re-entrant
`lock_root()`, and `txn_wrote_knowledge_pages` still compares only the `knowledge/` subtree oid
(issue #71). Both scaffolds are checked against the project's own `okf::check` / `check_index` /
`check_log`, and a legacy base is verified end to end.

## Stage 4 — MCP tool surface

- `kb_create_base` gains `format`.
- New `kb_validate_page` (validate before write, as BioOKF's own toolchain does).
- `kb_lint` returns typed diagnostics.
- `instructions.md` and a new pair of skills teach the agent **when** to pick each profile.
- Every new mutating tool joins `KB_RATCHETING_TOOLS` (DR-8).

**Gate:** the privacy repo-grep assertions are re-run and each change to a counted site is
deliberate. A private session creating a base still yields a PRIVATE base.

## Stage 5 — Sub-agent macros

`INGEST_PROCEDURE` / `QUERY_PROCEDURE` / `LINT_PROCEDURE` become profile-aware. The BioOKF prompt
injects the type cheatsheet and the predicate table; the OKF prompt stays short and permissive.

**Gate:** token cost of the injected vocabulary is measured, not assumed; a real ingest into a
BioOKF base produces valid typed edges without a retry storm.

## Stage 6 — HTTP routes, OpenAPI, TS client

`POST /knowledge/bases` takes `format`; `GET /knowledge/graph` returns the typed graph; lint returns
typed diagnostics. Then `just generate-openapi && cd ui/desktop && npm run generate-api`.

**Gate:** the generated client compiles and the SSE terminal-frame contract test still passes.

## Stage 7 — Desktop UI: a comprehensive design pass

This stage is a full redesign of the Knowledge section, not only the typed-graph work. The brief is
to revise **every** design choice in the section and update the graph aesthetics so it reads as
native to the app — matching the app as it actually is, not only as `design.md` describes it.

The design itself is produced and reviewed before any code is written; the binding output is
[`ui-spec.md`](ui-spec.md), reviewed by a three-reviewer committee (design-system fidelity,
accessibility with real contrast arithmetic, and implementation feasibility).

**Design pass inputs**
- The binding rule set extracted from `design.md` (all 1659 lines), the theme-system architecture,
  the three `themes/*.theme.mjs` sources, and the still-open entries in the Drift register.
- A per-component drift audit of all ~30 files under `components/knowledge/`, verdict per visual
  choice: on-system or drift.
- A pattern-library extraction from the sections that are **not** Knowledge (shell, chat, Home,
  Settings, Applications, artifact panel) — the conventions a new section must copy to feel native.
- BioOKF Studio's information architecture: its inspector, legend, facet and lint surfaces.

**Surfaces specified and rebuilt**
- The section shell, header band and panes.
- KB selector palette and trigger, now carrying the **format chooser** (OKF vs BioOKF with guidance
  on when to pick which).
- The ingest panel and all five of its states.
- The graph panel: 28-type palette generated into `themes.generated.ts` beside the existing heat
  ramp (never hand-written into `main.css`), a hashed fallback for arbitrary OKF types (DR-11),
  tapered edges, dashed-red negation with struck-through labels, focus glow, priority-ranked
  collision-avoiding labels, density LOD, and memoised label computation (DR-9).
- The legend: 28 types grouped into their 7 families in the app's chip vocabulary.
- The facet rail: node type, predicate, source, status.
- The typed inspector: frontmatter, inbound and outbound edges, provenance chain to `raw/`.
- The change-log drawer, the tier control, and every empty / loading / error state.
- Credibility moves to the node ring so the type can take the fill (DR-9b).

**Gate:** verified in a **real browser** via a harness on the `.artifact-harness` pattern — jsdom has
no canvas layout, no WebGL, does not run Tailwind and does not evaluate `:has()`, so it can catch
none of this. All three theme families, light and dark. Contrast measured, not asserted.

## Stage 8 — Verification in the real app

Build the app, create one base in each profile, ingest a real source into each, inspect the graph,
export and re-import, and confirm a legacy base still opens. Then run the full suites and record
measured counts back into these docs.

**Gate:** the app does the thing, observed — not inferred from a passing unit test.

## Related documentation

- [`design.md`](design.md) — the decision records these stages implement.
- [`progress.md`](progress.md) — live progress.
