# OKF migration — design and decision records

> **What this is.** The normative design for BioRouter's knowledge-base format after the OKF
> migration, plus the decision records that justify each choice and the alternatives rejected.
> **Status:** Current (in progress).
> **Audience:** Contributors working on the Knowledge subsystem.

## 1. The two profiles

| | **OKF** (default) | **BioOKF** (opt-in profile) |
| --- | --- | --- |
| Base spec | OKF v0.2 | OKF v0.2 + BioOKF v0.5 |
| `type` | any non-empty string | one of **28** controlled values |
| Edge predicate | any non-empty string | one of **35** (24 positive + 11 `not_<X>`) |
| Edge provenance | optional | `knowledge_level` + `agent_type` + `primary_source` **required** |
| Domain/range | not checked | checked, reported as lint warnings |
| Unknown values | accepted silently | accepted on read, **flagged** by lint |
| Good for | general memory, retrieval, dev notes, project context | biomedical literature, curated biology, cross-institution exchange |

Both profiles write the **same on-disk shape**. BioOKF only adds constraints; a BioOKF bundle is
always a valid OKF bundle. This is the property that lets one reader, one graph deriver and one
renderer serve both.

## 2. The page contract

Every non-reserved `.md` under a bundle is a **concept document**: YAML frontmatter + Markdown body.

```yaml
---
type: Molecule                    # REQUIRED (OKF §4.1). Open in OKF, closed in BioOKF.
identifier: Interleukin-6         # BioOKF primary key: human-readable AND bundle-unique.
                                  # NOTE: `title` is a DEPRECATED ALIAS for `identifier`
                                  # (SPEC §14) — BioOKF mode emits `identifier` only.
description: A pleiotropic pro-inflammatory cytokine.
subtype: protein                  # agent-coined, never validated
tags: [cytokine, inflammation]
xref: [UniProtKB:P05231, HGNC:6018]
status: stable                    # draft | stable | deprecated  (OKF §5.4)
stale_after: 2027-01-01           # absolute date (OKF §5.5)
generated: { by: biorouter/claude-opus-5, at: 2026-08-19T12:00:00Z }   # OKF §5.2
verified:
  - { by: "human:wgu", at: 2026-08-19T13:00:00Z }
sources:                          # OKF §5.1 — document-level provenance, objective signals only
  - id: pmid-32504360
    resource: raw/pmid-32504360/original.pdf
    title: Elevated IL-6 and severe COVID-19
    author: "human:Chen et al."
    last_modified: 2020-06-01
edges:                            # BioOKF §6 typed graph layer; legal (ignored) in OKF mode
  - predicate: associated_with
    object: COVID-19              # the target NODE's identifier
    knowledge_level: statistical_association
    agent_type: text_mining_agent
    primary_source: Chen 2020 (IL-6 and severe COVID-19)   # a source NODE's identifier
    effect_metric: hazard_ratio
    effect_size: 2.9
    p_value: 3.0e-6
br_page_id: 01J8X...              # BioRouter producer extension: identity across renames
---

# Interleukin-6

A pleiotropic pro-inflammatory cytokine.[^pmid-32504360] It is
[associated with](/knowledge/disease/covid-19.md) severe disease.

[^pmid-32504360]: Elevated IL-6 and severe COVID-19
```

...and the **source node** that `primary_source` joins to, which BioOKF mode must materialize as a
real page (`knowledge/publication/chen-2020-il6.md`) — not as a `sources[]` entry, which is a YAML
mapping with no `type` and is not traversable:

```yaml
---
type: Publication
identifier: Chen 2020 (IL-6 and severe COVID-19)
xref: [PMID:32504360]
raw_source: [raw/pmid-32504360/original.pdf]     # anchors the chain to immutable bytes
---
```

**Reserved files** — `index.md` and `log.md` carry no `type` and are not concepts. **`schema.md`
carries `type: Schema`** — see DR-23; the earlier claim that "OKF is silent on it, which makes it
legal" was wrong.

## 3. Decision records

### DR-1 — Target OKF v0.2, not v0.1

BioOKF v0.5's comparison table is written against OKF **v0.1**; upstream OKF is at **v0.2** (commit
`3fcbb9f8`, 2026-07-24). v0.2 adds four frontmatter families (`sources`, `generated`/`verified`,
`status`/`stale_after`, Attested Computation) and makes two breaking changes: `timestamp` →
`generated.at`, and the body `# Citations` list → frontmatter `sources`.

**Decision:** target v0.2. Emit `generated.at`, not `timestamp`; emit `sources`, not a `# Citations`
list. Accept both on read.

**Why:** v0.2's provenance/trust layer is exactly what a biomedical KB needs and what BioRouter
already computes but throws away. Pinning to v0.1 for BioOKF parity would mean writing a format that
is already superseded, and BioOKF's own additions are orthogonal to everything v0.2 added.

**Consequence:** where BioOKF v0.5 and OKF v0.2 both specify provenance, we carry **both** (DR-4).

### DR-2 — Three link forms are read; one is written per profile

There are three link grammars in play and they are not interchangeable:

