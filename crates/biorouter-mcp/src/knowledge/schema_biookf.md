---
type: Schema
title: Maintenance schema (BioOKF v0.5)
description: How this knowledge base is written and maintained, in the BioOKF v0.5 profile over OKF v0.2.
sources:
  - id: biookf-v0-5
    resource: https://github.com/biookf/biookf/blob/main/SPEC.md
    title: BioOKF v0.5
---

# Knowledge base — maintenance schema (BioOKF v0.5)

This document tells you how to maintain *this particular* knowledge base. It is
read fresh on every macro call. Edit it freely to shape the knowledge folder's
voice, structure and conventions — but keep the frontmatter block above, and
keep the page contract below, because the graph is derived from it.

This base is written in the **BioOKF v0.5 profile over OKF v0.2**. Everything an
OKF bundle allows is still allowed; BioOKF only adds constraints. Three of them
matter on every page:

- `type` is one of **28** controlled values, not free text.
- an edge's `predicate` is one of **35** controlled values.
- every edge carries a provenance triplet: `knowledge_level`, `agent_type` and
  `primary_source`.

Unknown values are still *read* — nothing rejects a page — but `kb_lint` flags
them, and a base full of flagged values is not exchangeable, which is the whole
reason to be in this profile.

## Layout

- `raw/<source-id>/` — original files plus a derived `source.md` and
  `meta.yaml`. **Read-only.**
- `knowledge/<type>/<slug>.md` — one file per node, in a directory named after
  its lowercased `type`: `knowledge/molecule/interleukin-6.md`,
  `knowledge/disease/covid-19.md`. The four source-type directories exist to
  start you off; create the entity directories as you use their types.
- `index.md` — the bundle's catalog. You maintain it on every change.
- `log.md` — the change log. You append on every change.
- `schema.md` — this file.

Only `index.md` and `log.md` are reserved. **Every other `.md` file in the
bundle is a concept document and must carry frontmatter with a non-empty
`type`** — including this one.

## Node types (28)

{{NODE_TYPES}}

Only **Publication, Study, Dataset and Agent** may be cited as an edge's
`primary_source`. `Population`, `GeographicLocation`, `Concept` and `Other` are
context, not evidence.

## Page format

```yaml
---
type: Molecule                     # REQUIRED, one of the 28 above
identifier: Interleukin-6          # REQUIRED in this profile: the page's
                                   # primary key, human-readable AND unique in
                                   # this bundle. Other pages cite this string.
description: A pleiotropic pro-inflammatory cytokine.
subtype: protein                   # free text, never validated
tags: [cytokine, inflammation]
xref: [UniProtKB:P05231, HGNC:6018]
status: stable                     # draft | stable | deprecated
sources:
  - id: pmid-32504360
    resource: raw/pmid-32504360/original.pdf
    title: Elevated IL-6 and severe COVID-19
    author: "human:Chen et al."
    last_modified: 2020-06-01
edges:
  - predicate: associated_with
    object: COVID-19               # the TARGET node's `identifier`
    knowledge_level: statistical_association
    agent_type: text_mining_agent
    primary_source: Chen 2020 (IL-6 and severe COVID-19)
    effect_metric: hazard_ratio    # optional quantitative bundle
    effect_size: 2.9
    p_value: 3.0e-6
  - predicate: reported_in
    object: Chen 2020 (IL-6 and severe COVID-19)
    knowledge_level: knowledge_assertion
    agent_type: text_mining_agent
    primary_source: Chen 2020 (IL-6 and severe COVID-19)
---
```

`identifier` is the join key for the whole graph: `object` and `primary_source`
both name a node by its `identifier`, never by a file path. Two pages with the
same `identifier` are an error, and an `object` naming nothing is a dangling
edge.

## Edges

**Only `edges:` entries carry a predicate and provenance.** Bundle-local
markdown links and `[[…]]` links are recorded as graph edges too, but as
*untyped* ones — so a relationship you want typed, attributed and queryable goes
in `edges:`. A typed entry wins the tie-break against a prose link restating it.

Predicates:

{{PREDICATES}}

Every edge needs all three provenance keys:

- `knowledge_level` — one of: {{KNOWLEDGE_LEVELS}}
- `agent_type` — one of: {{AGENT_TYPES}}
- `primary_source` — the `identifier` of a Publication, Study, Dataset or Agent
  node that **exists in this bundle**. Materialise it as a real page; a
  `sources[]` entry is not a node and cannot be traversed.

