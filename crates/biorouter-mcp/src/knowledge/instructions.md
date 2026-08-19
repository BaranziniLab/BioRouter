# Knowledge extension

You can create and maintain personal knowledge bases backed by markdown
knowledge folders + git history. Use these primitive tools to read and write the knowledge folder.

Common operations:

- `kb_list_bases` — see which knowledge bases are visible to this session.
- `kb_create_base` — create a new one. Takes a `format` — see "Two formats" below.
- `kb_add_raw_source` — ingest a URL or pasted text. The result is filed under
  `raw/<source-id>/` with `source.md` and `meta.yaml`; credibility is auto-classified.
  This does NOT create knowledge pages — read the source and write knowledge pages with
  `kb_write_page` to integrate the source into the knowledge graph.
- `kb_list_pages` / `kb_read_page` / `kb_write_page` — knowledge CRUD.
- `kb_validate_page` — check a page against its base's format **before** writing it.
  Writes nothing; returns diagnostics, each with a stable rule id, a severity, the page
  or edge it is about, and a message. In a BioOKF base, validate every draft.
- `kb_get_graph` — derived nodes+edges for visualisation. The graph is
  rebuilt automatically whenever you `kb_write_page`, so pages you author show
  up in the Knowledge tab without any extra step.
- `kb_export` — export a knowledge base to a `.brkb` archive file on disk
  (returns the path). `kb_import` — import a `.brkb` file as a new knowledge
  base. Use these for portability instead of shelling out to zip the folder.
- `kb_list_history` / `kb_restore_state` — git-backed change log + revert.
- `kb_search` — search curated knowledge pages. If you omit `kb_id`, the search runs across **every knowledge base in this session** and each hit is tagged with the `kb_id` it came from. Cite that id when you use a hit.
- `kb_search_raw_sources` — search original raw source markdown only. Use this rarely, when the user specifically asks for raw/original/source-document evidence or when curated pages clearly omit a needed detail.

Two formats:

A knowledge base is written in one of two formats, chosen when it is created and fixed
for its lifetime — this build has no conversion between them. Both write the same kind
of file (YAML frontmatter + markdown body); BioOKF only adds constraints, so a BioOKF
base is also a valid OKF base.

- **OKF** (the default) — the Open Knowledge Format v0.2. Open vocabulary: a page's
  `type` is any word that fits, and relationships are ordinary markdown links to other
  pages. Use it for general-purpose memory, retrieval, development and design notes,
  project and codebase context, meeting records, personal knowledge — anything that is
  not biomedical.
- **BioOKF** — OKF v0.2 plus the BioOKF v0.5 profile: a controlled vocabulary of 28
  entity types and 35 relationship predicates, where every asserted relationship carries
  provenance (how the claim is known, what produced it, and which source page it came
  from). Use it for biomedical literature, curated biology, clinical or genomic
  knowledge, and for anything meant to be exchanged with another institution or another
  BioOKF tool.

Choosing: if the subject is not biomedical, choose OKF. A biomedical vocabulary does not
make a non-biomedical base stricter, it makes it wrong — every page ends up typed
`Other`. If the user has not said and the subject could go either way, ask; failing that
choose OKF. The base's own `schema.md` states which format it is in and carries the
vocabulary if it has one, so read it before writing into a base you did not create.

A base created before this format shipped keeps working exactly as it did, with its own
`title`/`kind` frontmatter and `[[wiki links]]`. It is never rewritten, and
`kb_validate_page` reports nothing for it — that is the right answer for such a base,
not a failure.

Retrieval behavior:

- If this extension is enabled and `kb_list_bases` shows at least one visible knowledge base, treat the knowledge base as a low-cost memory source. At even a slight hint that the answer may depend on stored papers, project context, prior ingested material, biomedical domain notes, or "what do we know about..." style wording, run `kb_search` before answering.
- Prefer the curated graph pages returned by `kb_search`. They are the pruned working knowledge layer.
- Do not search raw sources by default. Only use `kb_search_raw_sources`, or `kb_search` with `include_raw_sources=true`, when the user explicitly asks for raw/original sources, source documents, verbatim provenance, or when curated pages are insufficient and the answer would otherwise be weak.

Knowledge bases in this session:

- Every base `kb_list_bases` returns is in play. There is no narrower "active" list to manage: a `kb_search` with no `kb_id` already covers all of them, and any tool call may name any base with an explicit `kb_id`.
- One of them is the **primary**. It is the base that KB-less writes land in, and the base that single-base reads (`kb_list_pages`, `kb_read_page`, `kb_get_graph`, `kb_list_history`) default to when you omit `kb_id`. Call `kb_get_active` to see the session's bases and which is primary; call `kb_set_active` to move the primary to another of them.
- **Do not switch the primary in order to read another base.** Pass that base's `kb_id` on the call. Changing the primary changes where writes go for the rest of the session, which is rarely what the user asked for.
- Writes name their base. `kb_write_page`, `kb_add_raw_source`, `kb_append_log`, `kb_restore_state` and the transaction tools all require `kb_id` — this is deliberate, so a write is never ambiguous. Tools that write on the user's behalf without one (for example `platform__ingest_conversation`) use the primary and tell you which base they used.
- If the session has no primary, a KB-less write fails and the error lists the bases you can choose from. Call `kb_set_active` with one of them, or pass `kb_id` on the call. A primary is never invented for you: no base is made primary just because it was created, or because it is the only one in the session.
- The primary can move without you asking. Removing its base from this session **promotes** the primary to the first remaining base rather than leaving the write target dangling; deleting that base leaves the session with **no** primary. Re-read `kb_get_active` rather than assuming the primary you saw earlier still holds.

Personal context (Soul):

- The built-in **Soul** knowledge base (`kb_id` "soul") holds durable facts about this user — how they approach problems, the tools and commands they prefer, and personal details. When a request would benefit from knowing the user (personalising an answer, recalling their preferences or working style, or "what do you know about me"), search it with `kb_search` using `kb_id="soul"`.
- A hidden knowledge base is excluded from the default cross-base `kb_search` (the one with no `kb_id`), but you can still search it directly by passing its `kb_id`. Soul may be hidden, so prefer the explicit `kb_id="soul"` form when you want personal context.

Every mutating tool commits to git. The history is the source of truth.
