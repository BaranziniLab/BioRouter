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
identifier: Interleukin-6         # bundle-unique human-readable key (BioOKF; recommended in OKF)
title: Interleukin-6 (IL6)        # OKF display name
description: A pleiotropic pro-inflammatory cytokine.
subtype: protein                  # agent-coined, never validated
tags: [cytokine, inflammation]
xref: [UniProtKB:P05231, HGNC:6018]
status: stable                    # draft | stable | deprecated  (OKF §5.4)
stale_after: 2027-01-01           # absolute date (OKF §5.5)
generated: { by: biorouter/claude-opus-5, at: 2026-08-19T12:00:00Z }   # OKF §5.2
verified:
  - { by: "human:wgu", at: 2026-08-19T13:00:00Z }
sources:                          # OKF §5.1 — provenance with objective signals
  - id: pmid-32504360
    resource: raw/pmid-32504360/original.pdf
    title: Elevated IL-6 and severe COVID-19
    author: "human:Chen et al."
    last_modified: 2020-06-01
edges:                            # BioOKF §6 typed graph layer; permitted (ignored) in OKF mode
  - predicate: associated_with
    object: COVID-19
    knowledge_level: statistical_association
    agent_type: text_mining_agent
    primary_source: Elevated IL-6 and severe COVID-19
    effect_metric: hazard_ratio
    effect_size: 2.9
    p_value: 3.0e-6
br_page_id: 01J8X...              # BioRouter producer extension: identity across renames
---

# Interleukin-6 (IL6)

A pleiotropic pro-inflammatory cytokine.[^pmid-32504360] It is [associated with](/knowledge/disease/covid-19.md) severe disease.

[^pmid-32504360]: Elevated IL-6 and severe COVID-19
```

**Reserved files** — `index.md` and `log.md` carry no `type` and are not concepts. `schema.md` is a
BioRouter/BioOKF producer extension (OKF is silent on it, which makes it legal).

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
`schema_version` to `2`. A base with `schema_version: 1` is a **legacy wiki base** and is read
through the legacy path, unchanged, until the user migrates it.

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

**Decision:** any new mutating tool is added there in the same commit that introduces it, and the
repo-grep test that counts these sites is updated deliberately, never mechanically.

**Why:** a new OKF write tool that is not in that list lets a private model write content into a base
that stays PUBLIC. That is a privacy hole, not a bookkeeping miss.

### DR-9 — Port the BioOKF *aesthetic*; do not port a GPU library, because there is not one

The user's brief asked to borrow BioOKF Studio's "ultra-fast graph rendering libraries". There are
none: `app/studio/dist/app.js` is a dependency-free vanilla-JS **Canvas 2D** renderer. What makes it
fast is technique — Barnes-Hut quadtree (θ=0.9), a 20-iteration warm start, viewport culling,
energy-based auto-settle, and a pre-indexed neighbour map.

**Decision:** keep `react-force-graph-2d` (which already has d3-force's Barnes-Hut) and port the
*techniques and the visual language*: the 28-type palette, tapered edges (0.85 → 0.42), dashed-red
`not_<X>` styling with struck-through labels, focus glow, priority-ranked collision-avoiding labels,
and the density/LOD formulas. Additionally fix the shadow-canvas hit-test that currently re-runs the
full label painter for every node on every hover.

**Why not sigma.js/cosmos.gl now:** measured empirically against BioRouter's renderer CSP
(`main.ts:4816`), `new Function` throws `EvalError` (killing ngraph) and Blob-URL workers are blocked
(killing `FA2LayoutSupervisor`). `@cosmograph/cosmos` also reverted to **CC-BY-NC-4.0** at v3.4.0 —
non-commercial only, unusable for a UCSF-distributed app; only the OpenJS fork `@cosmos.gl/graph` is
MIT. A renderer swap is a recorded future option (sigma.js v3 + graphology, synchronous
`forceAtlas2.assign` only), gated on measured graph sizes, not taken now.

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
