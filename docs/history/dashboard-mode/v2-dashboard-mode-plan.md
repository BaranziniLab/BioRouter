# Dashboard Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename Lab Meeting Mode → Dashboard Mode end-to-end; comfort-size spawning; deterministic soft-tile + relaxation layout engine; per-window full ChatInput pickers; coherent `/pair`; `Session N` default naming with user-set precedence; window-size restore on exit; toolbar polish.

**Architecture:** Layout engine is a pure pipeline of partition → sizing → slot → repulse → relax → snap, with zero RNG (all "perturbation" comes from stable windowId hashes). Per-window ChatInput un-hides the existing pickers. localStorage one-shot migrates the old key. `BaseChat` flips `coherent` default to `true` so /pair matches dashboard chats.

**Tech Stack:** React 19, TypeScript, Vite, Electron Forge, Tailwind, lucide-react. Test runners: Vitest (unit/component) + Playwright (E2E via CDP at port 9222).

---

## File Structure

### Renamed (directory `LabMeeting/` → `Dashboard/`)

| Old path | New path | Notes |
|---|---|---|
| `ui/desktop/src/components/LabMeeting/LabMeetingProvider.tsx` | `…/Dashboard/DashboardProvider.tsx` | rename class + identifiers |
| `…/LabMeeting/LabMeetingProvider.test.tsx` | `…/Dashboard/DashboardProvider.test.tsx` | rename test imports |
| `…/LabMeeting/LabMeetingBoard.tsx` | `…/Dashboard/DashboardBoard.tsx` | |
| `…/LabMeeting/LabMeetingToolbar.tsx` | `…/Dashboard/DashboardToolbar.tsx` | |
| `…/LabMeeting/LabMeetingRoute.tsx` | `…/Dashboard/DashboardRoute.tsx` | |
| `…/LabMeeting/ChatWindow.tsx` | `…/Dashboard/ChatWindow.tsx` | content unchanged here (changed in later tasks) |
| `…/LabMeeting/WindowTitleBar.tsx` | `…/Dashboard/WindowTitleBar.tsx` | |
| `…/LabMeeting/ResizeHandle.tsx` | `…/Dashboard/ResizeHandle.tsx` | |
| `…/LabMeeting/TuckSidebar.tsx` | `…/Dashboard/TuckSidebar.tsx` | |
| `…/LabMeeting/TuckedCard.tsx` | `…/Dashboard/TuckedCard.tsx` | |
| `…/LabMeeting/BackToLabMeetingPill.tsx` | `…/Dashboard/BackToDashboardPill.tsx` | |
| `…/LabMeeting/layoutEngine.ts` | `…/Dashboard/layoutEngine.ts` | rewritten in Task 7 |
| `…/LabMeeting/layoutEngine.test.ts` | `…/Dashboard/layoutEngine.test.ts` | extended in Task 7 |
| `…/LabMeeting/labMeetingStorage.ts` | `…/Dashboard/dashboardStorage.ts` | also renames `STORAGE_KEY` |
| `…/LabMeeting/labMeetingStorage.test.ts` | `…/Dashboard/dashboardStorage.test.ts` | extended w/ migration test |
| `…/LabMeeting/palette.ts` | `…/Dashboard/palette.ts` | `NAME_POOL` replaced by `Session N` generator |
| `…/LabMeeting/palette.test.ts` | `…/Dashboard/palette.test.ts` | |
| `…/LabMeeting/useLabMeetingDrag.ts` | `…/Dashboard/useDashboardDrag.ts` | content unchanged |
| `…/LabMeeting/index.ts` | `…/Dashboard/index.ts` | re-export new names |
| `ui/desktop/src/contexts/LabMeetingContext.tsx` | `…/contexts/DashboardContext.tsx` | rename type/hook |

### Removed
- `LabMeetingStatusBar.tsx` — deleted entirely (per spec §7c).

### Created
- `ui/desktop/src/components/Dashboard/SessionNamePill.tsx` — editable name pill rendered inside `BaseChat` content (Task 11).

### Modified (existing files)
- `ui/desktop/src/App.tsx` — route path & provider import.
- `ui/desktop/src/components/Layout/AppLayout.tsx` — icon swap, button label, pill import.
- `ui/desktop/src/components/icons/app-icons.tsx` — add `LayoutDashboard`.
- `ui/desktop/src/components/BaseChat.tsx` — `coherent` default true; mount `SessionNamePill`; drop `hideStatusBar` plumb.
- `ui/desktop/src/components/ChatInput.tsx` — remove `hideStatusBar` prop.
- `ui/desktop/src/main.ts` — IPC channel rename + `dashboard:exit`.
- `ui/desktop/src/preload.ts` — bridge method rename + new method.

---

## Conventions

- Paths absolute from repo root `/Users/wgu/Desktop/biorouter`.
- Frontend tests: `cd ui/desktop && npm run test:run -- <path>`.
- Type-check: `cd ui/desktop && npx tsc --noEmit`.
- Lint: `cd ui/desktop && npm run lint:check`.
- Commits use the existing `feat(dashboard):` / `fix(dashboard):` / `refactor(dashboard):` styling.
- Frequent commits — one commit per task, sometimes per sub-step.

---

# Task 1: Rename — directory, files, identifiers

**Files:** all under `ui/desktop/src/components/LabMeeting/` (folder rename); `ui/desktop/src/contexts/LabMeetingContext.tsx`.

### Context

Mechanical refactor. Move the folder, rename every identifier, update import paths. No behavior change. We split this task into git-mv + sed sweeps so the diff is reviewable.

- [ ] **Step 1: Move the folder via `git mv` to preserve history**

```bash
cd /Users/wgu/Desktop/biorouter
git mv ui/desktop/src/components/LabMeeting ui/desktop/src/components/Dashboard
git mv ui/desktop/src/contexts/LabMeetingContext.tsx ui/desktop/src/contexts/DashboardContext.tsx
```

- [ ] **Step 2: Rename files inside the folder**

```bash
cd ui/desktop/src/components/Dashboard
git mv LabMeetingProvider.tsx DashboardProvider.tsx
git mv LabMeetingProvider.test.tsx DashboardProvider.test.tsx
git mv LabMeetingBoard.tsx DashboardBoard.tsx
git mv LabMeetingToolbar.tsx DashboardToolbar.tsx
git mv LabMeetingRoute.tsx DashboardRoute.tsx
git mv BackToLabMeetingPill.tsx BackToDashboardPill.tsx
git mv labMeetingStorage.ts dashboardStorage.ts
git mv labMeetingStorage.test.ts dashboardStorage.test.ts
git mv useLabMeetingDrag.ts useDashboardDrag.ts
git rm LabMeetingStatusBar.tsx
```

(LabMeetingStatusBar deletion is per spec §7c — replaced by per-window pickers.)

- [ ] **Step 3: Replace identifiers across the codebase**

Use a single combined sed sweep over all `.ts`, `.tsx` files. Note ordering: replace the longer/more specific tokens first so we don't get partial matches.

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop
# Update content (note: sed -i '' on macOS)
files=$(grep -rIl --include='*.ts' --include='*.tsx' -E "Lab.?Meeting|labMeeting|lab-meeting" src/ 2>/dev/null)
for f in $files; do
  sed -i '' \
    -e 's/BackToLabMeetingPill/BackToDashboardPill/g' \
    -e 's/LabMeetingProvider/DashboardProvider/g' \
    -e 's/LabMeetingBoard/DashboardBoard/g' \
    -e 's/LabMeetingToolbar/DashboardToolbar/g' \
    -e 's/LabMeetingStatusBar/DashboardStatusBar/g' \
    -e 's/LabMeetingRoute/DashboardRoute/g' \
    -e 's/LabMeetingContext/DashboardContext/g' \
    -e 's/LabMeetingState/DashboardState/g' \
    -e 's/LabMeetingApi/DashboardApi/g' \
    -e 's/LabWindow\b/DashboardWindow/g' \
    -e 's/useOptionalLabMeeting/useOptionalDashboard/g' \
    -e 's/useLabMeetingDrag/useDashboardDrag/g' \
    -e 's/useLabMeeting\b/useDashboard/g' \
    -e 's|components/LabMeeting/|components/Dashboard/|g' \
    -e 's|LabMeetingContext|DashboardContext|g' \
    -e 's|contexts/LabMeetingContext|contexts/DashboardContext|g' \
    -e 's/labMeetingEnter/dashboardEnter/g' \
    -e 's/labMeetingExit/dashboardExit/g' \
    -e 's/lab-meeting:enter/dashboard:enter/g' \
    -e 's/lab-meeting:exit/dashboard:exit/g' \
    -e "s|'/lab-meeting'|'/dashboard'|g" \
    -e 's|"/lab-meeting"|"/dashboard"|g' \
    -e 's|#/lab-meeting|#/dashboard|g' \
    -e 's/labMeetingStorage/dashboardStorage/g' \
    -e 's/biorouter\.labmeeting\.v1/biorouter.dashboard.v1/g' \
    -e 's/Lab Meeting Mode/Dashboard/g' \
    -e 's/Lab Meeting/Dashboard/g' \
    -e 's/Back to Lab Meeting/Back to Dashboard/g' \
    "$f"
done
```

- [ ] **Step 4: Verify grep is clean**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop
grep -rin "lab.\?meeting\|LabMeeting\|labMeeting\|labmeeting" src/ \
  | grep -v "src/test/setup.ts" | head
```

