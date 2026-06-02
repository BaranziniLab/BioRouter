# Knowledge Chat Integration + Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out the Knowledge feature by tying the chat side to the Knowledge view (active-KB picker chip in `ChatInput`, persistent active-KB across chat sessions, a `/knowledge` slash-command palette entry), wrap up the polish items that Plan 5 explicitly deferred (retracted-source `!` badge in the graph), and ship `CLAUDE.md` + release-notes guidance. After Plan 6 the feature ships.

**Architecture:**
- **Active-KB persistence (new):** the MCP server's `ActiveKbState` is currently in-memory and per-process; a new chat session boots without it. Persist the active-KB id to `~/.config/biorouter/knowledge/.active-kb` (single line, just the id). `KnowledgeService` gets `get_active_persisted()` / `set_active_persisted()`. The MCP server's `ActiveKbState::new()` reads the file at construction; both `kb_set_active` and a new `POST /knowledge/active` HTTP route write through to it.
- **UI ↔ server sync:** `KnowledgeContext` (frontend) calls the new `POST /knowledge/active` when the user picks a KB in either the Knowledge view OR the chat-side chip. The MCP server picks up the change next time it spawns. (Same-process live sync from HTTP into a running MCP stdio process is out of scope — chat sessions read from the file at boot.)
- **Chat chip:** a new `BottomMenuKnowledgeSelection.tsx` in `ui/desktop/src/components/bottom_menu/` mirrors the existing `BottomMenuSkillSelection.tsx` pattern. The chip shows the active KB's name (or "No KB") and opens a small popover listing all KBs. Mounted in `ChatInput.tsx` alongside the existing `BottomMenuModeSelection` / `BottomMenuExtensionSelection` / `BottomMenuSkillSelection` chips.
- **Slash command palette:** there is already a slash-command popover (`/`-triggered) wired through `MentionPopover`. Add one entry: `/knowledge`. Selecting it inserts a small templated prompt that nudges the model to use the `kb_*` tools (e.g. "Using the Knowledge extension, … ").
- **Retracted badge:** `ForceGraphCanvas` already has the `retractedColor` constant imported but unused. Read `node.credibility_tier` is wrong shape for retraction (it's tier, not a flag); the retraction lives in `raw/<id>/meta.yaml` and is surfaced via graph node credibility derivation — confirm the GraphNode schema actually carries the retraction signal, or extend `graph.rs` to write it. Render a small red "!" overlay on retracted nodes.

**Tech Stack:** Rust (axum, utoipa) for the new HTTP route; React 19 + Tailwind for the chip + slash-entry; CSS-only badge.

**Source spec:** [`docs/superpowers/specs/2026-05-30-knowledge-design.md`](../specs/2026-05-30-knowledge-design.md). Prior plans: 1-5 covered phases 1-13. Plan 6 covers phases 14 (chat integration) and 15 (polish + docs). The spec's scope notes on retracted nodes are at ~L195-210; on chat integration at "Acceptance plan #14".

**This is Plan 6 of 6.** After Plan 6 there is no Plan 7 — every spec phase has a task.

**TDD note:** Backend tasks add Rust integration tests. Frontend tasks rely on `npm run typecheck` + `npm run lint:check`. Per user instruction: **no Playwright runs in this plan.** Use cargo tests + lint + tsc for smoke.

---

## Before starting

- [ ] **Pre-step A:** baseline.

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge && source bin/activate-hermit
git rev-parse --abbrev-ref HEAD       # expect feature/knowledge
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -3
cargo test -p biorouter-mcp --lib knowledge:: 2>&1 | tail -3
cargo test -p biorouter-mcp --test knowledge_revert_integration 2>&1 | tail -3
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -3
cd ../..
```

Expected:
- `knowledge_routes`: 18 passed
- `biorouter-mcp` lib `knowledge::`: 120 passed
- `knowledge_revert_integration`: 1 passed
- `npx tsc --noEmit`: zero errors

- [ ] **Pre-step B:** integration points to know.

  - `crates/biorouter-mcp/src/knowledge/server.rs:15-46` — `ActiveKbState` (Tokio `RwLock<Option<String>>`).
  - `crates/biorouter-mcp/src/knowledge/server.rs:450-470` — `kb_set_active` / `kb_get_active` MCP tools.
  - `crates/biorouter-server/src/routes/knowledge.rs` — existing route module.
  - `ui/desktop/src/components/ChatInput.tsx:21-23` — existing bottom-menu chip imports.
  - `ui/desktop/src/components/bottom_menu/BottomMenuSkillSelection.tsx` — reference style for the new chip.
  - `ui/desktop/src/components/MentionPopover.tsx` — slash-command list and selection.
  - `ui/desktop/src/components/knowledge/KnowledgeContext.tsx` — frontend active-KB state.
  - `crates/biorouter-mcp/src/knowledge/types.rs:88-104` — `GraphNode` schema (does it carry `retracted: bool`?).

---

## File structure (decomposition map)

**Backend:**

```
crates/biorouter-mcp/src/knowledge/
├── service.rs                   — MODIFY: add get_active_persisted / set_active_persisted
├── server.rs                    — MODIFY: ActiveKbState reads/writes the persisted file
├── paths.rs                     — MODIFY: add active_kb_path(root)
└── (no new files)

crates/biorouter-server/src/routes/knowledge.rs   — MODIFY: GET + POST /knowledge/active
crates/biorouter-server/tests/knowledge_routes.rs — MODIFY: active-KB roundtrip test

ui/desktop/openapi.json + sdk.gen.ts + types.gen.ts — REGEN
```

**Frontend:**

```
ui/desktop/src/components/bottom_menu/
└── BottomMenuKnowledgeSelection.tsx                — NEW: KB chip + popover

ui/desktop/src/components/
├── ChatInput.tsx                                    — MODIFY: mount the new chip
└── MentionPopover.tsx                               — MODIFY: add /knowledge slash entry

ui/desktop/src/components/knowledge/
├── KnowledgeContext.tsx                             — MODIFY: POST /knowledge/active on setActiveKbId
└── graph/ForceGraphCanvas.tsx                       — MODIFY: render retracted "!" badge

CLAUDE.md                                            — MODIFY: knowledge section
```

---

## Task 1: Persist active-KB to disk

The MCP server is spawned per chat process, so the in-memory `ActiveKbState` is lost across sessions. Persist to `<root>/.active-kb` (one line, just the id) so a new chat boot sees the user's last pick.

**Files:**
- Modify: `crates/biorouter-mcp/src/knowledge/paths.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/service.rs`
- Modify: `crates/biorouter-mcp/src/knowledge/server.rs`

- [ ] **Step 1: Add `active_kb_path` helper.**

In `crates/biorouter-mcp/src/knowledge/paths.rs`, after the existing `kb_root` helper:

```rust
/// Returns `<knowledge-root>/.active-kb` — the file that persists the
/// currently-active KB id across MCP-server processes.
pub fn active_kb_path(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".active-kb")
}
```

- [ ] **Step 2: Add `get_active_persisted` + `set_active_persisted` to `KnowledgeService`.**

In `crates/biorouter-mcp/src/knowledge/service.rs`, fold into the existing `impl KnowledgeService` block that owns `read_page`:

```rust
/// Read the persisted active-KB id (set via the UI or `kb_set_active`).
/// Returns `Ok(None)` if no file exists or the file is empty.
pub fn get_active_persisted(&self) -> anyhow::Result<Option<String>> {
    let path = crate::knowledge::paths::active_kb_path(self.root());
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&path)?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

