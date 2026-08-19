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
| 3 | Store, manifest, service; legacy bases untouched | **DONE** — 6 files + 2 schema templates, 523 tests | committee gate NOT yet run |
| 4 | MCP tool surface + skills | **DONE** — `kb_create_base` takes a format, new `kb_validate_page`, typed lint diagnostics, 4 skills; 556 tests | committee gate NOT yet run |
| 5 | Sub-agent macros (profile-aware) | **DONE** — profile-aware procedures, the vocabulary as tool-schema enums, materialized source nodes, a diagnosable rejection; 583 tests | committee gate NOT yet run |
| 6 | HTTP routes + OpenAPI + TS client | **DONE** — `format` on create, the typed graph on the wire, typed lint diagnostics, format-aware `.brkb` import; regenerated spec + TS client; 587 tests | committee gate NOT yet run |
| 7 | Desktop UI design pass (spec + Slice A) | Spec revised after committee; Slice A IN PROGRESS | — |
| 8 | Verification in the real app | NOT STARTED | — |

## Measured baselines

Captured before any change, so a later "pre + N" assertion has something true to stand on.

| Suite | Count | Measured |
| --- | --- | --- |
| `cargo test -p biorouter-mcp --lib knowledge::` (baseline, before any OKF work) | 283 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stages 0+1) | **446** = 283 + 97 + 66 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 1.5) | **473** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 2) | **496** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 3) | **523** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after the DR-26..28 corrections) | **529** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 4) | **556** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 5) | **583** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::` (after Stage 6) | **587** | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::okf` | 97 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib knowledge::biookf` | 66 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib` (whole crate lib) | 1217 passed, 7 pre-existing ignores | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_routes` | 47 | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_routes` (after Stage 6) | **53** | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_routes_e2e` | 2 | 2026-08-19 |
| `cargo test -p biorouter-server --test knowledge_ingest_stream` | 2 | 2026-08-19 |
| `cargo test -p biorouter-mcp --lib` (whole crate, after Stage 5) | 1356 passed, 7 pre-existing ignores | 2026-08-19 |
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

### 2026-08-19 (Stage 3)
- **Stage 3 landed.** `Manifest` gains `format` / `okf_version` / `biookf_version`, all
  `#[serde(default)]` (DR-12); `create_base_as` takes a `KbFormat` and scaffolds the matching tree,
  `schema.md`, `index.md` and `log.md`; `log::append` writes OKF §9's `## YYYY-MM-DD` groups instead
  of the Karpathy `## [date] kind | summary` heading it had been writing since the subsystem was
  built. `knowledge::` 496 → 523.
- **The schema generation is 3, and the automatic ladder stops at 2.** Reaching generation 3 means
  rewriting a base's *pages*, which is the format migration DR-17 refuses on an unauthenticated path
  and DR-22 defers; `AUTOMATIC_SCHEMA_CEILING < CURRENT_SCHEMA_VERSION` is asserted in a `const`
  block so bumping one number without the other fails the build rather than a test.
- **`Manifest::profile()` is the accessor, not `Manifest::format`.** The field defaults to `okf` on
  every `manifest.yaml` on disk, so a check written against it alone treats every legacy base as
  already-OKF — DR-6's trap, reached from the reader instead of the writer. `profile()` folds the
  generation in and answers `None` for a legacy base.
- **An unknown `format` word reads as plain OKF rather than failing the load.** `list_bases` still
  drops a base whose manifest will not parse, so a strict enum would have put DR-12's persisted-data-loss
  cascade one typo away; every profile is OKF plus constraints, so reading an unknown one as plain OKF
  loses constraints and never content.
- **Verified by mutation**: 16 deliberate defects applied one at a time — including numbering OKF
  generation 2, raising the ladder ceiling, dropping `serde(default)` from `format`, emitting
  `biookf_version` into `index.md`, emitting the revision unquoted, reverting the log heading, and
  scaffolding outside `knowledge/` — and every one was caught by the test that should catch it.