| Form | Source | Meaning |
| --- | --- | --- |
| `[label](/path/to/concept.md)` | OKF v0.2 §6.1 | untyped directed edge |
| `edges:` frontmatter entry | BioOKF v0.5 §6 | typed, attributed edge |
| `[[wiki-link]]` | BioRouter today | untyped directed edge |

**Decision:** the deriver reads **all three**, permanently. Writers emit markdown links (OKF) plus
`edges:` (BioOKF mode). `[[…]]` is never emitted by new code but is read forever.

**Why:** OKF has no wikilink syntax at all — markdown links are the carrier, and they render as real
navigation on GitHub, in Obsidian and in BioRouter's own artifact preview, where `[[…]]` renders as
literal brackets. But every existing base is `[[…]]`-only, and CLAUDE.md already records the failure
mode this creates: *"If a graph has nodes but no edges, the underlying pages likely lack `[[…]]`
cross-references."* Dropping the legacy reader would reproduce that bug on every base on disk.

**Rejected:** a one-shot rewriter that converts `[[…]]` to markdown links. It cannot resolve a link
whose target page does not exist yet, which OKF explicitly says is legal.

### DR-3 — `br_page_id` gives a page identity that survives renames

Today node identity is `slug(basename).to_lowercase()` (`graph.rs:129`). A rename breaks every
inbound edge, two pages with the same basename in different directories silently merge, and
case-folding destroys gene-symbol distinctions (`IL6` vs `Il6`).

**Decision:** stamp an immutable `br_page_id` (ULID) into frontmatter on first write. Resolve edges
against, in order: `br_page_id` → `identifier` → `title` → slugified basename.

**Why:** OKF §4.1 explicitly permits producer keys and §11 forbids consumers from rejecting them, so
this costs nothing in conformance. The resolution ladder means no existing base has to be rewritten
to keep working.

### DR-4 — Two provenance models, carried together, neither converted

OKF v0.2 attributes a **claim** with a markdown footnote `[^id]` keyed to a `sources[].id`. BioOKF
v0.5 attributes an **edge** with `primary_source` naming a source *node* by `identifier`.

**Decision:** carry both. `sources[]` is the document-level record of what this page was built from
(and is what a foreign OKF consumer reads); `edges[].primary_source` is the per-claim originating
source in BioOKF mode. A source node's `raw_source` anchors it to `raw/…`.

**Why:** they answer different questions and neither subsumes the other. Converting one into the
other would lose information in both directions — footnotes attribute prose claims that are not
edges, and `primary_source` attributes edges that appear in no prose.

### DR-5 — Credibility: emit signals, keep the verdict as an extension

