# Knowledge — founding design for the personal knowledge base feature

> **What this is.** The origin design document for BioRouter's Knowledge feature: git-backed markdown knowledge bases maintained by an LLM, a credibility classifier, graph derivation, the MCP tool surface, the HTTP routes, the `.brkb` portability format, and the desktop UI.
> **Status:** Historical record — approved 2026-05-30 and built out across the six implementation plans in this folder. The feature shipped; `crates/biorouter-mcp/src/knowledge/` contains `brkb.rs`, `git.rs`, `graph.rs`, `credibility/`, `convert/` and `macros/` substantially as specified here.
> **Audience:** developers working on the Knowledge subsystem, and agents that need the original rationale behind a design decision.
> **Design owner:** Wanjun Gu, BioRouter maintainer (Baranzini Lab, UCSF). No other contact was recorded on the original document.

Knowledge lets a user maintain one or more personal knowledge bases inside BioRouter. The pattern follows what this document calls the **Karpathy incremental-knowledge idea**: rather than re-deriving knowledge from raw documents on every query, an LLM incrementally builds and maintains a persistent, interlinked markdown knowledge folder that sits between the user and the raw sources. The design targets what the document later calls the **Karpathy-scale regime** — a handful of knowledge bases, each up to a few hundred pages — not a search engine over thousands of them. The original document cited Andrej Karpathy's gist for this idea without recording a URL; no link is available to reproduce here.

This is the design as approved, preserved for the reasoning it carries. For what the code does *today*, read `crates/biorouter-mcp/src/knowledge/` and the Knowledge section of `CLAUDE.md`. Where the shipped implementation diverged from this design, an inline note says so.

## How to read this document

The document runs from concept to delivery plan, in five parts:

1. **Scope** — Summary, Goals, Non-goals.
2. **Design** — Architecture overview, Data model, Credibility classifier.
3. **Interfaces** — MCP tool surface, Sub-agent loop, Conversion pipeline, Graph derivation, History and revert, `.brkb` format, Server endpoints.
4. **Frontend** — sidebar and routing, component tree, graph rendering, ingest flow, change log, active-KB state, chat integration, styling.
5. **Delivery** — Test plan, Implementation phasing, Risks and mitigations, Open questions.

## Summary

A new top-level feature called **Knowledge** that lets a user maintain one or
more personal, customizable knowledge bases inside BioRouter. The pattern
follows Andrej Karpathy's incremental LLM knowledge-building idea: instead of re-deriving knowledge
from raw documents on every query, an LLM incrementally builds and maintains
a persistent, interlinked markdown knowledge folder between the user and the raw sources.

Knowledge is delivered as

1. a new built-in MCP extension (`biorouter-mcp/src/knowledge/`) that exposes
   primitive and macro tools for ingesting, querying, and linting knowledge bases;
2. a new top-level UI route in the desktop app, slotted between Skills and
   Settings in the sidebar, that surfaces the source-ingestion panel, the
   knowledge graph, and the change-log drawer;
3. an export/import format `.brkb` (BioRouter Knowledge Base) that bundles a
   knowledge base for portability across users.

The Knowledge extension is built-in but disabled by default. When enabled,
the chat agent can call its tools to digest, query, and lint the active
knowledge base directly from a conversation.

## Goals

- Knowledge accumulates: every source improves the knowledge base, not just an index.
- The knowledge folder is fully LLM-maintained; the user curates sources and asks questions.
- Sources are credibility-aware (peer-reviewed paper, preprint, book,
  gray literature, web, personal) so the graph naturally reflects what is
  well-supported vs anecdotal.
- The user can roll back any edit via a git-backed change log.
- Knowledge bases are portable: one zipped `.brkb` file moves the knowledge folder,
  raw sources, and full history between machines and users.
- A knowledge base looks and feels like a first-class BioRouter surface,
  not a side panel.

## Non-goals

- Real-time collaborative editing of a single knowledge base by multiple humans.
- Hosting a remote knowledge base service. Everything runs locally.
- Replacing the Memory extension. Memory is short-form key/value;
  Knowledge is long-form structured knowledge folder.
- Building a search engine optimized for thousands of KBs. We target the
  Karpathy-scale regime (a handful of bases, each up to ~hundreds of pages).

## Architecture overview

Three layers, mirroring the Karpathy gist:

1. **Raw sources** — immutable user documents (PDFs, HTML, DOCX, CSV,
   pasted text). Stored under `<kb-root>/raw/`.