### 2026-08-19 (Stage 4)
- **Stage 4 landed.** The tool surface and the agent-facing guidance:
  - `kb_create_base` takes a `format` (`okf` | `biookf`, defaulting to OKF), and its tool
    description is written as the lever that decides which — the two profiles, what each costs,
    and the rule that decides the ambiguous case ("if it is not biomedical, choose OKF; a
    biomedical vocabulary does not make a non-biomedical base stricter, it makes it wrong").
  - New **`kb_validate_page`**: OKF conformance plus, in BioOKF mode, the profile, against a
    bundle index that includes the draft. It is what lets a sub-agent fix one page instead of
    failing a whole ingest, and it is what BioOKF's own toolchain does before every write.
  - `kb_lint` returns **typed diagnostics** — stable rule id, severity, subject, message —
    alongside the four lists it always returned. The four re-appear as `kb.*` rules; the format
    layers arrive as `okf.*` and `biookf.*`, so a reader can tell "untidy" from "not conformant".
  - Four skills: `knowledge-choose-a-format`, `knowledge-ingest-okf`, `knowledge-ingest-biookf`
    (carrying the seven-step typing decision procedure), `knowledge-lint`.
- **The parameter is a `String` parsed by hand, not an `Option<KbFormat>`, and that is the
  interesting decision.** `KbFormat`'s own `Deserialize` is lenient on purpose (DR-12: a
  `manifest.yaml` that fails to load costs the user their pointers), so a typed parameter would
  have read `bio-okf` as OKF, created a plain base, returned success, and left the model to find
  out pages later — with no conversion available (DR-26). A request is not a file. DR-7 already
  draws the line: producers are held to a higher bar than consumers. `schemars(with)` keeps the
  generated schema an `enum` of the two, so the strict parse is the backstop and not the first
  line of defence.
- **Both DR-8 surfaces, as DR-8 asks.** `kb_validate_page` joined `KB_ID_GATED_TOOLS` and
  deliberately did **not** join `KB_RATCHETING_TOOLS`: it writes nothing, and a tier raise is
  permanent, so ratcheting on a check that committed nothing is a one-way loss of reach bought
  for nothing. `every_tool_the_router_exposes_is_classified_by_the_probe_table` fired on the
  first run — which is the invariant working — and was answered with a probe row carrying the
  explicit `ratchets: false` decision, not by editing the assertion.
- **The sub-agent surface is now pinned three ways.** DR-8's load-bearing invariant —
  `KbToolDispatch` must never accept a `kb_id` — had no test. It has three:
  `no_sub_agent_tool_takes_a_kb_id` checks the schema the model is shown *and* greps the
  production half of the file for the argument key, and
  `a_kb_id_in_the_arguments_does_not_move_the_dispatch` sends one anyway and asserts the read does
  not reach the other base and the write lands in the bound one.
- **A latent non-determinism, found by a test written for something else.** Three of the four
  lint lists are collected out of a `HashMap`/`HashSet`, so the same base linted into a different
  order on every run. It was invisible while the report was four bags of strings; it stops being
  invisible the moment the entries are diagnostics, because a report diff becomes noise and the
  `MAX_DIAGNOSTICS` cut picks a different subset each time. The four lists are sorted now.
- **A legacy base is checked against nothing, and says so.** DR-26 in both new surfaces:
  `kb_validate_page` reports `format: null` with an empty list, and `kb_lint` gives such a base
  the four hygiene rules and no format layer. Running OKF conformance over a base this build has
  promised never to rewrite would report a *decision* as one error per page and bury the findings
  that are real.

#### Deviation to know about
The four skills ship in their own array (`KNOWLEDGE_SKILLS`), not in `BUILTIN_SKILLS`. That array
is also the desktop **Contexts** list, hand-synced into two TypeScript copies and pinned by
`ui/desktop/src/components/settings/contexts/contexts.test.ts`, which reads `skills_extension.rs`'s
*source text* — so adding them there would fail a UI test from a Rust-only change, and Stage 4 does
not touch `ui/`. The separation is honest on its own terms (these trigger on a task; a Context is
always-on self-knowledge), and `is_builtin_skill_name` covers both so `count_user_skills` does not
report four skills the user never installed. **What it leaves for Stage 7:** the desktop's
`BUILTIN_SKILL_NAMES` does not list them, so the Skills pane shows a Delete control on them.
Deleting lasts until the next startup, when the seeder rewrites the folder.

