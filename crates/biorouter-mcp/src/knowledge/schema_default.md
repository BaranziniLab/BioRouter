# Knowledge Base — Maintenance Schema

This document tells the LLM how to maintain *this particular* knowledge base.
It is read fresh on every macro call. Edit it freely to shape the knowledge
folder's voice, structure, and conventions.

## Layout

- `raw/<source-id>/` — original files + derived `source.md` + `meta.yaml`. **Read-only.**
- `knowledge/sources/<source-id>.md` — one page per source; summary + key extractions + outbound links.
- `knowledge/entities/<name>.md` — proper nouns (genes, drugs, people, datasets, methods).
- `knowledge/concepts/<name>.md` — ideas, mechanisms, theories.
- `knowledge/notes/<slug>.md` — ad-hoc pages, including queries-as-pages.
- `<name>.md` at the root of the knowledge folder — cross-cutting hubs (top of the graph).
- `index.md` — flat catalog of all pages. You maintain it on every change.
- `log.md` — chronological log; you append on every change.

## Page format

Every knowledge page starts with YAML frontmatter:

```yaml
---
title: <Human Title>
kind: entity | concept | source | note | hub
tags: [optional]
credibility_inherits: [source-id-1, source-id-2]   # which sources back this page
last_updated: 2026-05-30T12:00:00Z
contradiction: false   # set true to render as a flag node
---
```

Body is prose markdown with `[[knowledge-link]]` cross-references.

### Cross-reference rules (the graph depends on these)

The knowledge graph is derived **purely** from `[[link]]` patterns in page
bodies. If you do not emit links, the graph will have nodes but no edges.

When you write or update any knowledge page:

1. Every mention of another entity or concept that has (or should have) its
   own page **must** be wrapped in `[[double brackets]]`. Match the target
   page's title exactly (case-insensitive); the deriver slugifies both sides.
   Good: `[[EPAS1]] interacts with [[HIF2A]] under [[hypoxia]].`
   Bad:  `EPAS1 interacts with HIF2A under hypoxia.`
2. Every source page **must** include a `## Related pages` section listing
   every entity/concept it touches, one `- [[Name]]` bullet per line.
3. Every entity/concept page **must** include a `## Sources` section with
   one `- [[source-id]]` bullet per supporting source.
4. Prefer linking over re-stating. If a fact lives on another page, write
   `See [[Page Name]]` instead of restating it.

The lint workflow (`kb_lint`) reports pages with no inbound links as orphans
— fix them by adding inbound `[[links]]` from related pages.

## Ingest workflow

For a source already staged by the ingestion pipeline:

1. Read `raw/<source-id>/source.md` and `meta.yaml`.
2. Decide what biomedical entities and concepts the source touches.
3. Create or update `knowledge/sources/<source-id>.md` with: 2-3 sentence summary,
   key claims as bullets, methods if applicable, limitations, and outbound
   links to entity/concept pages.
4. For each entity/concept mentioned: if a page exists, update it; otherwise
   create it. Always include a backlink to the source page.
5. If a claim in the new source contradicts an existing page, mark the
   conflicting page with `contradiction: true` in frontmatter and add a
   section "## Open contradictions" listing both positions and the sources.
6. Update `index.md` with new/modified pages.
7. Append a one-line log entry to `log.md` of the form
   `## [<date>] ingest | <source-title>`.

## Credibility discipline

- Peer-reviewed papers and books outweigh preprints, gray literature, and
  web posts. Reflect this in language: hedge claims sourced only from web
  or personal materials ("according to a blog post", "the user noted").
- Never silently elevate a web claim to a knowledge-page assertion — always cite.

## Query workflow

To answer a knowledge question:

1. Search the knowledge folder for pages matching the question's entities/concepts.
2. Read the most relevant pages.
3. Compose an answer that cites pages with `[[knowledge-link]]`.
4. Write an answer page only when the user asks to save it, and only after this
   retired base has been replaced with a supported OKF or BioOKF base. Follow
   that base's current schema; do not write new content using this legacy format.

## Lint workflow

`kb_lint` is read-only and refuses retired pre-OKF bases. The historical checks
below describe the old format, not an available repair workflow:

1. Find pages with no inbound links (orphans).
2. Find pages flagged `contradiction: true`.
3. Find concepts mentioned in source pages but lacking their own page.
4. Find sources >90 days old whose claims are not referenced anywhere.
5. Return a report without edits. Do not pass repair parameters to `kb_lint`.

## Tone

Concise, scientific, evidence-led. No hype, no certainty without citation.
