# Menu Bar & System Tray Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the piecemeal Electron default-menu mutations with a fully custom `Menu.buildFromTemplate()` call that adds Go, Extensions, Providers menus; updates File/View/Help menus; and fixes macOS tray left-click behavior.

**Architecture:** All main-process changes live in `main.ts` (new `buildApplicationMenu()` function replaces ~230 lines of mutation code) and `autoUpdater.ts` (updated `updateTrayMenu()` + one new export). Two small additions to `dependencyChecker.ts` and `extensionUpdater.ts` expose already-existing logic as callable functions rather than fire-and-forget startup hooks.

**Tech Stack:** Electron `Menu`, `MenuItem`, `MenuItemConstructorOptions`, `BrowserWindow`, `shell`, `ipcMain`, `Tray`.

---

## File Map

| File | What changes |
|------|-------------|
| `ui/desktop/src/main.ts` | Add `buildApplicationMenu()`, delete old mutation block (~lines 2218–2445), add darwin tray left-click |
| `ui/desktop/src/utils/autoUpdater.ts` | Rewrite `updateTrayMenu()` body; export `openUpdateSettings` |
| `ui/desktop/src/utils/dependencyChecker.ts` | Export `triggerDependencyCheck()` |
| `ui/desktop/src/utils/extensionUpdater.ts` | No change — `runExtensionUpdateCheck` already exported |

---

## Task 1: Add macOS tray left-click handler

**Files:**
- Modify: `ui/desktop/src/main.ts:974-976`

- [ ] **Step 1: Open `main.ts` and find the `createTray` function**

  Locate the block around line 974:
  ```typescript
  if (process.platform === 'win32') {
    tray.on('click', showWindow);
  }
  ```

- [ ] **Step 2: Extend the condition to include darwin**

  Replace that block with:
  ```typescript
  if (process.platform === 'win32' || process.platform === 'darwin') {
    tray.on('click', showWindow);
  }
  ```

  This makes left-clicking the tray icon on macOS show the window silently (no dropdown), consistent with the existing Windows behavior. On macOS, `tray.on('click', ...)` fires on left-click; right-click still shows the context menu via `setContextMenu`.

- [ ] **Step 3: Commit**

  ```bash
  cd ui/desktop
  git add src/main.ts
  git commit -m "fix(tray): left-click shows window on macOS"
  ```

---

## Task 2: Export `openUpdateSettings` from autoUpdater

**Files:**
- Modify: `ui/desktop/src/utils/autoUpdater.ts:760-769`

The `openUpdateSettings` function already exists at line 760 but is not exported. The new menu needs to call it from `main.ts`.

- [ ] **Step 1: Export the function**

  Change line 760 from:
  ```typescript
  function openUpdateSettings() {
  ```
  to:
  ```typescript
  export function openUpdateSettings() {
  ```

- [ ] **Step 2: Import it in `main.ts`**

  Find the existing autoUpdater import block around line 43–49 of `main.ts`:
  ```typescript
  import {
    getUpdateAvailable,
    registerUpdateIpcHandlers,
    setTrayRef,
    setupAutoUpdater,
    updateTrayMenu,
  } from './utils/autoUpdater';
  ```

  Add `openUpdateSettings` to the import:
  ```typescript
  import {
    getUpdateAvailable,
    openUpdateSettings,
    registerUpdateIpcHandlers,
    setTrayRef,
    setupAutoUpdater,
    updateTrayMenu,
  } from './utils/autoUpdater';
  ```

- [ ] **Step 3: Verify TypeScript compiles**

  ```bash
  cd ui/desktop
  npx tsc --noEmit
  ```
  Expected: no errors related to `openUpdateSettings`.

- [ ] **Step 4: Commit**

  ```bash
  git add src/utils/autoUpdater.ts src/main.ts
  git commit -m "feat(menu): export openUpdateSettings from autoUpdater"
  ```

---

## Task 3: Export `triggerDependencyCheck` from dependencyChecker

**Files:**
- Modify: `ui/desktop/src/utils/dependencyChecker.ts`

