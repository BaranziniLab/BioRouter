# Plan 5 — knowledge graph view and change-log drawer

> **What this is.** Plan 5 of the six-plan Knowledge buildout: replacing the Plan-4 right-column placeholder with a `react-force-graph-2d` credibility-coloured graph, and a git-history change-log drawer with preview and restore.
> **Status:** Historical record — executed and shipped. `ui/desktop/src/components/knowledge/` contains the `graph/` and `changelog/` directories this plan's decomposition map specifies, and `CLAUDE.md` describes the force-graph and change-log drawer as shipped. The unticked `- [ ]` checkboxes below are the plan as written, not outstanding work.
> **Audience:** developers working on the Knowledge desktop UI, and agents tracing why the graph or the history drawer behaves the way it does.
>
> **Plan numbering.** "Plan *N* of 6" refers to the six sibling documents in this
> folder, `plan-1-…` through `plan-6-…`, executed in order against the design in
> [`founding-design.md`](founding-design.md).

Plan 4 shipped the Knowledge route with a live ingest panel and a placeholder where the graph belonged. This plan fills that column in, and fixes a verified defect found by the Plan 4 end-to-end run: knowledge bases were producing 11 nodes and 0 edges, because the default schema never told the sub-agent to emit `[[knowledge-link]]` cross-references.

> **Warning — the UI mockup this plan depends on is unrecoverable.** Visual
> decisions below reference a mockup at `/Users/wgu/Downloads/biorouter_knowledge.html`.
> That is a personal Downloads path, never committed to the repo, and no copy
> survives. The shipped components under `ui/desktop/src/components/knowledge/` are
> the only remaining record of what was meant. A later redesign of this surface is
> captured in [`docs/history/ui-overhaul-2026-07/knowledge-view-redesign.md`](../ui-overhaul-2026-07/knowledge-view-redesign.md).

> **Note — worktree paths and line anchors are point-in-time.** Commands below
> `cd` into `/Users/wgu/Desktop/biorouter-knowledge`, the isolated git worktree the
> Knowledge branch was developed in; read it as your own checkout root. Line
> references into source files (for example `IngestPanel.tsx` "line 97-99") have
> long since moved — use the symbol names quoted alongside them.

**Goal:** Replace the Plan-4 `RightSidePlaceholder` in `KnowledgeView` with (a) a live `KnowledgeGraph` rendered via `react-force-graph-2d` (credibility-coloured nodes, hover dimming, click → side preview), and (b) a `ChangeLogDrawer` (slide-in sheet listing git history, click → preview at SHA, "Restore" → POST `/restore`). The graph auto-refreshes after every ingest. Also tighten `schema_default.md` so the sub-agent reliably emits `[[knowledge-link]]` cross-references (the verified gap from Plan 4 e2e: 11 nodes, 0 edges).

**Architecture:**
- **Two new panels** mounted in `KnowledgeView`'s right column: a `KnowledgeGraphPanel` (default) and a `ChangeLogDrawer` (a Radix `Sheet` overlay). A "Change log" button in the panel header opens the drawer.
- **Graph data flow:** `useKnowledgeGraph(kbId)` fetches `GET /knowledge/bases/:id/graph` via the generated SDK, exposes `{ graph, loading, error, refresh }`. The `IngestPanel` calls `refresh()` after each successful ingest. Two options were weighed — a shared `GraphRefreshContext`, or the simpler route of exposing a `refreshGraph` ref on the existing `KnowledgeContext`. **The plan takes the second:** Pre-step B and Task 5 both wire a `refreshGraphRef` onto `KnowledgeContext`.
- **Force-directed render:** `react-force-graph-2d`, with custom `nodeCanvasObject` (filled circle in credibility colour for sources, neutral fill for other kinds; bold ring + larger radius for the top-N degree-centrality "hub" nodes; red `!` badge for `retracted`). `linkCanvasObject` toggles solid vs dashed by source credibility. Hover dims non-neighbours to 0.35 opacity, highlights neighbour edges in `--cred-peer`.
- **Side preview:** Clicking a node opens an absolutely-positioned `NodePreview` card on the right of the panel showing the page's title, kind, credibility tier, and raw markdown body (fetched on demand via `kb_read_page` exposed as `GET /knowledge/bases/:id/page?path=...`). For source pages, it links to "Open raw" / "Open derived markdown".
- **Change log drawer:** Lists `HistoryEntry[]` (from `GET /history`) newest-first. Each row shows the relative time, `ChangeKind` chip, and summary. Clicking a row enters **preview mode** (graph header gets a banner "Previewing <sha-short> — read-only"). "Restore this state" calls `POST /restore`; the restore commit appears as a new `restore` log entry. Filter chips (`ingest|query|lint|restore`) operate client-side.
- **Schema fix for edges:** Append explicit `[[knowledge-link]]`-emission rules to `schema_default.md`. We do NOT auto-edit existing schemas (those are per-KB and user-owned).

**Tech stack:** React 19, TypeScript, the existing auto-generated TS API client at `ui/desktop/src/api/`, Tailwind utility classes, Radix `Sheet`. Two new npm deps in `ui/desktop/`: `react-force-graph-2d` and `d3-force` (peer of `react-force-graph-2d`). One new HTTP route on the backend: `GET /knowledge/bases/:id/page`.

**Source spec:** [`founding-design.md`](founding-design.md). UI mockup: `/Users/wgu/Downloads/biorouter_knowledge.html` (no longer available — see the warning above). Prior plans: [Plan 1](plan-1-storage-git-and-graph.md) (backend foundation), [Plan 2](plan-2-macros-and-subagent-loop.md) (macros + sub-agent), [Plan 3](plan-3-http-routes-and-export.md) (HTTP routes + export), [Plan 4](plan-4-knowledge-view-and-ingest.md) (frontend route + ingest panel).

**Series position:** Plan 5 of 6. [Plan 6](plan-6-chat-integration-and-closeout.md) = chat-side KB chip + slash commands + polish + docs.

**TDD note:** Same convention as Plans 1-4. Backend tasks add Rust integration tests under `crates/biorouter-server/tests/`. Frontend tasks rely on `npm run typecheck` + a Playwright smoke at the end (no unit-test culture in this repo for components). One task is a manual visual QA with the dev server.

**Execution convention:** the plan was written for an agentic worker driving it task-by-task with the `superpowers:subagent-driven-development` or `superpowers:executing-plans` skill. Steps use checkbox (`- [ ]`) syntax for tracking.

---

## Before starting

- [ ] **Pre-step A:** branch + baseline.

```bash
cd /Users/wgu/Desktop/biorouter-knowledge && source bin/activate-hermit
git rev-parse --abbrev-ref HEAD       # expect feature/knowledge
# Backend baseline
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -3
# Frontend baseline
cd ui/desktop && npm run lint:check 2>&1 | tail -5
cd ../..
```

If any baseline fails, fix that first — don't proceed on broken main.

- [ ] **Pre-step B:** familiarise yourself with the Plan-5 integration points.

  - Spec sections in [`founding-design.md`](founding-design.md): "Graph rendering", "Change log and revert", "History and revert", and "Color tokens" for the credibility palette.
  - Existing graph derivation: `crates/biorouter-mcp/src/knowledge/graph.rs` (regex `\[\[([^\]]+)\]\]` matches labels case-insensitively).
  - Existing HTTP: `get_graph`, `list_history`, `preview_state`, `restore_state` in `crates/biorouter-server/src/routes/knowledge.rs`.
  - Existing TS SDK methods (already generated): `getGraph`, `listHistory`, `previewState`, `restoreState` in `ui/desktop/src/api/sdk.gen.ts`.
  - Existing types in `ui/desktop/src/api/types.gen.ts`: `Graph`, `GraphNode`, `GraphEdge`, `HistoryEntry`, `ChangeKind`, `CredibilityTier`, `PageKind`.
  - Right-side placeholder to replace: `ui/desktop/src/components/knowledge/RightSidePlaceholder.tsx`.
  - Existing `KnowledgeContext`: `ui/desktop/src/components/knowledge/KnowledgeContext.tsx` (we will add a `refreshGraphRef` to it).
  - Existing ingest completion point: the `update(item.id, { status: 'done' });` call in `ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx` — this is where we'll trigger graph refresh.
  - Existing `Sheet` primitive: `ui/desktop/src/components/ui/sheet.tsx`.
  - Existing `Button` primitive: `ui/desktop/src/components/ui/button.tsx`.
  - Confirm 11-node / 0-edge gap on disk:
    ```bash
    find ~/.config/biorouter/knowledge -name 'graph-cache.json' -exec jq '.nodes | length, .edges | length' {} \;
    ```