Expected: empty output (zero hits). The single allowed exception is the in-test storage migration helper which keys the OLD value (see Task 4) — but at this point the test doesn't exist yet, so the output should be totally clean.

- [ ] **Step 5: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -20
```
Expected: no errors mentioning Lab/lab.

- [ ] **Step 6: Run renamed unit tests**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/
```
Expected: all four test files pass with the same counts as before (palette 4, layoutEngine 12, dashboardStorage 5, DashboardProvider 9).

- [ ] **Step 7: Commit**

```bash
cd /Users/wgu/Desktop/biorouter
git add ui/desktop/src
git commit -m "refactor(dashboard): rename LabMeeting → Dashboard end-to-end"
```

---

# Task 2: Icon — `Users` → `LayoutDashboard`

**Files:**
- Modify: `ui/desktop/src/components/icons/app-icons.tsx`
- Modify: `ui/desktop/src/components/Layout/AppLayout.tsx`

### Context

`LayoutDashboard` is a lucide-react icon representing a 4-panel dashboard. It's not yet exported from `app-icons.tsx`. We import it, wrap it in the project's `light()` helper, and export. Then swap the import in AppLayout. The unused `Users` import in AppLayout is left alone if anything else references it; otherwise removed.

- [ ] **Step 1: Add to imports block in app-icons.tsx**

Find the import block at the top of `ui/desktop/src/components/icons/app-icons.tsx` and add `LayoutDashboard as _LayoutDashboard,` in alphabetical position (between `Info` and `LucideIcon` types, or wherever L's land). Then add the named export `export const LayoutDashboard = light(_LayoutDashboard);` alongside the other exports.

Concrete edit — add this line to the lucide import block:

```ts
  LayoutDashboard as _LayoutDashboard,
```

And add this line to the exports block:

```ts
export const LayoutDashboard = light(_LayoutDashboard);
```

- [ ] **Step 2: Replace the icon in AppLayout.tsx**

In `ui/desktop/src/components/Layout/AppLayout.tsx`, find:

```ts
import { Plus, Users } from '../icons/app-icons';
```

Replace with:

```ts
import { Plus, LayoutDashboard } from '../icons/app-icons';
```

Then find the button:

```tsx
<Button
  onClick={() => navigate('/dashboard')}
  className="no-drag hover:!bg-background-medium"
  variant="ghost"
  size="xs"
  title="Open Lab Meeting Mode"
>
  <Users className="w-4 h-4" />
</Button>
```

Replace with:

```tsx
<Button
  onClick={() => navigate('/dashboard')}
  className="no-drag hover:!bg-background-medium"
  variant="ghost"
  size="xs"
  title="Open Dashboard"
>
  <LayoutDashboard className="w-4 h-4" />
</Button>
```

(The `'/dashboard'` route path was already updated in Task 1; "Open Lab Meeting Mode" → "Open Dashboard" is a textual cleanup that Task 1's sed left as "Open Dashboard" already, so this matches.)

Also in the same file, `BackToDashboardPill` already uses `Users` — replace its import & usage with `LayoutDashboard` as well:

```tsx
// in BackToDashboardPill.tsx
import { LayoutDashboard } from '../icons/app-icons';
// ...
<LayoutDashboard className="w-3.5 h-3.5" />
```

- [ ] **Step 3: Sanity build**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/icons/app-icons.tsx \
        ui/desktop/src/components/Layout/AppLayout.tsx \
        ui/desktop/src/components/Dashboard/BackToDashboardPill.tsx
git commit -m "feat(dashboard): swap Users icon for LayoutDashboard"
```

---

# Task 3: localStorage migration v1 → dashboard

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.ts`
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.test.ts`

### Context

Existing key was `biorouter.labmeeting.v1` (now in Task 1 string-replaced to `biorouter.dashboard.v1`). For users upgrading from v1, the OLD key may still hold their state. Add a one-shot migration: at load time, if the new key is absent and the OLD key (`biorouter.labmeeting.v1`) is present, copy it over and delete the old key.

The OLD key string must be re-added explicitly because Task 1's sed wiped it; we want to preserve the migration window. We hard-code the historical key string inside the migration helper only.

- [ ] **Step 1: Write the failing migration test**

In `ui/desktop/src/components/Dashboard/dashboardStorage.test.ts`, ADD this test block AT THE END of the file (before the closing `});` of the outermost `describe`):

```ts
  it('migrates v1 key (biorouter.labmeeting.v1) to new key on load', () => {
    const v1State = {
      version: 1,
      windows: [],
      focusedWindowId: null,
      T1: 6,
      T2: 8,
    };
    localStorage.setItem('biorouter.labmeeting.v1', JSON.stringify(v1State));
    const loaded = loadDashboardState();
    expect(loaded).toEqual(v1State);
    expect(localStorage.getItem('biorouter.dashboard.v1')).not.toBeNull();
    expect(localStorage.getItem('biorouter.labmeeting.v1')).toBeNull();
  });

  it('does NOT overwrite existing new key if old key also present', () => {
    const oldState = { version: 1, windows: [], focusedWindowId: null, T1: 4, T2: 5 };
    const newState = { version: 1, windows: [], focusedWindowId: null, T1: 7, T2: 9 };
    localStorage.setItem('biorouter.labmeeting.v1', JSON.stringify(oldState));
    localStorage.setItem('biorouter.dashboard.v1', JSON.stringify(newState));
    const loaded = loadDashboardState();
    expect(loaded?.T1).toBe(7);
    expect(loaded?.T2).toBe(9);
  });
```

Rename any test references inside the file from `loadLabMeetingState`/`saveLabMeetingState`/etc. to `loadDashboardState`/`saveDashboardState` — Task 1's sed should have already done that, but verify.

- [ ] **Step 2: Run tests, expect new tests to FAIL, others PASS**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/dashboardStorage.test.ts 2>&1 | tail -15
```
Expected: 2 new tests FAIL (load returns null when only the legacy key is present).

- [ ] **Step 3: Implement the migration**

In `ui/desktop/src/components/Dashboard/dashboardStorage.ts`, modify the `loadDashboardState` function. Locate it (the current body just reads the storage key and returns parsed JSON or null) and replace it with:

```ts
const LEGACY_STORAGE_KEY = 'biorouter.labmeeting.v1';

export function loadDashboardState(): SerializedDashboardState | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as SerializedDashboardState;
      if (parsed.version === STORAGE_VERSION) return parsed;
      return null;
    }
    // Migration: try the legacy v1 key
    const legacy = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!legacy) return null;
    const parsedLegacy = JSON.parse(legacy) as SerializedDashboardState;
    if (parsedLegacy.version !== STORAGE_VERSION) return null;
    // Promote to the new key and delete the old one
    saveDashboardState(parsedLegacy);
    localStorage.removeItem(LEGACY_STORAGE_KEY);
    return parsedLegacy;
  } catch {
    return null;
  }
}
```

(Hoist `LEGACY_STORAGE_KEY` to the top-level of the module, above the `loadDashboardState` definition.)

- [ ] **Step 4: Run tests, expect all green**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/dashboardStorage.test.ts 2>&1 | tail -10
```
Expected: 7 tests pass (5 existing + 2 new).

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/Dashboard/dashboardStorage.ts \
        ui/desktop/src/components/Dashboard/dashboardStorage.test.ts
git commit -m "feat(dashboard): one-shot migration of legacy labmeeting localStorage"
```

---

# Task 4: Window-size restore on exit — `dashboard:exit` IPC

**Files:**
- Modify: `ui/desktop/src/main.ts`
- Modify: `ui/desktop/src/preload.ts`
- Modify: `ui/desktop/src/components/Dashboard/DashboardRoute.tsx`

### Context

Currently `dashboard:enter` maximizes the BrowserWindow on entry. Need the mirror image: `dashboard:exit` calls `unmaximize()`, which Electron restores to the pre-maximize bounds (`windowStateKeeper` remembers them). DashboardRoute fires `enter` on mount and `exit` on unmount.

- [ ] **Step 1: Add `dashboard:exit` IPC handler in main.ts**

Locate the existing `dashboard:enter` handler (Task 1 renamed it from `lab-meeting:enter`). Right beneath it, add:

```ts
ipcMain.handle('dashboard:exit', (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (win && win.isMaximized()) win.unmaximize();
});
```

- [ ] **Step 2: Expose in preload.ts**

In `ui/desktop/src/preload.ts`, find `labMeetingEnter`/now `dashboardEnter` in the `ElectronAPI` type and in the `electronAPI` const. Add `dashboardExit` next to `dashboardEnter`.

In the type:

```ts
  dashboardEnter: () => Promise<void>;
  dashboardExit: () => Promise<void>;
```

In the const:

```ts
  dashboardEnter: () => ipcRenderer.invoke('dashboard:enter'),
  dashboardExit: () => ipcRenderer.invoke('dashboard:exit'),
```

- [ ] **Step 3: Call exit on route unmount**

In `ui/desktop/src/components/Dashboard/DashboardRoute.tsx`, find the existing `useEffect` that calls `electron?.dashboardEnter?.()`. Replace with:

```tsx
  useEffect(() => {
    const electron = (
      window as unknown as {
        electron?: { dashboardEnter?: () => Promise<void> | void; dashboardExit?: () => Promise<void> | void };
      }
    ).electron;
    electron?.dashboardEnter?.();
    return () => {
      electron?.dashboardExit?.();
    };
  }, []);
```