2. **The knowledge folder** — LLM-authored markdown pages in `<kb-root>/knowledge/`,
   interlinked via `[[knowledge-link]]` style references.
3. **The schema** — `<kb-root>/schema.md`, the per-KB Claude/Codex/AGENTS-style
   instruction doc that tells the LLM how to maintain *this particular* knowledge folder.

The Knowledge extension provides the operational layer that reads, writes,
and maintains these files. The desktop UI is a front-end into the same
extension via the BioRouter server.

```text
┌──────────────────────── Desktop UI (React) ────────────────────────┐
│   KnowledgeView                                                    │
│   ├── KBSelector (cmd-K palette)                                   │
│   ├── IngestPanel  (dropzone, paste, staged list, model picker)    │
│   ├── KnowledgeGraph (react-force-graph-2d)                        │
│   └── ChangeLogDrawer (timeline + revert)                          │
└────────────────────────────┬───────────────────────────────────────┘
                             │ HTTP + SSE (auto-generated TS client)
                             ▼
┌──────────────── biorouter-server  (Axum) ──────────────────────────┐
│   routes/knowledge.rs   → KnowledgeService                         │
└────────────────────────────┬───────────────────────────────────────┘
                             │ shared impl
                             ▼
┌──────────────── biorouter::knowledge::KnowledgeService ────────────┐
│   store · git · convert · credibility · graph · ingest · query    │
│   · lint · brkb                                                    │
└─────┬──────────────────────────────────────────────────────────────┘
      │ also exposed via MCP
      ▼
┌──────── biorouter-mcp::knowledge::KnowledgeServer ─────────────────┐
│   Primitive tools (kb_list_pages, kb_read_page, kb_write_page, …)  │
│   Macro tools    (kb_ingest_source, kb_query, kb_lint)             │
└────────────────────────────────────────────────────────────────────┘
```

> **Note — the shipped layering differs from this diagram.** The diagram shows
> `KnowledgeService` implemented in the `biorouter` crate with the MCP server
> layered on top. In the shipped code the whole module lives in
> `crates/biorouter-mcp/src/knowledge/`, and `crates/biorouter/src/knowledge/mod.rs`
> is a re-export (`pub use biorouter_mcp::knowledge::*;`) — done that way because
> `biorouter` depends on `biorouter-mcp`, so implementing the service in
> `biorouter` would have created a circular dependency. The path
> `biorouter::knowledge::KnowledgeService` still resolves, but the implementation
> is not there. This distinction matters when navigating the source.

The chat agent calls the MCP tools directly. The UI calls the HTTP routes.
Both paths delegate to the same `KnowledgeService`, so there is exactly one
implementation per operation.

## Data model

### Knowledge base on disk

One KB = one directory = one git repo. Stored at
`~/.config/biorouter/knowledge/<kb-id>/` (resolved via the existing
`etcetera` strategy used by Memory).

```text
<kb-root>/
├── manifest.yaml        # id, name, color, created_at, schema_version,
│                        #   default_model (optional)
├── schema.md            # per-KB Karpathy-style instruction doc, user-editable
├── index.md             # LLM-maintained catalog of pages
├── log.md               # human-readable chronological log, mirrors git history
├── raw/
│   └── <source-id>/
│       ├── original.<ext>   # untouched user file
│       ├── source.md        # derived markdown
│       └── meta.yaml        # url, title, ingested_at, credibility, sha256
├── knowledge/
│   ├── entities/   <name>.md
│   ├── concepts/   <name>.md
│   ├── sources/    <source-id>.md     # one page per source
│   ├── notes/      <slug>.md          # ad-hoc pages incl. queries-as-pages
│   └── *.md                            # cross-cutting hubs at the root
├── .biorouter-knowledge/
│   ├── graph-cache.json    # derived nodes+edges, regenerated on demand
│   └── credibility.yaml    # user-overridable URL→credibility rules
├── .gitignore              # excludes raw/<id>/original.* (binaries) and .crossref-cache/
└── .git/
```

`.brkb` is a zip of the whole directory including `.git` and `raw/*/original.*`.
The `.gitignore` keeps binary originals out of git history (so the repo stays
lean) but they still travel with the zip because we zip the entire folder.

### Manifest

`manifest.yaml` (per KB):