---

## File structure (decomposition map)

**Backend (new + modified):**

```text
crates/biorouter-mcp/src/knowledge/
├── store.rs                         — MODIFY: add `read_page_body(kb_root, rel_path)` helper (already exists as resolve_readable_path; expose Read trail)
├── service.rs                       — MODIFY: add `read_page(kb_id, path) -> Result<String>`
└── schema_default.md                — MODIFY: explicit [[link]] emission rules

crates/biorouter-server/src/routes/knowledge.rs   — MODIFY: add GET /knowledge/bases/:id/page
crates/biorouter-server/tests/knowledge_routes.rs — MODIFY: add read-page integration test

ui/desktop/openapi.json              — REGEN via `just generate-openapi`
ui/desktop/src/api/sdk.gen.ts        — REGEN via `npm run generate-api`
ui/desktop/src/api/types.gen.ts      — REGEN via `npm run generate-api`
```

**Frontend (new):**

```text
ui/desktop/src/components/knowledge/
├── graph/                           — NEW directory
│   ├── KnowledgeGraphPanel.tsx      — header + ForceGraph + side preview composition
│   ├── ForceGraphCanvas.tsx         — thin wrapper around react-force-graph-2d
│   ├── NodePreview.tsx              — clicked-node preview card (kb_read_page)
│   ├── credColors.ts                — CredibilityTier → hex map (mirrors spec)
│   └── graphStyle.ts                — radius, line widths, dashed flags
├── changelog/                       — NEW directory
│   ├── ChangeLogDrawer.tsx          — Sheet + entries list + filter chips + preview banner
│   └── ChangeKindChip.tsx           — colored chip for ChangeKind
├── hooks/
│   ├── useKnowledgeGraph.ts         — NEW: fetch graph, expose refresh
│   ├── useHistory.ts                — NEW: fetch history list
│   └── usePagePreview.ts            — NEW: fetch single page body for NodePreview
└── KnowledgeContext.tsx             — MODIFY: add `registerGraphRefresh` + `triggerGraphRefresh`
```

**Frontend (modified):**

```text
ui/desktop/src/components/knowledge/RightSidePlaceholder.tsx        — DELETE
ui/desktop/src/components/knowledge/KnowledgeView.tsx               — swap placeholder for KnowledgeGraphPanel
ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx     — call triggerGraphRefresh() after each successful ingest
ui/desktop/package.json                                             — add react-force-graph-2d + d3-force
```

---

## Task 1: Backend `kb_read_page` HTTP route + service method

A `NodePreview` panel needs the markdown body of any KB page. The `KnowledgeService` already has `resolve_readable_path` (Plan-2 fix 751c24d) which permits both `knowledge/*.md` and `raw/*/source.md`. We just expose it over HTTP.

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/service.rs`
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`
- Modify: `crates/biorouter-server/tests/knowledge_routes.rs`

- [ ] **Step 1: Confirm where `read_page` already lives, if at all.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
rg -n "pub fn read_page|pub async fn read_page" crates/biorouter-mcp/src/knowledge/
rg -n "resolve_readable_path" crates/biorouter-mcp/src/knowledge/
```

Expected: `resolve_readable_path` exists in `paths.rs`; a `KnowledgeService::read_page` may or may not exist. If it already exists with the expected signature, skip Step 3 and only add the HTTP route.

- [ ] **Step 2: Write the failing route test.**

Open `crates/biorouter-server/tests/knowledge_routes.rs` and append:

```rust
#[tokio::test]
async fn read_page_returns_markdown_body() {
    let h = harness().await;
    let body = json!({ "name": "rp", "description": null });
    let kb: serde_json::Value = post(&h, "/knowledge/bases", body).await;
    let kb_id = kb["id"].as_str().unwrap();

    // Seed a page directly on disk via the store API (test helper).
    let kb_root = std::path::PathBuf::from(kb["path"].as_str().unwrap());
    let knowledge_dir = kb_root.join("knowledge").join("notes");
    std::fs::create_dir_all(&knowledge_dir).unwrap();
    std::fs::write(
        knowledge_dir.join("hello.md"),
        "---\ntitle: Hello\nkind: note\n---\n\nbody text\n",
    )
    .unwrap();

    let resp: serde_json::Value = get(
        &h,
        &format!("/knowledge/bases/{kb_id}/page?path=knowledge/notes/hello.md"),
    )
    .await;
    assert!(resp["content"].as_str().unwrap().contains("body text"));

    let missing = get_raw(
        &h,
        &format!("/knowledge/bases/{kb_id}/page?path=knowledge/notes/nope.md"),
    )
    .await;
    assert_eq!(missing.status(), 404);

    let bad = get_raw(
        &h,
        &format!("/knowledge/bases/{kb_id}/page?path=../../etc/passwd"),
    )
    .await;
    // resolve_readable_path rejects path traversal — should be 400 not 500
    assert_eq!(bad.status(), 400);
}
```

If `get` / `get_raw` helpers don't exist in the harness file, mirror the existing `post`/`post_raw` patterns at the top of `knowledge_routes.rs`. Look at how Plan 3 wrote a non-JSON helper for the export route — same idea.

- [ ] **Step 3: Run the test, confirm it fails.**

```bash
cargo test -p biorouter-server --test knowledge_routes read_page_returns_markdown_body 2>&1 | tail -10
```

Expected: FAIL (route not found / 404 / 500 on missing endpoint).

- [ ] **Step 4: Add `read_page` to `KnowledgeService`.**

Open `crates/biorouter-mcp/src/knowledge/service.rs`. Find an existing method like `list_pages` to match the style. Add:

```rust
impl KnowledgeService {
    // ... existing methods ...

    /// Read the raw markdown body of a page (knowledge/*.md or raw/*/source.md).
    /// Path is interpreted relative to the KB root. Path traversal rejected.
    pub fn read_page(&self, kb_id: &str, rel_path: &str) -> anyhow::Result<String> {
        let kb_root = self.kb_root(kb_id)?;
        let abs = crate::knowledge::paths::resolve_readable_path(&kb_root, rel_path)?;
        if !abs.exists() {
            anyhow::bail!("page not found: {rel_path}");
        }
        Ok(std::fs::read_to_string(&abs)?)
    }
}
```

If `self.kb_root(kb_id)` is named differently (e.g. `self.resolve_kb_root`), match what's in the file. The simplest grep:

```bash
rg -n "fn kb_root|fn resolve_kb_root|fn root_for" crates/biorouter-mcp/src/knowledge/service.rs
```

- [ ] **Step 5: Add the HTTP route.**

In `crates/biorouter-server/src/routes/knowledge.rs`:

5a. Add to the router builder (find the `Router::new()` chain that already has `/page` peers like `/graph`, `/history`):

```rust
.route("/bases/{id}/page", get(read_page))
```

5b. Add the handler near the existing `get_graph` block (just before "Task 7" comment):

```rust
#[derive(serde::Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::IntoParams))]
pub struct ReadPageQuery {
    pub path: String,
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ReadPageResponse {
    pub content: String,
}

#[utoipa::path(
    get, path = "/knowledge/bases/{id}/page",
    params(
        ("id" = String, Path, description = "Knowledge base ID"),
        ReadPageQuery,
    ),
    responses(
        (status = 200, description = "Page content", body = ReadPageResponse),
        (status = 400, description = "Invalid path"),
        (status = 404, description = "Page not found"),
    )
)]
pub async fn read_page(
    State(svc): State<Arc<KnowledgeService>>,
    Path(id): Path<String>,
    Query(q): Query<ReadPageQuery>,
) -> Result<Json<ReadPageResponse>, (StatusCode, String)> {
    match svc.read_page(&id, &q.path) {
        Ok(content) => Ok(Json(ReadPageResponse { content })),
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("path traversal") || msg.contains("outside") {
                StatusCode::BAD_REQUEST
            } else if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((status, msg))
        }
    }
}
```

5c. Wire it into the OpenAPI doc. Find the `#[derive(OpenApi)]` block (search `OpenApi` in the file or in `lib.rs`) and add `read_page` to the `paths(...)` list, and `ReadPageQuery`, `ReadPageResponse` to `schemas(...)`.

