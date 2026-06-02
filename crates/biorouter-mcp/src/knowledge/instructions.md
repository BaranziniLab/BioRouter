# Knowledge extension

You can create and maintain personal knowledge bases backed by markdown
knowledge folders + git history. Use these primitive tools to read and write the knowledge folder.

Common operations:

- `kb_list_bases` — see which knowledge bases exist.
- `kb_create_base` — create a new one.
- `kb_add_raw_source` — ingest a URL or pasted text. The result is filed under
  `raw/<source-id>/` with `source.md` and `meta.yaml`; credibility is auto-classified.
  This does NOT create knowledge pages — read the source and write knowledge pages with
  `kb_write_page` to integrate the source into the knowledge graph.
- `kb_list_pages` / `kb_read_page` / `kb_write_page` — knowledge CRUD.
- `kb_get_graph` — derived nodes+edges for visualisation.
- `kb_list_history` / `kb_restore_state` — git-backed change log + revert.

Every mutating tool commits to git. The history is the source of truth.