OKF §5.1 is explicit: *"It does not store a credibility score: a score is subjective, unportable
across consumers, and goes stale."* BioRouter stores exactly that — `Credibility { tier, confidence,
reasoning, classifier_version }`.

**Decision:** emit the objective signals OKF asks for (`sources[].author`, `.last_modified`,
`.resource` carrying the DOI/PMID) **and** keep the computed verdict under a `br_credibility` key.
`retracted` is argued into BioOKF as a first-class signal, because a retraction is a fact, not a
judgement.

**Why:** an exported bundle then carries its evidence rather than its opinion, which is the right
property for cross-institution exchange — while BioRouter's own UI keeps the tier colouring it
already has.

### DR-6 — The profile lives in `manifest.yaml`, gated by `schema_version`

`Manifest.schema_version` exists, is always `1`, and is read by nothing.

**Decision:** add `format: okf | biookf` and `okf_version` / `biookf_version` to the manifest; bump
`schema_version` to **`3`**. A base below that is read through its own generation's path, unchanged,
until the user migrates it.

⚠ **Not `2` — that number was already taken.** Stage 1.5's S-g wired `schema_version` for the first
time and had to give the *existing* content a generation number: every base on disk is stamped `1`
while already carrying the cross-reference-rules schema, so `CURRENT_SCHEMA_VERSION = 2` now names
**that** generation, not OKF. An earlier draft of this record said "bump to 2", which would have
declared every existing base already-OKF and skipped its migration silently. The OKF schema is
generation **3**.

**Why (and the trap):** `tier.rs:55-74` records the counter-precedent — a reader that *refused* an
unfamiliar schema number locked users out of everything. So the version **selects a code path and
never rejects a base**. The `okf_version` also goes into the bundle-root `index.md` frontmatter,
which is the only place OKF permits frontmatter in an index file.

### DR-7 — Validation warns, never rejects

OKF §11 gives consumers five MUST-NOT-REJECT tolerances: missing optional fields, unknown types,
unknown keys, broken links, missing index files. BioOKF §11 asks a strict consumer to **flag**
unknowns rather than silently accept them.

**Decision:** one validator, two severities. In OKF mode an unknown type is not even a warning; in
BioOKF mode it is a lint warning naming the closest legal value. **Nothing anywhere rejects a page
on read.** `kb_write_page` may refuse a write in BioOKF mode with an actionable message, because a
write is a producer action and producers are held to a higher bar than consumers.

### DR-8 — Every new write tool joins `KB_RATCHETING_TOOLS`

`KB_RATCHETING_TOOLS = ["kb_write_page", "kb_add_raw_source", "kb_append_log"]` (`server.rs:102`) is
what makes a base inherit a private session's privacy tier.

**Decision — and it is two surfaces, not one.** The review found a second, completely independent
implementation of the same tool names that neither list can see:

1. **The MCP surface.** Every new tool naming a base joins `KB_ID_GATED_TOOLS` (`server.rs:74`); every
   mutating one also joins `KB_RATCHETING_TOOLS` (`server.rs:103`).
2. **The sub-agent surface.** `KbToolDispatch::call` (`subagent/kb_tools.rs:31-133`) dispatches
   `kb_write_page` / `kb_append_log` / `kb_add_raw_source` straight to `store::*`, and `tool_specs()`
   is the table the model actually sees. Neither passes through `KnowledgeServer::call_tool`, so
   neither `gated_kb_id` nor the ratchet runs, and
   `every_tool_the_router_exposes_is_classified_by_the_probe_table` cannot see it — it enumerates
   `KnowledgeServer::tool_router()`.

That second surface is safe **today** only because the three macros pre-ratchet at their entry *and*
`KbToolDispatch` never accepts a `kb_id` argument, so it can only ever touch the base the macro
already cleared.

**Therefore the load-bearing invariant is: `KbToolDispatch` must never accept a `kb_id`.** It is
pinned by a test, because it is the only thing making the macro-entry barrier sufficient. A new
sub-agent tool that took a base id would read a private base from a public session with no gate
anywhere on the path.

**Why:** a new OKF write tool missing from either surface lets a private model write content into a
base that stays PUBLIC. That is a privacy hole, not a bookkeeping miss.

**Corollary — a tool that writes *sometimes* is not classifiable, so it is split.** `kb_lint` names a
base, so it joins `KB_ID_GATED_TOOLS`. It does **not** join `KB_RATCHETING_TOOLS`, and the reason is
the shape of the table rather than a judgement about lint: both lists hold tool NAMES, and
`macros::lint::lint` has two halves — a scan that writes nothing and an autofix that rewrites pages.
"Ratchets when `autofix=true`" is unsayable in a list of names, and a row that is true half the time
is a row a reader has to open the tool to understand. So the MCP tool exposes `macros::lint::scan`
**only** — gated, not ratcheting, exactly `kb_validate_page`'s classification and for the same reason
(a permanent tier raise bought by a caller who merely looked). The autofix stays on the two surfaces
that name a provider and therefore have a tier to ratchet with: `biorouter kb lint --fix` and
`POST /knowledge/bases/{id}/lint`, both of which go through `macros::lint::lint`, which ratchets at
its own entry.

Read the general rule off that: when a candidate tool's ratchet decision depends on an argument,
narrow the tool until the decision is a constant, rather than widening the table until it can express
the ambiguity.

### DR-9 — Port the BioOKF *aesthetic*; do not port a GPU library, because there is not one

The brief asked to borrow BioOKF Studio's "ultra-fast graph rendering libraries". There are none:
`app/studio/dist/app.js` is a dependency-free vanilla-JS **Canvas 2D** renderer. What makes it fast
is technique — Barnes-Hut quadtree (θ=0.9), a 20-iteration warm start, viewport culling,
energy-based auto-settle, and a pre-indexed neighbour map.

**Decision:** keep `react-force-graph-2d` (which already has d3-force's Barnes-Hut) and port the
*techniques and the visual language*: the 28-type palette, tapered edges (0.85 → 0.42), dashed-red
`not_<X>` styling with struck-through labels, focus glow, priority-ranked collision-avoiding labels,
and the density/LOD formulas.

**The performance work, corrected.** An earlier draft of this record claimed the win was that
BioRouter never sets `nodePointerAreaPaint`, so force-graph's shadow-canvas hit-test re-runs the full
label painter per node. **That is wrong, and acting on it would have made things worse.** Measured
against the installed `force-graph@1.51.4`: in `dist/force-graph.mjs` the props split into
`bindFG = linkKapsule('forceGraph', …)` and `bindBoth = linkKapsule(['forceGraph','shadowGraph'], …)`,
and `nodeCanvasObject` / `nodeCanvasObjectMode` / `linkCanvasObject` are **all in `bindFG`**.
`bindBoth` carries only `nodeRelSize, nodeId, nodeVal, nodeVisibility, linkSource, linkTarget,
linkVisibility, linkCurvature`. The shadow graph is built with `.nodeColor('__indexColor')` and no
canvas object, so it paints plain arcs. There is no duplicate label pass to remove — and adding a
`nodePointerAreaPaint` would have *introduced* a second per-node painter where none existed.

The real per-frame cost is on the **visible** canvas: `ForceGraphCanvas.tsx` calls `prettyLabel()`
(five regex passes, two with mutating `lastIndex`) and `wrapLabel()` (one `ctx.measureText` per word,
plus a per-character shrink loop when ellipsising) for every labelled node on every frame — and at
`globalScale >= 1.75` every node is labelled. **Memoise the label computation per node**, and adopt
the density/LOD ladder so most nodes are not labelled at all.

**Why not sigma.js/cosmos.gl now:** measured empirically against BioRouter's renderer CSP
(`main.ts:4815`), `new Function` throws `EvalError` (killing ngraph) and Blob-URL workers are blocked
(killing `FA2LayoutSupervisor`; its synchronous `assign()` is fine). `@cosmograph/cosmos` also
reverted to **CC-BY-NC-4.0** at v3.4.0 — non-commercial only, unusable for a UCSF-distributed app;
only the OpenJS fork `@cosmos.gl/graph` is MIT, and the two share version numbers, so a copy-pasted
import silently pulls the wrong one. sigma v3 also opens three WebGL contexts per instance against
Chromium's ~16 cap, and BioRouter has multi-pane chat groups plus an artifact panel; Chromium's
failure mode is killing the *oldest* context, so an unrelated working graph goes blank. A renderer
swap is a recorded future option gated on measured graph sizes, not taken now.

### DR-9b — Type is the fill, credibility is the ring

`nodeFill()` in `credColors.ts` is today the **only** at-a-glance credibility channel, consumed by
the canvas, the inspector header dot and the legend. Colouring by node type would silently delete a
signal users have — and would contradict DR-5, which keeps the credibility verdict expressly so the
UI can keep showing it.

**Decision:** fill carries the node **type**; the existing node **ring** carries credibility.
`ForceGraphCanvas.tsx` already strokes every node, so on source-bearing nodes the stroke becomes the
credibility hue at a heavier width and every other node keeps the neutral ring. One line, no extra
fill-rate, both signals visible at once. Be honest about the fidelity: a 1.6px ring reads as
high-versus-low, not as six distinguishable tiers — the exact tier stays in the inspector and the
legend.

### DR-10 — Theme tokens, not the hex values

BioOKF Studio hardcodes `#1c2128` label ink and an `rgba(250,250,252,0.95)` halo — light-theme
values. BioRouter's `graphStyle.ts` already solved this correctly by *resolving* computed styles
rather than naming colours, with a comment recording why (the canvas cannot parse `var(--…)`).