- [ ] **Step 6: Run the test, confirm it passes.**

```bash
cargo test -p biorouter-server --test knowledge_routes read_page_returns_markdown_body 2>&1 | tail -10
```

Expected: PASS. If `resolve_readable_path` returns a string-matchable error for the `../../etc/passwd` case that doesn't match the `path traversal|outside` heuristic, broaden the heuristic to include the actual phrase used in [`crates/biorouter-mcp/src/knowledge/paths.rs`](../../../crates/biorouter-mcp/src/knowledge/paths.rs) (grep `bail!\|anyhow!` there).

- [ ] **Step 7: Regenerate OpenAPI + TS client.**

```bash
just generate-openapi
cd ui/desktop && npm run generate-api 2>&1 | tail -5
cd ../..
```

Then verify the new method appears:

```bash
grep -n "readPage\|read_page" ui/desktop/src/api/sdk.gen.ts
```

Expected: one `export const readPage = ...` line.

- [ ] **Step 8: Commit.**

```bash
git add crates/biorouter-mcp/src/knowledge/service.rs \
        crates/biorouter-server/src/routes/knowledge.rs \
        crates/biorouter-server/tests/knowledge_routes.rs \
        ui/desktop/openapi.json \
        ui/desktop/src/api/sdk.gen.ts \
        ui/desktop/src/api/types.gen.ts
git commit -m "feat(knowledge): GET /knowledge/bases/:id/page (markdown body for graph node preview)"
```

---

## Task 2: Schema default — explicit `[[knowledge-link]]` emission rules

The Plan-4 verification flagged 11 nodes / 0 edges because the sub-agent only used `[[link]]` syntax inconsistently in body prose. Strengthen `schema_default.md` so newly created KBs use clearer rules. We do **not** touch existing per-KB `schema.md` files — those are user-owned.

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/schema_default.md`

- [ ] **Step 1: Insert a new section after "Page format".**

Open `crates/biorouter-mcp/src/knowledge/schema_default.md`. Find the line `Body is prose markdown with [[knowledge-link]] cross-references.` and replace it with:

```markdown
Body is prose markdown with `[[knowledge-link]]` cross-references.

### Cross-reference rules (the graph depends on these)

The knowledge graph is derived **purely** from `[[link]]` patterns in page
bodies. If you do not emit links, the graph will have nodes but no edges.

When you write or update any knowledge page:

1. Every mention of another entity or concept that has (or should have) its
   own page **must** be wrapped in `[[double brackets]]`. Match the target
   page's title exactly (case-insensitive); the deriver slugifies both sides.
   Good: `[[EPAS1]] interacts with [[HIF2A]] under [[hypoxia]].`
   Bad:  `EPAS1 interacts with HIF2A under hypoxia.`
2. Every source page **must** include a `## Related pages` section listing
   every entity/concept it touches, one `- [[Name]]` bullet per line.
3. Every entity/concept page **must** include a `## Sources` section with
   one `- [[source-id]]` bullet per supporting source.
4. Prefer linking over re-stating. If a fact lives on another page, write
   `See [[Page Name]]` instead of restating it.

The lint workflow (`kb_lint`) reports pages with no inbound links as orphans
— fix them by adding inbound `[[links]]` from related pages.
```

- [ ] **Step 2: Verify the new file lints / has no broken markdown.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
cargo build -p biorouter-mcp 2>&1 | tail -3
# schema_default.md is embedded with include_str! — confirm:
rg -n 'include_str!.*schema_default' crates/biorouter-mcp/src/knowledge/
```

Expected: build succeeds. If `schema_default.md` is `include_str!`'d, the build is the only check that matters.

- [ ] **Step 3: Commit.**

```bash
git add crates/biorouter-mcp/src/knowledge/schema_default.md
git commit -m "fix(knowledge): default schema spells out [[link]] emission rules so graph edges actually exist"
```

---

## Task 3: Install graph dependencies

`react-force-graph-2d` (~30kb) wraps `force-graph` + `d3-force` with React lifecycle handling. `d3-force` is a peer dep but is already pulled in transitively; we declare it explicitly so the package-lock pins it.

**Files:**
- Modify: `ui/desktop/package.json`
- Modify: `ui/desktop/package-lock.json`

- [ ] **Step 1: Install.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm install react-force-graph-2d@1.27.1 d3-force@3.0.0 --save
```

Pin to known-good versions to avoid surprise ESM/CJS breakage (the package has a history of upstream issues). If npm refuses these exact pins because of peer constraints, use whatever's currently latest and note the version in the commit message.

- [ ] **Step 2: Verify the imports resolve in TS.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
node -e "import('react-force-graph-2d').then(m => console.log(Object.keys(m)))"
```

Expected: prints `[ 'default' ]` (the default export is the React component).

- [ ] **Step 3: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/package.json ui/desktop/package-lock.json
git commit -m "feat(ui): add react-force-graph-2d + d3-force for knowledge graph rendering"
```

---

## Task 4: `credColors.ts` and `graphStyle.ts` — visual constants

Centralised so changes to the palette happen in one place. Mirrors the spec's "Credibility palette" section.

**Files:**
- Create: `ui/desktop/src/components/knowledge/graph/credColors.ts`
- Create: `ui/desktop/src/components/knowledge/graph/graphStyle.ts`

- [ ] **Step 1: Create `credColors.ts`.**

```typescript
// ui/desktop/src/components/knowledge/graph/credColors.ts
import type { CredibilityTier, PageKind } from '../../../api/types.gen';

// Mirrors the spec's Credibility palette (docs/superpowers/specs/2026-05-30-knowledge-design.md ~L195).
// Blues = academic credibility ramp; warmer colors = web/personal/retracted.
export const credColor: Record<CredibilityTier, string> = {
  peer_reviewed: '#3d4878',
  book:          '#5a6394',
  preprint:      '#7d83b0',
  gray_lit:      '#a8acc8',
  web:           '#c9866a',
  personal:      '#b08aa8',
};
// `retracted` is a separate flag on the source meta, not a tier — color separately:
export const retractedColor = '#c98b8b';

// Non-source page kinds keep neutral colors so credibility coloring does not
// collide with node-kind coloring.
export const kindColor: Record<Exclude<PageKind, 'source'>, string> = {
  entity:   '#5b8aa5',
  concept:  '#7aa57c',
  hub:      '#c8a05b',
  note:     '#9a9a9a',
  flag:     '#c98b8b',
};

export function nodeFill(node: { kind: PageKind; credibility_tier?: CredibilityTier | null }): string {
  if (node.kind === 'source' && node.credibility_tier) {
    return credColor[node.credibility_tier];
  }
  if (node.kind === 'source') return '#a8acc8'; // unclassified source → gray-lit shade
  return kindColor[node.kind as Exclude<PageKind, 'source'>] ?? '#9a9a9a';
}
```

- [ ] **Step 2: Create `graphStyle.ts`.**

```typescript
// ui/desktop/src/components/knowledge/graph/graphStyle.ts
import type { CredibilityTier } from '../../../api/types.gen';

export const NODE_BASE_RADIUS = 5;
export const HUB_RADIUS = 9;
export const LABEL_FONT_PX = 11;
export const LABEL_FONT_PX_HUB = 13;
export const DIMMED_OPACITY = 0.35;