```yaml
id: ms-patient-analysis
name: MS Patient Analysis
color: "#5a6394"     # used by KB selector + chip
created_at: "2026-05-30T12:00:00Z"
schema_version: 1
default_model:       # optional; the UI ingest panel persists user's choice here
  provider: anthropic
  model: claude-sonnet-4-6
```

A separate top-level `~/.config/biorouter/knowledge/manifest.yaml` is the
**registry** — list of `{id, path}` mappings used for KB discovery. Imports
add an entry; deletes remove it; the on-disk folder is the source of truth
otherwise.

### Source meta

`raw/<source-id>/meta.yaml`:

```yaml
id: 8f3a2-arxiv-2403-12345
title: "Title of the paper"
url: "https://arxiv.org/abs/2403.12345"
ingested_at: "2026-05-30T13:14:15Z"
sha256: "..."
mime: "application/pdf"
original_filename: "paper.pdf"
credibility:
  tier: preprint
  confidence: 0.97
  publisher: "arXiv"
  venue: "arXiv:2403.12345"
  doi: null
  retracted: false
  reasoning: "URL host arxiv.org → preprint server."
  classifier_version: 1
```

### Credibility tiers

| Tier | Detection rules (in order) |
|---|---|
| `peer_reviewed` | DOI resolves via Crossref/OpenAlex to `type=journal-article` and `publisher` ∈ curated allow-list. |
| `preprint` | URL host ∈ {arxiv.org, biorxiv.org, medrxiv.org, chemrxiv.org, ssrn.com, …} OR Crossref `type=posted-content`. |
| `book` | ISBN present OR Crossref `type ∈ {book, book-chapter, monograph}` from a known academic publisher. |
| `gray_lit` | URL host matches `.gov`, `.edu`, WHO / CDC / NIH / FDA / ClinicalTrials.gov / IETF, or RFC patterns. |
| `web` | Any other http(s) URL. |
| `personal` | Local file drop, pasted text, or no detectable provenance. |

Plus an orthogonal `retracted: true` flag set when Crossref / OpenAlex report
the work as retracted.

### Color tokens

Credibility-tier colors are added once to the theme so they're themable:

```css
--cred-peer:      #3d4878;
--cred-book:      #5a6394;
--cred-preprint:  #7d83b0;
--cred-graylit:   #a8acc8;
--cred-web:       #c9866a;
--cred-personal:  #b08aa8;
--cred-retracted: #c98b8b;
```

Source nodes are drawn at full opacity using the matching `--cred-*` color.
Edge styling reflects credibility: peer-reviewed and book use solid 1.6 px,
preprint solid 1.3 px, gray-lit solid 1.2 px, web and personal dashed 1.0 px.
Retracted sources overlay a red `!` badge on the node and dash any outgoing
edges in `--cred-retracted`.

Non-source nodes (concept, entity, hub, flag) keep their existing mockup
colors so credibility coloring does not collide with node-kind coloring.

## Credibility classifier

A layered service `KnowledgeService::classify` runs cheap deterministic
checks before any LLM call:

```rust
pub async fn classify(input: &SourceInput) -> Result<Credibility> {
    let ids = identifiers::extract(input).await?;                // 1
    if let Some(doi) = &ids.doi {
        if let Some(c) = crossref::classify(doi).await? { return Ok(c); } // 2
        if let Some(c) = openalex::classify(doi).await? { return Ok(c); }
    }
    if let Some(c) = host_patterns::classify(input) { return Ok(c); }    // 3
    agentic::classify(input).await                                       // 4
}
```

1. **Identifier extraction** — DOI / arXiv ID / ISBN / PMID, pulled from
   URL, filename, HTML `<meta>`, or PDF metadata (`pdf-extract` already
   surfaces XMP tags).
2. **Crossref / OpenAlex lookup** — free, no auth, fast. Maps DOI → publisher
   + type + retraction status. We do not maintain a regex of publishers;
   we maintain a small allow-list of publisher **names** that count as
   `peer_reviewed`. Crossref/OpenAlex tell us what name to compare against.
   Responses cached in `~/.config/biorouter/knowledge/.crossref-cache/`.
3. **Host patterns** — preprint servers, `.gov`, `.edu`, WHO/CDC/NIH/FDA.
4. **Agentic fallback** — invoked only when 1–3 fail. A bounded sub-agent
   with three tools (`fetch_url`, `crossref_search`, `openalex_search`) and
   a max-5-step budget. Uses the smallest model available by default (e.g.,
   Haiku) and is gated behind a config flag.

