# Menu bar and system tray implementation plan

> **What this is.** The task-by-task build plan that turned the [menu bar and system tray design](design.md) into code: a single custom `buildApplicationMenu()` template replacing Electron's patched default menu, plus a fix for macOS tray left-click.
> **Status:** Historical record — written 2026-05-07 and executed. `buildApplicationMenu()` is present in `ui/desktop/src/main.ts` in the shipping app, so every task below landed. The line numbers this plan cites are from the 2026-05-07 tree and no longer point at the code they describe.
> **Audience:** agents and developers working on the Electron desktop shell.
> **Identifier scheme:** "Task 1"–"Task 5" are this document's own step groups, in execution order; later tasks refer back to earlier ones by number. They are local to this file and have no index elsewhere.

The desktop app previously built its menu by fetching Electron's default menu at startup and mutating it in place — roughly 230 lines of patching that made the real menu structure impossible to read off the source, and left BioRouter's own routes unreachable from the menu bar. This plan replaces that block with one declarative template and threads three already-existing internal functions out as exports so menu items can call them. Read [the design spec](design.md) first for what the finished menu contains and why; this document is the operational path to it.

> **Note for agentic workers.** Required sub-skill: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task.

> **Warning.** Every line-number reference below (`main.ts:974-976`, `autoUpdater.ts:760-769`, "delete old mutation block (~lines 2218–2445)", and similar) reflects the tree as it stood on 2026-05-07. The files have changed substantially since. Locate code by the function and symbol names given, never by the line numbers.

## Goal and approach

Replace the piecemeal Electron default-menu mutations with a fully custom `Menu.buildFromTemplate()` call that adds Go, Extensions, Providers menus; updates File/View/Help menus; and fixes macOS tray left-click behavior.

All main-process changes live in `main.ts` (new `buildApplicationMenu()` function replaces ~230 lines of mutation code) and `autoUpdater.ts` (updated `updateTrayMenu()` + one new export). Two small additions to `dependencyChecker.ts` and `extensionUpdater.ts` expose already-existing logic as callable functions rather than fire-and-forget startup hooks.

The technologies involved are Electron's `Menu`, `MenuItem`, `MenuItemConstructorOptions`, `BrowserWindow`, `shell`, `ipcMain` and `Tray`.

## File map

| File | What changes |
|------|-------------|
| `ui/desktop/src/main.ts` | Add `buildApplicationMenu()`, delete old mutation block (~lines 2218–2445), add darwin tray left-click |
| `ui/desktop/src/utils/autoUpdater.ts` | Rewrite `updateTrayMenu()` body; export `openUpdateSettings` |
| `ui/desktop/src/utils/dependencyChecker.ts` | Export `triggerDependencyCheck()` |

`ui/desktop/src/utils/extensionUpdater.ts` needs no change — `runExtensionUpdateCheck` is already exported. It appears in the plan only because `main.ts` must import it.

## Task 1: Add macOS tray left-click handler

Modify `ui/desktop/src/main.ts:974-976`.

Left-clicking the tray icon on macOS should show the window silently (no dropdown), consistent with the existing Windows behavior. On macOS, `tray.on('click', ...)` fires on left-click; right-click still shows the context menu via `setContextMenu`. The whole change is widening one platform check.

1. **Open `main.ts` and find the `createTray` function.** Locate the block around line 974:

   ```typescript
   if (process.platform === 'win32') {
     tray.on('click', showWindow);
   }
   ```

2. **Extend the condition to include darwin.** Replace that block with:

   ```typescript
   if (process.platform === 'win32' || process.platform === 'darwin') {
     tray.on('click', showWindow);
   }
   ```

3. **Commit.**

   ```bash
   cd ui/desktop
   git add src/main.ts
   git commit -m "fix(tray): left-click shows window on macOS"
   ```

## Task 2: Export `openUpdateSettings` from autoUpdater

Modify `ui/desktop/src/utils/autoUpdater.ts:760-769`.

The `openUpdateSettings` function already exists at line 760 but is not exported. The new menu needs to call it from `main.ts`.

1. **Export the function.** Change line 760 from:

   ```typescript
   function openUpdateSettings() {
   ```

   to:

   ```typescript
   export function openUpdateSettings() {
   ```

2. **Import it in `main.ts`.** Find the existing autoUpdater import block around line 43–49 of `main.ts`:

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

3. **Verify TypeScript compiles.**

   ```bash
   cd ui/desktop
   npx tsc --noEmit
   ```

   Expected: no errors related to `openUpdateSettings`.

4. **Commit.**

   ```bash
   git add src/utils/autoUpdater.ts src/main.ts
   git commit -m "feat(menu): export openUpdateSettings from autoUpdater"
   ```

## Task 3: Export `triggerDependencyCheck` from dependencyChecker

Modify `ui/desktop/src/utils/dependencyChecker.ts`.

