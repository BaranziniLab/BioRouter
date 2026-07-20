# Plan 4 — Knowledge view and ingest panel

> **What this is.** Plan 4 of the six-plan Knowledge buildout: the sidebar entry, the top-level `KnowledgeView` shell, the multi-KB command-palette selector, and the ingest panel with dropzone, paste box, staged list, model picker and live SSE progress.
> **Status:** Historical record — executed and shipped. `ui/desktop/src/components/knowledge/` contains `KnowledgeView.tsx`, `KnowledgeContext.tsx`, `KBSelector/`, `IngestPanel/`, `DispatchProgress.tsx` and `hooks/` — the tree this plan specifies. The unticked `- [ ]` checkboxes below are the plan as written, not outstanding work.
> **Audience:** developers working on the Knowledge desktop UI, and agents tracing why a component is shaped the way it is.
>
> **Plan numbering.** "Plan *N* of 6" refers to the six sibling documents in this
> folder, `plan-1-…` through `plan-6-…`, executed in order against the design in
> [`founding-design.md`](founding-design.md).

Plan 3 put the Knowledge backend behind HTTP. This plan builds the first user-facing surface on top of it: a Knowledge route in the sidebar where a user creates knowledge bases, drops files or pastes text and URLs, picks a model, hits Digest, and watches sub-agent events stream in. The right-hand column stays a placeholder until [Plan 5](plan-5-graph-view-and-change-log.md).

> **Warning — the UI mockup this plan depends on is unrecoverable.** Several
> visual decisions below are justified by reference to a mockup at
> `/Users/wgu/Downloads/biorouter_knowledge.html`. That is a personal Downloads
> path, never committed to the repo, and no copy survives. Where the plan says
> "matching the mockup", the shipped components in
> `ui/desktop/src/components/knowledge/` are the only remaining record of what was
> meant. A later redesign of this surface is captured in
> [`docs/history/ui-overhaul-2026-07/knowledge-view-redesign.md`](../ui-overhaul-2026-07/knowledge-view-redesign.md).

> **Warning — this plan has thin automated test coverage by design.** The repo had
> no strong frontend unit-test culture when this was written, so several tasks
> deliberately skip Vitest and lean on the existing Playwright end-to-end setup or
> on a manual smoke check via `just run-ui`. Tasks that do add Vitest tests say so
> explicitly. Treat the absence of a test step as a known gap, not as a signal that
> the behaviour is covered elsewhere.

> **Note — worktree paths, line anchors and test counts are point-in-time.**
> Commands below `cd` into `/Users/wgu/Desktop/biorouter-knowledge`, the isolated
> git worktree the Knowledge branch was developed in; read it as your own checkout
> root. The baseline gate ("14 passed") records the suite as it stood when this
> plan was written. Line anchors such as `AppSidebar.tsx:43-110` and
> `SkillsView.tsx#L150` have moved and never resolved as rendered markdown links —
> use the file paths and symbol names.

## Risks to watch during execution

- **Model picker reuse**: The chat's `ModelsBottomBar` is tangled with the chat-session state. If lifting it cleanly is hard, the Plan-4-simple read-only picker shipped above is acceptable; Plan 6 polishes it.
- **SSE through axum without `axum-extra::sse`**: the backend returns `text/event-stream` framed manually. The frontend `useIngestStream` parses it manually. If the backend's framing changes (e.g., from comma-split to JSON-stream), the parser breaks. The current backend format is documented in the Plan 3 handlers.
- **File upload from frontend**: Task 7's Digest button skips file uploads (only text/url). File uploads need multipart `FormData` against the `/raw` endpoint; the SDK may not generate a clean Blob-aware method. Hand-rolled `fetch` with `FormData` is the fallback.
- **`MainPanelLayout` + nested provider**: wrapping `KnowledgeView` inside `KnowledgeProvider` means the provider's API call fires every time the user clicks the Knowledge sidebar entry. Cache via `useEffect`/`useMemo` if it becomes annoying.
- **`crypto.randomUUID()`**: requires a secure context. The Electron renderer should provide one, but if the bundled environment doesn't, fall back to `Math.random().toString(36).substring(2)`.

## Scope and approach

**Goal:** Add a Knowledge entry in the sidebar (between Skills and Settings), build the top-level `KnowledgeView` shell, the multi-KB selector (trigger + cmd-K-style palette), and the ingest panel (dropzone, paste box, staged list, model picker, live progress). After Plan 4, users can create knowledge bases, drop files / paste text / paste URLs to ingest, pick a model, hit Digest, and watch sub-agent events stream in. The graph view + change-log drawer come in Plan 5.

**Architecture:**
- Sidebar entry at `path: '/knowledge'` follows the existing menuItems pattern.
- `KnowledgeView` is a new top-level route component using the existing `<MainPanelLayout>` shell.
- A `KnowledgeContext` holds the active KB id, persists it to `localStorage`, and exposes setter / list hooks. Used by every Knowledge component.
- The KB selector replicates the mockup: a trigger button (colored dot + name + "N sources · M pages") with click-to-open command palette. Built custom (no `cmdk` dep in the project) with a portal-mounted modal + keyboard nav.
- The IngestPanel mirrors the mockup's left column: dropzone, paste-text box (with URL-extraction chip preview), staged list, "Digest" button that POSTs each staged item to `/knowledge/bases/:id/ingest`. SSE events stream live into a collapsible `<DispatchProgress>` panel.
- The right side of the view is a placeholder (graph + change log) — Plan 5 fills it in.