**Decision:** the 28-type palette ships as authored hues, but every structural colour (ink, halo,
ring, ground) resolves from theme tokens. Node hues get a light/dark pair so they hold contrast in
all three theme families.

### DR-11 — Arbitrary OKF types get a deterministic colour

OKF mode allows any `type` string, so the palette cannot be a lookup table alone.

**Decision:** `TYPE_COLOR[type]` when the type is one of the 28; otherwise a deterministic hue from
an FNV-1a hash of the type string, at fixed saturation/lightness drawn from theme tokens. Same type
string ⇒ same colour, every session, every base.

## Related documentation

- [`stages.md`](stages.md) — the stepwise implementation plan.
- [`progress.md`](progress.md) — live progress.
- [`../../security/privacy-tiers.md`](../../security/privacy-tiers.md) — the ratchet DR-8 protects.

## 4. Decision records from the risk review

An adversarial review of the plan against the code found fourteen risks, two of them silent
data-loss. These records are the mitigations, and several of them **must land before** the stage
that would otherwise trip them. The re-sequencing is in [`stages.md`](stages.md) as **Stage 1.5,
Seams**.

### DR-12 — Every new `Manifest` field is `#[serde(default)]`, and a broken manifest is loud

`manifest::load` is a bare `serde_yaml::from_str` with no leniency — and `schema_version` does not
carry `#[serde(default)]` either. Adding one non-defaulted field fails the load for every
`manifest.yaml` on disk. The cascade from there is entirely silent and ends in **persisted data
loss**:

1. `list_bases` swallows the error — `if let Ok(m) = manifest::load(…)` (`service.rs:748`) — so the
   base does not report as broken, it *vanishes*.
2. `installed_kb_ids_unlocked` is built from `list_bases`, so the id leaves the installed universe.
3. `repair_decision` sees a stored primary that is not installed and returns `no_primary_for(…)`.
4. `apply_selection_unlocked` **writes the cleared pointer to disk** (`service.rs:1891`).

The user's `.active-kb` and every per-session pointer are destroyed, and downgrading does not
restore them. The trigger is the first thing a confused user does when their bases disappear: toggle
something — and `POST /knowledge/active` with a set-only edit reaches exactly that path.

**Decision:** (a) every added field carries `#[serde(default)]`, including a backfill default on the
existing `schema_version`; (b) `list_bases` surfaces an unreadable manifest instead of dropping it;
(c) a test asserts a v1 `manifest.yaml` with no `format` key still loads.

### DR-13 — Version the graph cache envelope *first*, and treat a stale cache as absent

`read_cache` is `Ok(Some(serde_json::from_str(&s)?))` (`graph.rs:114`) — a deserialize failure
**propagates as `Err`**, reaches `GET /knowledge/bases/{id}/graph`, and maps to a 404. Nothing on
that path ever rewrites the cache, so the Knowledge view shows a permanent error. The existing
self-heal is one hardcoded predicate (`n.id == "index" || n.id == "log"`) that detects exactly one
historical shape change and cannot detect a schema change.