The menu's "Check for Dependencies…" item needs to trigger the same runtime check that runs at startup. Export a thin wrapper that runs the check immediately and pushes `dependency-event` to all open windows. The wrapper always sends the full dependency list (even if all installed), which lets the modal decide whether to show.

1. **Add the export at the bottom of `dependencyChecker.ts`.** Append this after the existing `setupDependencyChecker` function:

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

2. **Import it in `main.ts`.** Find the dependencyChecker import around line 52:

   ```typescript
   import { registerDependencyIpcHandlers, setupDependencyChecker } from './utils/dependencyChecker';
   ```

   Add `triggerDependencyCheck`:

   ```typescript
   import { registerDependencyIpcHandlers, setupDependencyChecker, triggerDependencyCheck } from './utils/dependencyChecker';
   ```

3. **Verify TypeScript compiles.**

   ```bash
   cd ui/desktop
   npx tsc --noEmit
   ```

   Expected: no errors.

4. **Commit.**

   ```bash
   git add src/utils/dependencyChecker.ts src/main.ts
   git commit -m "feat(menu): export triggerDependencyCheck"
   ```

## Task 4: Update tray right-click context menu

Modify `ui/desktop/src/utils/autoUpdater.ts:772-831`.

Replace the body of `updateTrayMenu()` with items that match the spec: Home, New Chat, Settings, Extensions, Skills, Check for Updates, Quit. `openUpdateSettings` is exported by Task 2, so it is available here in the same file without further changes.

1. **Read the existing `updateTrayMenu` function.** It runs from line 772 to 831 and currently builds: [Update Available?], Show Window, separator, Quit.

2. **Replace the function body.** Replace everything inside `export function updateTrayMenu(hasUpdate: boolean) {` … `}` with:

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

3. **Verify TypeScript compiles.**

   ```bash
   cd ui/desktop
   npx tsc --noEmit
   ```

4. **Start the app and test the tray.**

   ```bash
   cd ui/desktop
   npm run start-gui
   ```

   Right-click the tray icon. Verify the context menu shows: Home, New Chat, Settings, — Extensions, Skills, — Check for Updates, — Quit. Click each to verify navigation works.

5. **Commit.**

   ```bash
   git add src/utils/autoUpdater.ts
   git commit -m "feat(tray): add navigation items to right-click context menu"
   ```

## Task 5: Build full application menu from scratch

Modify `ui/desktop/src/main.ts:2218-2445`.

This is the main task. Replace the ~230 lines of "get existing menu and mutate it" code with a single call to a new `buildApplicationMenu()` function defined just above `appMain()`.

The existing code to be removed spans from the comment `// Get the existing menu` (line 2218) through the closing brace of `if (menu) { Menu.setApplicationMenu(menu); }` at line 2445. The macOS dock menu block immediately above (lines 2205–2215) is **kept unchanged**.

Two structural points shape the template. Electron's `role: 'editMenu'` and `role: 'windowMenu'` expand to system defaults that cannot carry custom items inline, so the Find submenu and the Always on Top toggle are inserted into the built menu afterwards rather than declared in the template. And the plan imports one new function from `extensionUpdater.ts` so the "Check for Extension Updates" items have something to call.

1. **Add `runExtensionUpdateCheck` to the extensionUpdater import in `main.ts`.** Find the import around line 53:

   ```typescript
   import { scheduleExtensionUpdateCheck } from './utils/extensionUpdater';
   ```

   Add `runExtensionUpdateCheck`:

   ```typescript
   import { runExtensionUpdateCheck, scheduleExtensionUpdateCheck } from './utils/extensionUpdater';
   ```

2. **Define `buildApplicationMenu()` just above `appMain()` (around line 2080).** Insert the following function before `async function appMain() {`:

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

3. **Remove the old mutation block from `appMain` / `app.whenReady`.** Inside `appMain()` (or the `app.whenReady` callback — whichever wraps this code), delete from the comment `// Get the existing menu` through the closing brace of `if (menu) { Menu.setApplicationMenu(menu); }`.

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

4. **Call `buildApplicationMenu()` in its place.** Immediately after the dock menu block (where the old mutation code was), add:

   ```typescript
   buildApplicationMenu();
   ```

5. **Verify TypeScript compiles.**

   ```bash
   cd ui/desktop
   npx tsc --noEmit
   ```

   Expected: zero errors. Fix any type errors before proceeding (usually `as const` assertions on role/type fields).