## Sources are nodes, and they are linked twice

Provenance is carried by exactly two mechanisms, and a page that came from a
source needs both:

1. `primary_source` on every edge — which source attests *this* claim.
2. a `reported_in` edge from the page to that source node — the traversable
   link from a concept to where it was reported.

A source node's own `reported_in` edge cites **itself** as its
`primary_source`. That self-reference is the intended terminating case, not a
mistake.

A source node anchors the chain to immutable bytes:

```yaml
---
type: Publication
identifier: Chen 2020 (IL-6 and severe COVID-19)
xref: [PMID:32504360]
raw_source: [raw/pmid-32504360/original.pdf]
---
```

## Negation

Eleven predicates are negatable: prefix them with `not_` (`not_treats`,
`not_associated_with`). A negated edge is a claim in its own right and needs the
same provenance triplet. Never assert `<X>` and `not_<X>` between the same two
nodes.

## index.md

Sections are `#` headings; entries are one bullet each:

```markdown
# Molecules

* [Interleukin-6](knowledge/molecule/interleukin-6.md) - A pleiotropic pro-inflammatory cytokine.

# Publications

* [Chen 2020](knowledge/publication/chen-2020-il6.md) - IL-6 and severe COVID-19.
```

The bundle-root `index.md` carries an `okf_version` key in its frontmatter and
**nothing else** — not `biookf_version`, which lives in `manifest.yaml`.

## log.md

Entries are grouped under `## YYYY-MM-DD` date headings, newest first, and the
kind of change is a leading bold word in the bullet:

```markdown
## 2026-08-19

* **Ingest** — Chen 2020 (IL-6 and severe COVID-19)
```

Append through `kb_append_log`, which maintains this shape for you.

## Ingest workflow

When `kb_ingest_source` is called:

1. Read `raw/<source-id>/source.md` and `meta.yaml`.
2. Create the **source node** first — `type` one of Publication / Study /
   Dataset / Agent, with `xref` and `raw_source` — because every edge you write
   next has to cite it.
3. Decide which of the 28 types each entity in the source is.
4. For each entity: update the page if it exists, otherwise create it. Give it a
   `reported_in` edge to the source node, and typed `edges:` for the
   relationships the source actually asserts.
5. Put the numbers on the edge, not only in the prose. Any §7.3 slot the source
   reports: `effect_metric`, `effect_size`, `ci_lower`, `ci_upper`, `p_value`,
   `adjusted_p_value`, `standard_error`, `sample_size`, `sensitivity`,
   `specificity`, `auc`, `frequency`, `clinical_phase`, `response_direction`,
   `unit`. Write the value the source gives, including `<0.001` — a number you
   cannot write exactly is still better recorded than dropped. Keep *context*
   (`species_context`, `sex`, `age_group`, `timepoint`) as its own attributes;
   it qualifies the claim rather than measuring it.
6. If the source contradicts an existing page, do not overwrite it — add an
   `## Open contradictions` section naming both positions and their sources, or
   assert the `not_<X>` edge with its own provenance.
7. Update `index.md` with new and changed pages.
8. Append a log entry through `kb_append_log` with `kind=ingest`.

## Credibility discipline

Peer-reviewed papers and books outweigh preprints, gray literature and web
posts. Reflect that in `knowledge_level`: a claim the authors assert is
`knowledge_assertion`, a correlation is `statistical_association`, a model
output is `prediction`. Never silently elevate one to another.

## Query workflow

When `kb_query` is called:

1. Search the knowledge folder for pages matching the question.
2. Read the most relevant pages and follow their edges.
3. Answer, citing pages as markdown links, and say what backs each claim.
4. If `file_as_page=true`, write the answer to `knowledge/concept/<slug>.md`
   with `type: Concept` and append a log entry of kind `query`.

## Lint workflow

When `kb_lint` is called:

1. Types and predicates outside the controlled vocabularies.
2. Edges missing any of the three provenance keys.
3. `object` or `primary_source` naming an `identifier` that does not exist.
4. Duplicate `identifier`s, and identifiers that are opaque codes rather than
   human-readable names.
5. Source nodes with no `xref` and no `raw_source` — nothing anchors them.
6. `<X>` and `not_<X>` asserted between the same two nodes.
7. Return a report. If `autofix=true`, fix the easy ones and append a `lint`
   log entry.

## Tone

Concise, scientific, evidence-led. No hype, no certainty without a citation.