- [ ] **Step 4: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/main.ts ui/desktop/src/preload.ts \
        ui/desktop/src/components/Dashboard/DashboardRoute.tsx
git commit -m "feat(dashboard): restore BrowserWindow size on route exit"
```

---

# Task 5: Toolbar polish — Spawn label + standard button hover

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/DashboardToolbar.tsx`

### Context

Remove the `+` icon glyph from Spawn so it matches Organize and Clear (text-only). Align hover treatment with the rest of the app — sidebar nav items use `hover:bg-background-medium transition-colors duration-150` on their `Button` (variant ghost). Use the same class on all three toolbar buttons. The numeric T1/T2 inputs already use a consistent style.

- [ ] **Step 1: Inspect a known-good app button for hover class**

Open `ui/desktop/src/components/BioRouterSidebar/AppSidebar.tsx` and find the `SidebarMenuButton` render around line 188. Note the exact `className` substring: `hover:bg-background-medium transition-colors duration-150`.

- [ ] **Step 2: Update DashboardToolbar buttons**

Replace the three button definitions in `DashboardToolbar.tsx`:

OLD (Spawn):
```tsx
<Button size="xs" variant="ghost" onClick={() => dashboard.spawnWindow()} title="Spawn (⌘⇧N)">
  <Plus className="w-4 h-4" /> <span className="ml-1 text-xs">Spawn</span>
</Button>
```

NEW (Spawn):
```tsx
<Button
  size="xs"
  variant="ghost"
  onClick={() => dashboard.spawnWindow()}
  title="Spawn (⌘⇧N)"
  className="hover:bg-background-medium transition-colors duration-150"
>
  <span className="text-xs">Spawn</span>
</Button>
```

OLD (Organize):
```tsx
<Button size="xs" variant="ghost" onClick={() => dashboard.organize()} title="Re-tile">
  <span className="text-xs">Organize</span>
</Button>
```

NEW (Organize):
```tsx
<Button
  size="xs"
  variant="ghost"
  onClick={() => dashboard.organize()}
  title="Re-tile"
  className="hover:bg-background-medium transition-colors duration-150"
>
  <span className="text-xs">Organize</span>
</Button>
```

OLD (Clear):
```tsx
<Button size="xs" variant="ghost" onClick={() => dashboard.clearAll()} title="Close all">
  <span className="text-xs">Clear</span>
</Button>
```

NEW (Clear):
```tsx
<Button
  size="xs"
  variant="ghost"
  onClick={() => dashboard.clearAll()}
  title="Close all"
  className="hover:bg-background-medium transition-colors duration-150"
>
  <span className="text-xs">Clear</span>
</Button>
```

Also: the `Plus` import in the file is now unused — remove it:

OLD: `import { Plus } from '../icons/app-icons';`
NEW: (delete this line)

- [ ] **Step 3: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 4: Commit**

```bash
git add ui/desktop/src/components/Dashboard/DashboardToolbar.tsx
git commit -m "feat(dashboard): toolbar buttons match app hover style; drop Plus glyph"
```

---

# Task 6: Default name generator — `Session N`

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/palette.ts`
- Modify: `ui/desktop/src/components/Dashboard/palette.test.ts`

### Context

Default names become `Session 1`, `Session 2`, …. The badge number falls out of the name itself; we drop the `#N` badge text in Task 9.

- [ ] **Step 1: Update the failing tests**

Open `ui/desktop/src/components/Dashboard/palette.test.ts`. Replace the two `generateName` tests with:

```ts
  it('generateName returns "Session N" for any non-negative index', () => {
    expect(generateName(0)).toBe('Session 1');
    expect(generateName(1)).toBe('Session 2');
    expect(generateName(7)).toBe('Session 8');
    expect(generateName(99)).toBe('Session 100');
  });
```

And remove the old test referencing `NAME_POOL`. Also remove the `NAME_POOL` import from the top of the file; only keep `ACCENT_PALETTE, pickAccentColor, generateName`.

- [ ] **Step 2: Run tests — should fail**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/palette.test.ts 2>&1 | tail -10
```
Expected: FAIL — current `generateName(0)` returns `'Atlas'`, not `'Session 1'`.

- [ ] **Step 3: Replace `generateName` and remove `NAME_POOL`**

In `ui/desktop/src/components/Dashboard/palette.ts`, REPLACE the entire `NAME_POOL` array and `generateName` function with:

```ts
export function generateName(index: number): string {
  return `Session ${index + 1}`;
}
```

Delete the `NAME_POOL` constant entirely (and its `export`). If any other file imports `NAME_POOL`, that import is now stale — Task 1's sed didn't touch identifier `NAME_POOL`, but tests in step 1 removed the import. Verify with:

```bash
grep -rn "NAME_POOL" /Users/wgu/Desktop/biorouter/ui/desktop/src/
```
Expected: empty.

- [ ] **Step 4: Run tests — should pass**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/palette.test.ts 2>&1 | tail -10
```
Expected: 3 tests pass.

- [ ] **Step 5: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 6: Commit**

```bash
git add ui/desktop/src/components/Dashboard/palette.ts \
        ui/desktop/src/components/Dashboard/palette.test.ts
git commit -m "feat(dashboard): default name 'Session N' replaces Atlas/Nova pool"
```

---

# Task 7: New deterministic layout engine

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/layoutEngine.ts`
- Modify: `ui/desktop/src/components/Dashboard/layoutEngine.test.ts`

### Context

Re-implementing the pure layout function with:
1. Comfort-size sizing (capped at `COMFORT_W × COMFORT_H`, scaled down if board is crowded).
2. Initial slot placement using existing grid math but with capped cell size.
3. Pinned (`isManuallyPlaced`) windows committed verbatim.
4. Auto windows repulsed from pinned overlaps via cardinal-exit displacement.
5. Six deterministic relaxation passes to push apart auto-vs-auto overlaps.
6. 4-pixel snap; z-index by category.

Zero RNG. All tie-breaks via stable `hash32(windowId)`.

This task is large but self-contained. Implement TDD: write all new tests first, fail, implement, pass, commit. We extend the existing test file.

- [ ] **Step 1: Add a stable hash function with a test**

PREPEND this test block to `ui/desktop/src/components/Dashboard/layoutEngine.test.ts` (just inside the file, after existing imports):

```ts
import { hash32 } from './layoutEngine';

describe('hash32', () => {
  it('is deterministic across calls', () => {
    expect(hash32('hello')).toBe(hash32('hello'));
  });
  it('differs across distinct inputs (with high probability)', () => {
    expect(hash32('a')).not.toBe(hash32('b'));
  });
});
```

- [ ] **Step 2: Add `hash32` export to layoutEngine.ts**

At the top of `ui/desktop/src/components/Dashboard/layoutEngine.ts`, after the existing constants and before `bestGridConfig`, add:

```ts
// FNV-1a 32-bit hash — deterministic, no RNG.
export function hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = (h * 0x01000193) >>> 0;
  }
  return h >>> 0;
}
```

- [ ] **Step 3: Run hash test — should pass**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/layoutEngine.test.ts 2>&1 | tail -10
```
Expected: PASS (new hash32 tests + 12 existing tests).

- [ ] **Step 4: Add comfort-sizing & soft-tile constants to layoutEngine.ts**

In `ui/desktop/src/components/Dashboard/layoutEngine.ts`, near the top with the existing `Z_TILED`/`Z_OVERFLOW`/etc., add these constants AT MODULE TOP:

```ts
const COMFORT_W = 940;
const COMFORT_H = 800;
const MIN_W = 320;
const MIN_H = 240;
const EDGE_INSET = 6;
const FILL_FACTOR_MAX = 0.7;
const RELAX_PASSES = 6;
const RELAX_STEP_MAX = 20;
const SNAP_GRID = 4;
const Z_PINNED = 5;
```

(Existing `Z_TILED`, `Z_OVERFLOW`, `Z_FOCUSED`, `TARGET_ASPECT` stay.)

- [ ] **Step 5: Add new test cases (will fail until pipeline rewrite is in place)**

In `ui/desktop/src/components/Dashboard/layoutEngine.test.ts`, ADD this `describe` block at the end:

```ts
describe('computeLayout — soft-tile pipeline (deterministic, comfort-capped)', () => {
  const wideBoard = { width: 2112, height: 973 };
  const hugeBoard = { width: 4000, height: 2400 };

  it('n=1 → one window at comfort size, centered', () => {
    const out = computeLayout([mkWindow('a')], wideBoard, 6, 8, 'a');
    const r = out.get('a')!;
    expect(r.w).toBe(940);
    expect(r.h).toBe(800);
    // Centered (within snap tolerance)
    expect(Math.abs(r.x + r.w / 2 - wideBoard.width / 2)).toBeLessThanOrEqual(SNAP_GRID);
    expect(Math.abs(r.y + r.h / 2 - wideBoard.height / 2)).toBeLessThanOrEqual(SNAP_GRID);
  });

  it('n=2 → two comfort-size windows side by side', () => {
    const out = computeLayout([mkWindow('a'), mkWindow('b')], wideBoard, 6, 8, null);
    const a = out.get('a')!;
    const b = out.get('b')!;
    expect(a.w).toBe(940);
    expect(a.h).toBe(800);
    expect(b.w).toBe(940);
    expect(b.h).toBe(800);
    expect(a.x).toBeLessThan(b.x);                    // ordering
    expect(a.x + a.w).toBeLessThanOrEqual(b.x);       // no overlap
  });

  it('n=4 on a huge board caps cells at comfort size (not stretched)', () => {
    const ids = ['a','b','c','d'];
    const out = computeLayout(ids.map(i => mkWindow(i)), hugeBoard, 6, 8, null);
    for (const id of ids) {
      const r = out.get(id)!;
      expect(r.w).toBeLessThanOrEqual(COMFORT_W);
      expect(r.h).toBeLessThanOrEqual(COMFORT_H);
    }
  });

  it('determinism: 50 invocations with identical inputs produce equal output', () => {
    const ids = ['a','b','c','d','e','f','g'];
    const inputs = ids.map(i => mkWindow(i));
    const first = computeLayout(inputs, wideBoard, 6, 8, null);
    for (let i = 0; i < 49; i++) {
      const again = computeLayout(inputs, wideBoard, 6, 8, null);
      for (const id of ids) {
        expect(again.get(id)).toEqual(first.get(id));
      }
    }
  });

  it('shuffle stability: same per-id output across input orderings', () => {
    const ids = ['a','b','c','d','e','f'];
    const baseInputs = ids.map(i => mkWindow(i));
    const ref = computeLayout(baseInputs, wideBoard, 6, 8, null);
    // Reverse order
    const rev = computeLayout([...baseInputs].reverse(), wideBoard, 6, 8, null);
    for (const id of ids) {
      expect(rev.get(id)).toEqual(ref.get(id));
    }
  });

  it('pinned avoidance: auto windows do not overlap pinned > 5% area', () => {
    const pinned = mkWindow('p', {
      isManuallyPlaced: true,
      position: { x: 700, y: 250 },
      size: { w: 700, h: 500 },
    });
    const autos = ['a','b','c','d'].map(i => mkWindow(i));
    const out = computeLayout([pinned, ...autos], wideBoard, 6, 8, null);
    const pRect = out.get('p')!;
    expect(pRect.x).toBe(700); // pinned committed verbatim
    expect(pRect.y).toBe(250);
    expect(pRect.w).toBe(700);
    expect(pRect.h).toBe(500);
    for (const id of ['a','b','c','d']) {
      const r = out.get(id)!;
      const oxL = Math.max(r.x, pRect.x);
      const oxR = Math.min(r.x + r.w, pRect.x + pRect.w);
      const oyT = Math.max(r.y, pRect.y);
      const oyB = Math.min(r.y + r.h, pRect.y + pRect.h);
      const overlap = Math.max(0, oxR - oxL) * Math.max(0, oyB - oyT);
      const fraction = overlap / (r.w * r.h);
      expect(fraction).toBeLessThanOrEqual(0.05);
    }
  });

  it('edge guarantee: every rect inside [EDGE_INSET, board - EDGE_INSET]', () => {
    const ids = ['a','b','c','d','e','f','g','h'];
    const out = computeLayout(ids.map(i => mkWindow(i)), wideBoard, 6, 8, null);
    for (const id of ids) {
      const r = out.get(id)!;
      expect(r.x).toBeGreaterThanOrEqual(EDGE_INSET - 1);
      expect(r.y).toBeGreaterThanOrEqual(EDGE_INSET - 1);
      expect(r.x + r.w).toBeLessThanOrEqual(wideBoard.width - EDGE_INSET + 1);
      expect(r.y + r.h).toBeLessThanOrEqual(wideBoard.height - EDGE_INSET + 1);
    }
  });
});
```

Also at the top of the file, export `EDGE_INSET, COMFORT_W, COMFORT_H, SNAP_GRID` from `layoutEngine.ts` (used by tests):

In `layoutEngine.ts`, change the constants from `const X` to `export const X` for the four above.

