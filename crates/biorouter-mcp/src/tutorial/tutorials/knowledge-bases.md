# Working with Knowledge Bases: build a personal, citable library from papers and documents

## Purpose

Teach the user to create a knowledge base (KB), ingest sources, query it in
chat, explore the graph, and export/import it. Knowledge bases are Biorouter's
persistent memory for research material: markdown page trees, cross-linked
with `[[wiki-links]]`, versioned in git, and searchable from any session.

## Concepts to convey first (briefly)

- A KB lives at `~/.config/biorouter/knowledge/<kb-id>/` with `raw/`
  (original sources verbatim), `knowledge/` (curated pages the ingestion
  agent writes), `index.md`, `log.md` (change log), and `schema.md`
  (editable conventions that steer how pages are written).
- Every source is credibility-classified on ingest (peer-reviewed >
  preprint > book > gray literature > web > personal) using Crossref and
  OpenAlex lookups, so claims can be weighted by evidence quality.
- Every change is a git commit — history can be browsed and restored.

## Phase 1: Create a knowledge base

- **Desktop:** open **Knowledge** in the sidebar → create a new KB with a
  descriptive name (e.g. "MS genetics").
- **In chat:** ask the agent — the Knowledge extension exposes
  `kb_create_base`. Confirm with `kb_list_bases`.

## Phase 2: Ingest sources

- **Desktop:** the Knowledge page has an ingest panel — drop files (PDF,
  HTML, DOCX, CSV, markdown), or paste text/URLs. Ingestion streams its
  progress live: conversion, credibility check, then an agent integrating the
  content into curated pages.
- **In chat:** paste a URL or text and ask to add it to the KB
  (`kb_add_raw_source`); run an ingest to integrate it into pages.

Walk the user through ingesting 1-2 real sources from their field. Point out
afterwards how the source became a `knowledge/sources/` page with key claims,
plus entity/concept pages cross-linked with `[[…]]` references.

## Phase 3: Query the KB

- In any chat, questions touching stored material are answered with
  `kb_search` (BM25 over curated pages) and `kb_read_page` — answers cite
  pages.
- The chat composer has a KB selector to set the **active KB** for the
  session.
- Raw original sources are only searched on explicit request ("search the
  raw sources / original documents").

Demonstrate with a question the ingested sources can answer, and show how the
answer cites pages.

## Phase 4: Graph and history

- The Knowledge page renders a force-directed graph: pages are nodes,
  `[[wiki-links]]` are edges. If a graph has nodes but no edges, the pages
  lack cross-references — running a lint pass can add missing links.
- The change-log drawer shows the git history; any prior state can be
  previewed and restored.

## Phase 5: Share and back up

- Export a KB as a single `.brkb` archive (Knowledge page or `kb_export`)
  and import it on another machine (`kb_import`). This is the supported way
  to share curated knowledge with collaborators.

## Notes for the agent

- Ingest 1-2 sources interactively before batch-loading many; let the user
  see what a curated page looks like and adjust `schema.md` early if they
  want a different voice or structure.
- Remind users that editing `schema.md` changes how all future ingestion
  writes pages — it's the steering wheel for their KB.
- Large PDFs can take a while to digest; the streaming progress panel shows
  what's happening.