The user can always override via `credibility.yaml` (regex → forced tier),
the per-source `meta.yaml`, or a right-click "Reclassify…" action in the
graph.

## MCP tool surface

All `kb_*` tools accept an optional `kb_id`; when omitted, they use the
session-scoped active KB set via `kb_set_active`. The active KB is part of
the agent session state, not the extension, so it travels with the chat.

Mutating primitives also accept an optional `txn` (transaction handle).
When `txn` is set, the operation writes to a working branch and **does not
commit per call** — the macro that owns the transaction commits everything
at the end. When `txn` is unset, each mutating call commits individually
with a synthesized message. Transactions are created and finalized by
macros (or by an explicit pair `kb_begin_txn` / `kb_commit_txn`); they are
not exposed for general chat use to keep the surface simple.

### Primitives (synchronous, no LLM)

| Tool | Params | Returns |
|---|---|---|
| `kb_list_bases` | — | `[{id, name, color, sources, pages, links, last_modified}]` |
| `kb_create_base` | `{id, name, color?}` | manifest |
| `kb_set_active` | `{kb_id}` | ok |
| `kb_get_active` | — | `{kb_id?}` |
| `kb_list_pages` | `{kb_id?, path_prefix?}` | `[{path, title, kind, tags}]` |
| `kb_read_page` | `{kb_id?, path}` | `{content, frontmatter, last_modified, commit_sha}` |
| `kb_write_page` | `{kb_id?, path, content, commit_message, txn?}` | `{commit_sha?}` (sha returned only when no `txn`) |
| `kb_search` | `{kb_id?, query, limit?}` | `[{path, score, snippet}]` (BM25 over knowledge/ + raw source.md) |
| `kb_append_log` | `{kb_id?, kind, summary, delta?, txn?}` | `{commit_sha?}` |
| `kb_add_raw_source` | `{kb_id?, source, txn?}` (`{file}` / `{url}` / `{text, title?}`) | `{source_id, source_md_path, credibility, commit_sha?}` |
| `kb_classify_source` | `{kb_id?, source_id}` | `Credibility` |
| `kb_get_graph` | `{kb_id?}` | `{nodes, edges}` |
| `kb_list_history` | `{kb_id?, limit?}` | `[{commit_sha, kind, summary, delta, timestamp}]` |
| `kb_preview_state` | `{kb_id?, commit_sha}` | `{files, diff_summary}` |
| `kb_restore_state` | `{kb_id?, commit_sha}` | `{ok, new_commit_sha}` (uses `git revert`-style commit) |
| `kb_begin_txn` | `{kb_id?, label}` | `{txn}` (creates a working branch named `txn/<uuid>`) |
| `kb_commit_txn` | `{kb_id?, txn, summary, kind}` | `{commit_sha}` (squash-merges working branch into `main`) |
| `kb_abort_txn` | `{kb_id?, txn}` | `{ok}` (deletes the working branch) |

### Macros (run a sub-agent loop)

| Tool | Params | Behavior |
|---|---|---|
| `kb_ingest_source` | `{kb_id?, source, model?, focus?}` | Add to raw/, classify, drive a sub-agent that writes source page + updates entity/concept pages + adds cross-links + flags contradictions + appends log + commits as one logical change. |
| `kb_query` | `{kb_id?, question, model?, format?, file_as_page?}` | Search + synthesize answer with citations. If `file_as_page=true`, write to `knowledge/notes/` and commit. |
| `kb_lint` | `{kb_id?, model?, autofix?, scope?}` | Find orphans, contradictions, stale claims, missing pages. With `autofix`, sub-agent fixes; otherwise returns report. `scope` accepts `all` or `since:<commit>`. |

### KB management (UI-only, not LLM-callable)

`kb_delete_base`, `kb_export_brkb`, `kb_import_brkb` are HTTP endpoints, not
MCP tools. Keeping destructive operations off the LLM surface prevents an
off-the-rails agent from wiping a KB.

## Sub-agent loop (the macro engine)

When a macro is invoked, the extension instantiates a bounded agent:

- **Tools available:** only KB primitives plus a `complete()` sentinel.
- **System prompt:** composed of (1) the KB's `schema.md`, (2) a
  macro-specific operating procedure embedded as a string constant
  (`INGEST_PROCEDURE`, `QUERY_PROCEDURE`, `LINT_PROCEDURE`), (3) the macro
  inputs.