The menu's "Check for Dependencies…" item needs to trigger the same runtime check that runs at startup. Export a thin wrapper that runs the check immediately and pushes `dependency-event` to all open windows.

- [ ] **Step 1: Add the export at the bottom of `dependencyChecker.ts`**

  Append this after the existing `setupDependencyChecker` function:
  ```typescript
  export function triggerDependencyCheck(): void {
    try {
      const deps = checkAllDependencies();
      const payload: DependencyEvent = { type: 'check-results', deps };
      BrowserWindow.getAllWindows().forEach((win) => {
        if (!win.isDestroyed()) win.webContents.send('dependency-event', payload);
      });
    } catch (err) {
      log.error('[DependencyChecker] manual check error:', err);
    }
  }
  ```

  This always sends the full dependency list (even if all installed), which lets the modal decide whether to show.

- [ ] **Step 2: Import it in `main.ts`**

  Find the dependencyChecker import around line 52:
  ```typescript
  import { registerDependencyIpcHandlers, setupDependencyChecker } from './utils/dependencyChecker';
  ```

  Add `triggerDependencyCheck`:
  ```typescript
  import { registerDependencyIpcHandlers, setupDependencyChecker, triggerDependencyCheck } from './utils/dependencyChecker';
  ```

- [ ] **Step 3: Verify TypeScript compiles**

  ```bash
  cd ui/desktop
  npx tsc --noEmit
  ```
  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add src/utils/dependencyChecker.ts src/main.ts
  git commit -m "feat(menu): export triggerDependencyCheck"
  ```

---

## Task 4: Update tray right-click context menu

**Files:**
- Modify: `ui/desktop/src/utils/autoUpdater.ts:772-831`

Replace the body of `updateTrayMenu()` with items that match the spec: Home, New Chat, Settings, Extensions, Skills, Check for Updates, Quit.

- [ ] **Step 1: Read the existing `updateTrayMenu` function**

  It runs from line 772 to 831 and currently builds: [Update Available?], Show Window, separator, Quit.

- [ ] **Step 2: Replace the function body**

  Replace everything inside `export function updateTrayMenu(hasUpdate: boolean) {` … `}` with:

  ```typescript
  export function updateTrayMenu(hasUpdate: boolean) {
    if (!trayRef) return;

    // Helper: show any existing window, then navigate to a view.
    // If no windows are open, create one first.
    const showAndNavigate = (view: string) => {
      const windows = BrowserWindow.getAllWindows();
      if (windows.length === 0) {
        const recentDirs = loadRecentDirs();
        const openDir = recentDirs.length > 0 ? recentDirs[0] : null;
        ipcMain.emit('create-chat-window', {}, undefined, openDir);
        return;
      }
      windows.forEach((win) => {
        if (!win.isVisible()) win.show();
        win.focus();
      });
      windows[windows.length - 1].webContents.send('set-view', view);
    };

    const menuItems: MenuItemConstructorOptions[] = [];

    if (hasUpdate) {
      menuItems.push({ label: 'Update Available…', click: openUpdateSettings });
      menuItems.push({ type: 'separator' });
    }

    menuItems.push(
      { label: 'Home',       click: () => showAndNavigate('') },
      { label: 'New Chat',   click: () => showAndNavigate('') },
      { label: 'Settings',   click: () => showAndNavigate('settings') },
      { type: 'separator' },
      { label: 'Extensions', click: () => showAndNavigate('extensions') },
      { label: 'Skills',     click: () => showAndNavigate('skills') },
      { type: 'separator' },
      { label: 'Check for Updates', click: openUpdateSettings },
      { type: 'separator' },
      { label: 'Quit', click: () => app.quit() },
    );

    const contextMenu = Menu.buildFromTemplate(menuItems);
    trayRef.setContextMenu(contextMenu);
  }
  ```

  Note: `openUpdateSettings` is now exported (Task 2) so it's available here in the same file without changes.

- [ ] **Step 3: Verify TypeScript compiles**

  ```bash
  cd ui/desktop
  npx tsc --noEmit
  ```

- [ ] **Step 4: Start the app and test the tray**

  ```bash
  cd ui/desktop
  npm run start-gui
  ```

  Right-click the tray icon. Verify the context menu shows: Home, New Chat, Settings, — Extensions, Skills, — Check for Updates, — Quit. Click each to verify navigation works.

- [ ] **Step 5: Commit**

  ```bash
  git add src/utils/autoUpdater.ts
  git commit -m "feat(tray): add navigation items to right-click context menu"
  ```

---

## Task 5: Build full application menu from scratch

**Files:**
- Modify: `ui/desktop/src/main.ts:2218-2445`

This is the main task. Replace the ~230 lines of "get existing menu and mutate it" code with a single call to a new `buildApplicationMenu()` function defined just above `appMain()`.

The existing code to be removed spans from the comment `// Get the existing menu` (line 2218) through the closing brace of `if (menu) { Menu.setApplicationMenu(menu); }` at line 2445. The macOS dock menu block immediately above (lines 2205–2215) is **kept unchanged**.