The alternative failure is worse because it is quiet: give the new fields `#[serde(default)]` and
every existing cache deserializes, the self-heal stays false, and every pre-existing base serves
**typeless nodes forever** — the refactor appears to work and produces nothing.

**Decision:** land a cache-envelope `version` as its own change **before** any type change, and make
`read_cache` return `Ok(None)` (absent ⇒ re-derive) on both a parse failure and a version mismatch.
The version check subsumes and retires the scaffold-node predicate.

### DR-14 — One link parser, proven equivalent, before the grammar changes

The regex `\[\[([^\]]+)\]\]` is written three times in Rust — `graph.rs:11`, `macros/query.rs:230`,
`macros/lint.rs:61` — each followed by a *different* resolver, plus a fourth hand-rolled
`splitFrontmatter` in `NodePreview.tsx`. They already disagree: `[[knowledge/entities/x|X]]` is an
edge in the graph and an orphan in the lint, for the same page, with no test catching it.

Adding predicate-carrying links to one of them corrupts the others — `kb_query` would return the raw
string `treats:: COVID-19` to the user as a citation, and `kb_lint` would report a missing page for
every predicate token in the base.

**Decision:** collapse to one shared parser and resolver, with a test driving all three consumers
over one corpus and asserting they agree. This lands as a **standalone, behaviour-preserving change
before** the grammar is extended, so the equivalence test is meaningful.

### DR-15 — The `complete` sentinel must dispatch its siblings first

`loop_.rs:221` returns on seeing a `complete` tool call **before** pushing the assistant message and
before the dispatch loop, so every other call in that turn is discarded undispatched. Today that is
rare because the procedure ends with a separate `complete()` step. Under typed extraction the
natural shape is N assertion writes followed by `complete` in one turn — exactly the losing case.

The consequence is not a visible error: nothing under `knowledge/` changed, so
`txn_wrote_knowledge_pages` returns false, the txn aborts, and ingest bails with *"wrote no
knowledge pages"* — pointing the investigator at the model's authoring rather than at a dispatch bug.

**Decision:** dispatch the non-`complete` calls, then honour the sentinel. Six lines, landing
**before** any procedure rewrite, so the rewrite is not blamed for it.

### DR-16 — The vocabulary is declared in the tool schema, not in prose

`make_schema` (`subagent/kb_tools.rs:204`) emits only `{"type": T}` per property: no `description`,
no `enum`, no nesting. A closed vocabulary is therefore **unenforceable through the tool interface**
— the model gets no machine-readable list, the provider cannot constrain sampling, and an invalid
predicate is caught only at dispatch as free text. A failed call is fed back as `error: …` and does
not abort, so the model retries, burning steps against `max_steps` until the budget dies.

**Decision:** replace `make_schema` with real JSON Schema so `type` and `predicate` are
`enum`-constrained at the provider. This kills most of the prompt bloat *and* makes the vocabulary
enforceable — two problems, one fix. Dispatch errors name the closest legal value.

**Corollary (DR-16b):** `SubAgentBounds.max_tokens` is declared and never read, there is no
compaction, and `max_wall` is only checked *between* iterations. Wire `max_tokens`, cap the message
history, and put a timeout around the provider await.

### DR-17 — Migration is lazy, gated, and never a model tool

The four privacy write choke points are `call_tool` for the three `KB_RATCHETING_TOOLS`, the three
macros, `routes/apps.rs`, and `create_base_as`/`import_brkb`. A format migration is none of them.
Three concrete bypasses: an eager startup migration has **no caller identity at all** and would
rewrite every private base's pages with nothing having called `tier::assert_reachable`; a
`kb_migrate_format` tool missing from the tables writes content without ratcheting and reads a
private base from a public session; and a conversion calling `tier::raise_unlocked` directly trips
the grep-count test *and* raises a tier without its paired affiliation raise — the exact hole closed
in Task 50.

**Decision:** migration is **lazy** (on first reach, after `assert_reachable`, inside `lock_kb`),
never eager at startup, and is **not exposed as a model tool**. A user-initiated migration goes
through the HTTP surface with the same proof-of-user discipline `tier_user` already uses.

### DR-18 — `.brkb` import and `kb_restore_state` become format-aware

`brkb::import` extracts blindly; the in-archive `.brkb-provenance` marker is read as a tier floor and
discarded, and `Provenance.schema` is documented as a label with no reader — a version field that is
present and inert reads as protection but is not. `kb_restore_state` reproduces an old tree as a new
commit with no format awareness, so restoring across the migration reverts `schema.md` — re-teaching
the sub-agent the old format — and every page body.

**Decision:** `.brkb-provenance` gains a real `format` reader; import refuses with a repair message
rather than partial-extracting; a restore that crosses the migration commit re-runs the migration.

### DR-19 — Fixtures emit conformant pages by construction

`KB_TOOL_PROBES` sends `"content": "body"` for `kb_write_page` — a bare five-byte string with no
frontmatter — and the same shape recurs in the conversation-ingest fixture and `knowledge_routes.rs`.
A validating writer fails all of them, with a message about frontmatter under a test name about
tiers. The natural response is to loosen the validator, which defeats the change.

