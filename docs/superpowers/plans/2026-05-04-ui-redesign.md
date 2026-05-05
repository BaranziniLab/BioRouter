# BioRouter UI Redesign — Workspace Aesthetic

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform BioRouter from a "beige dashboard" to a "calm clinical workspace" — flatter surfaces, tighter spacing, thinner borders in place of heavy shadows — while keeping the warm cream color identity and all functionality intact.

**Architecture:** All style changes live in `src/styles/main.css` (CSS tokens + shadows) and the affected page/component `.tsx` files. No routing, data fetching, or logic is touched.

**Tech Stack:** Tailwind CSS v4, CSS custom properties, React/TSX

---

## Files changed

| File | What changes |
|---|---|
| `ui/desktop/src/styles/main.css` | Shadow token, neutral palette, new border-subtle token |
| `ui/desktop/src/components/sessions/SessionsInsights.tsx` | Remove heavy hero card; inline metrics row; compact |
| `ui/desktop/src/components/Hub.tsx` | Remove outer card wrapper on input area |
| `ui/desktop/src/components/sessions/SessionListView.tsx` | Card → row-style items; remove shadow-default |
| `ui/desktop/src/components/extensions/ExtensionsView.tsx` | Remove heavy header card; clean flat header |
| `ui/desktop/src/components/sessions/SessionItem.tsx` | Match new row style (for embedded usage) |
| `ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx` | Tighter spacing; more subtle active state |
| `.claude/commands/frontend-design.md` | New project skill documenting the design language |

---

## Phase 1 — CSS Design Tokens (main.css only)

**Goal:** Establish the calmer surface system. Every downstream component benefits automatically via CSS variables.

### Task 1.1 — Flatten `--shadow-default` and add `--shadow-card` / `--border-subtle`

- [ ] In `ui/desktop/src/styles/main.css` `:root`, replace the current `--shadow-default` with a much lighter single-layer shadow and add a named card border:

```css
/* inside :root { ... } */
--shadow-default:
  0px 1px 3px 0px rgba(0, 0, 0, 0.06),
  0px 0px 1px 0px rgba(0, 0, 0, 0.12);
--shadow-popover:
  0px 8px 24px 0px rgba(32, 25, 15, 0.10),
  0px 0px 1px 0px rgba(0, 0, 0, 0.15);
--border-subtle: var(--color-neutral-200);
```

- [ ] Do the same in `.dark { ... }`:

```css
--shadow-default:
  0px 1px 3px 0px rgba(0, 0, 0, 0.25),
  0px 0px 1px 0px rgba(0, 0, 0, 0.35);
--shadow-popover:
  0px 8px 24px 0px rgba(0, 0, 0, 0.4),
  0px 0px 1px 0px rgba(0, 0, 0, 0.5);
--border-subtle: var(--color-neutral-800);
```

- [ ] Expose `--border-subtle` in the `@theme inline` block:

```css
--color-border-subtle: var(--border-subtle);
```

### Task 1.2 — Slightly calm the neutral palette (less yellow-green)

The current `neutral-100: #f8f2df` has a noticeable yellow cast. Shift the anchor 3–5 points toward a purer warm-gray while keeping warmth.

- [ ] Update `@theme` neutrals in `main.css`:

```css
--color-neutral-50:  #faf8f3;
--color-neutral-100: #f4f0e6;
--color-neutral-200: #e8e1d2;
--color-neutral-300: #d4cab6;
--color-neutral-400: #b0a892;
--color-neutral-500: #88806a;
--color-neutral-600: #615a46;
--color-neutral-700: #403928;
--color-neutral-800: #282217;
--color-neutral-900: #16120c;
--color-neutral-950: #0d0a06;
```

- [ ] Commit:
```bash
git add ui/desktop/src/styles/main.css
git commit -m "design: flatten shadow token and calm neutral palette"
```

---

## Phase 2 — Home / Hub page

**Goal:** Remove the two-layer floating-card structure from the Hub. Page feels like an open workspace, not a dashboard.

### Task 2.1 — SessionsInsights: inline metrics, remove hero card

Currently the Hub has a large `rounded-2xl` card with `pt-16` padding, then two big metric cards below it. Replace with:
- Greeting text directly on the `bg-background-muted` canvas (no card wrapper)
- Metrics as a single inline row (not two tall cards)
- Recent sessions card kept but borders replacing shadow

- [ ] Open `ui/desktop/src/components/sessions/SessionsInsights.tsx`
- [ ] Replace the hero card section (lines ~183–196) — from the wrapping `<div … rounded-2xl mb-4>` to `</div>` — with a plain padded block:

```tsx
{/* Hero — text directly on canvas, no card wrapper */}
<div className="px-8 pt-16 pb-6">
  <p className="text-xs font-medium text-text-muted uppercase tracking-widest mb-3">BioRouter</p>
  <Greeting />
</div>
```

- [ ] Replace the two separate metric Cards (lines ~212–246) with one compact inline row:

```tsx
{/* Compact inline stats */}
<div className="flex gap-6 px-8 pb-6">
  <div>
    <p className="text-3xl font-mono font-light">
      {Math.max(insights?.totalSessions ?? 0, 0)}
    </p>
    <span className="text-[11px] text-text-muted uppercase tracking-wider">Sessions</span>
  </div>
  <div className="w-px bg-border-default self-stretch mx-1" />
  <div>
    <p className="text-3xl font-mono font-light">{formatTokens(insights?.totalTokens)}</p>
    <span className="text-[11px] text-text-muted uppercase tracking-wider">Tokens</span>
  </div>
</div>
```

- [ ] Replace the Recent chats Card (lines ~249–310) — keep it as a card but swap `boxShadow: var(--shadow-default)` → border:

```tsx
<Card
  className="w-full py-5 px-6 rounded-2xl bg-background-default border border-border-subtle"
  // remove: style={{ boxShadow: 'var(--shadow-default)' }}
>
```

- [ ] Update the matching skeleton block at the top of the file to mirror the same structure.

### Task 2.2 — Hub.tsx: remove the shadow wrapper from ChatInput

Currently the `ChatInput` at the bottom of `Hub.tsx` sits inside a `<div>` with `boxShadow: var(--shadow-default)`. The new flatter shadow token already reduces the visual weight, but also change border-radius from `rounded-2xl` to `rounded-xl` to make it feel less like a floating panel:

- [ ] In `Hub.tsx`, update the div wrapping `<ChatInput>`:

```tsx
<div className="mx-4 mb-4 rounded-xl overflow-hidden border border-border-subtle bg-background-default">
  <ChatInput … />
</div>
```

(Remove the `style={{ boxShadow: … }}` entirely — the border gives the boundary without elevation.)

- [ ] Commit:
```bash
git add ui/desktop/src/components/sessions/SessionsInsights.tsx \
        ui/desktop/src/components/Hub.tsx
git commit -m "design: flatten home page — inline metrics, remove hero card weight"
```

---

## Phase 3 — Session History (card grid → bordered rows)

**Goal:** Sessions list feels scannable like a table, not a card wall.

### Task 3.1 — Session card → row

- [ ] In `SessionListView.tsx`, find the `<Card … session-item …>` block and replace with a flat row:

```tsx
<div
  onClick={handleCardClick}
  className="session-item flex items-center justify-between gap-3 py-3 px-4 rounded-xl cursor-pointer transition-all duration-150 relative group border border-border-subtle bg-background-default hover:bg-background-muted hover:border-border-strong"
  ref={(el) => setSessionRefs(session.id, el)}
>
  {/* Title + date — main row */}
  <div className="flex-1 min-w-0">
    <h3 className="text-sm font-medium truncate">{session.name}</h3>
    <div className="flex items-center gap-3 mt-0.5 text-text-muted text-xs">
      <div className="flex items-center gap-1">
        <Calendar className="w-3 h-3 flex-shrink-0" />
        <span>{formatMessageTimestamp(Date.parse(session.updated_at) / 1000)}</span>
      </div>
      <div className="flex items-center gap-1 min-w-0">
        <Folder className="w-3 h-3 flex-shrink-0" />
        <span className="truncate max-w-[240px]">{session.working_dir}</span>
      </div>
    </div>
  </div>

  {/* Right-side stats + hover actions */}
  <div className="flex items-center gap-3 flex-shrink-0">
    <div className="flex items-center gap-2 text-xs text-text-muted font-mono">
      <span>{session.message_count} msgs</span>
      {session.total_tokens !== null && (
        <span>{(session.total_tokens || 0).toLocaleString()}</span>
      )}
      {extensionNames.length > 0 && (
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <div className="flex items-center gap-0.5" onClick={(e) => e.stopPropagation()}>
                <Puzzle className="w-3 h-3" />
                <span>{extensionNames.length}</span>
              </div>
            </TooltipTrigger>
            <TooltipContent side="top" className="max-w-xs">
              <div className="text-xs">
                <div className="font-medium mb-1">Extensions:</div>
                <ul className="list-disc list-inside">
                  {extensionNames.map((name) => <li key={name}>{name}</li>)}
                </ul>
              </div>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      )}
    </div>
    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
      <button onClick={handleOpenInNewWindowClick} className="p-1.5 rounded hover:bg-background-medium transition-colors" title="Open in new window">
        <ExternalLink className="w-3 h-3 text-text-muted" />
      </button>
      <button onClick={handleEditClick} className="p-1.5 rounded hover:bg-background-medium transition-colors" title="Edit session name">
        <Edit2 className="w-3 h-3 text-text-muted" />
      </button>
      <button onClick={handleDeleteClick} className="p-1.5 rounded hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors" title="Delete session">
        <Trash2 className="w-3 h-3 text-red-500" />
      </button>
      <button onClick={handleExportClick} className="p-1.5 rounded hover:bg-background-medium transition-colors" title="Export session">
        <Download className="w-3 h-3 text-text-muted" />
      </button>
    </div>
  </div>
</div>
```