Also import the one new function needed from `extensionUpdater.ts`.

- [ ] **Step 1: Add `runExtensionUpdateCheck` to the extensionUpdater import in `main.ts`**

  Find the import around line 53:
  ```typescript
  import { scheduleExtensionUpdateCheck } from './utils/extensionUpdater';
  ```

  Add `runExtensionUpdateCheck`:
  ```typescript
  import { runExtensionUpdateCheck, scheduleExtensionUpdateCheck } from './utils/extensionUpdater';
  ```

- [ ] **Step 2: Define `buildApplicationMenu()` just above `appMain()` (around line 2080)**

  Insert the following function before `async function appMain() {`:

  ```typescript
  function buildApplicationMenu() {
    const isMac = process.platform === 'darwin';

    // Find submenu — inserted into Edit after Select All (roles don't allow inline custom items)
    const findSubmenu: MenuItemConstructorOptions[] = [
      {
        label: 'Find…',
        accelerator: isMac ? 'Command+F' : 'Control+F',
        click() { BrowserWindow.getFocusedWindow()?.webContents.send('find-command'); },
      },
      {
        label: 'Find Next',
        accelerator: isMac ? 'Command+G' : 'Control+G',
        click() { BrowserWindow.getFocusedWindow()?.webContents.send('find-next'); },
      },
      {
        label: 'Find Previous',
        accelerator: isMac ? 'Shift+Command+G' : 'Shift+Control+G',
        click() { BrowserWindow.getFocusedWindow()?.webContents.send('find-previous'); },
      },
      ...(isMac
        ? [{
            label: 'Use Selection for Find',
            accelerator: 'Command+E',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('use-selection-find'); },
          } as MenuItemConstructorOptions]
        : []),
    ];

    const template: MenuItemConstructorOptions[] = [
      // ── BioRouter app menu (macOS only) ──────────────────────────────────
      ...(isMac
        ? [{
            label: 'BioRouter',
            submenu: [
              { role: 'about' as const },
              { type: 'separator' as const },
              {
                label: 'Settings',
                accelerator: 'CmdOrCtrl+,',
                click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'settings'); },
              },
              { type: 'separator' as const },
              {
                label: 'Check for Updates…',
                click: openUpdateSettings,
              },
              {
                label: 'Check for Dependencies…',
                click() { triggerDependencyCheck(); },
              },
              {
                label: 'Check for Extension Updates',
                click() { runExtensionUpdateCheck(); },
              },
              { type: 'separator' as const },
              { role: 'quit' as const, label: 'Quit BioRouter' },
            ],
          } as MenuItemConstructorOptions]
        : []),

      // ── Go ────────────────────────────────────────────────────────────────
      {
        label: 'Go',
        submenu: [
          {
            label: 'Home',
            accelerator: 'CmdOrCtrl+1',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', ''); },
          },
          {
            label: 'New Chat',
            accelerator: 'CmdOrCtrl+T',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', ''); },
          },
          {
            label: 'History',
            accelerator: 'CmdOrCtrl+2',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'sessions'); },
          },
          { type: 'separator' as const },
          {
            label: 'Workflows',
            accelerator: 'CmdOrCtrl+3',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'workflows'); },
          },
          {
            label: 'Scheduler',
            accelerator: 'CmdOrCtrl+4',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'schedules'); },
          },
          { type: 'separator' as const },
          {
            label: 'Extensions',
            accelerator: 'CmdOrCtrl+5',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions'); },
          },
          {
            label: 'Skills',
            accelerator: 'CmdOrCtrl+6',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'skills'); },
          },
        ],
      },

      // ── File ─────────────────────────────────────────────────────────────
      {
        label: 'File',
        submenu: [
          {
            label: 'New Chat',
            accelerator: 'CmdOrCtrl+T',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', ''); },
          },
          {
            label: 'New Window',
            accelerator: isMac ? 'Cmd+N' : 'Ctrl+N',
            click() { ipcMain.emit('create-chat-window'); },
          },
          { type: 'separator' as const },
          {
            label: 'Open Directory…',
            accelerator: 'CmdOrCtrl+O',
            click: () => openDirectoryDialog(),
          },
          ...(buildRecentFilesMenu().length > 0
            ? [{
                label: 'Recent Directories',
                submenu: buildRecentFilesMenu(),
              } as MenuItemConstructorOptions]
            : []),
          { type: 'separator' as const },
          { role: 'close' as const },
          {
            label: 'Focus BioRouter Window',
            accelerator: 'CmdOrCtrl+Alt+G',
            click() { focusWindow(); },
          },
        ],
      },

      // ── Edit (standard roles + Find inserted after build) ─────────────
      { role: 'editMenu' as const },

      // ── Extensions ───────────────────────────────────────────────────────
      {
        label: 'Extensions',
        submenu: [
          {
            label: 'Install Extension (.brxt)…',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions'); },
          },
          {
            label: 'Browse Extensions',
            click() { shell.openExternal('http://biorouter.ucsf.edu/baam'); },
          },
          {
            label: 'Add Custom Extension…',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'extensions'); },
          },
          { type: 'separator' as const },
          {
            label: 'Check for Extension Updates',
            click() { runExtensionUpdateCheck(); },
          },
        ],
      },

      // ── Providers ────────────────────────────────────────────────────────
      {
        label: 'Providers',
        submenu: [
          {
            label: 'Configure Providers…',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'configure-providers'); },
          },
          {
            label: 'Switch Model…',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'settings', 'models'); },
          },
          {
            label: 'Reset Provider',
            click() { BrowserWindow.getFocusedWindow()?.webContents.send('set-view', 'configure-providers'); },
          },
        ],
      },

      // ── View (theme toggles + standard Electron view roles) ──────────────
      {
        label: 'View',
        submenu: [
          {
            label: 'Light Mode',
            click() {
              BrowserWindow.getAllWindows().forEach((w) =>
                w.webContents.send('theme-changed', { theme: 'light', useSystemTheme: false })
              );
            },
          },
          {
            label: 'Dark Mode',
            click() {
              BrowserWindow.getAllWindows().forEach((w) =>
                w.webContents.send('theme-changed', { theme: 'dark', useSystemTheme: false })
              );
            },
          },
          {
            label: 'System Mode',
            click() {
              // useSystemTheme: true — ThemeContext reads OS preference and ignores the theme field
              BrowserWindow.getAllWindows().forEach((w) =>
                w.webContents.send('theme-changed', { theme: 'light', useSystemTheme: true })
              );
            },
          },
          { type: 'separator' as const },
          { role: 'reload' as const },
          { role: 'forceReload' as const },
          { role: 'toggleDevTools' as const },
          { type: 'separator' as const },
          { role: 'resetZoom' as const },
          { role: 'zoomIn' as const },
          { role: 'zoomOut' as const },
          { type: 'separator' as const },
          { role: 'togglefullscreen' as const },
        ],
      },

      // ── Help ─────────────────────────────────────────────────────────────
      {
        label: 'Help',
        submenu: [
          {
            label: 'Biorouter Documentation',
            click() { shell.openExternal('http://biorouter.ucsf.edu/docs'); },
          },
          { type: 'separator' as const },
          {
            label: 'Report a Bug…',
            click() {
              shell.openExternal(
                'https://github.com/BaranziniLab/biorouter/issues/new?template=bug_report.md'
              );
            },
          },
          {
            label: 'Request a Feature…',
            click() {
              shell.openExternal(
                'https://github.com/BaranziniLab/biorouter/issues/new?template=feature_request.md'
              );
            },
          },
          { type: 'separator' as const },
          { label: `v${version || app.getVersion()}`, enabled: false },
        ],
      },

      // ── Window (standard roles; Always on Top added after build) ─────────
      { role: 'windowMenu' as const },
    ];

    const menu = Menu.buildFromTemplate(template);

    // Insert Find submenu into Edit after Select All
    // (role: 'editMenu' expands to system defaults; custom items can't be inlined)
    const editMenu = menu.items.find((item) => item.label === 'Edit');
    if (editMenu?.submenu) {
      const selectAllIndex = editMenu.submenu.items.findIndex((item) => item.label === 'Select All');
      if (selectAllIndex >= 0) {
        editMenu.submenu.insert(
          selectAllIndex + 1,
          new MenuItem({ label: 'Find', submenu: Menu.buildFromTemplate(findSubmenu) })
        );
      }
    }

    // Add Always on Top to Window menu
    const windowMenu = menu.items.find((item) => item.label === 'Window');
    if (windowMenu?.submenu) {
      windowMenu.submenu.append(
        new MenuItem({
          label: 'Always on Top',
          type: 'checkbox',
          accelerator: isMac ? 'Cmd+Shift+T' : 'Ctrl+Shift+T',
          click(menuItem) {
            const win = BrowserWindow.getFocusedWindow();
            if (!win) return;
            const alwaysOnTop = menuItem.checked;
            if (isMac) {
              win.setAlwaysOnTop(alwaysOnTop, 'floating');
            } else {
              win.setAlwaysOnTop(alwaysOnTop);
            }
          },
        })
      );
    }

    Menu.setApplicationMenu(menu);
  }
  ```