- **Model:** caller's choice → session's chat model → configured default.
- **Limits:** max 30 steps, max 5 minutes wall time, max-tokens caller-controllable.
- **Streaming:** every step (tool call + result) emits a server-sent event;
  the UI consumes via `useIngestStream`, the chat shows inline progress.
- **Cancellation:** macro returns a job id; `DELETE` aborts.
- **Atomicity:** the macro calls `kb_begin_txn` before invoking the
  sub-agent, threads the resulting `txn` handle into every mutating
  primitive the sub-agent can call, and finalizes with `kb_commit_txn` on
  success (squash-merging the working branch into `main`) or
  `kb_abort_txn` on failure (deleting the working branch). The result is
  exactly one commit per macro invocation, regardless of how many pages
  the sub-agent touched. The KB is never left in a half-baked state.

## Conversion pipeline (`convert/`)

`KnowledgeService::add_raw_source` materializes any input into a
markdown-rendered raw entry. The pipeline:

- **File path** → dispatch by mime to the right handler:
  - HTML → `htmd` crate
  - PDF → `pdf-extract` for text; if extraction is empty/garbled, LLM
    fallback that hands the bytes (or page images) to the user's chosen
    multimodal model with a "clean markdown" prompt
  - DOCX → `docx-rs`
  - CSV → rendered as a markdown table
  - Plain text / Markdown → passthrough
- **URL** → `reqwest` download to a temp file, then run the file path
  handler with the resolved mime
- **Pasted text** → store the text as-is, then run a tiny URL-extraction
  pass: any http(s) URL found in the text is fetched and added as an
  additional source (so a researcher can paste a paragraph of notes and
  the linked papers come along automatically). User can opt out per-URL
  in the UI before staging.

Both the **original** file and the **derived markdown** land in
`raw/<source-id>/`. The original is preserved so the user can always
re-derive if conversion improves.

## Graph derivation

`graph.rs` parses the knowledge tree to build the in-memory graph:

- **Nodes:**
  - `source` — one per `raw/<id>/`. Color = credibility tier. Page = `knowledge/sources/<id>.md`.
  - `entity` — pages under `knowledge/entities/`. Color = `--t-green`.
  - `concept` — pages under `knowledge/concepts/`. Color = `--t-violet`.
  - `hub` — pages at the knowledge root. Color = `--accent`.
  - `note` — pages under `knowledge/notes/`. Color = `--ink-2`.
  - `flag` — synthesized for any page with frontmatter `contradiction: true`.
    Color = `--t-amber`.
- **Edges:** parsed from `[[knowledge-link]]` references in page bodies.
  Direction = embed-from → embed-to. Style derives from the source-node's
  credibility tier (when the source is on either end of the edge).

Output cached to `.biorouter-knowledge/graph-cache.json` and re-derived on
every successful commit. Frontend fetches it via `kb_get_graph`.

For KBs above ~500 nodes, we also store precomputed force-layout positions
alongside the cache so the UI can render without warming up the d3-force
simulation.

## History and revert

`kb_list_history` walks the git log. Each commit message follows a
machine-parseable header:

```text
[ingest] <source-title>

source_id: 8f3a2-arxiv-2403-12345
delta: +1 source · +6 pages · +9 links
```

Kinds: `ingest`, `link`, `flag`, `query`, `lint`, `restore`, `manual`.

`kb_preview_state` reads the tree at the requested SHA and returns the
diff vs HEAD. The UI renders this as a ghost-overlay on the graph (future
nodes drawn dashed and faded).

`kb_restore_state` uses `git revert <range>` to create a new commit that
applies the historical tree on top of HEAD. The restore itself shows up
as a log entry of kind `restore`, so it is auditable and itself reversible.

## The `.brkb` format

A `.brkb` file is a zip with the following root:

```text
<kb-id>/
├── manifest.yaml
├── schema.md
├── index.md
├── log.md
├── raw/ …
├── knowledge/ …
├── .biorouter-knowledge/ …
├── .gitignore
└── .git/
```

`kb_export_brkb` streams the zip to the response body. `kb_import_brkb`
accepts a multipart upload, unzips into a fresh directory under a new
`kb-id` (avoiding registry collisions), and updates the top-level registry
manifest.

## Server endpoints

New router under `crates/biorouter-server/src/routes/knowledge.rs`:

```text
GET    /knowledge/bases
POST   /knowledge/bases
DELETE /knowledge/bases/:id
GET    /knowledge/bases/:id
GET    /knowledge/bases/:id/graph
GET    /knowledge/bases/:id/pages
GET    /knowledge/bases/:id/pages/*path
PUT    /knowledge/bases/:id/pages/*path
GET    /knowledge/bases/:id/history
POST   /knowledge/bases/:id/preview
POST   /knowledge/bases/:id/restore
POST   /knowledge/bases/:id/raw          (multipart | {url} | {text})
POST   /knowledge/bases/:id/ingest       (SSE)
POST   /knowledge/bases/:id/query        (SSE)
POST   /knowledge/bases/:id/lint         (SSE)
GET    /knowledge/bases/:id/export       (binary)
POST   /knowledge/bases/import           (multipart)
POST   /knowledge/bases/:id/sources/:sid/reclassify
PUT    /knowledge/bases/:id/sources/:sid/credibility
```

After adding routes, `just generate-openapi` regenerates the TypeScript
client at `ui/desktop/src/api/`. We never hand-edit that directory.

## Frontend design

### Sidebar and routing

Three edits to wire the route in the existing pattern:

- `AppSidebar.tsx` — insert `{ type: 'item', path: '/knowledge',
  label: 'Knowledge', icon: Network, tooltip: 'Personal knowledge bases' }`
  between Skills and Settings. Add a new `Network` icon in
  `icons/app-icons.tsx` matching the mockup's "central node with rays" glyph.
- `App.tsx` — `import KnowledgeView from './components/knowledge/KnowledgeView'`,
  add `const KnowledgeRoute = () => <KnowledgeView />`, register
  `<Route path="knowledge" element={<KnowledgeRoute />} />`.

### Component tree

```text
ui/desktop/src/components/knowledge/
├── KnowledgeView.tsx
├── KBSelector/
│   ├── KBSelectorTrigger.tsx
│   └── KBSelectorPalette.tsx
├── IngestPanel/
│   ├── IngestPanel.tsx
│   ├── Dropzone.tsx
│   ├── PasteTextBox.tsx
│   ├── StagedList.tsx
│   └── IngestModelPicker.tsx
├── KnowledgeGraph/
│   ├── KnowledgeGraph.tsx
│   ├── nodeRenderer.ts
│   ├── edgeRenderer.ts
│   └── tooltips.tsx
├── ChangeLogDrawer/
│   ├── ChangeLogDrawer.tsx
│   ├── TimelineEntry.tsx
│   └── filters.tsx
├── DispatchProgress.tsx
├── KnowledgeContext.tsx
├── useKnowledgeBases.ts
├── useKnowledgeGraph.ts
├── useChangeLog.ts
├── useIngestStream.ts
└── styles.css
```

### Graph rendering

`react-force-graph-2d` with custom `nodeCanvasObject` / `linkCanvasObject`.
Hubs are extracted as the top-N degree-centrality pages and rendered larger
with bold labels. Hover dims non-neighbors to 0.6 opacity, highlights edges
to neighbors with `--t-green`, and shows a tooltip with title + tier +
reasoning + neighbor count. Click on a source node opens a side preview
with the original file, derived markdown, and `meta.yaml`. The graph
subscribes to the macro SSE stream so nodes appear with the mockup's `pop`
animation as ingestion progresses.

### Ingest flow

1. User picks a model in `IngestModelPicker`. Default = project default;
   choice persisted to `manifest.default_model`.
2. User drops files / pastes text / picks via Browse. Each lands in
   `StagedList`.
3. For pasted text, the UI runs URL extraction and shows discovered links
   as toggleable chips ("Will fetch & convert: 3 links").
4. **Digest** posts each staged item to `POST /knowledge/bases/:id/ingest`
   with the picked model. The SSE stream feeds `useIngestStream`, which
   renders into `DispatchProgress` and updates the graph live.
5. On completion, a toast shows the summary; the change log gains an entry.

### Change log and revert

Drawer slides in with the entry list, replicating the mockup's visual.
Click an entry → graph enters preview mode (read-only, banner across the
top). Future-state nodes draw as dashed ghosts. "Restore this state" calls
`POST /knowledge/bases/:id/restore` with the SHA. The restore commit is
itself added to the log. Filter chips operate client-side.

### Active KB state