**Tech stack:** React 19, TypeScript, the auto-generated TS API client at `ui/desktop/src/api/`, Tailwind utility classes matching existing BioRouter component style. No new npm deps unless a step explicitly calls one out.

**Source spec:** [`founding-design.md`](founding-design.md). UI mockup: `/Users/wgu/Downloads/biorouter_knowledge.html` (no longer available — see the warning above).

**Series position:** Plan 4 of 6. Plan 5 = graph view + change-log drawer. Plan 6 = chat-side KB chip + slash commands + polish.

**TDD note:** Same convention as Plans 1-3 — most tasks combine "write tests" + "write impl" into single steps. The frontend doesn't have a strong unit-test culture in this repo, so several tasks deliberately skip Vitest and lean on the existing Playwright e2e setup OR on a manual smoke check via `just run-ui`. Tasks that DO add Vitest tests will say so explicitly.

**Execution convention:** the plan was written for an agentic worker driving it task-by-task with the `superpowers:subagent-driven-development` or `superpowers:executing-plans` skill. Steps use checkbox (`- [ ]`) syntax for tracking.

---

## Before starting

- [ ] **Pre-step A:** branch + baseline.

```bash
cd /Users/wgu/Desktop/biorouter-knowledge && source bin/activate-hermit
git rev-parse --abbrev-ref HEAD       # expect feature/knowledge
# Verify backend tests still pass:
cargo test -p biorouter-server --test knowledge_routes 2>&1 | tail -3   # 14 passed
# Verify frontend builds:
cd ui/desktop && npm run typecheck 2>&1 | tail -3
```

If `npm run typecheck` doesn't exist, use `npm run lint:check` or `npm run build` for a fast type-only check.

- [ ] **Pre-step B:** skim the integration points the Plan 4 recon uncovered.

  - Sidebar menuItems in `ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx` — insert between the Skills and Settings entries.
  - Route registration in `ui/desktop/src/App.tsx` — the import, the `RouteWrapper`, and the `<Route>` element.
  - Layout: `ui/desktop/src/components/Layout/MainPanelLayout.tsx` wraps with `flex flex-col bg-background-muted h-full`.
  - Reference page style: `ui/desktop/src/components/skills/SkillsView.tsx`.
  - Generated SDK: `listBases`, `createBase`, `getGraph`, etc. already in `ui/desktop/src/api/sdk.gen.ts`.
  - Model picker reference: `ui/desktop/src/components/settings/models/bottom_bar/ModelsBottomBar.tsx` (drop-down trigger pattern).
  - Context pattern: `ui/desktop/src/contexts/ChatContext.tsx` (Provider + hook).
  - localStorage usage: see `ui/desktop/src/contexts/ThemeContext.tsx` — direct `window.localStorage.*`.

---

## File structure (decomposition map)

```text
ui/desktop/src/components/knowledge/                 — NEW directory
├── KnowledgeView.tsx                 — top-level page (MainPanelLayout + 2-col grid)
├── KnowledgeContext.tsx              — Provider + useKnowledgeContext() hook
├── hooks/
│   ├── useKnowledgeBases.ts          — list/create/delete via API
│   ├── useIngestStream.ts            — SSE consumer (EventSource wrapper)
│   └── useStagedSources.ts           — local state for the staged-list
├── KBSelector/
│   ├── KBSelectorTrigger.tsx         — the rounded button
│   └── KBSelectorPalette.tsx         — modal palette w/ keyboard nav + "Create" item
├── IngestPanel/
│   ├── IngestPanel.tsx               — column shell + Digest button
│   ├── Dropzone.tsx                  — drag/drop + Browse files
│   ├── PasteTextBox.tsx              — textarea + Stage button + URL chips preview
│   ├── StagedList.tsx                — list of staged sources w/ remove
│   └── IngestModelPicker.tsx         — model picker (reuses dropdown pattern)
├── DispatchProgress.tsx              — live SSE event renderer (collapsible)
├── RightSidePlaceholder.tsx          — graph + change-log stubs (Plan 5)
└── styles.css                        — scoped styles porting mockup tokens

ui/desktop/src/components/icons/app-icons.tsx  — extend with a Knowledge icon
ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx — add menu entry
ui/desktop/src/App.tsx                          — register the route
CLAUDE.md                                      — document Plan 4
```

---

## Task 1: Sidebar entry + route registration + bare `KnowledgeView` shell

**Files:**
- Modify: `ui/desktop/src/components/icons/app-icons.tsx` (add a Knowledge icon)
- Modify: `ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx` (add menu item)
- Modify: `ui/desktop/src/App.tsx` (register route)
- Create: `ui/desktop/src/components/knowledge/KnowledgeView.tsx` (empty shell)

- [ ] **Step 1: Add a Knowledge icon**

In `icons/app-icons.tsx`, find the existing icon exports (each is a named React component wrapping an SVG). Add one called `KnowledgeIcon` (or `Network`) — use the mockup's "central node with rays" glyph:

```tsx
export const KnowledgeIcon: React.FC<React.SVGProps<SVGSVGElement>> = (props) => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6}
       strokeLinecap="round" strokeLinejoin="round" {...props}>
    <circle cx="12" cy="12" r="3"/>
    <circle cx="5" cy="6" r="1.6"/>
    <circle cx="19" cy="6" r="1.6"/>
    <circle cx="6" cy="18" r="1.6"/>
    <circle cx="18" cy="18" r="1.6"/>
    <path d="M10 10.5L6 7M14 10.5l4-3.5M10 14l-4 3M14 14l4 3"/>
  </svg>
);
```

Match the export style other icons in the file use (named exports vs default).

- [ ] **Step 2: Add the sidebar entry between Skills and Settings**