- [ ] **Step 3: Remove the old mutation block from `appMain` / `app.whenReady`**

  Inside `appMain()` (or the `app.whenReady` callback — whichever wraps this code), delete from the comment `// Get the existing menu` through the closing brace of `if (menu) { Menu.setApplicationMenu(menu); }`.

  That block currently starts at approximately line 2218:
  ```typescript
  // Get the existing menu
  const menu = Menu.getApplicationMenu();
  ```
  and ends at approximately line 2445:
  ```typescript
    Menu.setApplicationMenu(menu);
  }
  ```

  **Keep** the macOS dock menu block immediately above it (lines 2205–2215):
  ```typescript
  // Setup macOS dock menu
  if (process.platform === 'darwin') {
    const dockMenu = Menu.buildFromTemplate([
      { label: 'New Window', click: () => { createNewWindow(app); } },
    ]);
    app.dock?.setMenu(dockMenu);
  }
  ```

- [ ] **Step 4: Call `buildApplicationMenu()` in its place**

  Immediately after the dock menu block (where the old mutation code was), add:
  ```typescript
  buildApplicationMenu();
  ```

- [ ] **Step 5: Verify TypeScript compiles**

  ```bash
  cd ui/desktop
  npx tsc --noEmit
  ```
  Expected: zero errors. Fix any type errors before proceeding (usually `as const` assertions on role/type fields).

