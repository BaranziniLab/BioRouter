# Contributing to the documentation

How BioRouter's documentation is written and maintained, and what is currently known to be
wrong with it. Come here before adding a document to `docs/`, before reformatting an
existing one, or when you find that a document and the code disagree.

This folder is about the documentation itself, not about the product. For how BioRouter
works, start at the [documentation index](../README.md).

| Document | What it covers |
|---|---|
| [How this documentation is organized](../organization.md) | The sorting system: where a document goes, when a new folder is justified, how to extend the tree for a feature area that does not exist yet, and what to do when the rules do not fit. Read it first. |
| [Documentation style guide](documentation-style.md) | The house style every file under `docs/` follows — the context header, headings, naming, prose, structure, links, and the checklist for adding a new document. |
| [Open documentation issues](open-documentation-issues.md) | A live register of unresolved problems found during the 2026-07-18 cleanup: places where the docs and the code disagree, dead references, and coverage gaps. Each needs a human decision, so none were silently fixed. |

## The short version

Two documents govern this tree. [How this documentation is organized](../organization.md) decides **where** a document lives; the [style guide](documentation-style.md) decides **what it looks like** inside. Read the first before creating anything, the second before writing it.

Every Markdown file in the tree opens with an H1, then a blockquote giving **What this
is**, **Status**, and **Audience**, then one orienting paragraph. `Status` is one of
`Current`, `Historical record — <what completed, when>`, or `Superseded — <what changed,
where the truth lives>`. That line is what lets a reader trust a document without reading
the git log, and it is the convention most worth protecting.

Folders are topics, not buckets. Each one carries a `README.md` index listing its files.
Records of completed work live under [`history/`](../history/), not beside living guidance.

## If you find a document that is wrong

Prefer fixing it. If fixing it would mean guessing which of two conflicting sources is
right — the doc versus the code, or two docs against each other — add an entry to
[open documentation issues](open-documentation-issues.md) instead, with `file:line`
evidence for what you actually checked. A recorded disagreement is far more useful than a
confidently invented resolution.

## Related documentation

- [Documentation index](../README.md) — the entry point this folder's conventions serve
- [`history/`](../history/) — where records of completed work belong
- [`CLAUDE.md`](../../CLAUDE.md) — repository-level guidance for agents working in this codebase