- [ ] **Step 6: Run tests — most new ones fail**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/layoutEngine.test.ts 2>&1 | tail -25
```
Expected: at least the comfort-size, pinned-avoidance, and edge-guarantee tests FAIL.

- [ ] **Step 7: Rewrite the body of `computeLayout` per the spec pipeline**

In `ui/desktop/src/components/Dashboard/layoutEngine.ts`, **REPLACE** the entire `computeLayout` function body (the current implementation) with the new pipeline below. Keep the signature, exports, helper imports unchanged. The full replacement (one continuous function):

```ts
export function computeLayout(
  windows: readonly LayoutInputWindow[],
  board: BoardSize,
  T1: number,
  T2: number,
  focusedWindowId: string | null
): Map<string, LayoutRect> {
  void T2;
  const out = new Map<string, LayoutRect>();

  // -------- Stage 1: Partition --------
  // Stable sort by windowId so input ordering doesn't affect output.
  const visible = windows
    .filter((w) => !w.isTucked)
    .slice()
    .sort((a, b) => (a.windowId < b.windowId ? -1 : a.windowId > b.windowId ? 1 : 0));

  const pinned = visible.filter((w) => w.isManuallyPlaced && w.position && w.size);
  const auto = visible.filter((w) => !(w.isManuallyPlaced && w.position && w.size));

  // Commit pinned rects to output immediately
  const pinnedRects: Array<{ id: string; x: number; y: number; w: number; h: number }> = [];
  for (const w of pinned) {
    const r = { id: w.windowId, x: w.position!.x, y: w.position!.y, w: w.size!.w, h: w.size!.h };
    pinnedRects.push(r);
    out.set(w.windowId, {
      x: r.x,
      y: r.y,
      w: r.w,
      h: r.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : Z_PINNED,
    });
  }

  if (auto.length === 0) return out;

  // -------- Stage 2: Cell size for auto --------
  const interiorW = Math.max(MIN_W, board.width - 2 * EDGE_INSET);
  const interiorH = Math.max(MIN_H, board.height - 2 * EDGE_INSET);
  const pinnedArea = pinnedRects.reduce((s, r) => s + r.w * r.h, 0);
  const availableArea = Math.max(MIN_W * MIN_H, interiorW * interiorH - pinnedArea);
  const totalComfort = auto.length * COMFORT_W * COMFORT_H;
  const fillFactor = totalComfort / availableArea;
  let cellW = COMFORT_W;
  let cellH = COMFORT_H;
  if (fillFactor > FILL_FACTOR_MAX) {
    const s = Math.sqrt(FILL_FACTOR_MAX / fillFactor);
    cellW = Math.max(MIN_W, Math.floor(COMFORT_W * s));
    cellH = Math.max(MIN_H, Math.floor(COMFORT_H * s));
  }

  // -------- Stage 3: Initial slot placement --------
  // Position state for auto windows, in board-local coordinates
  const autoPos = new Map<string, { x: number; y: number; w: number; h: number; zCat: number }>();

  if (auto.length <= 2) {
    // Comfort row: side-by-side, vertically centered
    const total = auto.length * cellW;
    const gap = (interiorW - total) / (auto.length + 1);
    const y = EDGE_INSET + Math.max(0, (interiorH - cellH) / 2);
    for (let i = 0; i < auto.length; i++) {
      const x = EDGE_INSET + gap + i * (cellW + gap);
      autoPos.set(auto[i].windowId, { x, y, w: cellW, h: cellH, zCat: Z_TILED });
    }
  } else {
    // Tile via bestGridConfig, but with comfort cap & block-centered
    const nTiled = Math.min(auto.length, T1);
    const { cols, rows } = bestGridConfig(nTiled, { width: interiorW, height: interiorH });
    // Each cell takes the smaller of (interiorW/cols, COMFORT_W) etc.
    const sizedW = Math.min(cellW, Math.floor(interiorW / cols));
    const sizedH = Math.min(cellH, Math.floor(interiorH / rows));
    // Block centering — start coordinates of the grid
    const itemsInLastRow = nTiled - cols * (rows - 1);
    const blockW = cols * sizedW;
    const blockH = rows * sizedH;
    const baseX = EDGE_INSET + Math.max(0, (interiorW - blockW) / 2);
    const baseY = EDGE_INSET + Math.max(0, (interiorH - blockH) / 2);

    // Tiled positions
    for (let i = 0; i < nTiled; i++) {
      const row = Math.floor(i / cols);
      const col = i % cols;
      const isLastRow = row === rows - 1;
      let x = baseX + col * sizedW;
      const y = baseY + row * sizedH;
      if (isLastRow && itemsInLastRow < cols) {
        const lastRowOffset = (blockW - itemsInLastRow * sizedW) / 2;
        x = baseX + lastRowOffset + col * sizedW;
      }
      autoPos.set(auto[i].windowId, { x, y, w: sizedW, h: sizedH, zCat: Z_TILED });
    }

    // Overflow at intersection points (existing math, simplified)
    const overflow = auto.slice(nTiled);
    if (overflow.length > 0) {
      const xs: number[] = [];
      const ys: number[] = [];
      for (let i = 0; i <= cols; i++) xs.push(baseX + i * sizedW);
      for (let j = 0; j <= rows; j++) ys.push(baseY + j * sizedH);
      const center = { x: baseX + blockW / 2, y: baseY + blockH / 2 };
      type C = { x: number; y: number; dist: number };
      const cand: C[] = [];
      for (const x of xs) {
        for (const y of ys) {
          cand.push({ x, y, dist: Math.hypot(x - center.x, y - center.y) });
        }
      }
      cand.sort((p, q) => p.dist - q.dist);
      // Dedupe within 40x30
      const deduped: Array<{ x: number; y: number }> = [];
      for (const p of cand) {
        const tooClose = deduped.some((q) => Math.abs(q.x - p.x) < 40 && Math.abs(q.y - p.y) < 30);
        if (!tooClose) deduped.push(p);
      }
      const ofW = Math.min(sizedW, Math.floor(interiorW * 0.5));
      const ofH = Math.min(sizedH, Math.floor(interiorH * 0.5));
      for (let i = 0; i < overflow.length; i++) {
        const base = deduped[i] ?? { x: center.x, y: center.y };
        const jitter = i * 8;
        const cx = base.x + jitter;
        const cy = base.y + jitter;
        const x = Math.max(EDGE_INSET, Math.min(board.width - EDGE_INSET - ofW, cx - ofW / 2));
        const y = Math.max(EDGE_INSET, Math.min(board.height - EDGE_INSET - ofH, cy - ofH / 2));
        autoPos.set(overflow[i].windowId, { x, y, w: ofW, h: ofH, zCat: Z_OVERFLOW + i });
      }
    }
  }

  // -------- Stage 4: Repulse auto-vs-pinned overlap (cardinal exit) --------
  for (const w of auto) {
    const r = autoPos.get(w.windowId);
    if (!r) continue;
    for (const p of pinnedRects) {
      const oxL = Math.max(r.x, p.x);
      const oxR = Math.min(r.x + r.w, p.x + p.w);
      const oyT = Math.max(r.y, p.y);
      const oyB = Math.min(r.y + r.h, p.y + p.h);
      const ow = oxR - oxL;
      const oh = oyB - oyT;
      if (ow <= 0 || oh <= 0) continue;
      // Four cardinal exits
      const exits = [
        { dx: p.x - (r.x + r.w), dy: 0 },                  // west of pinned
        { dx: p.x + p.w - r.x, dy: 0 },                    // east
        { dx: 0, dy: p.y - (r.y + r.h) },                  // north
        { dx: 0, dy: p.y + p.h - r.y },                    // south
      ];
      exits.sort((a, b) => Math.abs(a.dx) + Math.abs(a.dy) - (Math.abs(b.dx) + Math.abs(b.dy)));
      // Tie-break by hash parity if first two are equal magnitude
      let pick = exits[0];
      if (
        exits.length > 1 &&
        Math.abs(exits[0].dx) + Math.abs(exits[0].dy) === Math.abs(exits[1].dx) + Math.abs(exits[1].dy)
      ) {
        pick = (hash32(w.windowId) & 1) === 0 ? exits[0] : exits[1];
      }
      r.x = Math.max(EDGE_INSET, Math.min(board.width - EDGE_INSET - r.w, r.x + pick.dx));
      r.y = Math.max(EDGE_INSET, Math.min(board.height - EDGE_INSET - r.h, r.y + pick.dy));
    }
  }

  // -------- Stage 5: Auto-vs-auto relaxation (6 passes) --------
  for (let pass = 0; pass < RELAX_PASSES; pass++) {
    const delta = new Map<string, { dx: number; dy: number }>();
    for (let i = 0; i < auto.length; i++) {
      for (let j = i + 1; j < auto.length; j++) {
        const a = autoPos.get(auto[i].windowId);
        const b = autoPos.get(auto[j].windowId);
        if (!a || !b) continue;
        const oxL = Math.max(a.x, b.x);
        const oxR = Math.min(a.x + a.w, b.x + b.w);
        const oyT = Math.max(a.y, b.y);
        const oyB = Math.min(a.y + a.h, b.y + b.h);
        const ow = oxR - oxL;
        const oh = oyB - oyT;
        if (ow <= 0 || oh <= 0) continue;
        const overlapArea = ow * oh;
        const cxA = a.x + a.w / 2;
        const cyA = a.y + a.h / 2;
        const cxB = b.x + b.w / 2;
        const cyB = b.y + b.h / 2;
        let vx = cxA - cxB;
        let vy = cyA - cyB;
        let len = Math.hypot(vx, vy);
        if (len === 0) {
          // Deterministic unit direction from id-pair hash
          const h = hash32(auto[i].windowId + '|' + auto[j].windowId);
          const angle = ((h & 0xff) / 256) * Math.PI * 2;
          vx = Math.cos(angle);
          vy = Math.sin(angle);
          len = 1;
        }
        const mag = Math.min(RELAX_STEP_MAX, Math.sqrt(overlapArea) * 0.5);
        const ux = vx / len;
        const uy = vy / len;
        const dA = delta.get(auto[i].windowId) ?? { dx: 0, dy: 0 };
        dA.dx += ux * mag;
        dA.dy += uy * mag;
        delta.set(auto[i].windowId, dA);
        const dB = delta.get(auto[j].windowId) ?? { dx: 0, dy: 0 };
        dB.dx -= ux * mag;
        dB.dy -= uy * mag;
        delta.set(auto[j].windowId, dB);
      }
    }
    for (const w of auto) {
      const r = autoPos.get(w.windowId);
      const d = delta.get(w.windowId);
      if (!r || !d) continue;
      r.x = Math.max(EDGE_INSET, Math.min(board.width - EDGE_INSET - r.w, r.x + d.dx));
      r.y = Math.max(EDGE_INSET, Math.min(board.height - EDGE_INSET - r.h, r.y + d.dy));
    }
  }

  // -------- Stage 6: Snap & emit --------
  for (const w of auto) {
    const r = autoPos.get(w.windowId);
    if (!r) continue;
    const snap = SNAP_GRID;
    const sx = Math.round(r.x / snap) * snap;
    const sy = Math.round(r.y / snap) * snap;
    out.set(w.windowId, {
      x: sx,
      y: sy,
      w: r.w,
      h: r.h,
      zIndex: w.windowId === focusedWindowId ? Z_FOCUSED : r.zCat,
    });
  }

  return out;
}
```

- [ ] **Step 8: Update existing tests that no longer match the new engine**

Many of the v1 tests assumed the entire board was filled (no comfort cap). Update or remove those that no longer apply. Specifically, these tests in the existing block need adjustment:

- `'one window fills the board'` — REPLACE assertion. Was: `expect(r.w).toBe(1200)`. Was previously expected: cell fills 1200×800. Now: `r.w === 940, r.h === 800`, centered. Replace with a centered-comfort assertion. Actually the new dedicated `n=1` test in Task 7 Step 5 covers this — DELETE the old `one window fills the board` test.
- `'two windows tile 2×1'` — replaced by the new `n=2 → side-by-side comfort` test. DELETE.
- `'four windows tile 2×2'` — was for fully-filling 1200×800; the new engine produces 2×2 comfort-capped. Update the test assertion to: `expect(out.get('a')!.w).toBeLessThanOrEqual(COMFORT_W)` and that the four windows tile without overlap.
- `'five windows: 3×2 with last row centered'` — engine still does 3×2 last-row-centered semantics but with comfort cap. Update assertion: the last-row x offset is no longer relative to full-board width, but to the centered block. REPLACE: drop the `expectedLeft = (1200 - totalLastRowW) / 2` calculation; instead test `expect(d.y).toBeGreaterThan(0); expect(e.x).toBeGreaterThan(d.x);` plus the no-overlap invariant: `expect(d.x + d.w).toBeLessThanOrEqual(e.x)`.
- `'six windows: 3×2 grid'` — REPLACE with: no overlap among the six, and last cell `f` is rightmost of the bottom row.
- `'focused window receives top z-index'` — should still pass; KEEP unchanged.
- `'manually-placed window uses its stored position/size'` — should still pass.
- `'skips tucked windows entirely'` — should still pass.
- The overflow tests `'places overflow windows at grid intersection points'`, `'overflow renders above tiled in z-order'`, `'two overflow windows pick distinct intersection points'`, `'with T1=1, overflow stacks near center with jitter'` — need value adjustments because the block is now centered (offset by EDGE_INSET + maybe more), not flush at (0,0). Update each one's expectation: instead of literal coordinates like `expect(g.x).toBeCloseTo(200, 0)`, assert relative invariants: (i) overflow has higher zIndex than tiled; (ii) overflow cells fit inside the board; (iii) two overflow windows have distinct positions.

For each broken assertion, use this pattern instead:

```ts
const tiled = ['a','b','c','d','e','f'].map(id => out.get(id)!);
const ov = out.get('g')!;
const ov2 = out.get('h')!;
expect(ov.zIndex).toBeGreaterThan(tiled[0].zIndex);
expect(ov.x).toBeGreaterThanOrEqual(0);
expect(ov.x + ov.w).toBeLessThanOrEqual(wideBoard.width);
expect(ov2.x !== ov.x || ov2.y !== ov.y).toBe(true);
```

Concretely — DELETE these old assertions:
```ts
expect(out.get('a')!.w).toBe(600);
expect(out.get('a')!.h).toBe(800);
expect(b.w).toBe(600); expect(b.h).toBe(800); expect(b.x).toBe(600);
expect(out.get('a')!.w).toBe(600);
expect(out.get('a')!.h).toBe(400);
expect(out.get('d')!.x).toBe(600);
expect(out.get('d')!.y).toBe(400);
expect(out.get('f')!.x).toBe(800);
expect(out.get('f')!.y).toBe(400);
expect(g.x).toBeCloseTo(200, 0);
expect(g.y).toBeCloseTo(200, 0);
expect(g.w).toBe(400);
expect(g.h).toBe(400);
```

For the deleted tests `'one window fills the board'` and `'two windows tile 2×1'`, simply delete the `it(...)` blocks entirely — they're superseded by the new comfort-size tests.

For the `'four windows tile 2×2'` test, REPLACE the body with:

```ts
  it('four windows tile in a 2×2-ish grid without overlap', () => {
    const ids = ['a','b','c','d'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(4);
    // No-overlap invariant
    const rects = ids.map(id => out.get(id)!);
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i], b = rects[j];
        const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
        const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
        expect(ow * oh).toBe(0);
      }
    }
  });