/// Persist the active-KB id. Pass `None` to clear.
pub fn set_active_persisted(&self, id: Option<&str>) -> anyhow::Result<()> {
    let path = crate::knowledge::paths::active_kb_path(self.root());
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    match id {
        Some(id) => {
            crate::knowledge::paths::validate_kb_id(id)?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, id.as_bytes())?;
            std::fs::rename(tmp, &path)?;
        }
        None => {
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}
```

If `self.root()` doesn't exist, grep:

```bash
rg -n "fn root\(" crates/biorouter-mcp/src/knowledge/service.rs
```

and use whatever returns the knowledge root `Path`.

- [ ] **Step 3: Update `ActiveKbState` to bootstrap from disk.**

In `crates/biorouter-mcp/src/knowledge/server.rs`, find the `ActiveKbState` struct (around line 15). Add a constructor that reads the persisted file:

```rust
impl ActiveKbState {
    /// Bootstrap from disk. Used by the MCP server constructor.
    pub fn from_persisted(service: &KnowledgeService) -> Self {
        let initial = service
            .get_active_persisted()
            .ok()
            .flatten();
        Self {
            inner: tokio::sync::RwLock::new(initial),
        }
    }

    pub async fn set(&self, id: &str) {
        let mut g = self.inner.write().await;
        *g = Some(id.to_string());
    }

    pub async fn get(&self) -> Option<String> {
        self.inner.read().await.clone()
    }
}
```

If `ActiveKbState`'s field is named something other than `inner`, match what's there. If `ActiveKbState::default()` is used in `KnowledgeServer::new()`, swap that call site to `ActiveKbState::from_persisted(&service)`.

Also update the `kb_set_active` MCP tool to write through to disk:

```rust
pub async fn kb_set_active(
    &self,
    p: Parameters<SetActiveParams>,
) -> Result<CallToolResult, ErrorData> {
    crate::knowledge::paths::validate_kb_id(&p.0.kb_id).map_err(|e| into_err(e.into()))?;
    self.active.set(&p.0.kb_id).await;
    self.service
        .set_active_persisted(Some(&p.0.kb_id))
        .map_err(into_err)?;
    ok_json(&serde_json::json!({ "ok": true, "active_kb": p.0.kb_id }))
}
```

- [ ] **Step 4: Write a service-level test.**

Append to `crates/biorouter-mcp/src/knowledge/service.rs` test module (or wherever existing `service` unit tests live; grep `#[test]` in that file):

```rust
#[test]
fn active_kb_persists_to_disk() -> anyhow::Result<()> {
    let tmp = tempfile::TempDir::new()?;
    let svc = KnowledgeService::new(tmp.path().to_path_buf())?;
    assert!(svc.get_active_persisted()?.is_none());

    svc.set_active_persisted(Some("my-kb"))?;
    assert_eq!(svc.get_active_persisted()?.as_deref(), Some("my-kb"));

    // Setting again overwrites.
    svc.set_active_persisted(Some("other-kb"))?;
    assert_eq!(svc.get_active_persisted()?.as_deref(), Some("other-kb"));

    // Clearing removes the file.
    svc.set_active_persisted(None)?;
    assert!(svc.get_active_persisted()?.is_none());

    // Invalid IDs are rejected.
    let err = svc.set_active_persisted(Some("INVALID--KB"));
    assert!(err.is_err());
    Ok(())
}
```

If the `KnowledgeService` constructor needs more setup than `new(path)`, follow the pattern used by the existing tests in that file.

- [ ] **Step 5: Run tests.**

```bash
cargo test -p biorouter-mcp --lib knowledge:: active_kb_persists 2>&1 | tail -5
cargo test -p biorouter-mcp --lib knowledge:: 2>&1 | tail -3
```

Expected: pass.

- [ ] **Step 6: Commit.**

```bash
git add crates/biorouter-mcp/src/knowledge/paths.rs \
        crates/biorouter-mcp/src/knowledge/service.rs \
        crates/biorouter-mcp/src/knowledge/server.rs
git commit -m "feat(knowledge): persist active-KB id across MCP-server processes"
```

---

## Task 2: `GET` + `POST /knowledge/active` HTTP routes

The frontend needs to (a) read the current active KB on startup so the chat chip can render it, and (b) update it when the user picks elsewhere. These two routes are thin pass-throughs to `KnowledgeService::{get,set}_active_persisted`.

**Files:**
- Modify: `crates/biorouter-server/src/routes/knowledge.rs`
- Modify: `crates/biorouter-server/src/openapi.rs`
- Modify: `crates/biorouter-server/tests/knowledge_routes.rs`

- [ ] **Step 1: Failing test.**

Append to `crates/biorouter-server/tests/knowledge_routes.rs`:

```rust
#[tokio::test]
async fn active_kb_roundtrip() {
    let h = harness().await;

    // Empty initially.
    let resp: serde_json::Value = get(&h, "/knowledge/active").await;
    assert!(resp["active_kb"].is_null());

    // Create a KB to point at.
    let body = json!({ "name": "act", "description": null });
    let kb: serde_json::Value = post(&h, "/knowledge/bases", body).await;
    let kb_id = kb["id"].as_str().unwrap();

    // Set it.
    let _: serde_json::Value =
        post(&h, "/knowledge/active", json!({ "kb_id": kb_id })).await;

    // Read it back.
    let after: serde_json::Value = get(&h, "/knowledge/active").await;
    assert_eq!(after["active_kb"].as_str().unwrap(), kb_id);

    // Clear it.
    let _: serde_json::Value =
        post(&h, "/knowledge/active", json!({ "kb_id": null })).await;
    let cleared: serde_json::Value = get(&h, "/knowledge/active").await;
    assert!(cleared["active_kb"].is_null());

    // Invalid kb id returns 400.
    let bad = post_raw(&h, "/knowledge/active", json!({ "kb_id": "INVALID--KB" })).await;
    assert_eq!(bad.status(), 400);
}
```

If `post_raw` doesn't exist, add it next to `post` mirroring the `get_raw` helper added in Plan 5 Task 1.

- [ ] **Step 2: Run test, confirm it fails.**

```bash
cargo test -p biorouter-server --test knowledge_routes active_kb_roundtrip 2>&1 | tail -10
```

Expected: FAIL (route not found).

- [ ] **Step 3: Add routes + handlers.**

In `crates/biorouter-server/src/routes/knowledge.rs`:

3a. Add to the router builder:

```rust
.route("/active", get(get_active).post(set_active))
```

3b. Add DTOs + handlers near the other read-only routes:

```rust
#[derive(serde::Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SetActiveBody {
    /// `None` clears the active KB.
    pub kb_id: Option<String>,
}

#[derive(serde::Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ActiveKbResponse {
    pub active_kb: Option<String>,
}

#[utoipa::path(
    get, path = "/knowledge/active",
    responses((status = 200, description = "Current active KB id", body = ActiveKbResponse))
)]
pub async fn get_active(
    State(svc): State<Arc<KnowledgeService>>,
) -> Result<Json<ActiveKbResponse>, (StatusCode, String)> {
    let active_kb = svc
        .get_active_persisted()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ActiveKbResponse { active_kb }))
}

#[utoipa::path(
    post, path = "/knowledge/active",
    request_body = SetActiveBody,
    responses(
        (status = 200, description = "Set successfully", body = ActiveKbResponse),
        (status = 400, description = "Invalid kb id"),
    )
)]
pub async fn set_active(
    State(svc): State<Arc<KnowledgeService>>,
    Json(body): Json<SetActiveBody>,
) -> Result<Json<ActiveKbResponse>, (StatusCode, String)> {
    svc.set_active_persisted(body.kb_id.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(ActiveKbResponse { active_kb: body.kb_id }))
}
```

3c. Register in `crates/biorouter-server/src/openapi.rs`. Find the `#[derive(OpenApi)]` block and add `get_active`, `set_active` to `paths(...)` and `SetActiveBody`, `ActiveKbResponse` to `schemas(...)`.

- [ ] **Step 4: Test passes.**

```bash
cargo test -p biorouter-server --test knowledge_routes active_kb_roundtrip 2>&1 | tail -10
```

Expected: PASS.

Also run the full route suite:

```bash
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -5
```

Expected: 19 passed (was 18 → +1).

- [ ] **Step 5: Regen openapi + TS client.**

```bash
just generate-openapi
cd ui/desktop && npm run generate-api 2>&1 | tail -5
cd ../..
grep -n "getActive\|setActive" ui/desktop/src/api/sdk.gen.ts | head
```

Expected: two new exports, `getActive` and `setActive`.

- [ ] **Step 6: Commit.**

```bash
git add crates/biorouter-server/src/routes/knowledge.rs \
        crates/biorouter-server/src/openapi.rs \
        crates/biorouter-server/tests/knowledge_routes.rs \
        ui/desktop/openapi.json \
        ui/desktop/src/api/sdk.gen.ts \
        ui/desktop/src/api/types.gen.ts \
        ui/desktop/src/api/index.ts
git commit -m "feat(knowledge): GET + POST /knowledge/active for cross-session sync"
```

---

## Task 3: `KnowledgeContext` syncs to server

When the user picks a KB in the Knowledge view (or in the chat chip), persist that to the server via `setActive`. On startup, hydrate from `getActive`.

**Files:**
- Modify: `ui/desktop/src/components/knowledge/KnowledgeContext.tsx`

- [ ] **Step 1: Add API calls.**

Replace the `useEffect` that loads `bases` with one that loads bases **and** the active-KB from the server:

```typescript
// add to imports at the top:
import { listBases, getActive, setActive } from '../../api';
```

In the provider body, replace the existing `setActiveKbId` callback:

```typescript
const setActiveKbId = useCallback((id: string | null) => {
  setActiveKbIdState(id);
  if (id) localStorage.setItem(STORAGE_KEY_ACTIVE_KB, id);
  else localStorage.removeItem(STORAGE_KEY_ACTIVE_KB);
  // Fire-and-forget server sync. Failures are non-fatal (chat won't see
  // the pick until next reconnect, but the local UI keeps working).
  void setActive({ body: { kb_id: id }, throwOnError: false }).catch((err) => {
    console.warn('setActive (server sync) failed:', err);
  });
}, []);
```

Add a one-time hydration effect that prefers the server's persisted value over `localStorage` (so a fresh chat session picks up changes made elsewhere):

```typescript
useEffect(() => {
  void (async () => {
    try {
      const res = await getActive({ throwOnError: true });
      const server = res.data?.active_kb ?? null;
      if (server) {
        // Server wins; sync localStorage to it.
        setActiveKbIdState(server);
        localStorage.setItem(STORAGE_KEY_ACTIVE_KB, server);
      }
    } catch (err) {
      console.warn('getActive (server hydrate) failed:', err);
    }
  })();
  // Run once on mount.
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, []);
```

- [ ] **Step 2: Verify TS compiles.**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 3: Commit.**

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge
git add ui/desktop/src/components/knowledge/KnowledgeContext.tsx
git commit -m "feat(ui): sync active KB through POST /knowledge/active so chat sees it"
```

---

## Task 4: `BottomMenuKnowledgeSelection` chip in `ChatInput`

Mirrors the existing `BottomMenuSkillSelection` chip pattern. Shows the active KB's name (or "No KB"); clicking opens a popover listing all KBs; selecting one calls `setActiveKbId(id)` on a shared `KnowledgeContext`.

The catch: `ChatInput` is at the app's root and is NOT a descendant of `KnowledgeProvider` (which only wraps `KnowledgeView`). For this task we **lift `KnowledgeProvider` up** so it wraps the whole app. Verify by inspection that doing so doesn't break the existing view.

**Files:**
- Create: `ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx`
- Modify: `ui/desktop/src/App.tsx` (or wherever `KnowledgeProvider` currently lives) — lift the provider
- Modify: `ui/desktop/src/components/ChatInput.tsx` — mount the chip
- Modify: `ui/desktop/src/components/knowledge/KnowledgeView.tsx` — drop the inner `KnowledgeProvider` wrap (already provided at app root)

- [ ] **Step 1: Find the right place to mount `KnowledgeProvider` at the app root.**

```bash
grep -rn "KnowledgeProvider" ui/desktop/src/ 2>&1
```

Likely it's only in `KnowledgeView.tsx`. Open `ui/desktop/src/App.tsx` and pick a high-level provider stack (look for `ChatProvider`, `ConfigProvider`, etc.). Add `KnowledgeProvider` to the chain.

If lifting introduces a fetch-on-app-boot cost worse than negligible, that's fine — the `listBases` call is cheap.

- [ ] **Step 2: Drop the inner `KnowledgeProvider` from `KnowledgeView.tsx`.**

Change:

```typescript
export default function KnowledgeView() {
  return (
    <KnowledgeProvider>
      <KnowledgeViewInner />
    </KnowledgeProvider>
  );
}
```

to:

```typescript
export default function KnowledgeView() {
  return <KnowledgeViewInner />;
}
```

Also clean up the now-unused import.

- [ ] **Step 3: Create `BottomMenuKnowledgeSelection.tsx`.**

Use `BottomMenuSkillSelection.tsx` as the visual template (read it first to match style):

```typescript
// ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx
import { useState } from 'react';
import { BookOpen } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';
import { useKnowledge } from '../knowledge/KnowledgeContext';

export function BottomMenuKnowledgeSelection() {
  const { bases, activeKb, setActiveKbId } = useKnowledge();
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className="flex items-center gap-1.5 px-2 py-1 text-xs text-text-muted hover:text-text-default rounded-md hover:bg-background-muted/40"
          title="Active knowledge base"
        >
          <BookOpen className="h-3.5 w-3.5" />
          <span className="max-w-[140px] truncate">
            {activeKb?.name ?? 'No KB'}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64 p-1">
        {bases.length === 0 ? (
          <div className="p-3 text-xs text-text-muted">
            No knowledge bases yet. Create one in the Knowledge view.
          </div>
        ) : (
          <div className="flex flex-col">
            <button
              onClick={() => {
                setActiveKbId(null);
                setOpen(false);
              }}
              className={`px-3 py-2 text-xs text-left rounded hover:bg-background-muted ${
                !activeKb ? 'text-text-default' : 'text-text-muted'
              }`}
            >
              No active KB
            </button>
            {bases.map((b) => (
              <button
                key={b.id}
                onClick={() => {
                  setActiveKbId(b.id);
                  setOpen(false);
                }}
                className={`px-3 py-2 text-xs text-left rounded hover:bg-background-muted truncate ${
                  activeKb?.id === b.id ? 'text-text-default font-medium' : 'text-text-muted'
                }`}
                title={b.id}
              >
                {b.name}
              </button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
```

If `BookOpen` isn't already used elsewhere, alternatives in lucide-react: `Library`, `Database`, `FolderOpen`. Pick one that visually fits.

- [ ] **Step 4: Mount the chip in `ChatInput`.**

Open `ui/desktop/src/components/ChatInput.tsx`. Add the import:

```typescript
import { BottomMenuKnowledgeSelection } from './bottom_menu/BottomMenuKnowledgeSelection';
```

Find where the existing bottom-menu chips render together (search for `BottomMenuSkillSelection` or `BottomMenuExtensionSelection`). Mount the new chip alongside them — match the surrounding container's flex/gap conventions:

```tsx
<BottomMenuKnowledgeSelection />
```

- [ ] **Step 5: Verify TS compiles.**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 6: Commit.**

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge
git add ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx \
        ui/desktop/src/components/ChatInput.tsx \
        ui/desktop/src/components/knowledge/KnowledgeView.tsx \
        ui/desktop/src/App.tsx
git commit -m "feat(ui): chat-side KB chip in bottom menu + lift KnowledgeProvider to app root"
```

---

## Task 5: Add `/knowledge` slash-command entry

The chat input already has a slash-command popover (`/`-triggered). Add `/knowledge` as a registered command that inserts a short templated prompt.

**Files:**
- Modify: `ui/desktop/src/components/MentionPopover.tsx`

- [ ] **Step 1: Find the slash-command list.**

```bash
grep -n "slash\|SlashCommand\|isSlashCommand" ui/desktop/src/components/MentionPopover.tsx | head -20
```

Inspect the file. There is some array or object of slash commands (e.g. `SLASH_COMMANDS`, `slashCommands`, etc.). Read enough of the file to understand the registration shape: name, description, on-select behavior.

- [ ] **Step 2: Register `/knowledge`.**

Add an entry to the existing slash-command list. The shape will look something like:

```typescript
{
  name: '/knowledge',
  description: 'Use the active knowledge base',
  // on selection — insert templated prompt text:
  insert: 'Using the Knowledge extension on the active knowledge base, ',
}
```

Match the exact field names of the existing entries. If existing entries dispatch to handler functions rather than inserting text, follow that convention and emit a simple "insert and submit later" command — don't auto-submit.

If the slash-command system doesn't support "insert text on select" (only "run a command"), wire the entry to dispatch a `setDisplayValue` call against the textarea, prepending the templated string. The cleanest place is in `MentionPopover.tsx`'s selection handler — look at how an existing command behaves on select.

- [ ] **Step 3: Verify TS compiles.**

```bash
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -10
```

Expected: zero errors.

- [ ] **Step 4: Commit.**

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge
git add ui/desktop/src/components/MentionPopover.tsx
git commit -m "feat(ui): /knowledge slash command inserts templated prompt for the active KB"
```

---

## Task 6: Retracted source `!` badge in `ForceGraphCanvas`

The Plan-5 retracted-badge polish item, deferred because no test fixture had a retracted source. Implement the rendering; the data path is "if the source's credibility derivation marks it retracted, the graph node carries a flag".

First inspect the data shape:

```bash
rg -n "retracted" crates/biorouter-mcp/src/knowledge/types.rs crates/biorouter-mcp/src/knowledge/graph.rs crates/biorouter-mcp/src/knowledge/credibility/ 2>&1 | head
```

If `GraphNode` does NOT carry a `retracted: bool` field today, the cleanest fix is to add one in `types.rs` and populate it in `graph.rs::derive` from the source's `meta.yaml` retraction flag. If `GraphNode` already carries it, just consume it.

**Files (likely):**
- Modify: `crates/biorouter-mcp/src/knowledge/types.rs` — add `retracted: bool` field (if absent)
- Modify: `crates/biorouter-mcp/src/knowledge/graph.rs` — populate it
- Modify: `crates/biorouter-mcp/src/knowledge/credibility/` (or wherever retraction lives in `meta.yaml`) — surface the bool
- Modify: `crates/biorouter-mcp/src/knowledge/graph.rs` tests — add a retracted-source snapshot
- Modify: `ui/desktop/openapi.json` + `types.gen.ts` (via regen)
- Modify: `ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx` — render badge

### Step 1: Surface retraction on `GraphNode`.

If `types.rs::GraphNode` doesn't have a `retracted` field, add one:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: PageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credibility_tier: Option<CredibilityTier>,
    pub path: String,
    /// True if this is a source node whose `raw/<id>/meta.yaml` marks it retracted.
    #[serde(default)]
    pub retracted: bool,
}
```

In `graph.rs::derive`, when iterating `raw::list_sources(kb_root)`, read the source's retraction flag and set `n.retracted = true` on the matching node. Look at how `credibility_tier` is currently set there for the pattern.

The retraction signal lives in the source's `Credibility` struct (or `SourceCredibility` / similar) — grep `retracted` in `crates/biorouter-mcp/src/knowledge/credibility/` to find the boolean.

### Step 2: Add a small unit test in `graph.rs`.

Create or update an existing test that seeds a source with retraction set, runs `graph::derive`, and asserts the corresponding `GraphNode.retracted` is true.

Look at existing tests in `crates/biorouter-mcp/src/knowledge/graph.rs` or `tests/` — match the pattern.

### Step 3: Regenerate TS types.

```bash
just generate-openapi && cd ui/desktop && npm run generate-api && cd ../..
grep -n "retracted" ui/desktop/src/api/types.gen.ts | head
```

Expected: at least one hit on `GraphNode`.

### Step 4: Render badge in `ForceGraphCanvas`.

In `ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx`, inside `nodeCanvasObject`, after the existing fill + ring draw and BEFORE the label draw, add:

```typescript
if ((n as { retracted?: boolean }).retracted) {
  // Small red "!" badge top-right of the node.
  const bx = n.x + r * 0.7;
  const by = n.y - r * 0.7;
  const br = Math.max(3, r * 0.45);
  ctx.beginPath();
  ctx.arc(bx, by, br, 0, Math.PI * 2);
  ctx.fillStyle = retractedColor;
  ctx.fill();
  ctx.fillStyle = '#fff';
  ctx.font = `700 ${br * 1.2}px ui-sans-serif`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText('!', bx, by + 0.5);
}
```

The `retractedColor` import is already present at the top — remove the `void retractedColor;` no-op line at the bottom of the file now that the color is used.

### Step 5: Run tests + lint.

```bash
cargo test -p biorouter-mcp --lib knowledge:: 2>&1 | tail -3
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -3
cd ../..
```

Expected: all pass, zero new TS errors.

### Step 6: Commit.

```bash
git add crates/biorouter-mcp/src/knowledge/types.rs \
        crates/biorouter-mcp/src/knowledge/graph.rs \
        ui/desktop/src/components/knowledge/graph/ForceGraphCanvas.tsx \
        ui/desktop/openapi.json \
        ui/desktop/src/api/types.gen.ts \
        ui/desktop/src/api/sdk.gen.ts
git commit -m "feat(knowledge): retracted-source badge on graph nodes (data path + frontend rendering)"
```

If `graph::derive` reads from somewhere unexpected for retraction, add that file to the commit too.

---

## Task 7: `CLAUDE.md` update + final smoke

Document the Knowledge feature briefly so Claude has the right mental model when working on this repo.

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Append a `Knowledge` section.**

Open `CLAUDE.md`. Find the "Architecture" or "Rust Workspace" section. After the existing crate descriptions, append (or insert as a subsection of the architecture section):

```markdown
### Knowledge feature

The Knowledge feature (built across Plans 1-6 in `docs/superpowers/plans/2026-05-30..2026-06-01-*`) provides personal, LLM-maintained knowledge bases backed by markdown trees + git history.

- **Backend module:** `crates/biorouter-mcp/src/knowledge/` (types, store, git, graph, credibility, conversion, macros, sub-agent loop, MCP server).
- **HTTP routes:** `crates/biorouter-server/src/routes/knowledge.rs` (`/knowledge/bases`, `/ingest` SSE, `/graph`, `/history`, `/restore`, `/page`, `/active`, `/export`, `/import`).
- **Frontend:** `ui/desktop/src/components/knowledge/` (view, KB selector, ingest panel, force-graph + change-log drawer). The KB chip in `ChatInput` lives at `ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx`.
- **Storage layout:** `~/.config/biorouter/knowledge/<kb-id>/` with `raw/`, `knowledge/`, `index.md`, `log.md`, `schema.md`, and a hidden `.git/`. The active-KB id is persisted at `~/.config/biorouter/knowledge/.active-kb`.
- **Sub-agent loop:** `crates/biorouter-mcp/src/knowledge/subagent/loop_.rs` drives ingest / query / lint macros. Reads pages with `kb_read_page`, writes with `kb_write_page`, commits at the end of each macro.

When working on the Knowledge feature:
- Run `cargo test -p biorouter-mcp --lib knowledge::` (~120 tests) and `cargo test -p biorouter-server --test knowledge_routes` (~19 tests) for backend changes.
- The TS SDK is auto-generated; after touching `routes/knowledge.rs`, run `just generate-openapi && cd ui/desktop && npm run generate-api`.
- Graph derivation lives in `graph.rs` and depends on the sub-agent emitting `[[knowledge-link]]` markers; the default `schema_default.md` reinforces this. Don't expect edges if the underlying pages have no `[[…]]` cross-references.
```

- [ ] **Step 2: Run full smoke.**

```bash
cd /Users/wgu/Desktop/BioRouter-knowledge && source bin/activate-hermit
cargo fmt --check 2>&1 | tail -5
cargo clippy -p biorouter-server -p biorouter-mcp --tests 2>&1 | tail -10
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -3
cargo test -p biorouter-mcp --lib knowledge:: 2>&1 | tail -3
cargo test -p biorouter-mcp --test knowledge_revert_integration 2>&1 | tail -3
cargo test -p biorouter-mcp --test knowledge_macros_e2e 2>&1 | tail -3
cd ui/desktop && npx tsc --noEmit 2>&1 | tail -3
npm run lint:check 2>&1 | tail -3
cd ../..
```

Expected:
- fmt: clean (or fix Plan-6 touched files only, commit separately)
- clippy: clean
- all test suites pass with counts: `knowledge_routes` 19, `biorouter-mcp::knowledge` 121+, `knowledge_revert_integration` 1, `knowledge_macros_e2e` ≥1
- tsc: zero errors
- lint: no new errors (21 pre-existing remain)

- [ ] **Step 3: Sanity-check git history.**

```bash
git log --oneline ^main HEAD | head -20
```

Expected: every Plan-6 commit visible, no `WIP` / `fixup`.

- [ ] **Step 4: Commit.**

```bash
git add CLAUDE.md
git commit -m "docs(claude.md): document Knowledge feature module + key invariants"
```

---

## Spec coverage cross-check

| Spec phase | Plan |
|---|---|
| 1. Backend skeleton | 1 |
| 2. Conversion pipeline | 2 |
| 3. Credibility classifier | 2 |
| 4. Graph derivation | 2 |
| 5. Macros: ingest | 2 |
| 6. Macros: query + lint | 2 |
| 7. History + restore | 2 |
| 8. .brkb export/import | 3 |
| 9. Server routes | 3 |
| 10. Frontend route + KB selector | 4 |
| 11. Frontend ingest panel | 4 |
| 12. Frontend graph view | 5 |
| 13. Change log drawer + revert | 5 |
| 14. Chat integration | **6** (this plan) |
| 15. Polish + docs | **6** (this plan: retracted badge + CLAUDE.md) |

All 15 phases covered.

**Deliberately deferred** (would be a Plan 7 if scoped):
- Future-state ghost-node rendering in change-log preview mode. Requires a new `GET /knowledge/bases/:id/state-at?sha=…` returning the full graph at that SHA. The current Plan-5 "preview" mode shows a banner only; the graph keeps showing current data. If the user wants it, write Plan 7.
- Server-side stored layout positions for >500-node KBs (graph perf risk noted in the spec's Risks section). Not needed until a real user hits the limit.