**Decision:** land a `valid_page(type, title, body)` fixture helper **before** the validator, so
fixtures track the format by construction and the validator change touches one helper rather than
twenty call sites.

### DR-20 — The grep-count invariant tests are re-derived, never re-numbered

Six tests pin privacy choke points by reading their own source and counting occurrences:
`the_tier_ratchet_has_no_production_call_site_that_skips_the_affiliation` (exactly 2),
`can_reach_is_assert_reachable_negated_and_nothing_else`,
`this_file_asks_the_barrier_and_never_re_spells_it`,
`exactly_one_writer_outside_the_ratchet_saves_the_tier_store`,
`the_proof_of_user_is_constructed_in_exactly_one_place`, and
`every_kb_tool_is_gated_or_exempt_for_a_pinned_reason`.

Moving code fails them even when behaviour is unchanged.

**Decision:** when one fails, the fix is to route the new call through the existing funnel
(`stamp_base_unlocked` / `raise_tier_and_affiliation`) — which is what the failure messages
themselves say. Bumping the count requires an explicit note in the commit body saying why the new
site is safe.

### DR-21 — A plain-OKF bundle export would be a second, ungated transfer door

The design repeatedly frames OKF as the format for "cross-institution exchange". Today the **only**
transfer path carries provenance: `brkb::export` writes a `.brkb-provenance` entry with tier and
owners, `import` reads it as a floor and a union, and `export_brkb` refuses to package a base whose
owners it cannot establish. A plain OKF-bundle export has no such marker — and re-importing one
would land an **unclaimed** base floored only at the importer's own tier. That is exactly the
laundering path `Provenance::owners` was added to close: export a base you legitimately own, and any
other institution's model can then reach it.

**Decision:** there is **no plain-OKF export path in this work.** `.brkb` remains the only transfer
door, and it keeps its provenance sidecar. The OKF-conformance win is that a `.brkb` *contains* a
conformant bundle — so unzipping one by hand yields something Obsidian, GitHub or another OKF
consumer can read — but BioRouter's own import stays gated.

