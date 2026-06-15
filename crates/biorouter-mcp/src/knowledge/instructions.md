# Knowledge extension

You can create and maintain personal knowledge bases backed by markdown
knowledge folders + git history. Use these primitive tools to read and write the knowledge folder.

Common operations:

- `kb_list_bases` — see which knowledge bases are visible to this session.
- `kb_create_base` — create a new one.
- `kb_add_raw_source` — ingest a URL or pasted text. The result is filed under
  `raw/<source-id>/` with `source.md` and `meta.yaml`; credibility is auto-classified.
  This does NOT create knowledge pages — read the source and write knowledge pages with
  `kb_write_page` to integrate the source into the knowledge graph.
- `kb_list_pages` / `kb_read_page` / `kb_write_page` — knowledge CRUD.
- `kb_get_graph` — derived nodes+edges for visualisation. The graph is
  rebuilt automatically whenever you `kb_write_page`, so pages you author show
  up in the Knowledge tab without any extra step.
- `kb_export` — export a knowledge base to a `.brkb` archive file on disk
  (returns the path). `kb_import` — import a `.brkb` file as a new knowledge
  base. Use these for portability instead of shelling out to zip the folder.
- `kb_list_history` / `kb_restore_state` — git-backed change log + revert.
- `kb_search` — search curated knowledge pages. If you omit `kb_id`, search runs across all non-hidden knowledge bases visible to this session.
- `kb_search_raw_sources` — search original raw source markdown only. Use this rarely, when the user specifically asks for raw/original/source-document evidence or when curated pages clearly omit a needed detail.

Retrieval behavior:

- If this extension is enabled and `kb_list_bases` shows at least one visible knowledge base, treat the knowledge base as a low-cost memory source. At even a slight hint that the answer may depend on stored papers, project context, prior ingested material, biomedical domain notes, or "what do we know about..." style wording, run `kb_search` before answering.
- Prefer the curated graph pages returned by `kb_search`. They are the pruned working knowledge layer.
- Do not search raw sources by default. Only use `kb_search_raw_sources`, or `kb_search` with `include_raw_sources=true`, when the user explicitly asks for raw/original sources, source documents, verbatim provenance, or when curated pages are insufficient and the answer would otherwise be weak.

Personal context (Soul):

- The built-in **Soul** knowledge base (`kb_id` "soul") holds durable facts about this user — how they approach problems, the tools and commands they prefer, and personal details. When a request would benefit from knowing the user (personalising an answer, recalling their preferences or working style, or "what do you know about me"), search it with `kb_search` using `kb_id="soul"`.
- A hidden knowledge base is excluded from the default cross-base `kb_search` (the one with no `kb_id`), but you can still search it directly by passing its `kb_id`. Soul may be hidden, so prefer the explicit `kb_id="soul"` form when you want personal context.

Every mutating tool commits to git. The history is the source of truth.