In `AppSidebar.tsx`, locate the Skills item (~line 89). Add immediately after it:

```tsx
{
  type: 'item' as const,
  path: '/knowledge',
  label: 'Knowledge',
  icon: KnowledgeIcon,
  tooltip: 'Personal knowledge bases',
},
```

Update the icon import at the top of the file to include `KnowledgeIcon`.

- [ ] **Step 3: Register the route**

In `App.tsx`:

```tsx
// Top imports (alphabetical-ish among siblings):
import KnowledgeView from './components/knowledge/KnowledgeView';

// Among the other RouteWrapper consts (~line 214):
const KnowledgeRoute = () => <KnowledgeView />;

// In the <Routes> tree (~line 618), between Skills and Settings:
<Route path="knowledge" element={<KnowledgeRoute />} />
```

- [ ] **Step 4: Create the bare `KnowledgeView` shell**

```tsx
// ui/desktop/src/components/knowledge/KnowledgeView.tsx
import { MainPanelLayout } from '../Layout/MainPanelLayout';

export default function KnowledgeView() {
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
          <div className="border-r border-border-subtle p-6">
            <p className="text-sm text-text-muted">Ingest panel coming in later tasks.</p>
          </div>
          <div className="p-6">
            <p className="text-sm text-text-muted">Graph view comes in Plan 5.</p>
          </div>
        </div>
      </div>
    </MainPanelLayout>
  );
}
```

- [ ] **Step 5: Smoke test**

```bash
cd ui/desktop && npm run typecheck 2>&1 | tail -5     # OR npm run build
```

Then `just run-ui` and confirm: Knowledge sidebar entry appears between Skills and Settings; clicking it loads the page; placeholder text renders.

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/icons/app-icons.tsx \
        ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx \
        ui/desktop/src/App.tsx \
        ui/desktop/src/components/knowledge/KnowledgeView.tsx
git commit -m "feat(ui): Knowledge sidebar entry + route + bare shell"
```

---

## Task 2: `KnowledgeContext` with localStorage persistence

**Files:**
- Create: `ui/desktop/src/components/knowledge/KnowledgeContext.tsx`
- Modify: `ui/desktop/src/components/knowledge/KnowledgeView.tsx` (wrap with provider)

- [ ] **Step 1: Implement**

```tsx
// KnowledgeContext.tsx
import { createContext, ReactNode, useCallback, useContext, useEffect, useMemo, useState } from 'react';
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
}

const KnowledgeContext = createContext<KnowledgeContextType | null>(null);

export function KnowledgeProvider({ children }: { children: ReactNode }) {
  const [bases, setBases] = useState<Manifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeKbId, setActiveKbIdState] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY_ACTIVE_KB)
  );

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

  useEffect(() => { void refresh(); }, [refresh]);

  // If activeKbId points to a base that no longer exists, clear it.
  useEffect(() => {
    if (activeKbId && bases.length > 0 && !bases.some((b) => b.id === activeKbId)) {
      setActiveKbId(null);
    }
  }, [activeKbId, bases, setActiveKbId]);

  const activeKb = useMemo(
    () => bases.find((b) => b.id === activeKbId) ?? null,
    [bases, activeKbId]
  );

  const value: KnowledgeContextType = { bases, loading, activeKb, activeKbId, setActiveKbId, refresh };
  return <KnowledgeContext.Provider value={value}>{children}</KnowledgeContext.Provider>;
}

export function useKnowledge(): KnowledgeContextType {
  const ctx = useContext(KnowledgeContext);
  if (!ctx) throw new Error('useKnowledge must be used inside <KnowledgeProvider>');
  return ctx;
}
```

- [ ] **Step 2: Wrap `KnowledgeView` with the provider**

```tsx
// KnowledgeView.tsx
import { KnowledgeProvider } from './KnowledgeContext';

export default function KnowledgeView() {
  return (
    <KnowledgeProvider>
      <KnowledgeViewInner />
    </KnowledgeProvider>
  );
}

function KnowledgeViewInner() {
  // existing JSX
}
```

- [ ] **Step 3: Smoke test**

`npm run typecheck` (or `build`). The view should still render; the API call happens in the background.

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): KnowledgeContext with bases listing + localStorage active-KB"
```

---

## Task 3: KB selector trigger + palette

**Files:**
- Create: `ui/desktop/src/components/knowledge/KBSelector/KBSelectorTrigger.tsx`
- Create: `ui/desktop/src/components/knowledge/KBSelector/KBSelectorPalette.tsx`
- Modify: `ui/desktop/src/components/knowledge/KnowledgeView.tsx` (mount the trigger in the left column header)

The trigger is the rounded button from the mockup: a small colored dot, a name in bold, a meta line ("N sources · M pages"), and a chevron. Clicking opens the palette modal — a search input + filtered/grouped list + "Create new KB" item at the bottom. Keyboard navigation: ↑/↓ to move, Enter to select, Esc to close.

- [ ] **Step 1: Trigger component**

```tsx
// KBSelectorTrigger.tsx
import { useState } from 'react';
import { ChevronDown } from 'lucide-react';   // already used elsewhere in the app
import { useKnowledge } from '../KnowledgeContext';
import { KBSelectorPalette } from './KBSelectorPalette';

export function KBSelectorTrigger() {
  const { activeKb } = useKnowledge();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="w-full inline-flex items-center gap-2 px-3 py-2 rounded-lg border border-border-subtle bg-background-surface hover:border-border-default transition-colors"
      >
        <span className="w-2 h-2 rounded-full"
          style={{ background: activeKb?.color ?? 'var(--text-muted)' }} />
        <span className="flex-1 text-left min-w-0">
          <span className="block text-sm font-medium truncate">
            {activeKb?.name ?? 'Select a knowledge base'}
          </span>
        </span>
        <ChevronDown className="w-3 h-3 text-text-muted" />
      </button>
      {open && <KBSelectorPalette onClose={() => setOpen(false)} />}
    </>
  );
}
```