If a first-class OKF export is wanted later, it must carry the same sidecar and reuse
`export_brkb`'s unreadable-store refusal, and its importer must apply the same `max(marker,
importer)` floor and owner union. A format-conversion feature must not become a transfer feature by
accident.

### DR-22 — What ships today, and what is explicitly deferred

The nine-stage plan is not one day of work. `crates/biorouter-mcp/src/knowledge/` alone is 20,560
lines of Rust; `routes/knowledge.rs` is another 2,142; the baselines to keep green are 283 + 46 Rust
tests and ~91 desktop cases; and this worktree has no `ui/desktop/node_modules` and only a debug
`target/`, so there is an unbudgeted cold `npm ci` and a full build before the UI stages can even
start.

**The day's coherence test:** *a user can create a knowledge base in either format, ingest into it,
and see typed structure in the graph.* Everything needed for that ships; everything else is deferred
with a stated reason.

**Deferred, each for a reason:**

- **DR-3 `br_page_id`** — it is the only change that rewrites frontmatter on *existing* pages, and
  the DR-3 resolution ladder means edges keep working without it. Ship the ladder, defer the stamp.
- **`kb_migrate_format`** — DR-6 already guarantees legacy bases keep working untouched, so
  migration is a convenience, not a prerequisite. Deferring it also defers the whole of DR-17's
  choke-point risk.
- **Attested Computations** — a genuinely good fit for reproducible analysis (DR-1), and entirely
  separable. Not needed to create, ingest, or render.
- **A renderer swap** — DR-9 already defers it, gated on measured graph sizes.

Deferred items keep their decision records so the reasoning is not lost.


## 5. Decision records from the spec-fidelity review

### DR-23 — `schema.md` is a typed concept document, not an untyped third reserved file

An earlier draft asserted that `schema.md` is legal untyped because "OKF is silent on it". **OKF is
not silent.** §3.1 reserves exactly two filenames and then states: *"All other `.md` files are
concept documents."* Conformance rules 1 and 2 (§11) require every non-reserved `.md` to carry a
parseable frontmatter block with a non-empty `type`. An untyped `schema.md` is therefore a
**conformance failure**, not an extension.

BioOKF itself has this problem — its §3 reserves a third file, `SCHEMA.md`, as *"not concept
documents and carry no `type`"*, which is a genuine deviation from its parent spec and the reason
"every conformant BioOKF bundle is also a conformant OKF bundle" holds against v0.1 but **not**
against v0.2.

**Decision:** BioRouter is stricter than BioOKF here, because being stricter costs nothing. Our
`schema.md` gets frontmatter `type: Schema` plus a `title` and `description`. It is then a perfectly
ordinary concept document, the bundle passes OKF §11 rules 1 and 2 with no carve-out, and the graph
deriver simply skips it as a scaffold page exactly as it skips `index.md` and `log.md` today.

**Corollary:** `biookf_version` does not go in the root `index.md`. OKF §8 permits `okf_version` and
nothing else in the one place it allows index frontmatter, so `biookf_version` lives in
`manifest.yaml` beside `format`.

### DR-24 — BioOKF provenance has *two* mechanisms, and DR-4 carried only one

SPEC §8.1 is explicit that provenance is carried by **exactly two** things, both naming source nodes
by `identifier`: the required per-edge `primary_source`, *and* the `reported_in` edge — the explicit,
traversable link from a concept to its source node. v0.5 dropped the node-level `provided_by`
precisely because a `reported_in` edge already says it.

DR-4 described only `primary_source`. **Decision:** BioOKF-mode ingest emits both — a `reported_in`
edge from each concept page to the source node it came from, and `primary_source` on every edge. A
`reported_in` edge carries its own `primary_source`, and by convention that is the edge's **own
object** (a source attests its own contents). That self-reference is the intended terminating base
case, **not** a lint error, and the lint must special-case it or it will flag every source in the
base.

### DR-25 — One relationship, one edge: the deriver deduplicates across grammars

BioOKF §4 is unambiguous: *"Only `edges:` entries are part of the graph."* A page may legally restate
an edge in prose and may use advisory links for navigation. So a page that has both an `edges:` entry
and a markdown link to the same target asserts **one** relationship, not two — but DR-2 reads all
three grammars and nothing in Stage 2 deduplicates them.

**Decision:** the deriver reduces to a set keyed on `(from, to, predicate)`. A typed `edges:` entry
**wins** over an untyped link to the same target — the untyped one is absorbed rather than emitted as
a second, predicate-less edge beside it. In OKF mode, where there are no predicates, links to the
same target collapse to one edge. Without this a BioOKF page renders every relationship twice: once
typed, once grey.

## 6. Decisions forced by the Stage 2/3 gate

### DR-26 — An existing base has no path to OKF in this release, and that is deliberate

The Stage 2/3 gate measured it: `CURRENT_SCHEMA_VERSION` is 3, but `AUTOMATIC_SCHEMA_CEILING` is 2,
so the automatic ladder stops one generation below OKF — and `kb_migrate_format` was deliberately not
built (DR-17, DR-22). A base stamped generation 2 therefore does not migrate, by **any** path.

**Decision:** this is the intended behaviour for this release, not an oversight, and DR-6's phrase
"until the user migrates it" overstated what ships. **New** bases are OKF or BioOKF; **existing**
bases keep working unchanged on their own generation, with their own schema, their own graph and no
loss of function. What they do not get is typed nodes and edges.

**Why not just raise the ceiling:** rewriting every page of an existing base is exactly the operation
DR-17 identified as a fifth privacy write choke point — one that bypasses all four that exist, and
that in its eager form has no caller identity at all, so nothing would have called
`tier::assert_reachable` before rewriting a private base. Shipping the format without shipping that
hazard is the right trade for one day of work. Migration is the first item of the next pass, and it
must be lazy, gated behind `assert_reachable` inside `lock_kb`, and never a model-facing tool.

### DR-27 — The quantitative bundle is an open map, not a fixed field list

Stage 2 emitted six flat statistical fields (`effect_metric`, `effect_size`, `ci_lower`, `ci_upper`,
`p_value`, `sample_size`) because that is what its task named. The committee-reviewed UI spec §2.1
instead specifies an open `quantitative` map, *"so a vocabulary addition needs no renderer change"*.

**Decision:** the UI spec wins. BioOKF §7.3 lists around twenty quantitative slots —
`adjusted_p_value`, `standard_error`, `sensitivity`, `specificity`, `auc`, `frequency`,
`clinical_phase`, `response_direction`, `unit` and more — and six flat fields silently drop fourteen
of them. Stage 2 argued the rest survive in `qualifiers`, but that conflates two different things:
`qualifiers` is BioOKF's **context** map (`species_context`, `sex`, `age_group`, `timepoint`), and
putting a p-value in it is a category error that a renderer cannot undo.

**Also adopted from §2.1:** `GraphEdge.synthesized` (the faint dashed provenance treatment) and
`GraphNode.degree` (hub sizing). Both are consumed by the renderer and neither can be derived
cheaply in the client.

### DR-28 — Typed fields stay `Option`; the UI spec is wrong here, not the code

UI spec §2.1 draws `node_type`, `identifier` and `predicate` as **required** strings. Stage 2 made
them `Option` and was right to: a legacy page genuinely has no `type`, and a required field would
have to be filled with a fabricated value — which is worse than an absent one, because a consumer
cannot tell an invented `Concept` from a declared one.

**Decision:** they stay `Option`. The UI spec is amended, and the renderer treats absence as "untyped
legacy page" — which is a real state that needs its own rendering, not an error.

**Related:** no new field takes `#[schema(required)]`. The `Manifest` pattern pairs `serde(default)`
with `schema(required)` because the server always serializes those fields, so the default describes
only the read side. Here the defaults describe genuinely absent data, so `required` would be a false
statement about the response and the generated TypeScript would be wrong at **runtime** rather than
at compile time.

## 7. Decisions forced by KB-to-KB merge

### DR-29 — A merge ships its deterministic half only, and the split is where the risk is

