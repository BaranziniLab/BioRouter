---
name: knowledge-choose-a-format
description: "Choosing between the two knowledge-base formats, OKF and BioOKF, and what each one commits you to. Load this whenever you are about to call kb_create_base, whenever the user asks for a new knowledge base, notebook, memory, wiki or literature collection, or whenever you need to know which format an existing base is in and what its rules are."
---

# Choosing a knowledge-base format

Every Biorouter knowledge base is written in one of two formats. The choice is made
once, at `kb_create_base`, and **this build has no conversion between them** — so it is
worth thirty seconds of thought rather than a default nobody revisits.

Both formats write the same kind of file: a `.md` page under `knowledge/` with a YAML
frontmatter block and a markdown body. BioOKF only *adds* rules, so every BioOKF base is
also a valid OKF base. What differs is what the rules are and what checks them.

## The two, side by side

| | **OKF** (default) | **BioOKF** |
| --- | --- | --- |
| Spec | Open Knowledge Format v0.2 | OKF v0.2 + the BioOKF v0.5 profile |
| A page's `type` | any non-empty word you like | one of **28** controlled values |
| Relationships | markdown links to other pages | typed `edges:` with **35** predicates |
| Provenance | optional, per document | **required** on every edge |
| An unknown value | accepted, silently | accepted, and flagged by lint |
| Cost per page | low | higher — every claim gets typed and cited |
| What it buys | a graph *you* can navigate | a graph *other tools* can read |

## Pick OKF when

The subject is not biomedical. That is very nearly the whole rule, and it is a stronger
rule than it looks: a controlled biomedical vocabulary does not make a non-biomedical
base *stricter*, it makes it *wrong*, because every page ends up typed `Other` and every
relationship ends up as `associated_with`. You have paid the cost and bought nothing.

Concretely, OKF is the format for:

- general memory and retrieval — "what do we know about X", things to recall later;
- development notes, architecture and design records, decisions and their reasons;
- project and codebase context, runbooks, environment quirks;
- meeting notes, reading notes, personal knowledge;
- anything exploratory, where the shape of the subject is not settled yet.

## Pick BioOKF when

The content is biomedical **and** at least one of these is true:

- it comes from the literature — papers, preprints, reviews, trials, datasets;
- it is curated biology someone else will rely on: genes, molecules, diseases,
  phenotypes, pathways, variants, exposures;
- it is destined for exchange — another institution, another BioOKF tool, a shared
  bundle, a publication supplement;
- the value is in the *relationships* being queryable and attributable, not in the prose.

The 28 types and 35 predicates are a shared language. Their whole point is that somebody
who has never seen your base can ask "what treats this disease, and how well is it
known?" and get an answer. If nobody is ever going to ask a question like that, you do
not need them.

## When you are not sure

Ask the user. It is a one-sentence question — *"is this biomedical knowledge you'll want
to query and share, or notes for us?"* — and it is far cheaper than the alternative,
which is discovering the answer after fifty pages with no way to convert.

If you cannot ask, **choose OKF**. It is the common base of both, so nothing written in
it is invalid; a BioOKF base that should have been OKF is full of `Other`, which is
worse.

## Creating the base

```
kb_create_base { id: "ms-literature", name: "MS Literature", format: "biookf" }
kb_create_base { id: "project-notes",  name: "Project Notes" }          # format defaults to okf
```

Only `okf` and `biookf` are accepted. A misspelling is refused, on purpose — a silent
fallback would hand you the opposite of what you asked for.

## Working in a base you did not create

Read its `schema.md` first: `kb_read_page { kb_id: "…", path: "schema.md" }`. It states
which format the base is in and, for BioOKF, carries the full vocabulary. Do not infer
the format from the pages you happen to have read.

Some bases predate both formats — `title:` / `kind:` frontmatter and untyped
`[[wiki links]]`. That storage is **retired**: Biorouter purges it on startup, and until
it has, every tool that writes or validates refuses it — `kb_write_page`, `kb_add_raw_source`, `kb_validate_page`, `kb_lint`,
`kb_begin_txn` and `kb_append_log` —
returning "uses the retired pre-OKF format; restart Biorouter to finish the legacy purge".
The read-only tools (`kb_read_page`, `kb_search`, `kb_get_graph`, `kb_list_pages`,
`kb_list_history`, `kb_export`) still work, so the content is recoverable. If you meet
one, stop and tell the user to restart Biorouter; do not try to repair the base
yourself.

`biorouter kb create` has no `--format` flag, so a base created from the terminal is
always OKF. Use `kb_create_base` to pick BioOKF.

## Next

- **knowledge-ingest-okf** — the loop for turning a source into pages in an OKF base.
- **knowledge-ingest-biookf** — the same loop plus the typing decision procedure.
- **knowledge-lint** — reading a lint report and fixing what it found.