- [ ] **Step 6: Start the app and do a full menu walkthrough**

  ```bash
  ENABLE_PLAYWRIGHT=1 npm run start-gui
  ```

  Check each menu in order:

  **BioRouter (macOS):**
  - About BioRouter — opens About dialog ✓
  - Settings ⌘, — navigates to settings view ✓
  - Check for Updates… — opens settings update section ✓
  - Check for Dependencies… — triggers dependency modal (or "all good" if nothing missing) ✓
  - Check for Extension Updates — runs silently in background ✓
  - Quit BioRouter ⌘Q — quits ✓

  **Go:**
  - Home ⌘1 — navigates to hub ✓
  - New Chat ⌘T — navigates to hub ✓
  - History ⌘2 — navigates to sessions ✓
  - Workflows ⌘3 — navigates to workflows ✓
  - Scheduler ⌘4 — navigates to schedules ✓
  - Extensions ⌘5 — navigates to extensions ✓
  - Skills ⌘6 — navigates to skills ✓

  **File:**
  - New Chat ⌘T — navigates to hub ✓
  - New Window ⌘N — opens new window ✓
  - Open Directory… ⌘O — opens directory picker ✓
  - Recent Directories — shows submenu with recent dirs ✓
  - Close Window ⌘W — closes window ✓
  - Focus BioRouter Window ⌥⌘G — shows and focuses window ✓

  **Edit:**
  - Undo/Redo/Cut/Copy/Paste — standard behavior ✓
  - Find submenu with Find… ⌘F — opens find bar ✓

  **Extensions:**
  - Install Extension (.brxt)… — navigates to extensions page ✓
  - Browse Extensions — opens browser to baam.html ✓
  - Add Custom Extension… — navigates to extensions page ✓
  - Check for Extension Updates — triggers update check silently ✓

  **Providers:**
  - Configure Providers… — navigates to configure-providers route ✓
  - Switch Model… — navigates to settings models section ✓
  - Reset Provider — navigates to configure-providers route ✓

  **View:**
  - Light Mode — switches app to light theme ✓
  - Dark Mode — switches app to dark theme ✓
  - System Mode — follows OS appearance ✓
  - Reload / Force Reload / DevTools — standard Electron behavior ✓
  - Zoom In/Out/Reset — standard ✓
  - Toggle Fullscreen — standard ✓

  **Help:**
  - Biorouter Documentation — opens docs URL in browser ✓
  - Report a Bug… — opens GitHub issue URL in browser ✓
  - Request a Feature… — opens GitHub issue URL in browser ✓
  - Version label (disabled) — shows current version ✓

  **Window:**
  - Standard window items ✓
  - Always on Top — toggles float mode ✓

  **Tray (right-click):**
  - Home, New Chat, Settings — navigate correctly ✓
  - Extensions, Skills — navigate correctly ✓
  - Check for Updates — navigates to update section ✓
  - Quit — quits app ✓

  **Tray (left-click on macOS):**
  - Shows window silently (no dropdown) ✓