`.brkb` import always mints a **fresh** id (`brkb::import`'s collision loop is written to, so an
import can never re-tier an existing base). The consequence is a user-visible dead end: a
collaborator sends an archive, you import it, and you now own two bases describing one domain with
no path to one graph.

BioOKF's `biookf-merge` skill has two halves. The **mechanical** half — dedup a raw source by
content hash, rename on collision, rewrite every reference to what was renamed, carry over what does
not collide — is deterministic and testable. The **judgement** half — deciding that the incoming
`IL-6` and the destination's `IL6` are the same concept, collapsing them, harmonising prose and
subtype names — is an LLM loop.

**Decision:** ship the mechanical half as `crates/biorouter-mcp/src/knowledge/merge.rs`. An
identifier that exists in both bases is **not collapsed**; the incoming one is renamed and every
reference to it (edge `object`, edge `primary_source`, `raw_source` paths, and every body link in
all three grammars) is repointed. The judgement half is a macro, and belongs on the foundation this
one lays.

**Why that direction:** a wrong collapse destroys a curated page and the user has no way to know it
happened. A wrong rename leaves two pages and a rename record — visible, and reversible by hand.

**Two tools, not one with a `dry_run` flag** (DR-8's corollary, applied): `KB_RATCHETING_TOOLS` is a
set of tool NAMES, so "ratchets when `dry_run` is false" is unsayable in it. `kb_merge_preview` is
gated and does not ratchet; `kb_merge` is gated and does. A single tool would permanently privatise
a public base because a private chat *looked at what a merge would do*.

### DR-30 — A merge is the fifth privacy write choke point, and it takes `import_brkb`'s rule

DR-17 named four write choke points and warned that a fifth would bypass all of them. A merge is one:
it is a content-touching write whose content comes from **another base**.

**Decision — two controls, in this order.**

1. **The barrier, over BOTH ids.** You cannot merge a base you cannot read into a base you cannot
   write. The preview takes it too: the report quotes the source's page paths and identifiers
   straight back to the caller, so a model barred from reading the source must be barred from
   previewing it. The destination is gated by `KB_ID_GATED_TOOLS` at the `call_tool` seam (it is the
   argument spelled `kb_id`); the source takes its own `assert_reachable` inside `merge_bases`,
   because one seam resolves one id.
2. **The fold, before a byte is written.** The destination takes `max` over the tier axis and the
   **union** over owning institutions — *exactly* the rule `service::import_brkb` applies to an
   incoming archive, reused rather than re-derived, because merging base A into base B is that
   transfer with the archive step removed. A merge can raise either axis and can never lower one.

The fold precedes the write for the same reason the `call_tool` ratchet does: a merge that fails
after it leaves the destination raised with no content added, which the user can see and undo with
the tier control, where the other order can leave a private source's content in a base that reads
PUBLIC.

**It adds no new ratchet call site.** `absorb_classification` routes through
`stamp_base_unlocked` — DR-20's instruction applied rather than its number bumped — so
`the_tier_ratchet_has_no_production_call_site_that_skips_the_affiliation` still reads 2. What *did*
move is the master-switch read: `tier::ratchets_are_live` is now the one spelling both ratchets use,
because the dry run is a third **reader** of that decision and a preview that disagrees with the
write it previews is the specific failure a preview exists to prevent.

**The HTTP route carries no caller barrier**, and that is `GET /bases/{id}/export`'s position, not a
new one: it is the user's own path, they can already read both bases from the Knowledge view, and
DR-14 governs what a *model* can reach. What separates that branch from the tool channel is a
proof-of-user — `merge::UserKbMerge`, a ZST with a private field, minted in exactly one place and
deliberately a **separate type** from `tier_user::UserKbTierChange` so one proof is not spendable on
the other subject.

### DR-31 — The merge copies; it does not move, and it is one transaction

`bokf-core::merge_raw` relocates the secondary's `raw/` with `fs::rename`.

**Decision:** copy. The whole merge is one `git::Txn` on the **destination's** repository, and the
source has its own repository that is not in that transaction — so a move could not be rolled back
and the atomicity promise would be a lie. A BioRouter source base is also a first-class object with
a registry entry, a tier entry, session pointers and a history; emptying its `raw/` would leave every
one of *its* pages' `raw_source` dangling, and deleting a base is a separate user-initiated action.

**Atomicity is `abort_txn` plus an explicit undo list, and both are needed.** A page written and not
yet committed on the transaction branch is *untracked*, and a copied `raw/<id>/original.pdf` is
*gitignored* (`raw/*/original.*`) — neither is reachable by any checkout, however forceful. Deleting
the undo list is measurable: `a_failure_mid_merge_leaves_the_destination_byte_identical` goes red
with both the copied source and two written pages left behind.

**"The destination stayed canonical" is checked twice, and the two are different questions.**
`plan_violations` asks *would this plan write over the destination* — the only one a **dry run** can
answer, since a post-merge comparison there is vacuously green. `verify_snapshot` asks *did the write
do what the plan said*, and runs before the squash commit so a violation aborts. The snapshot carries
three sets rather than the reference's one: identifiers, page paths (a legacy or plain-OKF page may
declare no identifier at all) and raw ids (where a merge does most of its moving).
