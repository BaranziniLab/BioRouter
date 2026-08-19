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

- `Manifest` gains `format`, `okf_version`, `biookf_version`; `schema_version` → 2 (DR-6).
- `create_base_as` takes a `KbFormat` and scaffolds the matching tree and `schema.md`.
- A legacy base (`schema_version: 1`) keeps working untouched; an explicit `kb_migrate_format`
  upgrades one on request.
- `index.md` and `log.md` are written in the OKF shapes (`# Section` + `* [Title](link) - desc`;
  `## YYYY-MM-DD` groups) — both are silent conformance failures today.

**Gate:** the three durability invariants hold — tmp+rename on every durable write, no re-entrant
`lock_root()`, and `txn_wrote_knowledge_pages` still compares only the `knowledge/` subtree oid
(issue #71). Migration is verified on a copy of a real base.

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

## Stage 7 — Desktop UI

- **Format chooser** at creation (`KBSelectorPalette.tsx:107`), two cards with the guidance text.
- **Typed graph**: 28-type palette with theme-token structure and a hashed fallback for arbitrary
  OKF types (DR-11), tapered edges, dashed-red negation, focus glow, collision-avoiding labels,
  density LOD.
- **Faceting**: filter by node type, predicate, source, status.
- **Inspector**: typed frontmatter, edges in and out, provenance chain to `raw/`.
- **Shadow-canvas fix**: `nodePointerAreaPaint` / `linkPointerAreaPaint`.

**Gate:** verified in a real browser via a harness — jsdom has no canvas layout and cannot catch any
of this. All three theme families, light and dark.

## Stage 8 — Verification in the real app

Build the app, create one base in each profile, ingest a real source into each, inspect the graph,
export and re-import, and confirm a legacy base still opens. Then run the full suites and record
measured counts back into these docs.

**Gate:** the app does the thing, observed — not inferred from a passing unit test.

## Related documentation

- [`design.md`](design.md) — the decision records these stages implement.
- [`progress.md`](progress.md) — live progress.