### 2026-08-19 (Stage 5)
- **Stage 5 landed.** The sub-agent macros are profile-aware, and the closed vocabulary moved
  from prose into the tool schema.
  - **`tool_specs` takes a format.** A BioOKF base gains `kb_write_concept` — whose `type`,
    `predicate`, `knowledge_level` and `agent_type` are declared as JSON Schema `enum`s built
    from the vocabulary's own tables — and `kb_validate_page`. The eight tools an OKF or legacy
    run has always been handed are byte-identical, asserted across all three formats.
  - **The procedures split two ways, not three.** A legacy base (DR-26) gets the OKF procedure,
    and the two places the two would otherwise differ — the link grammar and the directory
    convention — are written to defer to `schema.md`, which is correct for both. A third string
    would have been a third thing to keep in step for nothing.
  - **The BioOKF procedure carries the typing decision procedure**, the `Disease` /
    `Phenotype` / `BiomedicalMeasure` disambiguation, and the rule that a concept fitting none
    of the 27 substantive types takes `Other` with a note and never an invented type.
  - **Source nodes are materialized in Rust, not asked for** (DR-24). One
    `Publication`/`Dataset` page per source, with `xref`, `raw_source`, and its own
    self-citing `reported_in` edge; the sub-agent is told its identifier verbatim.
  - **A diagnosable failure.** A rejected value comes back as a typed `VocabularyRejection`
    naming the closest legal one, and a budget that runs out while one is outstanding reports
    `DoneReason::VocabularyRetriesExhausted` instead of `StepBudgetReached`.

#### Measured, because the gate asks for measured
The system prompt is `schema.md` + the procedure, re-sent on every one of up to 30 iterations.

| | bytes | ~tokens |
| --- | --- | --- |
| ingest prompt, OKF/legacy | 8,937 | 2,234 |
| ingest prompt, BioOKF | 14,397 | 3,599 |
| — of which the BioOKF procedure | 4,453 | 1,113 |
| — of which the typing decision procedure | 1,887 | 471 |
| the vocabulary as prose, **avoided** | 4,975 | 1,243 |

Over a 30-step run that is 421 KB sent rather than 567 KB. The 4,975-byte counterfactual is a
**floor**: it is generated from the vocabulary itself with one line per entry carrying only the
name, the family and the domain/range — a paste that actually taught the vocabulary would gloss
each entry, which is where the brief's 6–12 KB estimate comes from. Both numbers are printed by
`the_vocabulary_costs_the_prompt_nothing_per_step` under `--nocapture`.

#### Three things the work found
- **The first measurement failed its own assertion, and it was right to.** The BioOKF procedure
  came in at 6,141 bytes against a 4,975-byte vocabulary — prose costing more than the table it
  refuses to paste. The cause was real: it had grown a numbered ingest loop and a copy of §7.3's
  slot list that `schema.md`, which sits directly above it in the same prompt, already spells
  out. Cutting the duplication took it to 4,453.
- **Seeding the transaction broke issue #71's guarantee, silently.**
  `txn_wrote_knowledge_pages` compares the txn branch against **main**, which answers "did
  anything change" only while main is the last thing that wrote knowledge. Materializing the
  source node moves that subtree before the sub-agent starts, so a run that wrote nothing would
  have passed the check and returned a commit sha for work that never happened — the exact
  false success #71 closed. Ingest now takes a baseline after the seed
  (`GitRepo::txn_knowledge_tree_id`) and compares against that.
- **A typed writer is a data-loss machine unless it merges.** `kb_write_page` overwrites and
  `compose_page` builds a page from its arguments, so the second pass over a page would have
  deleted every key the tool has no parameter for — `sources`, `generated`, `br_credibility`,
  `br_page_id`, the body's prose, and every unknown producer key OKF §11 requires a consumer to
  preserve. None of it would have failed anything; the page would have stayed conformant and
  quietly lost its provenance. The existing frontmatter and body are now the base the call is
  written over.

#### A pre-existing leak, found on the way past
`ingest` opened its transaction and then ran three fallible steps before the sub-agent — the
`schema.md` read was already one of them — with a bare `?` on each. `begin_txn` moves HEAD onto the
transaction branch, so any of those failing left HEAD parked there, which is how the next write to
that base lands somewhere nobody is looking. The three now go through one `ingest_setup` call whose
error aborts the transaction, and the abort is pinned by a test (provoked by making `schema.md` a
directory).

