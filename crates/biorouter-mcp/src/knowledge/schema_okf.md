---
type: Schema
title: Maintenance schema (OKF v0.2)
description: How this knowledge base is written and maintained, in the Open Knowledge Format v0.2.
sources:
  - id: okf-v0-2
    resource: https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md
    title: Open Knowledge Format v0.2
---

# Knowledge base — maintenance schema (OKF v0.2)

This document tells you how to maintain *this particular* knowledge base. It is
read fresh on every macro call. Edit it freely to shape the knowledge folder's
voice, structure and conventions — but keep the frontmatter block above, and
keep the page contract below, because the graph is derived from it.

This base is written in the **Open Knowledge Format v0.2**. The vocabulary is
open: `type` may be any non-empty string you find useful, and nobody validates
it against a list. Pick a small set of type names and stay consistent.

## Layout

- `raw/<source-id>/` — original files plus a derived `source.md` and
  `meta.yaml`. **Read-only.**
- `knowledge/<type>/<slug>.md` — one file per concept, in a directory named
  after its lowercased `type`. `concept/`, `source/` and `note/` exist to start
  you off; create more as you coin more types.
- `index.md` — the bundle's catalog. You maintain it on every change.
- `log.md` — the change log. You append on every change.
- `schema.md` — this file.

Only `index.md` and `log.md` are reserved. **Every other `.md` file in the
bundle is a concept document and must carry frontmatter with a non-empty
`type`** — including this one.

## Page format

```yaml
---
type: Method                       # REQUIRED. Any non-empty string.
identifier: Heart rate variability # The page's name, unique in this bundle.
description: Beat-to-beat variation in heart rate.
subtype: physiological-measure     # free text, never validated
tags: [autonomic, cardiology]
xref: [MESH:D006339]               # external identifiers, `prefix:local`
status: stable                     # draft | stable | deprecated
stale_after: 2027-01-01            # after this date the page reads as stale
generated: { by: biorouter, at: 2026-08-19T12:00:00Z }
verified:
  - { by: "human:wgu", at: 2026-08-19T13:00:00Z }
sources:                           # what this page was built from
  - id: pmid-32504360
    resource: raw/pmid-32504360/original.pdf
    title: Elevated IL-6 and severe COVID-19
    author: "human:Chen et al."
    last_modified: 2020-06-01
---
```

`type` is the only always-required key. `identifier` is what other pages link
to, so give every page one and keep it unique. Everything else is optional; omit
what you do not know rather than inventing it. Unknown keys are preserved, so a
producer extension of your own is safe.

## Cross-references (the graph depends on these)

The knowledge graph is derived from links. **Write links as ordinary markdown
links to the target page's path:**

```markdown
Heart rate variability [falls with](/knowledge/concept/sympathetic-tone.md)
rising sympathetic tone.
```

Rules:

1. Every mention of another concept that has (or should have) its own page is a
   markdown link to that page's path. The link text is prose; the path is what
   resolves.
2. A link to a page that does not exist yet is legal and is recorded — create
   the page when you know enough to write it.
3. Every source page lists the concepts it touches; every concept page lists the
   sources that back it, under a `## Sources` heading.
4. Prefer linking over restating. If a fact lives on another page, link to it.

Older pages in this base may use `[[double bracket]]` links. Those are still
read, permanently — do not go and rewrite them, and do not write new ones.

## Attribution

Attribute a claim with a markdown footnote whose id matches a `sources[].id`.
The frontmatter of *this* file declares one, and the sentence below uses it, so
the mechanism is demonstrated rather than described:

Only `index.md` and `log.md` are reserved filenames.[^okf-v0-2]

[^okf-v0-2]: Open Knowledge Format v0.2, section 3.1

A footnote with no matching `sources[]` entry attributes nothing — to a reader
it looks sourced and is not.

## index.md

Sections are `#` headings; entries are one bullet each:

```markdown
# Concepts

* [Heart rate variability](knowledge/concept/heart-rate-variability.md) - Beat-to-beat variation in heart rate.

# Sources

* [Chen 2020](knowledge/source/chen-2020.md) - IL-6 and severe COVID-19.
```

The bundle-root `index.md` carries an `okf_version` key in its frontmatter and
**nothing else**. Do not add other keys to it.

## log.md

Entries are grouped under `## YYYY-MM-DD` date headings, newest first, and the
kind of change is a leading bold word in the bullet:

```markdown
## 2026-08-19

* **Ingest** — Chen 2020 (IL-6 and severe COVID-19)
```

Append through `kb_append_log`, which maintains this shape for you.

## Ingest workflow

For a source already staged by the ingestion pipeline:

1. Read `raw/<source-id>/source.md` and `meta.yaml`.
2. Decide what concepts the source touches, and what `type` each one is.
3. Create or update the source's own page: a two-to-three sentence summary, the
   key claims as bullets, methods and limitations if applicable, and links out
   to the concept pages.
4. For each concept: update the page if it exists, otherwise create it. Always
   link back to the source page and record it in `sources`.
5. If a claim contradicts an existing page, do not overwrite it — add an
   `## Open contradictions` section naming both positions and their sources.
6. Update `index.md` with new and changed pages.
7. Append a log entry through `kb_append_log` with `kind=ingest`.

## Credibility discipline

Peer-reviewed papers and books outweigh preprints, gray literature and web
posts. Reflect that in the language: hedge a claim sourced only from the web
("according to a blog post", "the user noted"). Never silently elevate a web
claim to a flat assertion — cite it.

## Query workflow

To answer a knowledge question:

1. Use `kb_search` to find pages matching the question.
2. Read the most relevant pages with `kb_read_page`.
3. Answer, citing pages as markdown links.
4. Write an answer page only when the user asks to save it. Validate the draft
   with `kb_validate_page`, write it to `knowledge/note/<slug>.md` with
   `kb_write_page`, update `index.md`, and append a `query` log entry.

## Lint workflow

`kb_lint` is read-only. It reports:

1. Pages with no `type`, or with no inbound links.
2. Links whose target page does not exist.
3. Concepts mentioned in source pages but with no page of their own.
4. Footnotes with no matching `sources[]` entry.
5. Return the diagnostics without changing pages or appending a log entry.

If the user requests repairs, validate and write each repair separately, then
run `kb_lint` again. Automated repair is available through the Knowledge panel
or `biorouter kb lint --fix`, not as a parameter on `kb_lint`.

## Tone

Concise, scientific, evidence-led. No hype, no certainty without a citation.