- [ ] **Step 2: Palette modal**

```tsx
// KBSelectorPalette.tsx
import { useEffect, useRef, useState } from 'react';
import { Search, Plus } from 'lucide-react';
import { createBase } from '../../../api';
import { useKnowledge } from '../KnowledgeContext';
import type { Manifest } from '../../../api/types.gen';

interface Props { onClose: () => void; }

export function KBSelectorPalette({ onClose }: Props) {
  const { bases, refresh, setActiveKbId } = useKnowledge();
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  // Filter bases by name (case-insensitive substring).
  const filtered: Manifest[] = bases.filter((b) =>
    b.name.toLowerCase().includes(query.toLowerCase())
  );
  const showCreate = query.length > 0 && !filtered.some((b) => b.id === slugify(query));
  const items: Array<Manifest | { create: true; slug: string; name: string }> = [
    ...filtered,
    ...(showCreate ? [{ create: true as const, slug: slugify(query), name: query }] : []),
  ];

  useEffect(() => { setCursor(0); }, [query]);

  function commitAt(i: number) {
    const it = items[i];
    if (!it) return;
    if ('create' in it) {
      // POST /knowledge/bases
      void (async () => {
        try {
          const res = await createBase({ throwOnError: true, body: { id: it.slug, name: it.name } });
          await refresh();
          if (res.data?.id) setActiveKbId(res.data.id);
        } catch (err) {
          console.error('createBase failed', err);
        } finally {
          onClose();
        }
      })();
    } else {
      setActiveKbId(it.id);
      onClose();
    }
  }

  function onKey(e: React.KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    else if (e.key === 'ArrowDown') { e.preventDefault(); setCursor((c) => Math.min(c + 1, items.length - 1)); }
    else if (e.key === 'ArrowUp')   { e.preventDefault(); setCursor((c) => Math.max(c - 1, 0)); }
    else if (e.key === 'Enter')     { e.preventDefault(); commitAt(cursor); }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/30 backdrop-blur-sm" onClick={onClose}>
      <div className="w-[540px] max-w-[92vw] max-h-[70vh] bg-background-surface border border-border-subtle rounded-2xl shadow-2xl overflow-hidden flex flex-col" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center gap-2 px-4 py-3 border-b border-border-subtle">
          <Search className="w-4 h-4 text-text-muted" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder="Switch knowledge base… type to search"
            className="flex-1 bg-transparent outline-none text-sm"
          />
          <kbd className="text-[10px] font-mono text-text-muted border border-border-subtle rounded px-1.5 py-0.5">esc</kbd>
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          {items.length === 0 && (
            <div className="px-4 py-6 text-center text-sm text-text-muted">No knowledge base matches.</div>
          )}
          {items.map((it, i) => (
            <div
              key={'create' in it ? `__create_${it.slug}` : it.id}
              onMouseEnter={() => setCursor(i)}
              onClick={() => commitAt(i)}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer ${i === cursor ? 'bg-background-muted' : ''}`}
            >
              {'create' in it ? (
                <>
                  <Plus className="w-3 h-3 text-text-muted" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">Create "{it.name}"</div>
                    <div className="text-[10px] font-mono text-text-muted">new knowledge base</div>
                  </div>
                </>
              ) : (
                <>
                  <span className="w-2 h-2 rounded-full" style={{ background: it.color }} />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{it.name}</div>
                    <div className="text-[10px] font-mono text-text-muted">{it.id}</div>
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function slugify(s: string): string {
  return s.toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .substring(0, 64);
}
```

- [ ] **Step 3: Mount trigger in `KnowledgeView`**

Replace the left column placeholder with `<KBSelectorTrigger />`.

- [ ] **Step 4: Smoke test**

`just run-ui`. Open Knowledge → click trigger → type a name → Enter creates a KB; existing KBs (if any) appear in the list; arrow keys + Enter switch between them.

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/knowledge/KBSelector ui/desktop/src/components/knowledge/KnowledgeView.tsx
git commit -m "feat(ui): KBSelector trigger + cmd-K-style palette (filter/create/keyboard nav)"
```

---

## Task 4: `useKnowledgeBases` hook + delete-base affordance

**Files:**
- Create: `ui/desktop/src/components/knowledge/hooks/useKnowledgeBases.ts`

The hook centralizes mutation calls so components don't directly import SDK methods. It also exposes a `delete` operation (with confirm, used elsewhere).

- [ ] **Step 1: Implement**

```ts
// hooks/useKnowledgeBases.ts
import { useCallback } from 'react';
import { createBase as apiCreate, deleteBase as apiDelete } from '../../../api';
import { useKnowledge } from '../KnowledgeContext';

export function useKnowledgeBases() {
  const { refresh, setActiveKbId, activeKbId } = useKnowledge();

  const create = useCallback(async (id: string, name: string, color?: string) => {
    const res = await apiCreate({ throwOnError: true, body: { id, name, ...(color ? { color } : {}) } });
    await refresh();
    return res.data;
  }, [refresh]);

  const remove = useCallback(async (id: string) => {
    await apiDelete({ throwOnError: true, path: { id } });
    if (activeKbId === id) setActiveKbId(null);
    await refresh();
  }, [refresh, activeKbId, setActiveKbId]);

  return { create, remove };
}
```

- [ ] **Step 2: No new UI surface yet** — the hook gets used by Task 3's palette (which already does its own create call inline) and by an eventual "delete" button in Plan 5. For now, this is just the API surface.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/knowledge/hooks
git commit -m "feat(ui): useKnowledgeBases hook (create + delete)"
```

---

## Task 5: `IngestPanel` layout + Dropzone

**Files:**
- Create: `ui/desktop/src/components/knowledge/IngestPanel/IngestPanel.tsx`
- Create: `ui/desktop/src/components/knowledge/IngestPanel/Dropzone.tsx`
- Modify: `KnowledgeView.tsx` (replace left-column placeholder with IngestPanel)

- [ ] **Step 1: `useStagedSources` hook**

```ts
// hooks/useStagedSources.ts
import { useCallback, useState } from 'react';

export type StagedSource =
  | { kind: 'file'; id: string; file: File; status: 'pending' | 'ingesting' | 'done' | 'error'; error?: string }
  | { kind: 'url'; id: string; url: string; status: 'pending' | 'ingesting' | 'done' | 'error'; error?: string }
  | { kind: 'text'; id: string; text: string; title?: string; status: 'pending' | 'ingesting' | 'done' | 'error'; error?: string };

export function useStagedSources() {
  const [items, setItems] = useState<StagedSource[]>([]);
  const add = useCallback((s: StagedSource) => setItems((xs) => [...xs, s]), []);
  const remove = useCallback((id: string) => setItems((xs) => xs.filter((s) => s.id !== id)), []);
  const update = useCallback((id: string, patch: Partial<StagedSource>) =>
    setItems((xs) => xs.map((s) => (s.id === id ? { ...s, ...patch } as StagedSource : s))), []);
  const clear = useCallback(() => setItems([]), []);
  return { items, add, remove, update, clear };
}
```

- [ ] **Step 2: Dropzone**

```tsx
// Dropzone.tsx
import { useCallback, useRef, useState } from 'react';
import { Upload, FolderOpen, Clipboard } from 'lucide-react';

interface Props {
  onFiles: (files: File[]) => void;
  onPasteTextRequested: () => void;
}

export function Dropzone({ onFiles, onPasteTextRequested }: Props) {
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const onDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) onFiles(files);
  }, [onFiles]);

  return (
    <div
      onDragEnter={(e) => { e.preventDefault(); setDragging(true); }}
      onDragOver={(e) => { e.preventDefault(); setDragging(true); }}
      onDragLeave={(e) => { e.preventDefault(); setDragging(false); }}
      onDrop={onDrop}
      className={`relative border-2 border-dashed rounded-xl p-6 text-center transition-colors ${
        dragging ? 'border-success-default bg-background-muted' : 'border-border-subtle bg-background-surface'
      }`}
    >
      <input
        ref={inputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => {
          const files = e.target.files ? Array.from(e.target.files) : [];
          if (files.length > 0) onFiles(files);
          e.target.value = '';
        }}
      />
      <Upload className="w-7 h-7 mx-auto text-text-muted" />
      <div className="mt-2 text-sm font-medium">Drag & drop to stage</div>
      <div className="mt-1 text-xs text-text-muted">Papers, snippets, HTML, datasets, .brkb</div>
      <div className="mt-3 flex flex-wrap gap-1.5 justify-center text-[10px] font-mono text-text-muted">
        {['pdf', 'md', 'html', 'docx', 'csv', 'brkb'].map((ext) => (
          <span key={ext} className="border border-border-subtle rounded px-1.5 py-0.5">.{ext}</span>
        ))}
      </div>
      <div className="mt-3 flex gap-2">
        <button onClick={() => inputRef.current?.click()}
          className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-border-subtle bg-background-default text-xs hover:bg-background-muted">
          <FolderOpen className="w-3 h-3" /> Browse files
        </button>
        <button onClick={onPasteTextRequested}
          className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-border-subtle bg-background-default text-xs hover:bg-background-muted">
          <Clipboard className="w-3 h-3" /> Paste text
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: `IngestPanel` shell wiring everything together**

```tsx
// IngestPanel.tsx
import { useState } from 'react';
import { Dropzone } from './Dropzone';
import { useStagedSources } from '../hooks/useStagedSources';

export function IngestPanel() {
  const { items, add, remove, clear } = useStagedSources();
  const [showPasteBox, setShowPasteBox] = useState(false);

  function onFiles(files: File[]) {
    for (const file of files) {
      add({ kind: 'file', id: crypto.randomUUID(), file, status: 'pending' });
    }
  }

  return (
    <div className="flex flex-col gap-4 p-4">
      <Dropzone onFiles={onFiles} onPasteTextRequested={() => setShowPasteBox(true)} />
      {/* PasteTextBox + StagedList + Digest button come next */}
      <div className="text-xs text-text-muted">Staged: {items.length}</div>
    </div>
  );
}
```

Mount inside `KnowledgeView`'s left column.

- [ ] **Step 4: Smoke + commit**

`just run-ui`, drop a file onto the dropzone, see staged count increment.

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): Dropzone + useStagedSources + IngestPanel shell"
```

---

## Task 6: PasteTextBox with URL-extraction chip preview

**Files:**
- Create: `ui/desktop/src/components/knowledge/IngestPanel/PasteTextBox.tsx`
- Modify: `IngestPanel.tsx` (mount it)

The textarea: if the user pastes text with http(s) URLs, show a chip strip "Will fetch & convert: N links" with each link toggle-able. Stage either as a single text source OR as text + one URL source per kept chip.

- [ ] **Step 1: Implement**

```tsx
// PasteTextBox.tsx
import { useMemo, useState } from 'react';