```

For `'five windows: 3×2 with last row centered'`:

```ts
  it('five windows tile without overlap with last row centered', () => {
    const ids = ['a','b','c','d','e'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(5);
    const d = out.get('d')!;
    const e = out.get('e')!;
    expect(d.y).toBe(e.y);
    expect(d.x).toBeLessThan(e.x);
    expect(d.x + d.w).toBeLessThanOrEqual(e.x);
  });
```

For `'six windows: 3×2 grid'`:

```ts
  it('six windows tile without overlap', () => {
    const ids = ['a','b','c','d','e','f'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(6);
    const rects = ids.map(id => out.get(id)!);
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i], b = rects[j];
        const ow = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
        const oh = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
        expect(ow * oh).toBe(0);
      }
    }
  });
```

For each of the 4 overflow tests at the bottom, the structural assertions about zIndex and distinct positions still hold; only the literal coord checks fail. REPLACE the body of `'places overflow windows at grid intersection points sorted by centrality'` with:

```ts
    const ids = ['a','b','c','d','e','f','g'];
    const out = computeLayout(ids.map(i => mkWindow(i)), board, 6, 8, null);
    expect(out.size).toBe(7);
    const g = out.get('g')!;
    expect(g.zIndex).toBeGreaterThan(Z_TILED);
    expect(g.x).toBeGreaterThanOrEqual(0);
    expect(g.x + g.w).toBeLessThanOrEqual(board.width);
    expect(g.y).toBeGreaterThanOrEqual(0);
    expect(g.y + g.h).toBeLessThanOrEqual(board.height);
```

(Also add `import { Z_TILED } from './layoutEngine';` and `export const Z_TILED = 1;` in `layoutEngine.ts` if not already exported.)

The other three overflow tests keep their structural assertions — they don't reference literal coords.

- [ ] **Step 9: Run all layout-engine tests, expect green**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/layoutEngine.test.ts 2>&1 | tail -15
```
Expected: all tests pass (the 7 new + the updated 8 existing).

- [ ] **Step 10: Commit**

```bash
git add ui/desktop/src/components/Dashboard/layoutEngine.ts \
        ui/desktop/src/components/Dashboard/layoutEngine.test.ts
git commit -m "feat(dashboard): deterministic soft-tile + relaxation layout engine

Pure pipeline: partition → comfort-sizing → slot → repulse pinned →
6-pass auto-vs-auto relaxation → snap. Hash32 used for all tie-breaks.
No RNG anywhere. Repeated organize() is deterministic by construction."
```

---

# Task 8: Focus-pop respects board borders

**Files:**
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`

### Context

The focus pop's `-translate-y-0.5 scale-[1.01]` with default `transform-origin: center` makes a window touching a border visibly cross over it. Compute `transformOrigin` per-window based on which edges its rect touches; combine with conditional translate so the pop never overflows.

The layout engine already enforces `EDGE_INSET = 6` so windows are at least 6px inside the board.

- [ ] **Step 1: Compute transformOrigin in ChatWindow**

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, find the `stylePos` `useMemo` (returns transform + width + height + zIndex). Add a sibling `useMemo` that computes the transform origin and conditional translate:

```tsx
  const popStyle = useMemo(() => {
    if (!isFocused) return {};
    const TOUCH = 4;
    const leftTouching = rect.x <= TOUCH;
    const rightTouching = rect.x + rect.w >= boardSize.width - TOUCH;
    const topTouching = rect.y <= TOUCH;
    const bottomTouching = rect.y + rect.h >= boardSize.height - TOUCH;
    const ox = leftTouching ? 'left' : rightTouching ? 'right' : 'center';
    const oy = topTouching ? 'top' : bottomTouching ? 'bottom' : 'center';
    return {
      transformOrigin: `${ox} ${oy}`,
    };
  }, [isFocused, rect.x, rect.y, rect.w, rect.h, boardSize.width, boardSize.height]);
```

- [ ] **Step 2: Update the `focusClasses` to drop the negative translate when top-touching**

Find this snippet in `ChatWindow.tsx`:

```tsx
  const focusClasses = isFocused
    ? isSolo
      ? 'shadow-[0_8px_30px_rgb(0,0,0,0.18)]'
      : 'shadow-[0_12px_40px_rgb(0,0,0,0.22)] -translate-y-0.5 scale-[1.01]'
    : 'shadow-[0_4px_14px_rgb(0,0,0,0.10)]';
```

REPLACE with:

```tsx
  const TOUCH_PX = 4;
  const topTouching = rect.y <= TOUCH_PX;
  const focusClasses = isFocused
    ? isSolo
      ? 'shadow-[0_8px_30px_rgb(0,0,0,0.18)]'
      : `shadow-[0_12px_40px_rgb(0,0,0,0.22)] scale-[1.01] ${topTouching ? '' : '-translate-y-0.5'}`
    : 'shadow-[0_4px_14px_rgb(0,0,0,0.10)]';
```

- [ ] **Step 3: Merge `popStyle` into the rendered `style` prop**

Find the outer `<div>` of `ChatWindow` that uses `style={stylePos}`. Change to:

```tsx
<div
  className={…}
  style={{ ...stylePos, ...popStyle }}
  onMouseDown={…}
>
```

- [ ] **Step 4: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/Dashboard/ChatWindow.tsx
git commit -m "fix(dashboard): focus pop respects board borders (origin + conditional translate)"
```

---

# Task 9: BaseChat coherent by default + per-window pickers

**Files:**
- Modify: `ui/desktop/src/components/BaseChat.tsx`
- Modify: `ui/desktop/src/components/ChatInput.tsx`
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`
- Modify: `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx`
- Modify: `ui/desktop/src/components/Dashboard/DashboardRoute.tsx`
- Delete usage of `DashboardStatusBar` (file already deleted in Task 1)

### Context

Flip BaseChat to coherent-by-default, remove the `hideStatusBar` plumbing entirely so each dashboard window's ChatInput shows its full picker row. Drop `#N` badge from window title. Stop rendering `DashboardStatusBar` in the route.

- [ ] **Step 1: Flip the BaseChat `coherent` default**

In `ui/desktop/src/components/BaseChat.tsx`, find the BaseChatContent destructuring:

```tsx
  coherent = false,
  hideStatusBar = false,
```

Replace with:

```tsx
  coherent = true,
```

(`hideStatusBar` is dropped entirely.)

Also in the `BaseChatProps` interface, remove the `hideStatusBar` property:

OLD:
```ts
  /** Hide model/mode/cost/cwd footer in ChatInput. */
  hideStatusBar?: boolean;
```

DELETE this property and its JSDoc.

Find the `<ChatInput ... hideStatusBar={hideStatusBar} ... />` line and remove the prop. Find the conditional `border-t border-border-subtle/30` etc. wrapping that referenced `coherent` — leave those as-is (coherent is still a prop, just now true by default).

- [ ] **Step 2: Remove `hideStatusBar` from ChatInput**

In `ui/desktop/src/components/ChatInput.tsx`:

In the `ChatInputProps` interface, remove:

```ts
  /** If true, hide the bottom controls row ... */
  hideStatusBar?: boolean;
```

In the destructuring `function ChatInput({ ... })`, remove `hideStatusBar = false,`.

Find the JSX block:

```tsx
        {!hideStatusBar && (
        <>
        <DirSwitcher
          ...
```

Delete the `{!hideStatusBar && (` line and the `<>` line. Find the matching closing:

```tsx
          )}
        </div>
        </>
        )}
```

Delete the `</>` line and the closing `)}` (keep the `</div>` and the preceding `)}` from the `{sessionId && (...)}` conditional intact). Net result: the entire controls row is unconditionally rendered.

- [ ] **Step 3: Remove `hideStatusBar` from ChatWindow**

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, find:

```tsx
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            hideStatusBar
          />
```

Change to:

```tsx
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
          />
```

- [ ] **Step 4: Drop the `#N` badge from WindowTitleBar**

In `ui/desktop/src/components/Dashboard/WindowTitleBar.tsx`, find:

```tsx
      <span className="text-xs font-mono text-text-muted flex-shrink-0">#{badge}</span>
```

REMOVE this line. Also remove the `badge` prop from the interface and destructuring (since the name now self-numbers via `Session N`).

In the interface:

```ts
interface Props {
  name: string;
  badge: number;       // ← delete
  accentColor: string;
  ...
}
```

And in the destructure:

```tsx
export const WindowTitleBar: React.FC<Props> = ({
  name,
  badge,               // ← delete
  accentColor,
  ...
}) => {
```