- [ ] **Step 7: Commit**

  ```bash
  git add src/main.ts src/utils/extensionUpdater.ts
  git commit -m "feat(menu): full menu bar rebuild — Go, Extensions, Providers, View themes, Help"
  ```

---

## Self-Review

**Spec coverage:**
- BioRouter app menu (macOS) — covered in `buildApplicationMenu`, macOS-only block ✓
- Go menu with ⌘1–⌘6 shortcuts — covered ✓
- File menu with New Chat, New Window, Open Directory, Recent Dirs, Close, Focus — covered ✓
- Edit + Window kept unchanged via roles — covered (with existing Find and Always on Top additions preserved) ✓
- Extensions menu — covered ✓
- Providers menu — covered ✓
- View menu with theme modes + standard items — covered ✓
- Help menu with docs, bug, feature, version — covered ✓
- Tray left-click on macOS — Task 1 ✓
- Tray right-click menu — Task 4 ✓
- No new IPC channels — confirmed, all use existing `set-view`, `theme-changed` ✓
- External links via `shell.openExternal` — confirmed ✓

**Placeholder scan:** None found.

**Type consistency:**
- `'set-view'` channel — matches `App.tsx:527` listener ✓
- `'theme-changed'` channel — matches `ThemeContext.tsx:111` listener ✓
- View names (`''`, `'sessions'`, `'schedules'`, `'workflows'`, `'extensions'`, `'skills'`, `'settings'`, `'configure-providers'`) — all match routes in `App.tsx:594-639` ✓
- `theme-changed` payload `{ theme: 'light'|'dark', useSystemTheme: boolean }` — matches `ThemeContext.tsx:100-106` handler ✓
