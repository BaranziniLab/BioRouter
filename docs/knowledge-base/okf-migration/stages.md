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

- `GraphNode` gains `node_type`, `subtype`, `identifier`, `status`, `stale`, `external` and
  `degree`.
- `GraphEdge.relation` — the pre-cut socket — is finally populated, plus `predicate`, `negated`,
  `synthesized`, the provenance triplet (`knowledge_level`, `agent_type`, `primary_source`),
  `publications`, and the two open maps `quantitative` and `qualifiers`.
- **The contract is [ui-spec.md](ui-spec.md) §2.1, not the draft above it.** §2.1 revised this list
  and said so — "a change to stages.md Stage 2, taken here and to be mirrored there" — and the
  mirroring is this bullet. DR-27 then replaced the six flat statistical fields Stage 2 shipped
  (`effect_metric`, `effect_size`, `ci_lower`, `ci_upper`, `p_value`, `sample_size`) with the open
  `quantitative` map, because §7.3 names around twenty slots and six fields drop fourteen; and it
  added `synthesized` and `degree`. DR-28 keeps the typed fields `Option` against §2.1's drawing of
  them as required.
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
- `kb_lint` returns typed diagnostics — and is now a **tool** as well as an HTTP route and a CLI
  command, so an agent can check its own work. Read-only: the autofix half is deliberately not
  reachable from MCP (DR-8's corollary).
- `instructions.md` and a new pair of skills teach the agent **when** to pick each profile.
- Every new mutating tool joins `KB_RATCHETING_TOOLS` (DR-8).

**Gate:** the privacy repo-grep assertions are re-run and each change to a counted site is
deliberate. A private session creating a base still yields a PRIVATE base.

**Landed, with three notes worth carrying forward:**

- `kb_validate_page` is gated (`KB_ID_GATED_TOOLS`) and deliberately **not** ratcheting. It is the
  first tool that names a base and writes nothing, so DR-8's "every mutating one also" is answered
  with a no — a permanent tier raise for a check that committed nothing is a one-way loss of reach
  bought for nothing. The decision is recorded as a `ratchets: false` row in `KB_TOOL_PROBES`,
  which is what `every_tool_the_router_exposes_is_classified_by_the_probe_table` demands.
- DR-8's **second** surface — `KbToolDispatch` must never accept a `kb_id` — was pinned for the
  first time, by schema, by grep and by behaviour. It was load-bearing and untested.
- Four skills ship, not two: choosing a format, ingesting into OKF, ingesting into BioOKF (which
  carries the typing decision procedure) and reading a lint report. They ship in their own
  `KNOWLEDGE_SKILLS` array rather than in `BUILTIN_SKILLS`, because that array doubles as the
  desktop Contexts list and is pinned from `ui/`; see progress.md's deviation note for the Stage 7
  follow-up.

## Stage 5 — Sub-agent macros

`INGEST_PROCEDURE` / `QUERY_PROCEDURE` / `LINT_PROCEDURE` become profile-aware. The BioOKF prompt
injects the type cheatsheet and the predicate table; the OKF prompt stays short and permissive.

**Gate:** token cost of the injected vocabulary is measured, not assumed; a real ingest into a
BioOKF base produces valid typed edges without a retry storm.

**Landed, with four notes worth carrying forward:**

- **The vocabulary is an `enum` on `kb_write_concept`, not a table in the prompt.** DR-16's fix
  for two problems with one change: the provider can constrain sampling with it, and it costs
  the prompt nothing per step. Measured — 8,937 bytes for an OKF ingest prompt, 14,397 for a
  BioOKF one, against a 4,975-byte *floor* for the vocabulary as prose that would have been paid
  on all 30 iterations. Figures in [`progress.md`](progress.md).
- **The measurement failed first, correctly.** The BioOKF procedure was bigger than the
  vocabulary it replaced, because it had grown a numbered loop duplicating the `schema.md`
  sitting directly above it in the same prompt. The assertion is on the *procedure* rather than
  the assembled prompt, because the two `schema.md` templates differ for Stage 3's reasons.
- **Materializing the source node broke issue #71's guarantee** until the wrote-knowledge check
  was re-baselined after the seed: a run that wrote nothing would otherwise have committed.
- **A typed writer must merge, not replace.** Rewriting a page through `kb_write_concept` would
  have dropped `sources`, `generated`, `br_credibility`, `br_page_id`, the body and every
  preserved unknown key — conformantly, and therefore invisibly.

⚠ **Half of the gate has not been run.** The token cost is measured (a test prints it). *"A real
ingest into a BioOKF base produces valid typed edges without a retry storm"* is exercised only
against a `MockCompleter`: the bundle a scripted run produces is asserted conformant, edge for
edge, but no live provider has been asked to choose among 28 types with the enums in front of it.
That is Stage 8's job, and until it runs, the claim that the retry storm is gone is a design
argument rather than an observation.

## Stage 6 — HTTP routes, OpenAPI, TS client

`POST /knowledge/bases` takes `format`; `GET /knowledge/graph` returns the typed graph; lint returns
typed diagnostics; `.brkb` import becomes format-aware (DR-18). Then
`just generate-openapi && cd ui/desktop && npm run generate-api`.

**Gate:** the generated client compiles and the SSE terminal-frame contract test still passes.

**Landed, with four notes worth carrying forward:**

- **The route parses `format` strictly, and the reasoning is Stage 4's applied one layer out.**
  `KbFormat`'s `Deserialize` is lenient because DR-12 traces what a failing `manifest.yaml` costs
  the user, so the body field is an `Option<String>` checked by hand; `schema(value_type)` keeps
  the published contract — and the generated TypeScript — an `enum` of exactly the two words. A
  misspelt format is a **400 that creates nothing**, asserted on disk as well as on the wire.
- **The typed graph was already derived; what Stage 6 added is proof it reaches a client.** Stage
  2's tests assert the deriver fills the fields, which is a different claim: an over-eager
  `skip_serializing_if`, an unregistered `$ref`, or a route answering from an older cache would
  each leave them green and the renderer blind. The new route test reads the JSON only.
- **The lint stream's terminal frame is a `LintResult`, not a `LintReport`** — the wrapper carries
  `commit_sha` and `fixes_applied`. Both are published as `components.schemas` and neither is
  declared as the response body, because the body is an event stream: `body = LintResult` would
  type the generated client's return value as JSON and be wrong at runtime. The test found the
  wrapper; the first draft of the schema registration had missed it.
- **DR-18's refusal is an ORDERING, not a check.** The provenance marker is the last entry the
  exporter writes, so a format check made inside the extraction loop fires with the whole base
  already unpacked — exactly the partial extraction DR-18 forbids. The marker is now read in a
  pre-pass, before the extraction root is created, and the mutation (move the check back after
  `create_dir_all`) fails the test that says so.

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