/// Width + dashed-ness from the source page's credibility tier.
/// peer_reviewed/book → solid 1.6px, preprint solid 1.3px, gray_lit solid 1.2px,
/// web/personal dashed 1.0px. Default solid 1.0px when tier unknown.
export function edgeStyle(tier: CredibilityTier | null | undefined): { width: number; dash: number[] | null } {
  switch (tier) {
    case 'peer_reviewed':
    case 'book':       return { width: 1.6, dash: null };
    case 'preprint':   return { width: 1.3, dash: null };
    case 'gray_lit':   return { width: 1.2, dash: null };
    case 'web':
    case 'personal':   return { width: 1.0, dash: [4, 3] };
    default:           return { width: 1.0, dash: null };
  }
}

/// Top-N degree centrality threshold for "hub" treatment.
export const HUB_TOP_N = 6;
```

- [ ] **Step 3: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors.

- [ ] **Step 4: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/graph/
git commit -m "feat(ui): credibility colour map + graph visual constants"
```

---

## Task 5: `useKnowledgeGraph` hook + `KnowledgeContext` refresh wiring

The hook owns graph state. The context exposes a `triggerGraphRefresh()` callable so `IngestPanel` can poke the panel without prop-drilling.

**Files:**
- Create: `ui/desktop/src/components/knowledge/hooks/useKnowledgeGraph.ts`
- Modify: `ui/desktop/src/components/knowledge/KnowledgeContext.tsx`

- [ ] **Step 1: Create `useKnowledgeGraph.ts`.**

```typescript
// ui/desktop/src/components/knowledge/hooks/useKnowledgeGraph.ts
import { useCallback, useEffect, useState } from 'react';
import { getGraph } from '../../../api';
import type { Graph } from '../../../api/types.gen';

export interface UseKnowledgeGraphResult {
  graph: Graph | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useKnowledgeGraph(kbId: string | null): UseKnowledgeGraphResult {
  const [graph, setGraph] = useState<Graph | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!kbId) {
      setGraph(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await getGraph({ path: { id: kbId }, throwOnError: true });
      setGraph(res.data ?? null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setGraph(null);
    } finally {
      setLoading(false);
    }
  }, [kbId]);

  useEffect(() => { void refresh(); }, [refresh]);

  return { graph, loading, error, refresh };
}
```

- [ ] **Step 2: Extend `KnowledgeContext` with a refresh ref.**

Open `ui/desktop/src/components/knowledge/KnowledgeContext.tsx`. Replace the interface + provider with:

```typescript
import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { listBases } from '../../api';
import type { Manifest } from '../../api/types.gen';

const STORAGE_KEY_ACTIVE_KB = 'knowledge_active_kb';

interface KnowledgeContextType {
  bases: Manifest[];
  loading: boolean;
  activeKb: Manifest | null;
  activeKbId: string | null;
  setActiveKbId: (id: string | null) => void;
  refresh: () => Promise<void>;
  /// Registered by KnowledgeGraphPanel so IngestPanel can request a re-fetch
  /// after each successful ingest. No-op if no graph is mounted.
  registerGraphRefresh: (fn: (() => Promise<void>) | null) => void;
  triggerGraphRefresh: () => void;
}

const KnowledgeContext = createContext<KnowledgeContextType | null>(null);

export function KnowledgeProvider({ children }: { children: ReactNode }) {
  const [bases, setBases] = useState<Manifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY_ACTIVE_KB)
  );
  const graphRefreshRef = useRef<(() => Promise<void>) | null>(null);

  const setActiveKbId = useCallback((id: string | null) => {
    setActiveKbIdState(id);
    if (id) localStorage.setItem(STORAGE_KEY_ACTIVE_KB, id);
    else localStorage.removeItem(STORAGE_KEY_ACTIVE_KB);
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listBases({ throwOnError: true });
      setBases(res.data || []);
    } catch (err) {
      console.error('listBases failed:', err);
      setBases([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const registerGraphRefresh = useCallback((fn: (() => Promise<void>) | null) => {
    graphRefreshRef.current = fn;
  }, []);

  const triggerGraphRefresh = useCallback(() => {
    void graphRefreshRef.current?.();
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (activeKbId && bases.length > 0 && !bases.some((b) => b.id === activeKbId)) {
      setActiveKbId(null);
    }
  }, [activeKbId, bases, setActiveKbId]);

  const activeKb = useMemo(
    () => bases.find((b) => b.id === activeKbId) ?? null,
    [bases, activeKbId]
  );

  const value: KnowledgeContextType = {
    bases,
    loading,
    activeKb,
    activeKbId,
    setActiveKbId,
    refresh,
    registerGraphRefresh,
    triggerGraphRefresh,
  };

  return <KnowledgeContext.Provider value={value}>{children}</KnowledgeContext.Provider>;
}

export function useKnowledge(): KnowledgeContextType {
  const ctx = useContext(KnowledgeContext);
  if (!ctx) throw new Error('useKnowledge must be used inside <KnowledgeProvider>');
  return ctx;
}
```

The earlier `// TODO Plan 6: ...` comment was scaffolding for active-KB server sync — that's still Plan 6, leave it removed for now (no behavioral change).

- [ ] **Step 3: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors. If `useKnowledge` is imported anywhere already, all existing call sites still work (no removed methods).

- [ ] **Step 4: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/hooks/useKnowledgeGraph.ts \
        ui/desktop/src/components/knowledge/KnowledgeContext.tsx
git commit -m "feat(ui): useKnowledgeGraph hook + graph-refresh ref in KnowledgeContext"
```

---

## Task 6: `ForceGraphCanvas` — react-force-graph-2d wrapper

The actual force-directed render. Self-contained: receives `graph`, `hoveredId`, `selectedId`, `previewSet` (for dashed ghost nodes in preview mode) and renders. No data fetching here.

**Files:**
- Create: `ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx`

- [ ] **Step 1: Create the component.**

```typescript
// ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx
import { useEffect, useMemo, useRef, useState } from 'react';
import ForceGraph2D, { ForceGraphMethods } from 'react-force-graph-2d';
import type { Graph, GraphNode } from '../../../api/types.gen';
import { nodeFill, retractedColor } from './credColors';
import {
  DIMMED_OPACITY,
  edgeStyle,
  HUB_RADIUS,
  HUB_TOP_N,
  LABEL_FONT_PX,
  LABEL_FONT_PX_HUB,
  NODE_BASE_RADIUS,
} from './graphStyle';

interface Props {
  graph: Graph;
  selectedId: string | null;
  hoveredId: string | null;
  onHover: (id: string | null) => void;
  onNodeClick: (node: GraphNode) => void;
  /// Optional: if set, nodes whose id is NOT in this set are dimmed and dashed
  /// (used in "preview at SHA" mode to ghost future-state additions).
  visibleSet: Set<string> | null;
}

interface Sized { width: number; height: number; }