- [ ] Replace the `<SessionSkeleton>` Card with a matching div skeleton:

```tsx
<div className="flex items-center justify-between gap-3 py-3 px-4 rounded-xl border border-border-subtle bg-background-default">
  <div className="flex-1 min-w-0">
    <Skeleton className={`h-4 ${titleWidths[variant % titleWidths.length]} mb-1.5`} />
    <div className="flex items-center gap-3">
      <Skeleton className="h-3 w-20" />
      <Skeleton className={`h-3 ${pathWidths[variant % pathWidths.length]}`} />
    </div>
  </div>
  <div className="flex items-center gap-2">
    <Skeleton className="h-3 w-12" />
    <Skeleton className={`h-3 ${tokenWidths[variant % tokenWidths.length]}`} />
  </div>
</div>
```

- [ ] Remove the `import { Card } from '../ui/card'` if `Card` is no longer used elsewhere in the file (check first with grep).

- [ ] Change the `session-grid` layout in `main.css` to a `flex flex-col` gap — the grid was designed for cards, not rows:

In `main.css`, update `.session-grid`:
```css
.session-grid {
  display: flex;
  flex-direction: column;
  gap: 0.375rem; /* 6px between rows */
  contain: layout style paint;
}
```

- [ ] Commit:
```bash
git add ui/desktop/src/components/sessions/SessionListView.tsx \
        ui/desktop/src/styles/main.css
git commit -m "design: history — card grid → compact border rows"
```

---

## Phase 4 — Extensions page header

**Goal:** Remove the bulky floating header card; use a flat header section.

### Task 4.1 — Flatten ExtensionsView header

- [ ] In `ExtensionsView.tsx`, replace the wrapping `<div … rounded-2xl … style={{ boxShadow }}>` header card (starting around line 100) with a plain padded header:

```tsx
{/* Flat header — no card wrapper */}
<div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
  <div className="flex flex-col page-transition">
    <div className="flex justify-between items-center mb-1">
      <h1 className="text-2xl font-semibold tracking-tight">Extensions</h1>
    </div>
    <p className="text-sm text-text-muted mb-2">
      These extensions use the Model Context Protocol (MCP). They can expand BioRouter's
      capabilities using three main components: Prompts, Resources, and Tools.{' '}
      {getSearchShortcutText()} to search.
    </p>
    <p className="text-sm text-text-muted mb-4">
      Extensions enabled here are used as the default for new chats. You can also toggle
      active extensions during chat.
    </p>
    <div className="flex gap-3">
      <Button className="flex items-center gap-2" variant="default" onClick={() => setIsAddModalOpen(true)}>
        <Plus className="h-4 w-4" />
        Add custom extension
      </Button>
      <Button
        className="flex items-center gap-2"
        variant="outline"
        onClick={() => window.open('https://baranzinilab.github.io/biorouter-landing/baam.html', '_blank')}
      >
        <GPSIcon size={12} />
        Browse extensions
      </Button>
    </div>
  </div>
</div>
```

- [ ] Commit:
```bash
git add ui/desktop/src/components/extensions/ExtensionsView.tsx
git commit -m "design: extensions — flat header, remove floating hero card"
```

---

## Phase 5 — Sidebar compactness

**Goal:** Tighter nav items; more restrained active state.

### Task 5.1 — AppSidebar spacing and active state

The sidebar nav items currently have generous padding inherited from shadcn/ui's `SidebarMenuButton`. Make them slightly more compact.

- [ ] In `AppSidebar.tsx`, where `SidebarMenuButton` is used, confirm `size` prop is set:

```tsx
<SidebarMenuButton
  size="sm"        // ← was "default" or not set
  isActive={isItemActive(item.path, currentPath || '')}
  onClick={...}
  tooltip={item.tooltip}
>
```

