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

## Ingest workflow

When `kb_ingest_source` is called:

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

When `kb_query` is called:

1. Search the knowledge folder for pages matching the question's entities/concepts.
2. Read the most relevant pages.
3. Compose an answer that cites pages with `[[knowledge-link]]`.
4. If `file_as_page=true`, write the answer to `knowledge/notes/<slug>.md` and
   include it in the response. Append a log entry of kind `query`.

## Lint workflow

When `kb_lint` is called:

1. Find pages with no inbound links (orphans).
2. Find pages flagged `contradiction: true`.
3. Find concepts mentioned in source pages but lacking their own page.
4. Find sources >90 days old whose claims are not referenced anywhere.
5. Return a report. If `autofix=true`, fix the easy ones (add missing
   cross-references, create stub pages) and append a `lint` log entry.

## Tone

Concise, scientific, evidence-led. No hype, no certainty without citation.