In ChatWindow.tsx, remove `badge={win.badge}` from the `<WindowTitleBar>` invocation.

- [ ] **Step 5: Remove DashboardStatusBar render from DashboardRoute**

In `ui/desktop/src/components/Dashboard/DashboardRoute.tsx`, find the JSX:

```tsx
    <div className="h-full w-full flex flex-col min-h-0 bg-background-muted">
      <DashboardToolbar />
      <DashboardBoard />
      <DashboardStatusBar />
    </div>
```

Replace with:

```tsx
    <div className="h-full w-full flex flex-col min-h-0 bg-background-muted">
      <DashboardToolbar />
      <DashboardBoard />
    </div>
```

Also remove the `import { DashboardStatusBar } from './DashboardStatusBar';` line.

- [ ] **Step 6: Run unit tests for any regressions**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/ 2>&1 | tail -15
```
Expected: PASS (counts unchanged, ~30+).

- [ ] **Step 7: Type-check + lint**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -10
```

- [ ] **Step 8: Commit**

```bash
git add ui/desktop/src/components/BaseChat.tsx \
        ui/desktop/src/components/ChatInput.tsx \
        ui/desktop/src/components/Dashboard/ChatWindow.tsx \
        ui/desktop/src/components/Dashboard/WindowTitleBar.tsx \
        ui/desktop/src/components/Dashboard/DashboardRoute.tsx
git commit -m "feat(dashboard): coherent /pair default; per-window pickers; drop badge"
```

---

# Task 10: SessionNamePill component + integration in BaseChat

**Files:**
- Create: `ui/desktop/src/components/Dashboard/SessionNamePill.tsx`
- Modify: `ui/desktop/src/components/BaseChat.tsx`
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`

### Context

Inline-editable name displayed at the top of the chat content area. Works in both `/pair` and inside dashboard windows. Double-click → input, Enter to commit, Esc to cancel. Calls a callback the consumer wires up to `updateSessionName` (for /pair) or to `renameWindow` + `updateSessionName` (for dashboard windows).

For dashboard windows, the name lives in `DashboardWindow.name`. For /pair standalone, the name lives in biorouterd's session — read via `session.name`, write via `updateSessionName` API.

- [ ] **Step 1: Implement the pill component**

Create `ui/desktop/src/components/Dashboard/SessionNamePill.tsx`:

```tsx
import React, { useEffect, useRef, useState } from 'react';

interface Props {
  name: string;
  onRename: (newName: string) => void;
  /** Optional accent color dot. */
  accentColor?: string;
  className?: string;
}

export const SessionNamePill: React.FC<Props> = ({ name, onRename, accentColor, className }) => {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setDraft(name);
  }, [name]);
  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== name) onRename(trimmed);
    setEditing(false);
  };

  return (
    <div className={`inline-flex items-center gap-2 px-2 py-1 rounded-md ${className ?? ''}`}>
      {accentColor && (
        <span
          className="inline-block w-2 h-2 rounded-full flex-shrink-0"
          style={{ backgroundColor: accentColor }}
        />
      )}
      {editing ? (
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setDraft(name);
              setEditing(false);
            }
          }}
          className="bg-transparent text-sm font-medium outline-none border-b border-border-subtle min-w-[120px]"
        />
      ) : (
        <span
          className="text-sm font-medium cursor-text"
          onDoubleClick={() => setEditing(true)}
          title="Double-click to rename"
        >
          {name}
        </span>
      )}
    </div>
  );
};
```

- [ ] **Step 2: Wire BaseChat to render the pill**

In `ui/desktop/src/components/BaseChat.tsx`:

Add the import near the top:

```tsx
import { SessionNamePill } from './Dashboard/SessionNamePill';
import { updateSessionName } from '../api';
```

Add an optional prop to BaseChat so dashboard windows can intercept the rename (since dashboard wraps biorouterd's rename with provider state update). Add to `BaseChatProps`:

```ts
  /** Optional: overrides the default rename behavior (which calls biorouterd updateSessionName). */
  onRenameSession?: (newName: string) => void;
  /** Optional accent dot color (dashboard windows pass theirs). */
  accentColor?: string;
```

In the `BaseChatContent` destructure, add: `onRenameSession, accentColor,`.

Default rename handler: write to biorouterd via `updateSessionName`. Inside `BaseChatContent`, after the existing `lastSetNameRef`, add:

```tsx
  const handleRename = (newName: string) => {
    if (onRenameSession) {
      onRenameSession(newName);
      return;
    }
    if (!sessionId) return;
    void updateSessionName({
      path: { session_id: sessionId },
      body: { name: newName },
    });
  };
```

Render the pill in the header row, right above the `ScrollArea`. Find the `<div className={coherent ? '...' : '...'}> <ScrollArea ref={scrollRef} ...>` block. Just inside the wrapper div, BEFORE the ScrollArea, add:

```tsx
          <div className="flex-shrink-0 px-4 pt-3">
            <SessionNamePill
              name={session?.name || 'New session'}
              onRename={handleRename}
              accentColor={accentColor}
            />
          </div>
```

- [ ] **Step 3: Wire ChatWindow to pass `onRenameSession` and accent color**

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, find the existing `<BaseChat>` render and add the new props:

```tsx
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            accentColor={win.accentColor}
            onRenameSession={(newName) => {
              dashboard.renameWindow(win.windowId, newName);
              // also propagate to biorouterd so History reflects it
              void updateSessionName({
                path: { session_id: win.sessionId },
                body: { name: newName },
              });
            }}
          />
```

Add the import: `import { updateSessionName } from '../../api';` at the top of ChatWindow.tsx.

- [ ] **Step 4: Type-check**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npx tsc --noEmit 2>&1 | head -15
```

- [ ] **Step 5: Commit**

```bash
git add ui/desktop/src/components/Dashboard/SessionNamePill.tsx \
        ui/desktop/src/components/BaseChat.tsx \
        ui/desktop/src/components/Dashboard/ChatWindow.tsx
git commit -m "feat(dashboard): editable SessionNamePill in BaseChat; rename propagates to biorouterd"
```

---

# Task 11: `userSetName` flag + biorouterd-name sync

**Files:**
- Modify: `ui/desktop/src/contexts/DashboardContext.tsx`
- Modify: `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`
- Modify: `ui/desktop/src/components/Dashboard/ChatWindow.tsx`
- Modify: `ui/desktop/src/components/Dashboard/dashboardStorage.ts`
- Modify: `ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx`

### Context

When biorouterd auto-renames a session based on conversation content, the window should follow — unless the user explicitly renamed first. Adds a `userSetName: boolean` to each `DashboardWindow` and threads it through state + storage.

- [ ] **Step 1: Add `userSetName` to the type**

In `ui/desktop/src/contexts/DashboardContext.tsx`, add `userSetName: boolean;` to `DashboardWindow`:

```ts
export interface DashboardWindow {
  windowId: string;
  sessionId: string;
  name: string;
  userSetName: boolean;     // NEW
  badge: number;
  accentColor: string;
  ...
}
```

- [ ] **Step 2: Update storage shape**

In `ui/desktop/src/components/Dashboard/dashboardStorage.ts`, add `userSetName: boolean;` to `SerializedDashboardWindow`. (Order: alongside `name`.)

In storage's `loadDashboardState` path, default the field to `false` if missing (i.e., when loading data persisted before this change):

In the helper that returns the parsed state, no special handling required if we trust JSON.parse — but to be safe, do a post-parse defaulting. Inside `loadDashboardState`, after the version check and before returning `parsed`, add:

```ts
parsed.windows = parsed.windows.map((w) =>
  typeof w.userSetName === 'boolean' ? w : { ...w, userSetName: false }
);
```

- [ ] **Step 3: Update spawnWindow & renameWindow in DashboardProvider**

In `ui/desktop/src/components/Dashboard/DashboardProvider.tsx`:

In `spawnWindow`, the new `DashboardWindow` literal needs `userSetName: false`:

```ts
      const newWin: DashboardWindow = {
        windowId: nextWindowId(),
        sessionId,
        name,
        userSetName: false,   // NEW
        badge,
        accentColor,
        ...
      };
```

In `renameWindow`, set `userSetName: true`:

```ts
  const renameWindow: DashboardApi['renameWindow'] = useCallback((windowId, name) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId ? { ...w, name, userSetName: true } : w
      ),
    }));
  }, []);
```

Add a new API method on `DashboardApi` for the biorouterd-driven auto-name sync — one that does NOT set `userSetName`:

In `DashboardContext.tsx`, add to the API:

```ts
  /** Called from useChatStream when biorouterd auto-names the session. */
  syncSessionName: (windowId: string, name: string) => void;
```

In `DashboardProvider.tsx`, implement:

```ts
  const syncSessionName: DashboardApi['syncSessionName'] = useCallback((windowId, name) => {
    setState((prev) => ({
      ...prev,
      windows: prev.windows.map((w) =>
        w.windowId === windowId && !w.userSetName && w.name !== name ? { ...w, name } : w
      ),
    }));
  }, []);
```

Add `syncSessionName` to the `api` memo's deps list and value:

```ts
  const api: DashboardApi = useMemo(
    () => ({
      ...,
      syncSessionName,
    }),
    [..., syncSessionName]
  );
```

- [ ] **Step 4: Subscribe to session.name from ChatWindow**