`KnowledgeContext` holds `activeKbId` and persists to `localStorage`. The
setter also calls `kb_set_active` so chat sessions see the same active KB.
Chat input bar gains a small KB chip next to the model selector (only
when the extension is enabled). Click → opens the same Cmd-K palette as
the Knowledge view. × clears the active KB.

### Chat integration

Knowledge is a built-in extension, disabled by default. Settings →
Extensions surfaces the toggle. When enabled:

- The chat's system prompt is extended with a short block describing the
  Knowledge tools and the active-KB convention.
- Slash commands available in chat input:
  - `/kb` — open the KB palette
  - `/kb-ingest <url-or-path>` — pre-fills an `kb_ingest_source` turn
  - `/kb-query <question>` — pre-fills an `kb_query` turn

### Styling

We do not fork the mockup's beige palette. We map the mockup's mental model
into BioRouter's existing dashboard tokens — dot-grid background, card
surfaces, monospace accents — using the project palette. The credibility
ramp (`--cred-*`) is the only net-new color addition. A small scoped CSS
file `knowledge/styles.css` carries Knowledge-only adjustments so it does
not bleed into other surfaces.

## Test plan

### Rust unit tests

- `convert/*` — round-trip fixtures (HTML/PDF/DOCX/CSV) with `insta` snapshots.
- `credibility/identifiers.rs` — table-driven extraction coverage.
- `credibility/crossref.rs` + `openalex.rs` — `wiremock` against recorded
  fixtures, one assertion per tier.
- `credibility/allowlist.rs` — every publisher in the project allow-list
  resolves to `peer_reviewed`.
- `store.rs` — temp-dir based; create KB, write pages, read back, verify
  commit messages.
- `git.rs` — create/commit/list_history/preview/restore against a temp repo.
- `graph.rs` — fixture knowledge tree → snapshot the derived nodes+edges JSON.
- `brkb.rs` — pack → unpack → structural equality including `.git`.

### Integration tests (`crates/biorouter-test/`)

- `knowledge_ingest_integration.rs` — end-to-end with mocked LLM (VCR
  cassette via the existing `BIOROUTER_RECORD_MCP` pattern). Ingest a
  fixture article, verify raw/, knowledge pages, log entry, single commit.
- `knowledge_query_integration.rs` — pre-built KB, run `kb_query`, assert
  citation accuracy.
- `knowledge_lint_integration.rs` — fixture with planted contradictions,
  assert flags raised.
- `knowledge_revert_integration.rs` — ingest, ingest, restore, assert state.
- `knowledge_brkb_roundtrip.rs` — export → import → equality.

### Server tests

HTTP-level smoke tests via `tower::ServiceExt::oneshot` per endpoint. One
SSE test drives a mock ingest and verifies the event order
(`source-added` → `page-written` → `commit` → `done`).

### Frontend tests

- Vitest units for `KBSelector` (filter + keyboard nav), `Dropzone`
  (routing), `useIngestStream` (SSE parsing + abort), `useChangeLog`
  (preview/restore transitions), and node/edge renderers (canvas-mock
  snapshot per tier).
- Playwright E2E `knowledge_basic.spec.ts`: open Knowledge, create KB,
  drop a fixture file, click Digest, wait for graph to populate, open
  change log, restore, assert graph rewinds. Backend stubbed with recorded
  fixture SSE streams.

## Implementation phasing

> **Outcome.** All fifteen phases below were delivered. They were executed as the
> six implementation plans that sit beside this document in this folder — the
> backend foundation, the macros and sub-agent loop, the HTTP routes and export,
> the Knowledge view and ingest panel, the graph view and change log, and the chat
> integration and closeout. The table is preserved in its original future tense as
> the plan of record; read the plan documents for what each step actually produced.

The feature ships on a `feature/knowledge` branch off `main`, isolated
from the current `feature/multimodal-image-input` work. Phases:

| Phase | Scope | Verifiable outcome |
|---|---|---|
| 1. Backend skeleton | New `knowledge/` module, `KnowledgeServer` MCP scaffold with `kb_list_bases`, `kb_create_base`, `kb_list_pages`, `kb_read_page`, `kb_write_page`. Git wrapper. Default `schema.md` embedded. Register in `BUILTIN_EXTENSIONS`. | `cargo test -p biorouter-mcp` green; manual CLI invocation creates a KB. |
| 2. Conversion pipeline | `convert/` module for HTML/PDF/DOCX/CSV/url-fetch/note. `kb_add_raw_source`. | Per-format unit tests green; manual: drop a PDF, see raw/ populated. |
| 3. Credibility classifier | `credibility/` with identifiers → Crossref/OpenAlex → host patterns → agentic fallback. `kb_classify_source` + `credibility.yaml`. | Tier integration tests green; allow-list table test passes. |
| 4. Graph derivation | `graph.rs` + `kb_get_graph`. | Snapshot tests pass; manual inspection on a fixture KB. |
| 5. Macros: ingest | `ingest.rs` macro driver with sub-agent loop, SSE event emission, atomic commit, cancellation. | E2E ingest test green with mocked LLM. |
| 6. Macros: query + lint | `query.rs`, `lint.rs`. | Integration tests green. |
| 7. History + restore | `kb_list_history`, `kb_preview_state`, `kb_restore_state` (revert-style). | E2E revert test passes. |
| 8. .brkb export/import | `brkb.rs` + HTTP endpoints. | Round-trip integration test passes. |
| 9. Server routes | `routes/knowledge.rs`. `just generate-openapi`. | Server smoke tests green; generated TS client has the new methods. |
| 10. Frontend route + KB selector | Sidebar entry, route registration, `KnowledgeView` shell, `KBSelector`, `KnowledgeContext`. | Knowledge route loads; create/switch KBs works. |
| 11. Frontend ingest panel | `IngestPanel`, `Dropzone`, `PasteTextBox`, `StagedList`, `IngestModelPicker`, `useIngestStream`, `DispatchProgress`. | Drop file → Digest → live progress → toast. |
| 12. Frontend graph view | `KnowledgeGraph` with custom renderers and tooltips. Live updates via SSE. | Graph renders fixture KB; credibility colors correct; nodes appear with pop animation. |
| 13. Change log drawer + revert | `ChangeLogDrawer`, `TimelineEntry`, filter chips, preview banner, restore flow. | Playwright E2E covers revert. |
| 14. Chat integration | KB chip in chat input bar, `/kb*` slash commands, system-prompt extension. | Manual: enable extension, set active KB, ask chat to summarize. |
| 15. Polish + docs | Empty states, error toasts, keyboard shortcuts, graph performance pass, CLAUDE.md update. | `just check-everything` green; visual QA. |

The extension stays disabled by default until Phase 14, so partial work
never affects users who do not opt in.

## Risks and mitigations

- **Sub-agent loop quality** is bounded by `schema.md` + the operating
  procedure templates. We invest in 3–5 realistic biomedical fixture
  sources early to anchor evaluation.
- **PDF conversion quality** — `pdf-extract` handles text PDFs well,
  multi-column or image-only PDFs badly. LLM fallback covers these but is
  slow. We document the supported subset and treat the rest as best-effort.
- **Graph layout on large KBs** — d3-force becomes sluggish past ~500 nodes.
  Mitigation: server-side stored layout positions per commit; UI falls back
  to that pre-laid-out skeleton when a KB exceeds the threshold.
- **Crossref / OpenAlex availability** — responses cached per DOI; offline
  mode falls through to host patterns + agentic (with `network=offline` hint
  to the agent).
- **Concurrent edits** — chat agent and UI could both write to the same KB.
  A per-KB `Mutex` at the service layer serializes commits; the UI shows a
  "Knowledge is busy" badge while a macro is running.

## Open questions

None at design time.

> **Note.** No open questions were recorded when this design was approved, and none
> were added afterwards. Design questions that arose during implementation were
> resolved inside the individual plan documents rather than being folded back here.

## Related documentation

- [Plan 1 — storage, git and graph](plan-1-storage-git-and-graph.md) — the backend foundation this design specifies, task by task.
- [Plan 2 — macros and sub-agent loop](plan-2-macros-and-subagent-loop.md) — how the `kb_ingest_source` / `kb_query` / `kb_lint` macros were actually built.
- [Plan 3 — HTTP routes and export/import](plan-3-http-routes-and-export.md) — the shipped shape of the `/knowledge/*` routes and the `.brkb` format.
- [Plan 4 — Knowledge view and ingest panel](plan-4-knowledge-view-and-ingest.md) — the frontend route, KB selector, and ingest panel described above.
- [Knowledge ingestion format roadmap](../../knowledge-base/ingestion-format-roadmap.md) — the follow-on work extending the conversion pipeline beyond the formats listed here.