#### Every fix was mutation-tested
Six deliberate defects, applied one at a time: skip the transaction abort, ask the wrote-knowledge
question against main again, compose a page without merging the existing one, stop recognising a
`VocabularyRejection`, drop the `enum` from the predicate schema, and remove the source node's
self-citing `reported_in` edge. Five were caught by the test that should catch them. **The sixth was
not** — nothing asserted the seeded source node carried its own `reported_in`, which is exactly the
half of DR-24 that DR-4 had missed. The assertion was added and the mutation re-run against it.

#### Two deliberate narrownesses
- **`source_xrefs` reads the title and URL, never the document body.** A PDF states its own DOI
  in its text far more often than in its filename, so this costs real hits — but a paper's
  reference list is full of *other* papers' identifiers and a regex cannot tell them apart. A
  missing `xref` is an enrichment opportunity (`raw_source` already anchors the node, so
  `biookf.source.unanchored` does not fire); a wrong one is a false claim about which paper this
  is, propagated to every edge citing the node. Reading the document and judging is the
  sub-agent's job, and the procedure tells it to extend the page.
- **`source_node_type` only distinguishes `Dataset` from `Publication`.** It is the one
  distinction a mime type decides. Guessing `Study` from a title containing "trial" would be a
  heuristic that is wrong quietly.

### 2026-08-19 (Stage 6)
- **Stage 6 landed.** The HTTP surface and the generated TypeScript client.
  - `POST /knowledge/bases` takes **`format`**, routed to `create_base_in` so the manifest, the
    scaffolded tree and `schema.md` are still written in one transaction.
  - `GET /knowledge/bases/{id}/graph` carries **every** field Stage 2 added — `node_type`,
    `subtype`, `identifier`, `status`, `stale`, `external`, `degree` on a node; `predicate`,
    `negated`, `synthesized`, the §8.1 provenance triplet, `publications` and the two open maps
    `quantitative` / `qualifiers` on an edge. `KbFormat` joined the OpenAPI components beside
    Stage 2's `QuantitativeValue`; without it `Manifest.format` was a dangling `$ref` the
    generator does not complain about.
  - The lint stream's terminal frame is published as a **`LintResult`** wrapping a `LintReport`
    with typed `Diagnostics`.
  - **`.brkb` is format-aware (DR-18).** The marker gains a real `format` (schema 2 → 3), written
    from the packed tree's own `Manifest::profile()`, and import refuses an unreadable profile
    with a repair message.
  - `just generate-openapi` and `npm run generate-api` re-run; `npx tsc --noEmit` and the 2,743
    desktop unit tests pass with **no hand edits under `ui/`** — the regeneration was the whole
    of the frontend change.
- **DR-21 held: no plain-OKF export path was added.** `.brkb` remains the only transfer door and
  it keeps its provenance sidecar. The new `format` field rides *inside* that sidecar, so it
  inherits the tier floor and the owner union rather than opening a second, unmarked route.
- **The marker's `format` is an `Option<String>`, not an `Option<KbFormat>`, and DR-18 rests on
  it.** A typed field would read a profile this build has never heard of as plain `okf` — DR-12's
  leniency, correctly applied to a file already written and catastrophically applied to a decision
  about whether to extract — and `import` could then never refuse. It is also written from
  `profile()` and not `format`, so a legacy base's archive declares nothing rather than claiming
  to be an OKF bundle.
- **Three claims were mutation-tested**, two on the create route and one on import: ignoring the
  requested `format`, accepting an unknown one instead of refusing, and moving the import refusal
  to after the extraction root is created. Each was caught by the test that should catch
  it, and the third is the one that matters — the refusal's correctness is an *ordering*, and a
  check that fires one line too late is indistinguishable from a correct one in every other
  assertion.

#### Two things left standing, deliberately
- **`kb_restore_state` is untouched.** DR-18 pairs the import fix with "a restore that crosses the
  migration commit re-runs the migration" — and there is no migration to re-run: DR-22 defers it
  and DR-26 says an existing base has no path to OKF in this release. Wiring a hook to nothing
  would read as protection and be none.
- **`PUT /bases/{id}/pages/{path}` still does not invalidate the graph cache.** Pre-existing, not
  introduced here, and not on the OKF path (every macro rebuilds the cache), but it is why the new
  graph test clears `graph-cache.json` rather than writing its page through the route.