In `ui/desktop/src/components/Dashboard/ChatWindow.tsx`, the `BaseChat` rendered inside the window reads `session` via its `useChatStream`. We can't read that from outside BaseChat — so subscribe inside ChatWindow with its own `useChatStream`-like access OR expose `onSessionUpdate` from BaseChat.

The simpler approach: add a new optional `onSessionUpdate?: (session: Session | null) => void` prop to BaseChat. Inside BaseChat, fire the callback whenever `session?.name` changes:

In `BaseChatProps`, add:

```ts
  /** Notify parent when the underlying session object changes (e.g., biorouterd renamed it). */
  onSessionUpdate?: (session: { id: string; name: string } | null) => void;
```

In `BaseChatContent`, add after the existing `lastSetNameRef` effect:

```tsx
  useEffect(() => {
    if (!onSessionUpdate) return;
    if (!session) return;
    onSessionUpdate({ id: session.id, name: session.name });
  }, [session?.id, session?.name, onSessionUpdate]);
```

Make sure to add `onSessionUpdate` to the destructuring at the top of `BaseChatContent`.

In `ChatWindow.tsx`, pass the callback:

```tsx
          <BaseChat
            setChat={setChat}
            sessionId={win.sessionId}
            suppressEmptyState={false}
            coherent
            accentColor={win.accentColor}
            onRenameSession={(newName) => {
              dashboard.renameWindow(win.windowId, newName);
              void updateSessionName({
                path: { session_id: win.sessionId },
                body: { name: newName },
              });
            }}
            onSessionUpdate={(s) => {
              if (s?.name) dashboard.syncSessionName(win.windowId, s.name);
            }}
          />
```

- [ ] **Step 5: Add a provider test for sync + userSet precedence**

In `ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx`, ADD this test block at the end of the outer `describe`:

```ts
  it('syncSessionName updates name when userSetName is false', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.syncSessionName(id, 'Auto-named by AI'));
    expect(result.current.state.windows[0].name).toBe('Auto-named by AI');
    expect(result.current.state.windows[0].userSetName).toBe(false);
  });

  it('syncSessionName respects userSetName and does NOT overwrite user rename', async () => {
    const { result } = renderHook(() => useDashboard(), { wrapper });
    await act(async () => {
      await result.current.spawnWindow();
    });
    const id = result.current.state.windows[0].windowId;
    act(() => result.current.renameWindow(id, 'My Project'));
    expect(result.current.state.windows[0].userSetName).toBe(true);
    act(() => result.current.syncSessionName(id, 'Auto-named by AI'));
    // User-set name wins
    expect(result.current.state.windows[0].name).toBe('My Project');
  });
```

- [ ] **Step 6: Run tests, expect green**

```bash
cd /Users/wgu/Desktop/biorouter/ui/desktop && npm run test:run -- src/components/Dashboard/ 2>&1 | tail -15
```
Expected: all tests pass; counts now include the new provider tests.

- [ ] **Step 7: Commit**

```bash
git add ui/desktop/src/contexts/DashboardContext.tsx \
        ui/desktop/src/components/Dashboard/DashboardProvider.tsx \
        ui/desktop/src/components/Dashboard/DashboardProvider.test.tsx \
        ui/desktop/src/components/Dashboard/dashboardStorage.ts \
        ui/desktop/src/components/Dashboard/ChatWindow.tsx \
        ui/desktop/src/components/BaseChat.tsx
git commit -m "feat(dashboard): userSetName flag; biorouterd auto-rename sync"
```

---

# Task 12: Playwright debugger end-to-end validation

**Files:** none modified (validation only).

### Context

Launch the dev app via Terminal.app (`script` workaround we found in the prior session), connect via Playwright MCP at port 9222, and exercise every change.

- [ ] **Step 1: Kill any prior Electron, launch dev with CDP**

```bash
cd /Users/wgu/Desktop/biorouter
killall -9 Electron "Electron Helper" 2>/dev/null
osascript -e 'tell application "Terminal" to do script "/tmp/launch-biorouter.sh"'
# wait until CDP is up
until curl -s http://localhost:9222/json/version >/dev/null 2>&1; do sleep 2; done
echo "CDP ready"
```

- [ ] **Step 2: Verify the LayoutDashboard icon next to `+`**

Via Playwright MCP, snapshot the top-left toolbar. The button next to `+` should have `title="Open Dashboard"` and render the `LayoutDashboard` SVG glyph.

- [ ] **Step 3: Click LayoutDashboard → navigate to `/dashboard`; auto-spawn one window at 940×800 centered**

```js
// browser_evaluate
const dashBtn = document.querySelector('button[title="Open Dashboard"]');
dashBtn.click();
// wait for navigation + auto-spawn
```

Read `localStorage.getItem('biorouter.dashboard.v1')` — should contain one window with `name: 'Session 1'`, `userSetName: false`.

Measure the rendered window's bounding rect: width ~940, height ~800, centered on the board.

- [ ] **Step 4: Spawn 6, 8 → verify cell cap and no overlap**

Click Spawn 5 more times → 6 windows total. All cells should be capped at ≤940×800. Overlap of any two: 0 (assert via JS overlap math).

- [ ] **Step 5: Resize one window via the bottom-right handle to confirm pin + Organize round-trip**

```js
// browser_evaluate
// programmatically resize: set isManuallyPlaced=true via provider for ease
// Actually use the resize handle via pointer events on the handle's bbox.
// Then click Organize and verify the window snaps back to engine-computed position.
```

After clicking Organize: `state.windows.every(w => !w.isManuallyPlaced)` is true; computeLayout output is identical to before the manual resize.

- [ ] **Step 6: Resize check: dragging shrinks window; window stays at user size**

Drag the bottom-right corner of one window inward. Verify `state.windows[i].size === {w: smaller, h: smaller}` and `isManuallyPlaced === true`. Re-render of the board respects this (the engine pins it).

- [ ] **Step 7: Focus pop near edge does not push past border**

Manually place a window so its right edge is flush with the board's right inset. Click to focus it. Capture screenshot. Window's bounding-client-rect right edge should be ≤ board's right edge (no pixel bleed).

- [ ] **Step 8: Rename via SessionNamePill in /pair**

Navigate to /pair (open Chat). The pill at the top shows the session name. Double-click → type "Renamed in Chat" → Enter. Verify the session in History reflects the rename (call `listSessions` via the renderer's api module).

- [ ] **Step 9: Auto-rename precedence**

Spawn a new dashboard window. Don't rename it. Send a single message that triggers biorouterd's auto-naming. After a delay, the window's name should follow biorouterd's. Now manually rename → `userSetName: true`. Wait again — the name stays put.

- [ ] **Step 10: Navigate away → BrowserWindow unmaximizes**

From `/dashboard`, click Home in the sidebar. Verify `window.outerWidth` and `outerHeight` come back to 940×800 (or whatever pre-maximize size was). Click the floating "Back to Dashboard" pill → re-enters → maximizes again.

- [ ] **Step 11: Persistence across reload**

`location.reload()`. State restored from `biorouter.dashboard.v1`. Names, positions, sizes, T1/T2 all intact.

- [ ] **Step 12: Spec-cleanup grep**

```bash
grep -rin "lab.\?meeting\|LabMeeting\|labMeeting" /Users/wgu/Desktop/biorouter/ui/desktop/src/ | head
```
Expected: empty.

- [ ] **Step 13: Tag**

```bash
git commit --allow-empty -m "feat(dashboard): end-to-end Playwright validation complete"
```

---

## Self-review notes (post-write)

**Spec coverage check:**
- §1 Rename → Tasks 1, 3 (storage), 2 (icon-related rename in pill)
- §2 Icon → Task 2
- §3 Comfort sizing → Task 7
- §4 Focus pop within borders → Task 8 (+ EDGE_INSET in Task 7)
- §5 Toolbar polish + Organize/Clear → Task 5 (visual); Organize/Clear behavior is fixed implicitly by Task 7 (engine now produces a different output post-resize, so Organize visibly works)
- §6 Window-size restore → Task 4
- §7 Coherent /pair + per-window pickers → Task 9
- §8 Tab names → Tasks 6 (default), 10 (pill), 11 (userSet + sync)
- §9 Layout engine → Task 7
- §10 User-resize → covered by Task 7's pinned handling; UI already exists from v1
- §13 Migration → Task 3

**Placeholder scan:** every step has concrete code or commands. No TBDs.

**Type consistency:** `DashboardWindow`, `DashboardApi`, `useDashboard`, `dashboard:enter` / `dashboard:exit`, `dashboardEnter` / `dashboardExit`, `loadDashboardState` / `saveDashboardState`, `biorouter.dashboard.v1`, `STORAGE_KEY` — all consistent across tasks.

**Known v1+ notes:**
- Drag-from-sidebar-to-evoke is unchanged from v1; still works.
- Per-window full ChatInput pickers means each window will show a full bottom toolbar — visually busier than v1's hidden state. The user has explicitly requested this (§7b).
- Coherent `/pair` flip is the only user-facing change to a non-dashboard route. Smoke-test in Task 12 Step 8.

---

## Execution

Plan complete. Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks. Fastest iteration; smallest review surface per cycle.
2. **Inline Execution** — execute tasks in this session using executing-plans; batch execution with checkpoints.

Per the user's request ("implement the plans using subagents if needed"), default to **Subagent-Driven**.