6. **Start the app and do a full menu walkthrough.**

   ```bash
   ENABLE_PLAYWRIGHT=1 npm run start-gui
   ```

   Check each menu in order. Every item below was walked and behaved as described.

   **BioRouter (macOS):**

   | Item | Expected behaviour |
   |------|--------------------|
   | About BioRouter | Opens About dialog |
   | Settings `⌘,` | Navigates to settings view |
   | Check for Updates… | Opens settings update section |
   | Check for Dependencies… | Triggers dependency modal (or "all good" if nothing missing) |
   | Check for Extension Updates | Runs silently in background |
   | Quit BioRouter `⌘Q` | Quits |

   **Go:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Home `⌘1` | Navigates to hub |
   | New Chat `⌘T` | Navigates to hub |
   | History `⌘2` | Navigates to sessions |
   | Workflows `⌘3` | Navigates to workflows |
   | Scheduler `⌘4` | Navigates to schedules |
   | Extensions `⌘5` | Navigates to extensions |
   | Skills `⌘6` | Navigates to skills |

   **File:**

   | Item | Expected behaviour |
   |------|--------------------|
   | New Chat `⌘T` | Navigates to hub |
   | New Window `⌘N` | Opens new window |
   | Open Directory… `⌘O` | Opens directory picker |
   | Recent Directories | Shows submenu with recent dirs |
   | Close Window `⌘W` | Closes window |
   | Focus BioRouter Window `⌥⌘G` | Shows and focuses window |

   **Edit:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Undo/Redo/Cut/Copy/Paste | Standard behavior |
   | Find submenu with Find… `⌘F` | Opens find bar |

   **Extensions:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Install Extension (.brxt)… | Navigates to extensions page |
   | Browse Extensions | Opens browser to baam.html |
   | Add Custom Extension… | Navigates to extensions page |
   | Check for Extension Updates | Triggers update check silently |

   **Providers:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Configure Providers… | Navigates to configure-providers route |
   | Switch Model… | Navigates to settings models section |
   | Reset Provider | Navigates to configure-providers route |

   **View:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Light Mode | Switches app to light theme |
   | Dark Mode | Switches app to dark theme |
   | System Mode | Follows OS appearance |
   | Reload / Force Reload / DevTools | Standard Electron behavior |
   | Zoom In/Out/Reset | Standard |
   | Toggle Fullscreen | Standard |

   **Help:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Biorouter Documentation | Opens docs URL in browser |
   | Report a Bug… | Opens GitHub issue URL in browser |
   | Request a Feature… | Opens GitHub issue URL in browser |
   | Version label (disabled) | Shows current version |

   **Window:**

   | Item | Expected behaviour |
   |------|--------------------|
   | Standard window items | Standard behavior |
   | Always on Top | Toggles float mode |

   **Tray (right-click):**

   | Item | Expected behaviour |
   |------|--------------------|
   | Home, New Chat, Settings | Navigate correctly |
   | Extensions, Skills | Navigate correctly |
   | Check for Updates | Navigates to update section |
   | Quit | Quits app |

   **Tray (left-click on macOS):** shows window silently, with no dropdown.

7. **Commit.**

   ```bash
   git add src/main.ts src/utils/extensionUpdater.ts
   git commit -m "feat(menu): full menu bar rebuild — Go, Extensions, Providers, View themes, Help"
   ```

## Self-review

Spec coverage against [the design spec](design.md):

| Spec item | Covered by |
|-----------|------------|
| BioRouter app menu (macOS) | `buildApplicationMenu`, macOS-only block |
| Go menu with `⌘1`–`⌘6` shortcuts | Task 5 |
| File menu with New Chat, New Window, Open Directory, Recent Dirs, Close, Focus | Task 5 |
| Edit + Window kept unchanged via roles | Task 5, with existing Find and Always on Top additions preserved |
| Extensions menu | Task 5 |
| Providers menu | Task 5 |
| View menu with theme modes + standard items | Task 5 |
| Help menu with docs, bug, feature, version | Task 5 |
| Tray left-click on macOS | Task 1 |
| Tray right-click menu | Task 4 |
| No new IPC (inter-process communication) channels | Confirmed — all use existing `set-view` and `theme-changed` |
| External links via `shell.openExternal` | Confirmed |

Placeholder scan: none found.

Type consistency:

| Symbol | Checked against |
|--------|-----------------|
| `'set-view'` channel | `App.tsx:527` listener |
| `'theme-changed'` channel | `ThemeContext.tsx:111` listener |
| View names (`''`, `'sessions'`, `'schedules'`, `'workflows'`, `'extensions'`, `'skills'`, `'settings'`, `'configure-providers'`) | All match routes in `App.tsx:594-639` |
| `theme-changed` payload `{ theme: 'light'\|'dark', useSystemTheme: boolean }` | `ThemeContext.tsx:100-106` handler |

## Related documentation

- [Menu bar and system tray design](design.md) — the spec this plan implements; read it first for the intended menu contents and the tray behaviour rationale.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later review of the same Electron main-process code, useful for what has changed since.
- [v1.72.1 bug fix batch](../desktop-ui-fixes/v1-72-1-bug-fix-batch.md) — a neighbouring desktop-shell plan from the same period, in the same task-and-checkbox format.
- [Agent browser debugging](../../desktop-ui/agent-browser-debugging.md) — how to drive the running app for a menu walkthrough like the one in Task 5.
- [Auto-update test checklist](../../releases/auto-update-test-checklist.md) — covers the update surfaces the "Check for Updates…" menu and tray items open.
