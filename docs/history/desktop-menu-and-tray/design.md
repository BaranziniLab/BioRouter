# Menu Bar & System Tray Redesign

**Goal:** Replace the default Electron menu bar with BioRouter-specific menus and improve tray behavior (left-click = show window silently, right-click = context menu with key actions).

**Architecture:** All changes are in `ui/desktop/src/main.ts` and `ui/desktop/src/utils/autoUpdater.ts`. The menu bar is rebuilt with `Menu.buildFromTemplate` and set via `Menu.setApplicationMenu`. Tray behavior is updated on the `tray` instance already managed in `autoUpdater.ts`. Navigation items send IPC messages to the focused renderer window (`webContents.send('set-view', routeName)`), matching the existing pattern used for Settings.

**Tech Stack:** Electron `Menu`, `MenuItem`, `Tray`, `shell.openExternal` for external links.

---

## Menu Bar Structure

### BioRouter (app menu — macOS only)

- About BioRouter *(existing)*
- — separator —
- Settings `⌘,`
- — separator —
- Check for Updates…
- Check for Dependencies…
- Check for Extension Updates
- — separator —
- Quit BioRouter `⌘Q`

### Go

- Home `⌘1`
- New Chat `⌘T`
- History `⌘2`
- — separator —
- Workflows `⌘3`
- Scheduler `⌘4`
- — separator —
- Extensions `⌘5`
- Skills `⌘6`

### File

- New Chat `⌘T`
- New Window `⌘N`
- — separator —
- Open Directory… `⌘O`
- Recent Directories ▶ *(submenu, existing)*
- — separator —
- Close Window `⌘W`
- Focus BioRouter Window `⌥⌘G`

### Extensions

- Install Extension (.brxt)…
- Browse Extensions *(opens http://biorouter.ucsf.edu/baam)*
- Add Custom Extension…
- — separator —
- Check for Extension Updates

### Providers

- Configure Providers…
- Switch Model…
- Reset Provider

### View

- Light Mode
- Dark Mode
- System Mode
- — separator —
- *(standard Electron view items: Reload, Force Reload, Toggle DevTools, Zoom In/Out/Reset, Toggle Fullscreen)*

### Help

- Biorouter Documentation *(opens http://biorouter.ucsf.edu/docs)*
- — separator —
- Report a Bug… *(opens GitHub issues)*
- Request a Feature… *(opens GitHub discussions or issues)*
- — separator —
- v{version} *(disabled label showing current app version)*

### Edit, Window

Kept unchanged — macOS requires these for standard system behavior (copy/paste shortcuts, window management).

---

## System Tray

**Left click (macOS):** Show window silently — no dropdown, brings BioRouter to front immediately. Matches existing Windows behavior (already implemented via `tray.on('click', showWindow)`). Fix: add the same `tray.on('click', showWindow)` handler for `darwin`.

**Right click — context menu:**

- Home
- New Chat
- Settings
- — separator —
- Extensions
- Skills
- — separator —
- Check for Updates
- — separator —
- Quit

---

## Navigation IPC Pattern

All "Go to X" menu items send to the focused window:

```js
webContents.send('set-view', 'home')      // or 'chat', 'sessions', 'schedules',
                                           // 'workflows', 'extensions', 'skills'
```

This matches the existing `set-view` handler already wired in the renderer. For tray items that need a window to exist first, the handler creates one if none are open (same pattern as existing "Show Window" in tray).

External links use `shell.openExternal(url)`.

Theme switching sends:

```js
webContents.send('set-theme', 'light' | 'dark' | 'system')
```

The renderer's existing theme system handles this via the `ThemeSelector` component context.

---

## Files Changed

| File | Change |
|------|--------|
| `ui/desktop/src/main.ts` | Full menu bar rebuild: Go, File, Extensions, Providers, View (+ theme items), Help. Tray left-click fix for macOS. |
| `ui/desktop/src/utils/autoUpdater.ts` | Update `updateTrayMenu()` with the new right-click context menu items. |

---

## Out of Scope

- No changes to the renderer routing logic
- No new IPC channels (reuse `set-view`, `set-theme`, existing check/install handlers)
- Windows and Linux menu bars follow the same structure (BioRouter app menu is macOS-only; other platforms get Go/File/Extensions/Providers/View/Help)