const URL_RE = /https?:\/\/[^\s<>"')]+[^\s<>"').,;:!?]/g;

interface Props {
  onStage: (text: string, title: string, urls: string[]) => void;
  onCancel: () => void;
}

export function PasteTextBox({ onStage, onCancel }: Props) {
  const [text, setText] = useState('');
  const [title, setTitle] = useState('');
  const detectedUrls = useMemo(() => {
    const set = new Set<string>();
    let m: RegExpExecArray | null;
    while ((m = URL_RE.exec(text)) !== null) set.add(m[0]);
    URL_RE.lastIndex = 0;
    return Array.from(set);
  }, [text]);

  const [includeUrls, setIncludeUrls] = useState<Record<string, boolean>>({});
  const urlsToFetch = detectedUrls.filter((u) => includeUrls[u] !== false);

  return (
    <div className="bg-background-surface border border-border-subtle rounded-xl overflow-hidden">
      <input
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Optional title…"
        className="w-full px-3 py-2 text-xs border-b border-border-subtle bg-transparent outline-none"
      />
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Paste notes, snippets, or a chunk of prose. URLs will be extracted and offered for ingestion."
        className="w-full min-h-[100px] px-3 py-2 text-xs bg-transparent outline-none resize-y"
      />
      {detectedUrls.length > 0 && (
        <div className="border-t border-border-subtle px-3 py-2 flex flex-wrap gap-1.5">
          <span className="text-[10px] text-text-muted self-center mr-1">Will fetch:</span>
          {detectedUrls.map((u) => {
            const on = includeUrls[u] !== false;
            return (
              <button key={u} onClick={() => setIncludeUrls({ ...includeUrls, [u]: !on })}
                className={`text-[10px] font-mono px-2 py-0.5 rounded-full border ${on ? 'border-border-default bg-background-muted' : 'border-border-subtle text-text-muted line-through'}`}>
                {u.length > 36 ? u.substring(0, 33) + '…' : u}
              </button>
            );
          })}
        </div>
      )}
      <div className="border-t border-border-subtle px-3 py-2 flex justify-between items-center">
        <span className="text-[10px] text-text-muted">{text.length} chars</span>
        <div className="flex gap-1.5">
          <button onClick={onCancel} className="text-xs px-2.5 py-1 rounded text-text-muted hover:text-text-default">Cancel</button>
          <button
            disabled={!text.trim()}
            onClick={() => onStage(text.trim(), title.trim() || 'Pasted note', urlsToFetch)}
            className="text-xs px-2.5 py-1 rounded bg-text-default text-background-surface font-medium disabled:opacity-50">
            Stage
          </button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Wire into `IngestPanel`**

```tsx
{showPasteBox && (
  <PasteTextBox
    onCancel={() => setShowPasteBox(false)}
    onStage={(text, title, urls) => {
      add({ kind: 'text', id: crypto.randomUUID(), text, title, status: 'pending' });
      for (const url of urls) add({ kind: 'url', id: crypto.randomUUID(), url, status: 'pending' });
      setShowPasteBox(false);
    }}
  />
)}
```

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): PasteTextBox with URL chip extraction + staging"
```

---

## Task 7: StagedList + IngestModelPicker + Digest button (without SSE yet)

**Files:**
- Create: `ui/desktop/src/components/knowledge/IngestPanel/StagedList.tsx`
- Create: `ui/desktop/src/components/knowledge/IngestPanel/IngestModelPicker.tsx`
- Modify: `IngestPanel.tsx`

The model picker reuses the dropdown pattern from `ModelsBottomBar.tsx` (small Brain-icon chip + dropdown). For Plan 4 it picks from the user's configured providers + models. Pull the current model from the existing `useModelAndProvider()` hook (find it via grep in the existing chat code) and let the user override it locally for ingest.

The Digest button (when clicked) iterates staged items, POSTs each to `/knowledge/bases/:id/raw` (for files/url/text), and then for each successfully-raw'd source POSTs to `/knowledge/bases/:id/ingest` with the picked model. Task 8 swaps this synchronous flow for SSE streaming.

For Task 7, a SIMPLIFIED Digest button: just call the raw + ingest endpoints with `fetch`-based generated SDK; await both; update status on each staged item. NO event streaming yet.

- [ ] **Step 1: StagedList**

```tsx
// StagedList.tsx
import { FileText, Globe, ClipboardList, X } from 'lucide-react';
import type { StagedSource } from '../hooks/useStagedSources';

interface Props {
  items: StagedSource[];
  onRemove: (id: string) => void;
  onClear: () => void;
}

export function StagedList({ items, onRemove, onClear }: Props) {
  if (items.length === 0) {
    return <div className="text-xs text-text-muted py-3 text-center">Nothing staged yet.</div>;
  }
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[10px] uppercase tracking-wide text-text-muted">Staged · {items.length}</span>
        <button onClick={onClear} className="text-[10px] text-text-muted hover:text-text-default">clear all</button>
      </div>
      {items.map((s) => {
        const Icon = s.kind === 'file' ? FileText : s.kind === 'url' ? Globe : ClipboardList;
        const label = s.kind === 'file' ? s.file.name : s.kind === 'url' ? s.url : (s.title || s.text.substring(0, 60));
        return (
          <div key={s.id} className="flex items-center gap-2 px-2.5 py-2 bg-background-surface border border-border-subtle rounded-lg">
            <Icon className="w-3.5 h-3.5 text-text-muted flex-shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="text-xs font-medium truncate">{label}</div>
              {s.status !== 'pending' && (
                <div className="text-[10px] font-mono text-text-muted">{s.status}{s.error ? `: ${s.error}` : ''}</div>
              )}
            </div>
            <button onClick={() => onRemove(s.id)} className="text-text-muted hover:text-text-default">
              <X className="w-3 h-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: IngestModelPicker**

For Plan 4, the SIMPLEST viable model picker: a dropdown showing the user's currently-configured models from the existing config/providers state. To minimize new code, expose the model as a `useState<ModelRef>` inside `IngestPanel`, default to whatever the chat's `currentModel` is (read from existing config), and let the user click a button that opens an `<AddModelInline>`-style chooser. If the existing chat-side model picker (`ModelsBottomBar`) is already self-contained enough to mount inside `IngestPanel` without dragging the rest of the chat in, just import + use it. If it's tangled, build a minimal picker:

```tsx
// IngestModelPicker.tsx
import { Brain, ChevronDown } from 'lucide-react';
import type { ModelRef } from '../../../api/types.gen';

interface Props {
  value: ModelRef;
  onChange: (v: ModelRef) => void;
}

export function IngestModelPicker({ value, onChange: _ }: Props) {
  // Plan-4-simple: read-only display. Plan-6 polish adds a real chooser.
  return (
    <div className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border border-border-subtle bg-background-surface text-xs">
      <Brain className="w-3 h-3 text-text-muted" />
      <span className="truncate max-w-[200px]">{value.provider} / {value.model}</span>
      <ChevronDown className="w-3 h-3 text-text-muted" />
    </div>
  );
}
```

Have IngestPanel seed the default `ModelRef` from the existing `BIOROUTER_MODEL` / `BIOROUTER_PROVIDER` config keys (read via the existing `useConfig` hook or whatever reads `~/.config/biorouter/config.yaml`'s model defaults).

- [ ] **Step 3: Digest button (sync version)**

```tsx
// In IngestPanel.tsx, add a "Digest" button at the bottom:
const onDigest = async () => {
  if (!activeKbId) return;
  for (const item of items) {
    update(item.id, { status: 'ingesting' });
    try {
      // POST /knowledge/bases/:id/raw — different body shape per kind
      // ... (for Plan 4 only: handle 'text' and 'url' modes; file uploads come in Task 8)
      // Then POST /knowledge/bases/:id/ingest with the model + a source ref
      // (Plan 4 stub: call the API; ignore the SSE stream; await completion)
      update(item.id, { status: 'done' });
    } catch (err) {
      update(item.id, { status: 'error', error: (err as Error).message });
    }
  }
};
```

The exact API method names from the generated SDK — `addRawSource`, `ingest` (or `ingestSource`, etc.) — should be looked up in `ui/desktop/src/api/sdk.gen.ts`. Use them.

- [ ] **Step 4: Smoke + commit**

`just run-ui`, paste text, hit Digest, watch staged item flip to `ingesting` → `done`.

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): StagedList + IngestModelPicker + synchronous Digest"
```

---

## Task 8: `useIngestStream` SSE hook + `DispatchProgress` live view

**Files:**
- Create: `ui/desktop/src/components/knowledge/hooks/useIngestStream.ts`
- Create: `ui/desktop/src/components/knowledge/DispatchProgress.tsx`
- Modify: `IngestPanel.tsx` (call the SSE hook instead of fire-and-forget fetch)

The backend SSE endpoint emits `data: {SubAgentEvent}\n\n` lines plus terminal `event: done\ndata: {result}\n\n` or `event: error\ndata: {message}\n\n`. Build a small wrapper using `fetch` + a ReadableStream (NOT `EventSource` — EventSource doesn't support POST bodies).

- [ ] **Step 1: Implement the SSE hook**

```ts
// hooks/useIngestStream.ts
import { useCallback, useRef, useState } from 'react';

export type SubAgentEvent =
  | { kind: 'step'; index: number; assistant_text: string }
  | { kind: 'tool_call'; name: string; args: unknown }
  | { kind: 'tool_result'; name: string; ok: boolean; summary: string }
  | { kind: 'done'; reason: string; final_text: string };

export interface StreamState {
  events: SubAgentEvent[];
  status: 'idle' | 'streaming' | 'done' | 'error';
  finalResult: unknown;
  error?: string;
}

export function useIngestStream() {
  const [state, setState] = useState<StreamState>({ events: [], status: 'idle', finalResult: null });
  const abortRef = useRef<AbortController | null>(null);

  const start = useCallback(async (url: string, body: unknown) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setState({ events: [], status: 'streaming', finalResult: null });

    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);

      const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
      let buf = '';
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += value;
        const blocks = buf.split('\n\n');
        buf = blocks.pop() ?? '';
        for (const block of blocks) {
          const lines = block.split('\n');
          let eventName = 'message';
          let data = '';
          for (const line of lines) {
            if (line.startsWith('event: ')) eventName = line.substring(7).trim();
            else if (line.startsWith('data: ')) data += line.substring(6);
          }
          if (eventName === 'done') {
            const parsed = data ? JSON.parse(data) : null;
            setState((s) => ({ ...s, status: 'done', finalResult: parsed }));
          } else if (eventName === 'error') {
            const parsed = data ? JSON.parse(data) : { message: 'unknown' };
            setState((s) => ({ ...s, status: 'error', error: parsed.message }));
          } else {
            try {
              const ev = JSON.parse(data);
              setState((s) => ({ ...s, events: [...s.events, ev] }));
            } catch { /* ignore malformed */ }
          }
        }
      }
    } catch (e) {
      if ((e as Error).name !== 'AbortError') {
        setState((s) => ({ ...s, status: 'error', error: (e as Error).message }));
      }
    }
  }, []);

  const abort = useCallback(() => { abortRef.current?.abort(); }, []);

  return { ...state, start, abort };
}
```

- [ ] **Step 2: `DispatchProgress`**

```tsx
// DispatchProgress.tsx
import { ChevronDown, ChevronRight } from 'lucide-react';
import { useState } from 'react';
import type { StreamState } from './hooks/useIngestStream';

export function DispatchProgress({ state }: { state: StreamState }) {
  const [open, setOpen] = useState(true);
  if (state.status === 'idle') return null;
  return (
    <div className="border border-border-subtle rounded-xl bg-background-surface">
      <button onClick={() => setOpen(!open)} className="w-full flex items-center justify-between px-3 py-2">
        <span className="text-xs font-medium">
          {state.status === 'streaming' ? 'Digesting…' :
           state.status === 'done' ? 'Done' :
           state.status === 'error' ? `Error: ${state.error}` : ''}
          <span className="ml-2 text-text-muted">{state.events.length} steps</span>
        </span>
        {open ? <ChevronDown className="w-3 h-3" /> : <ChevronRight className="w-3 h-3" />}
      </button>
      {open && (
        <div className="border-t border-border-subtle px-3 py-2 max-h-[240px] overflow-y-auto flex flex-col gap-1.5">
          {state.events.map((ev, i) => (
            <div key={i} className="text-[10px] font-mono text-text-muted">
              {ev.kind === 'step' && <>step {ev.index}: {ev.assistant_text.substring(0, 80)}</>}
              {ev.kind === 'tool_call' && <>→ {ev.name}({JSON.stringify(ev.args).substring(0, 60)})</>}
              {ev.kind === 'tool_result' && <>← {ev.name}: {ev.ok ? '✓' : '✗'} {ev.summary.substring(0, 60)}</>}
              {ev.kind === 'done' && <>done: {ev.reason}</>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Wire SSE into IngestPanel**

Replace the sync Digest flow from Task 7 with a per-item SSE call using `useIngestStream`. Each staged item maps to a separate ingest stream (or, simpler: process serially, one item at a time, reusing the same stream state).

Render `<DispatchProgress state={stream} />` below the staged list.

- [ ] **Step 4: Smoke + commit**

`just run-ui`. Pick a model. Paste a small note. Hit Digest. Watch events stream into the DispatchProgress panel in real time. After it finishes, `bases` refresh and the graph (currently placeholder, Plan 5) would show the new node.

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): useIngestStream SSE hook + DispatchProgress live event view"
```

---

## Task 9: Right-side placeholder (graph + change log)

**Files:**
- Create: `ui/desktop/src/components/knowledge/RightSidePlaceholder.tsx`

Just a placeholder with two regions ("Knowledge graph" header above empty space; "Change log" toggle button) so the layout looks right. Plan 5 replaces it with the real graph + drawer.

- [ ] **Step 1: Implement**

```tsx
export function RightSidePlaceholder() {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-6 py-3 border-b border-border-subtle">
        <span className="text-xs text-text-muted">knowledge graph (coming in Plan 5)</span>
        <button className="text-xs text-text-muted border border-border-subtle px-2.5 py-1 rounded-md">Change log</button>
      </div>
      <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
        Graph view will render here once you ingest some sources.
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Mount** in `KnowledgeView`'s right column, replacing the placeholder text.

- [ ] **Step 3: Commit**

```bash
git add ui/desktop/src/components/knowledge
git commit -m "feat(ui): right-side placeholder (graph/change-log layout slots)"
```

---

## Task 10: Final polish + CLAUDE.md

- [ ] **Step 1: Polish**
  - Verify the Knowledge view works with no KBs (empty state) and with multiple KBs.
  - Verify the trigger button shows the active KB color dot + name correctly.
  - Verify the palette can create a new KB, switch between KBs, and handles the no-match case.
  - Verify drag-drop, paste-text, and the staged list look right.
  - Verify Digest streams events and final commit_sha appears.
  - Confirm there are no console errors in DevTools (`just debug-ui-main-process` for the chrome inspector).

- [ ] **Step 2: CLAUDE.md**

In the Core Agent Library OR in the Frontend section, add a bullet:

```markdown
- **`ui/desktop/src/components/knowledge/`** — Top-level Knowledge route in
  the sidebar (between Skills and Settings). Provides KB selector, ingest
  panel (dropzone / paste text / URL extraction), live SSE-streamed
  digestion progress. Graph view + change log come in Plan 5.
```

- [ ] **Step 3: Frontend tests pass (if a Vitest suite was extended)**

```bash
cd ui/desktop && npm run typecheck 2>&1 | tail -5    # OR npm run lint:check
```

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(claude): document Plan 4 Knowledge frontend (route + ingest)"
```

---

## Related documentation

- [Knowledge founding design](founding-design.md) — the component tree, ingest flow and styling rules this plan implements.
- [Plan 3 — HTTP routes and export/import](plan-3-http-routes-and-export.md) — the `/knowledge/*` endpoints and SSE framing every component here calls.
- [Plan 5 — graph view and change log](plan-5-graph-view-and-change-log.md) — fills in the right-column placeholder Task 9 leaves behind.
- [Plan 6 — chat integration and closeout](plan-6-chat-integration-and-closeout.md) — moves active-KB state off `localStorage` and adds the chat-side KB chip.
- [Knowledge view redesign](../ui-overhaul-2026-07/knowledge-view-redesign.md) — the later visual rework of this surface.
