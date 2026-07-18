# Documentation style guide

Every Markdown file under `docs/` follows this. It exists so that any document in the
tree can be opened cold — by a person or by an agent — and immediately answer three
questions: what is this, is it still true, and where do I go next.

Apply it when you add a document, and when you touch an existing one enough that its
framing no longer matches its content. The whole tree was brought to this style on
2026-07-18; keeping it consistent is cheaper than doing that again.

## 1. The context sandwich (mandatory)

A reader — human or agent — must know within the first screen *what this is*, *why it
exists*, and *whether it is still true*. And on finishing, *where to go next*.

Every file opens with, in this exact order:

```markdown
# Title In Sentence Case

> **What this is.** One or two sentences naming the document's job.
> **Status:** Current | Historical record (completed <date>) | Superseded by [x](y)
> **Audience:** end users | developers working on <subsystem> | agents | maintainers

<One short orienting paragraph: the problem this doc addresses and the context a
reader needs before the first heading. No jargon that is not defined here or linked.>
```

and closes with:

```markdown
## Related documentation

- [Neighbouring doc](../path/to.md) — one clause on why you'd read it
```

Rules:
- The blockquote block is the **context header**. It is required on every file except the
  two cases below.
- **Exception 1 — `README.md` index files.** These open with a one-paragraph description
  of the folder's scope instead, plus a table of the folder's files.
- **Exception 2 — a dated series whose folder index frames the whole set.** The release
  notes under [`releases/notes/`](../releases/notes/README.md) are the only current
  example: each file's purpose is evident from its version-numbered filename, every entry
  has the same shape, and the folder README already states that all of them are frozen
  records of a release as it shipped. Repeating a status header 18 times would add noise
  without adding information. Do not stretch this exception to cover a folder of unlike
  documents — if the files differ in kind, each needs its own header.
- This file is itself the third case: a style guide that opens by stating what it is in
  prose. Prefer the header; deviate only when the document's first paragraph genuinely
  does the same work better.
- `Status:` is required on anything dated, planned, or reporting completed work.
  Historical records say so in the header — never leave a reader guessing whether a
  plan was executed.
- If the document uses project-internal identifiers (`BR-43`, `spec-017`, wave names),
  the header or an early section must say what the identifier scheme is and where the
  index lives.

## 2. Titles and headings

- One `# H1` per file, matching both the filename and the document's real subject.
- Sentence case: `## Running the test suite`, not `## Running The Test Suite`.
- No skipped levels (`##` then `####`).
- Headings are noun phrases or imperatives, never bare nouns like `## Notes`.
- Do not number headings manually; let the structure carry the order.

## 3. Filenames and folders

- `kebab-case.md`. No `ALL_CAPS` except `README.md`.
- The name states the *purpose*, not the process that produced it:
  `findings.md` → `defects-found-and-fixed.md` when that is what it holds.
- Dated reports carry an ISO date **suffix** when the date is load-bearing:
  `desktop-reliability-review-2026-07.md`.
- Every folder holds exactly one topic and has a `README.md` index listing its files
  with a one-line description each.

## 4. Prose

- Second person for instructions ("Run `cargo test`"), third person for description.
  Never drift between them inside a section.
- Present tense for how the system behaves. Past tense only in historical records.
- Define an acronym or internal term on first use in the file — do not assume the
  reader arrived from a neighbouring page.
- Prefer short paragraphs (≤ 5 lines) over walls of text. Break a long section into
  subsections rather than letting it run.
- No filler openers ("In this document, we will…"). State the thing.

## 5. Structure elements

- **Tables** for comparisons across a fixed set of attributes, and for reference
  material with a stable shape (flags, env vars, endpoints). Header row always.
- **Bullets** for unordered sets; **numbered lists** only for genuine sequences.
- **Fenced code blocks always carry a language tag** (`bash`, `rust`, `ts`, `json`,
  `yaml`, `text`). Shell blocks show the command, not a `$` prompt.
- File paths, commands, env vars, and identifiers in backticks.
- Callouts use the blockquote form: `> **Note.**`, `> **Warning.**`, `> **Why.**`.

## 6. Links

- Relative links between docs, with the `.md` extension.
- Link text describes the destination; never "click here" or a bare URL in prose.
- When referencing repo source, link the path: `crates/biorouter/src/agents/agent.rs`.
- Every link must resolve. Fix or remove dead ones; do not leave them.

## 7. What not to do when reformatting an existing document

- Do not invent facts to fill a section. If the source doc did not say it, it does not
  go in. Preserve technical content exactly — this is a formatting and framing pass.
- Do not delete substantive detail to make a doc shorter.
- Do not change code samples, command lines, version numbers, or measured results.
- Do not add a "Table of contents" to a file under ~200 lines.

## 8. Adding a document

1. Put it in the folder whose topic it belongs to. If no folder fits, create one — a
   folder is a topic, not a bucket, and it needs a `README.md` index of its own.
2. Name it for its purpose in kebab-case.
3. Give it the context header before you write the body. If you cannot state what the
   document is for in one sentence, the document does not know what it is yet.
4. Add it to the folder's `README.md` table.
5. If it is a record of work rather than living guidance, it belongs under `history/`,
   and its status line says what completed and when.

## Related documentation

- [Documentation index](../README.md) — the entry point this style serves