function useSize(): [React.RefObject<HTMLDivElement>, Sized] {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<Sized>({ width: 600, height: 400 });
  useEffect(() => {
    if (!ref.current) return;
    const el = ref.current;
    const ro = new ResizeObserver(() => {
      const r = el.getBoundingClientRect();
      setSize({ width: Math.max(1, Math.floor(r.width)), height: Math.max(1, Math.floor(r.height)) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size];
}

export function ForceGraphCanvas({
  graph,
  selectedId,
  hoveredId,
  onHover,
  onNodeClick,
  visibleSet,
}: Props) {
  const fgRef = useRef<ForceGraphMethods | undefined>(undefined);
  const [containerRef, size] = useSize();

  // Convert API graph → force-graph data. react-force-graph mutates nodes
  // (adds x/y) and links — so we keep our own stable copies.
  const data = useMemo(() => {
    const nodes = graph.nodes.map((n) => ({ ...n }));
    const links = graph.edges.map((e) => ({ source: e.from, target: e.to, relation: e.relation }));
    return { nodes, links };
  }, [graph]);

  // Degree centrality for hub treatment.
  const hubIds = useMemo(() => {
    const deg = new Map<string, number>();
    for (const e of graph.edges) {
      deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
      deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
    }
    return new Set(
      [...deg.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, HUB_TOP_N)
        .map(([id]) => id)
    );
  }, [graph]);

  // Neighbour map for hover dimming.
  const neighbours = useMemo(() => {
    const m = new Map<string, Set<string>>();
    const touch = (a: string, b: string) => {
      if (!m.has(a)) m.set(a, new Set());
      m.get(a)!.add(b);
    };
    for (const e of graph.edges) {
      touch(e.from, e.to);
      touch(e.to, e.from);
    }
    return m;
  }, [graph]);

  const focusId = selectedId ?? hoveredId;

  return (
    <div ref={containerRef} className="w-full h-full overflow-hidden">
      <ForceGraph2D
        ref={fgRef as unknown as React.MutableRefObject<ForceGraphMethods>}
        graphData={data}
        width={size.width}
        height={size.height}
        cooldownTicks={120}
        d3VelocityDecay={0.3}
        nodeRelSize={NODE_BASE_RADIUS}
        backgroundColor="transparent"
        onNodeHover={(n) => onHover((n as GraphNode | null)?.id ?? null)}
        onNodeClick={(n) => onNodeClick(n as GraphNode)}
        nodeCanvasObject={(rawNode, ctx, globalScale) => {
          const n = rawNode as GraphNode & { x: number; y: number };
          const isHub = hubIds.has(n.id);
          const r = isHub ? HUB_RADIUS : NODE_BASE_RADIUS;
          const dim =
            (focusId && focusId !== n.id && !neighbours.get(focusId)?.has(n.id)) ||
            (visibleSet && !visibleSet.has(n.id));
          ctx.globalAlpha = dim ? DIMMED_OPACITY : 1.0;
          ctx.beginPath();
          ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
          ctx.fillStyle = nodeFill(n);
          ctx.fill();
          if (isHub) {
            ctx.lineWidth = 1.5;
            ctx.strokeStyle = '#1f1f1f';
            ctx.stroke();
          }
          // Label
          const fs = (isHub ? LABEL_FONT_PX_HUB : LABEL_FONT_PX) / globalScale;
          ctx.font = `${isHub ? '600' : '400'} ${fs}px ui-sans-serif, system-ui, -apple-system`;
          ctx.fillStyle = '#cfd2dc';
          ctx.textAlign = 'left';
          ctx.textBaseline = 'middle';
          ctx.fillText(' ' + n.label, n.x + r + 1, n.y);
          ctx.globalAlpha = 1.0;
        }}
        linkCanvasObject={(rawLink, ctx) => {
          const l = rawLink as { source: GraphNode & { x: number; y: number }; target: GraphNode & { x: number; y: number } };
          const tier =
            (l.source.kind === 'source' ? l.source.credibility_tier : null) ??
            (l.target.kind === 'source' ? l.target.credibility_tier : null);
          const style = edgeStyle(tier);
          const dim =
            focusId &&
            l.source.id !== focusId &&
            l.target.id !== focusId;
          ctx.globalAlpha = dim ? DIMMED_OPACITY : 0.9;
          ctx.strokeStyle = focusId && (l.source.id === focusId || l.target.id === focusId)
            ? '#7aa57c' // --t-green
            : '#5b6072';
          ctx.lineWidth = style.width;
          if (style.dash) ctx.setLineDash(style.dash);
          else ctx.setLineDash([]);
          ctx.beginPath();
          ctx.moveTo(l.source.x, l.source.y);
          ctx.lineTo(l.target.x, l.target.y);
          ctx.stroke();
          ctx.setLineDash([]);
          ctx.globalAlpha = 1.0;
        }}
        linkCanvasObjectMode={() => 'replace'}
      />
    </div>
  );
}

// Visualise retractedColor in dev to keep it tree-shaken-out warnings quiet
// when no retracted sources exist in the graph yet.
void retractedColor;
```

- [ ] **Step 2: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors. If `ForceGraphMethods` doesn't export from the version we pinned, change the ref type to `unknown` — runtime won't care; we only need the imperative ref for future Plan-6 features.

- [ ] **Step 3: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx
git commit -m "feat(ui): ForceGraphCanvas renders knowledge graph with credibility colors + hover dimming"
```

---

## Task 7: `usePagePreview` hook + `NodePreview` component

When the user clicks a node, fetch the page body and show it in a side card. Falls back to a "page missing" message for orphaned graph nodes.

**Files:**
- Create: `ui/desktop/src/components/knowledge/hooks/usePagePreview.ts`
- Create: `ui/desktop/src/components/knowledge/graph/NodePreview.tsx`

- [ ] **Step 1: Create `usePagePreview.ts`.**

```typescript
// ui/desktop/src/components/knowledge/hooks/usePagePreview.ts
import { useEffect, useState } from 'react';
import { readPage } from '../../../api';

export interface UsePagePreviewResult {
  content: string | null;
  loading: boolean;
  error: string | null;
}

export function usePagePreview(kbId: string | null, path: string | null): UsePagePreviewResult {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!kbId || !path) {
      setContent(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    (async () => {
      try {
        const res = await readPage({ path: { id: kbId }, query: { path }, throwOnError: true });
        if (!cancelled) setContent(res.data?.content ?? null);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setContent(null);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [kbId, path]);

  return { content, loading, error };
}
```

If the generated SDK uses `query` differently (some versions inline path queries), grep the file:

```bash
grep -n "readPage" /Users/wgu/Desktop/biorouter-knowledge/ui/desktop/src/api/sdk.gen.ts
```

and follow whatever signature it generated.

- [ ] **Step 2: Create `NodePreview.tsx`.**

```typescript
// ui/desktop/src/components/knowledge/graph/NodePreview.tsx
import { X } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { usePagePreview } from '../hooks/usePagePreview';
import { nodeFill } from './credColors';

interface Props {
  kbId: string;
  node: GraphNode;
  onClose: () => void;
}

export function NodePreview({ kbId, node, onClose }: Props) {
  const { content, loading, error } = usePagePreview(kbId, node.path);

  return (
    <div className="absolute top-12 right-4 w-[360px] max-h-[calc(100%-5rem)] bg-background-surface border border-border-subtle rounded-lg shadow-lg flex flex-col overflow-hidden z-10">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle">
        <div className="flex items-center gap-2 min-w-0">
          <span
            aria-hidden
            className="w-2.5 h-2.5 rounded-full flex-shrink-0"
            style={{ background: nodeFill(node) }}
          />
          <div className="flex flex-col min-w-0">
            <div className="text-sm font-medium truncate">{node.label}</div>
            <div className="text-xs text-text-muted truncate">
              {node.kind}
              {node.credibility_tier ? ` · ${node.credibility_tier.replace('_', ' ')}` : ''}
            </div>
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose} className="flex-shrink-0">
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 text-xs leading-relaxed font-mono whitespace-pre-wrap text-text-default">
        {loading && <span className="text-text-muted">Loading…</span>}
        {error && <span className="text-red-400">{error}</span>}
        {!loading && !error && (content ?? <span className="text-text-muted">No content.</span>)}
      </div>
      <div className="border-t border-border-subtle px-4 py-2 text-xs text-text-muted">
        {node.path}
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors.

- [ ] **Step 4: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/hooks/usePagePreview.ts \
        ui/desktop/src/components/knowledge/graph/NodePreview.tsx
git commit -m "feat(ui): NodePreview card shows page body when a graph node is clicked"
```

---

## Task 8: `KnowledgeGraphPanel` — composes everything for the right column

Owns the header (active KB name + "Change log" button), the `ForceGraphCanvas`, and the `NodePreview`. Registers its refresh with `KnowledgeContext`. Shows empty / loading states.

**Files:**
- Create: `ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx`

- [ ] **Step 1: Create the component.**

```typescript
// ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
import { useEffect, useState } from 'react';
import { History, RefreshCw } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { useKnowledge } from '../KnowledgeContext';
import { useKnowledgeGraph } from '../hooks/useKnowledgeGraph';
import { ForceGraphCanvas } from './ForceGraphCanvas';
import { NodePreview } from './NodePreview';

interface Props {
  onOpenChangeLog: () => void;
  /// When set, the panel is in read-only "preview at SHA" mode. The banner
  /// shows the SHA; the graph still renders the current data (ghosting of
  /// future-state nodes is a Plan-6 polish item).
  previewSha: string | null;
  onClearPreview: () => void;
}

export function KnowledgeGraphPanel({ onOpenChangeLog, previewSha, onClearPreview }: Props) {
  const { activeKbId, activeKb, registerGraphRefresh } = useKnowledge();
  const { graph, loading, error, refresh } = useKnowledgeGraph(activeKbId);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [selected, setSelected] = useState<GraphNode | null>(null);

  // Expose refresh() to KnowledgeContext so IngestPanel can call it after each ingest.
  useEffect(() => {
    registerGraphRefresh(refresh);
    return () => registerGraphRefresh(null);
  }, [refresh, registerGraphRefresh]);

  return (
    <div className="flex flex-col h-full relative">
      <div className="flex items-center justify-between px-6 py-3 border-b border-border-subtle">
        <div className="flex items-center gap-2 text-xs text-text-muted">
          <span className="font-medium text-text-default">
            {activeKb?.name ?? 'No knowledge base'}
          </span>
          {graph && (
            <span>
              · {graph.nodes.length} {graph.nodes.length === 1 ? 'page' : 'pages'}
              {' · '}
              {graph.edges.length} {graph.edges.length === 1 ? 'link' : 'links'}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void refresh()}
            disabled={!activeKbId || loading}
            title="Refresh graph"
          >
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={onOpenChangeLog}
            disabled={!activeKbId}
            title="Open change log"
          >
            <History className="h-4 w-4 mr-1" />
            Change log
          </Button>
        </div>
      </div>

      {previewSha && (
        <div className="px-6 py-2 bg-yellow-900/30 border-b border-yellow-700/50 text-xs text-yellow-200 flex items-center justify-between">
          <span>Previewing commit {previewSha.slice(0, 7)} — read-only</span>
          <button onClick={onClearPreview} className="underline">
            Exit preview
          </button>
        </div>
      )}

      <div className="flex-1 relative min-h-0">
        {!activeKbId && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-text-muted">
            Select a knowledge base to see its graph.
          </div>
        )}
        {activeKbId && error && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-red-400">
            {error}
          </div>
        )}
        {activeKbId && !error && graph && graph.nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center text-sm text-text-muted">
            No pages yet. Ingest a source to populate the graph.
          </div>
        )}
        {activeKbId && !error && graph && graph.nodes.length > 0 && (
          <ForceGraphCanvas
            graph={graph}
            selectedId={selected?.id ?? null}
            hoveredId={hoveredId}
            onHover={setHoveredId}
            onNodeClick={(n) => setSelected(n)}
            visibleSet={null}
          />
        )}
        {selected && activeKbId && (
          <NodePreview kbId={activeKbId} node={selected} onClose={() => setSelected(null)} />
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors.

- [ ] **Step 3: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/graph/KnowledgeGraphPanel.tsx
git commit -m "feat(ui): KnowledgeGraphPanel composes ForceGraph + NodePreview + refresh wiring"
```

---

## Task 9: `useHistory` hook + `ChangeKindChip` + `ChangeLogDrawer`

The drawer fetches `/history`, lists entries, supports filter chips, and triggers restore.

**Files:**
- Create: `ui/desktop/src/components/knowledge/hooks/useHistory.ts`
- Create: `ui/desktop/src/components/knowledge/changelog/ChangeKindChip.tsx`
- Create: `ui/desktop/src/components/knowledge/changelog/ChangeLogDrawer.tsx`

- [ ] **Step 1: Create `useHistory.ts`.**

```typescript
// ui/desktop/src/components/knowledge/hooks/useHistory.ts
import { useCallback, useEffect, useState } from 'react';
import { listHistory, restoreState } from '../../../api';
import type { HistoryEntry } from '../../../api/types.gen';

export interface UseHistoryResult {
  history: HistoryEntry[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  restore: (commitSha: string) => Promise<string>;   // returns new commit sha
}

export function useHistory(kbId: string | null): UseHistoryResult {
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!kbId) {
      setHistory([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const res = await listHistory({ path: { id: kbId }, query: { limit: 200 }, throwOnError: true });
      setHistory(res.data ?? []);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setHistory([]);
    } finally {
      setLoading(false);
    }
  }, [kbId]);

  const restore = useCallback(async (commitSha: string) => {
    if (!kbId) throw new Error('no active KB');
    const res = await restoreState({
      path: { id: kbId },
      body: { commit_sha: commitSha },
      throwOnError: true,
    });
    const sha = res.data?.new_commit_sha ?? '';
    await refresh();
    return sha;
  }, [kbId, refresh]);

  useEffect(() => { void refresh(); }, [refresh]);

  return { history, loading, error, refresh, restore };
}
```

- [ ] **Step 2: Create `ChangeKindChip.tsx`.**

```typescript
// ui/desktop/src/components/knowledge/changelog/ChangeKindChip.tsx
import type { ChangeKind } from '../../../api/types.gen';

const styleByKind: Record<ChangeKind, string> = {
  ingest:  'bg-blue-900/40 text-blue-200',
  link:    'bg-purple-900/40 text-purple-200',
  flag:    'bg-red-900/40 text-red-200',
  query:   'bg-green-900/40 text-green-200',
  lint:    'bg-yellow-900/40 text-yellow-200',
  restore: 'bg-orange-900/40 text-orange-200',
  manual:  'bg-zinc-800 text-zinc-300',
};

export function ChangeKindChip({ kind }: { kind: ChangeKind }) {
  return (
    <span className={`inline-block text-[10px] uppercase tracking-wide rounded px-1.5 py-0.5 ${styleByKind[kind]}`}>
      {kind}
    </span>
  );
}
```

- [ ] **Step 3: Create `ChangeLogDrawer.tsx`.**

```typescript
// ui/desktop/src/components/knowledge/changelog/ChangeLogDrawer.tsx
import { useMemo, useState } from 'react';
import type { ChangeKind, HistoryEntry } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '../../ui/sheet';
import { useKnowledge } from '../KnowledgeContext';
import { useHistory } from '../hooks/useHistory';
import { ChangeKindChip } from './ChangeKindChip';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPreview: (sha: string) => void;
  onRestored: () => void;
}

const ALL_KINDS: ChangeKind[] = ['ingest', 'link', 'flag', 'query', 'lint', 'restore', 'manual'];

function relativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  const diff = Date.now() - t;
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}

export function ChangeLogDrawer({ open, onOpenChange, onPreview, onRestored }: Props) {
  const { activeKbId, triggerGraphRefresh } = useKnowledge();
  const { history, loading, error, restore } = useHistory(activeKbId);
  const [activeKinds, setActiveKinds] = useState<Set<ChangeKind>>(new Set(ALL_KINDS));
  const [restoring, setRestoring] = useState<string | null>(null);

  const filtered = useMemo(
    () => history.filter((h) => activeKinds.has(h.kind)),
    [history, activeKinds]
  );

  function toggleKind(k: ChangeKind) {
    setActiveKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }

  async function handleRestore(entry: HistoryEntry) {
    if (!window.confirm(`Restore knowledge base to ${entry.commit_sha.slice(0, 7)}? A new revert commit will be created.`)) {
      return;
    }
    setRestoring(entry.commit_sha);
    try {
      await restore(entry.commit_sha);
      triggerGraphRefresh();
      onRestored();
    } catch (err) {
      window.alert(`Restore failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setRestoring(null);
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[420px] sm:max-w-[420px] flex flex-col p-0">
        <SheetHeader className="px-5 py-3 border-b border-border-subtle">
          <SheetTitle className="text-sm">Change log</SheetTitle>
        </SheetHeader>

        <div className="px-5 py-2 border-b border-border-subtle flex flex-wrap gap-1.5">
          {ALL_KINDS.map((k) => (
            <button
              key={k}
              onClick={() => toggleKind(k)}
              className={`text-[10px] uppercase tracking-wide rounded px-1.5 py-0.5 border ${
                activeKinds.has(k)
                  ? 'border-border-default text-text-default'
                  : 'border-border-subtle text-text-muted opacity-50'
              }`}
            >
              {k}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto">
          {loading && <div className="p-5 text-xs text-text-muted">Loading…</div>}
          {error && <div className="p-5 text-xs text-red-400">{error}</div>}
          {!loading && !error && filtered.length === 0 && (
            <div className="p-5 text-xs text-text-muted">No history entries match.</div>
          )}
          {!loading && !error && filtered.map((entry) => (
            <div
              key={entry.commit_sha}
              className="px-5 py-3 border-b border-border-subtle hover:bg-background-muted/40"
            >
              <div className="flex items-center gap-2 mb-1">
                <ChangeKindChip kind={entry.kind} />
                <span className="text-[10px] text-text-muted font-mono">
                  {entry.commit_sha.slice(0, 7)}
                </span>
                <span className="text-[10px] text-text-muted ml-auto">
                  {relativeTime(entry.timestamp)}
                </span>
              </div>
              <div className="text-xs text-text-default mb-2">{entry.summary}</div>
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => onPreview(entry.commit_sha)}
                  className="text-[11px] h-6 px-2"
                >
                  Preview
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void handleRestore(entry)}
                  disabled={restoring !== null}
                  className="text-[11px] h-6 px-2"
                >
                  {restoring === entry.commit_sha ? 'Restoring…' : 'Restore'}
                </Button>
              </div>
            </div>
          ))}
        </div>
      </SheetContent>
    </Sheet>
  );
}
```

- [ ] **Step 4: Verify TS compiles.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge/ui/desktop
npm run lint:check 2>&1 | tail -10
```

Expected: no new errors. If `Sheet`'s prop API differs from the snippet (`side`, `SheetTitle` location), match what's in [`ui/desktop/src/components/ui/sheet.tsx`](../../../ui/desktop/src/components/ui/sheet.tsx).

- [ ] **Step 5: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/hooks/useHistory.ts \
        ui/desktop/src/components/knowledge/changelog/
git commit -m "feat(ui): ChangeLogDrawer with filter chips, preview, and restore flow"
```

---

## Task 10: Wire it all into `KnowledgeView`, remove placeholder, trigger refresh from ingest

This is the assembly task — the user-visible payoff.

**Files:**
- Modify: `ui/desktop/src/components/knowledge/KnowledgeView.tsx`
- Modify: `ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx`
- Delete: `ui/desktop/src/components/knowledge/RightSidePlaceholder.tsx`

- [ ] **Step 1: Edit `KnowledgeView.tsx`.**

Replace the imports of `RightSidePlaceholder` + its usage. The new file:

```typescript
// ui/desktop/src/components/knowledge/KnowledgeView.tsx
import { useEffect, useState } from 'react';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { KnowledgeProvider } from './KnowledgeContext';
import { KBSelectorTrigger } from './KBSelector/KBSelectorTrigger';
import { IngestPanel } from './IngestPanel/IngestPanel';
import { KnowledgeGraphPanel } from './graph/KnowledgeGraphPanel';
import { ChangeLogDrawer } from './changelog/ChangeLogDrawer';

export default function KnowledgeView() {
  return (
    <KnowledgeProvider>
      <KnowledgeViewInner />
    </KnowledgeProvider>
  );
}

function KnowledgeViewInner() {
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [changeLogOpen, setChangeLogOpen] = useState(false);
  const [previewSha, setPreviewSha] = useState<string | null>(null);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'k' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    }
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <MainPanelLayout>
      <div className="flex flex-col min-w-0 flex-1 overflow-y-auto relative" data-search-scroll-area>
        <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
          <div className="flex flex-col page-transition">
            <h1 className="text-2xl font-semibold tracking-tight mb-1">Knowledge</h1>
            <p className="text-sm text-text-muted mb-0">
              Personal, LLM-maintained knowledge bases.
            </p>
          </div>
        </div>
        <div className="flex-1 grid grid-cols-1 lg:grid-cols-[360px_1fr] min-h-0">
          <div className="border-r border-border-subtle overflow-y-auto">
            <div className="p-6">
              <KBSelectorTrigger open={paletteOpen} onOpenChange={setPaletteOpen} />
            </div>
            <IngestPanel />
          </div>
          <div className="min-h-0">
            <KnowledgeGraphPanel
              onOpenChangeLog={() => setChangeLogOpen(true)}
              previewSha={previewSha}
              onClearPreview={() => setPreviewSha(null)}
            />
          </div>
        </div>
        <ChangeLogDrawer
          open={changeLogOpen}
          onOpenChange={setChangeLogOpen}
          onPreview={(sha) => {
            setPreviewSha(sha);
            setChangeLogOpen(false);
          }}
          onRestored={() => setChangeLogOpen(false)}
        />
      </div>
    </MainPanelLayout>
  );
}
```

- [ ] **Step 2: Edit `IngestPanel.tsx` to call `triggerGraphRefresh()` after each successful ingest.**

Open `ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx`. The file **already** imports `useKnowledge` from `../KnowledgeContext` (line ~7). Find where the component destructures from it (search `useKnowledge()`) and add `triggerGraphRefresh` to the destructure:

```typescript
const { activeKbId, /* ...other existing fields..., */ triggerGraphRefresh } = useKnowledge();
```

Then update the success branch (currently around line 97-99 — the `update(item.id, { status: 'done' });` line):

```typescript
} else {
  update(item.id, { status: 'done' });
  triggerGraphRefresh();   // NEW: tell the graph panel to re-fetch
}
```

No new import needed.

- [ ] **Step 3: Delete the placeholder.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git rm ui/desktop/src/components/knowledge/RightSidePlaceholder.tsx
```

- [ ] **Step 4: Verify the frontend builds.**

```bash
cd ui/desktop && npm run lint:check 2>&1 | tail -15
```

Expected: no new errors. If there are stale references to `RightSidePlaceholder` anywhere, fix them.

- [ ] **Step 5: Commit.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
git add ui/desktop/src/components/knowledge/KnowledgeView.tsx \
        ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx
# RightSidePlaceholder.tsx was already staged by `git rm`
git commit -m "feat(ui): wire KnowledgeGraphPanel + ChangeLogDrawer into KnowledgeView; auto-refresh on ingest"
```

---

## Task 11: Backfill an integration test for restore + history

Plan 3 added `list_history` / `preview_state` / `restore_state` route tests. Confirm we cover an end-to-end "create, ingest, restore" via service-level integration test if it's missing.

**Files:**
- Modify: `crates/biorouter-test/tests/knowledge_revert_integration.rs` (CREATE if missing)

- [ ] **Step 1: Check what exists.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
find crates -name 'knowledge_revert*' -o -name 'knowledge*revert*'
ls crates/biorouter-test/tests/ 2>/dev/null
```

If a knowledge revert test already exists, skim it and only add coverage that's missing. Otherwise, proceed.

- [ ] **Step 2: Write the integration test.**

Create `crates/biorouter-test/tests/knowledge_revert_integration.rs`:

```rust
//! End-to-end revert test that uses KnowledgeService directly (no HTTP).
//! Verifies: after creating a KB, writing a page, committing, then restore_state
//! to the prior commit, the page is gone and the history has a `restore` entry.

use biorouter_mcp::knowledge::{KnowledgeService, types::ChangeKind};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn restore_state_reverts_a_page_creation() -> anyhow::Result<()> {
    let tmp = TempDir::new()?;
    let svc = Arc::new(KnowledgeService::new_for_tests(tmp.path())?);
    let kb = svc.create_base("rev-test", None)?;
    let kb_id = kb.id.clone();

    // Seed page A, commit.
    let kb_root = tmp.path().join("bases").join(&kb_id);
    std::fs::create_dir_all(kb_root.join("knowledge").join("notes"))?;
    std::fs::write(
        kb_root.join("knowledge").join("notes").join("a.md"),
        "---\ntitle: A\nkind: note\n---\nA body\n",
    )?;
    svc.commit_now(&kb_id, ChangeKind::Manual, "add A")?;

    let history_after_a = svc.list_history(&kb_id, Some(20))?;
    let sha_at_a = history_after_a[0].commit_sha.clone();

    // Seed page B, commit.
    std::fs::write(
        kb_root.join("knowledge").join("notes").join("b.md"),
        "---\ntitle: B\nkind: note\n---\nB body\n",
    )?;
    svc.commit_now(&kb_id, ChangeKind::Manual, "add B")?;
    assert!(kb_root.join("knowledge/notes/b.md").exists());

    // Restore to A.
    let new_sha = svc.restore_state(&kb_id, &sha_at_a)?;
    assert!(!new_sha.is_empty());

    // B should be gone (the revert removed the add-B commit's effect).
    assert!(kb_root.join("knowledge/notes/a.md").exists());
    assert!(!kb_root.join("knowledge/notes/b.md").exists());

    // History gains a `restore` entry.
    let history_after = svc.list_history(&kb_id, Some(20))?;
    assert_eq!(history_after[0].kind, ChangeKind::Restore);
    Ok(())
}
```

If `KnowledgeService::new_for_tests` and `commit_now` don't exist exactly as written, grep for the closest equivalents:

```bash
rg -n "new_for_tests|fn new\(|fn commit\(|fn commit_now\(" crates/biorouter-mcp/src/knowledge/service.rs
```

and adapt. The point of the test is the **behavior** (page A persists, page B disappears, history gains a `restore` entry), not the specific method names.

- [ ] **Step 3: Run the test.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
cargo test -p biorouter-test --test knowledge_revert_integration 2>&1 | tail -10
```

Expected: PASS. If the test crate doesn't exist or the dep isn't wired, drop the test into `crates/biorouter-mcp/tests/` instead — same content, just resolve the test crate location to whatever exists.

- [ ] **Step 4: Commit.**

```bash
git add crates/biorouter-test/tests/knowledge_revert_integration.rs
git commit -m "test(knowledge): end-to-end restore_state reverts a page creation"
```

---

## Task 12: Visual QA + Playwright smoke

A focused end-to-end smoke that proves the new panel works against the real backend. Reuses the Playwright pattern from the Plan-4 e2e session.

**Files:**
- Manual / interactive; no files modified.

- [ ] **Step 1: Start the dev stack.**

If the dev server isn't already running:

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
just run-dev    # or just run-ui
```

For Playwright control, instead start with the live-debug flag:

```bash
cd ui/desktop
ENABLE_PLAYWRIGHT=1 npm run start-gui
```

(The playwright-debug skill explains the rest.)

- [ ] **Step 2: Smoke checklist** (mark each ✅ as you verify; use the active KB from the Plan-4 e2e):

  - [ ] Open Knowledge → an active KB selected. Right panel header shows `<KB name> · N pages · M links`.
  - [ ] Graph renders with credibility-coloured nodes (sources in blues/orange/purple per tier; entities/concepts in neutral palette).
  - [ ] Hovering a node dims non-neighbours; neighbour edges turn green.
  - [ ] Clicking a node opens the `NodePreview` card with the page's frontmatter + body.
  - [ ] Closing preview returns the graph to default.
  - [ ] Click "Change log" → drawer slides in from the right with one row per ingest commit (the 4 ingest commits from the Plan-4 e2e should be visible).
  - [ ] Toggle a filter chip off; matching rows disappear.
  - [ ] Click "Preview" on an older commit → drawer closes, banner appears at top of graph panel ("Previewing <sha> — read-only"); "Exit preview" clears it.
  - [ ] Click "Restore" on an older commit → confirm dialog → after restore, a new `restore` entry appears at top of the log and the graph re-fetches.
  - [ ] Ingest a new fixture URL (any short HTML page). After completion: graph auto-refreshes; new node(s) appear without manual refresh.
  - [ ] Verify the new schema's effect: open a freshly created KB → ingest a paper → confirm the graph has **at least one edge** (the Plan-4 0-edges bug should be fixed for new KBs).

- [ ] **Step 3: If anything fails, file a focused fix commit referencing the failing item.** Don't accumulate fixes.

---

## Task 13: Final pass — cleanup + commit polish

- [ ] **Step 1: Run the full check.**

```bash
cd /Users/wgu/Desktop/biorouter-knowledge
just check-everything 2>&1 | tail -30
```

Expected: all green. Fix any unrelated breakage you may have introduced (most likely a lint nit in the new TS files).

- [ ] **Step 2: Sanity-check the git history is clean.**

```bash
git log --oneline feature/knowledge ^main | head -20
```

Expected: one commit per Plan-5 task, in roughly the order above. No "WIP" / "fixup" commits left over.

- [ ] **Step 3: Done — handoff message to the user.**

> "Plan 5 complete. Right column of the Knowledge view now renders a live force-directed graph (credibility-coloured, hover-dim, click-to-preview) and a Change Log drawer with preview + restore. New backend route `GET /knowledge/bases/:id/page`. Default schema now requires `[[link]]` emission so new KBs build graphs with actual edges. Plan 6 (chat-side KB chip + slash commands + polish + docs) is the last one."

---

## Spec coverage cross-check

Tracking each spec requirement to the task that implements it. **Status** is one of
*Implemented*, *Implemented with variation*, or *Deferred*; **Task** cites the task
number in this plan.

| Spec requirement | Task | Status | Notes |
|---|---|---|---|
| `react-force-graph-2d` with custom node/link canvas objects | 3, 6 | Implemented | — |
| Hub treatment (top-N degree centrality, larger + bold label) | 6 | Implemented | — |
| Hover dims non-neighbours, highlights edges to neighbours | 6 | Implemented | — |
| Tooltip on hover with title + tier + reasoning + neighbour count | 7 | Implemented with variation | `NodePreview` replaces the transient tooltip — richer than a hover card. |
| Click source node → side preview (original + derived + `meta.yaml`) | 7 | Implemented with variation | Preview shows derived markdown; "original file" is link-only — see the `CLAUDE.md` note. |
| Credibility palette (peer, book, preprint, gray_lit, web, personal, retracted) | 4 | Implemented | — |
| Edge styling reflects credibility (solid widths + dashed for web/personal) | 4, 6 | Implemented | — |
| Retracted overlays red `!` badge | — | Deferred | Visualised via `retractedColor` only. Deferred to Plan 6 polish because no retracted source exists in the current test fixtures. Known gap. |
| Change log drawer with timeline + filter chips | 9 | Implemented | — |
| Click entry → graph enters preview mode (read-only banner) | 9, 10 | Implemented | — |
| Future-state nodes drawn dashed and faded | — | Deferred | Deferred to Plan 6. The banner indicates preview mode; ghosting future-state nodes needs a tree-at-SHA diff API that does not exist yet — `kb_preview_state` works per file, but a "list pages at SHA" call does not exist. Added to the Plan-6 polish task. |
| `POST /restore` creates a revert commit, log entry kind = `restore` | 9, 11 | Implemented | — |
| Graph subscribes to macro SSE stream so nodes pop in during ingest | 5, 10 | Implemented with variation | Refresh after each ingest — a simpler take than per-event SSE coupling. The spec said "subscribes to the macro SSE stream"; the practical UX is refetch-on-done. |
| `[[link]]`-based edge derivation | 2 | Implemented | The schema fix makes the sub-agent reliably emit links. |

Open gaps documented above are intentionally deferred to Plan 6 polish.

## Related documentation

- [Knowledge founding design](founding-design.md) — the graph rendering, change log and credibility palette sections this plan cross-checks itself against.
- [Plan 4 — Knowledge view and ingest panel](plan-4-knowledge-view-and-ingest.md) — builds the `RightSidePlaceholder` this plan replaces, and the ingest completion point that triggers graph refresh.
- [Plan 6 — chat integration and closeout](plan-6-chat-integration-and-closeout.md) — picks up the two gaps deferred in the cross-check table above.
- [Plan 3 — HTTP routes and export/import](plan-3-http-routes-and-export.md) — the `/graph`, `/history`, `/preview` and `/restore` routes consumed here, plus where the new `/page` route slots in.