The `sm` size variant from shadcn sidebar is h-8 vs the default h-9 — saves 4px per item.

- [ ] Commit:
```bash
git add ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx
git commit -m "design: sidebar — compact nav item height"
```

---

## Phase 6 — SessionListView page header (match Extensions pattern)

### Task 6.1 — Flatten SessionListView header

- [ ] In `SessionListView.tsx`, around line 775, replace the header card with the same flat pattern:

```tsx
{/* Flat page header */}
<div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
  <div className="flex justify-between items-center mb-1">
    <h1 className="text-2xl font-semibold tracking-tight">History</h1>
    <div className="flex gap-2">
      {/* import / export buttons unchanged */}
    </div>
  </div>
  <p className="text-sm text-text-muted">
    Browse and resume previous sessions. {getSearchShortcutText()} to search.
  </p>
</div>
```

- [ ] Commit:
```bash
git add ui/desktop/src/components/sessions/SessionListView.tsx
git commit -m "design: history — flat page header, match extensions pattern"
```

---

## Phase 7 — Design skill file

### Task 7.1 — Create `.claude/commands/frontend-design.md`

- [ ] Create `.claude/commands/frontend-design.md` documenting the BioRouter design language so future agents/sessions can match it:

```markdown
# BioRouter Frontend Design Language

Use this skill when making any frontend/UI changes to BioRouter.

## Design Philosophy

"A calm clinical workspace: warm, precise, sparse, trustworthy."

- Less card-heavy, more surface-based
- Thin borders instead of heavy shadows
- High information density without clutter
- Warm cream identity with professional restraint

## Color Tokens (defined in main.css)

| Token | Light value | Use |
|---|---|---|
| `bg-background-muted` | `neutral-50 #faf8f3` | Page/canvas background |
| `bg-background-default` | `#ffffff` | Card/surface background |
| `bg-background-medium` | `neutral-100 #f4f0e6` | Hover state, secondary surfaces |
| `border-border-subtle` | `neutral-200 #e8e1d2` | Card borders (replaces shadows for most elements) |
| `border-border-default` | `neutral-100` | Dividers |
| `text-text-default` | `#2a2520` | Primary text |
| `text-text-muted` | `#7a736c` | Secondary/metadata text |
| Accent orange | `#cf6d47` | Brand accent line; active indicators |

## Shadow Usage

| Token | When to use |
|---|---|
| `--shadow-default` | Floating composer, HMR overlays only — very light 1px shadow |
| `--shadow-popover` | Menus, dropdowns, modals |
| **No shadow** | Cards, rows, page sections — use `border border-border-subtle` instead |

## Page Header Pattern

All page views use a flat header (no card wrapper):

```tsx
<div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
  <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
  <p className="text-sm text-text-muted mt-1">{description}</p>
</div>
```

## Card / Row Pattern

Prefer rows over cards for list data:

```tsx
// Row (preferred for lists)
<div className="flex items-center gap-3 py-3 px-4 rounded-xl border border-border-subtle bg-background-default hover:bg-background-muted transition-colors">

// Card (for metric blocks or standalone panels)
<div className="p-5 rounded-xl border border-border-subtle bg-background-default">
```

## Typography

- Page title: `text-2xl font-semibold tracking-tight`
- Section label: `text-[11px] font-medium uppercase tracking-wider text-text-muted`
- Body: `text-sm text-text-default`
- Metadata: `text-xs font-mono text-text-muted`

## Spacing

- Page horizontal padding: `px-8`
- Section vertical padding: `py-6`
- Row height: `py-3` (12px top + bottom = 48px total with 24px content)
- Card padding: `p-5` or `p-6`
- Gap between rows: `gap-1.5` (6px)
- Gap between cards: `gap-3` (12px)

## Radius

- Rows: `rounded-xl` (12px)
- Cards: `rounded-xl` or `rounded-2xl`
- Composer: `rounded-xl`
- Tags/chips: `rounded-md` (6-8px)
```

- [ ] Commit:
```bash
git add .claude/commands/frontend-design.md
git commit -m "docs: add frontend-design skill documenting BioRouter design language"
```

---

## Verification

After all phases, use the `/debug-ui` skill to launch the dev app and take screenshots of:
1. Home page (Hub)
2. History page (SessionListView)
3. Extensions page
4. Settings page

Confirm:
- No heavy drop shadows on cards
- All page headers are flat (no floating card)
- Session list shows rows, not a card grid
- Warm cream palette is preserved but calmer
- Sidebar nav items are compact
