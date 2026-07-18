# Menu bar and system tray design

> **What this is.** The design spec for BioRouter's custom Electron application menu (BioRouter/Go/File/Extensions/Providers/View/Help) and for the system tray's left-click and right-click behaviour.
> **Status:** Historical record — written 2026-05-07 and implemented. The menu template landed as `buildApplicationMenu()` in `ui/desktop/src/main.ts`, and the specified items (including "Check for Extension Updates") are present in the shipping app. The live menu has since drifted from this listing; see [What has drifted since](#what-has-drifted-since).
> **Audience:** developers working on the Electron desktop shell.

Before this change, BioRouter shipped Electron's default application menu with a handful of items patched onto it at startup, so the app's own surfaces — chat history, workflows, the scheduler, extensions, skills, providers — had no menu-bar route at all, and the macOS tray icon opened a dropdown on left-click instead of simply showing the window. This spec defines the menu bar and tray as a single deliberate structure. The task-by-task implementation is in [the implementation plan](plan.md), which is the companion to this document; the plan carries the exact code, the concrete URLs, and the verification walkthrough.

## Approach

All changes are in `ui/desktop/src/main.ts` and `ui/desktop/src/utils/autoUpdater.ts`. The menu bar is rebuilt with `Menu.buildFromTemplate` and set via `Menu.setApplicationMenu`. Tray behaviour is updated on the `tray` instance already managed in `autoUpdater.ts`. Navigation items send IPC messages to the focused renderer window (`webContents.send('set-view', routeName)`), matching the existing pattern used for Settings.

The technologies involved are Electron's `Menu`, `MenuItem` and `Tray` classes, plus `shell.openExternal` for external links.

> **Note.** Shortcuts below are written as macOS glyphs. The implementation plan declares them as `CmdOrCtrl+…` accelerators, so Windows and Linux receive the `Ctrl` equivalent of the same key.

## Menu bar structure

### BioRouter (app menu — macOS only)

| Item | Shortcut | Notes |
|------|----------|-------|
| About BioRouter | — | Existing item |
| — separator — | | |
| Settings | `⌘,` | |
| — separator — | | |
| Check for Updates… | — | |
| Check for Dependencies… | — | |
| Check for Extension Updates | — | |
| — separator — | | |
| Quit BioRouter | `⌘Q` | |

### Go

| Item | Shortcut |
|------|----------|
| Home | `⌘1` |
| New Chat | `⌘T` |
| History | `⌘2` |
| — separator — | |
| Workflows | `⌘3` |
| Scheduler | `⌘4` |
| — separator — | |
| Extensions | `⌘5` |
| Skills | `⌘6` |

### File

| Item | Shortcut | Notes |
|------|----------|-------|
| New Chat | `⌘T` | |
| New Window | `⌘N` | |
| — separator — | | |
| Open Directory… | `⌘O` | |
| Recent Directories ▶ | — | Submenu, existing |
| — separator — | | |
| Close Window | `⌘W` | |
| Focus BioRouter Window | `⌥⌘G` | |

### Extensions

| Item | Notes |
|------|-------|
| Install Extension (.brxt)… | |
| Browse Extensions | Opens `http://biorouter.ucsf.edu/baam` |
| Add Custom Extension… | |
| — separator — | |
| Check for Extension Updates | |

### Providers

| Item |
|------|
| Configure Providers… |
| Switch Model… |
| Reset Provider |

### View

| Item | Notes |
|------|-------|
| Light Mode | |
| Dark Mode | |
| System Mode | |
| — separator — | |
| Standard Electron view items | Reload, Force Reload, Toggle DevTools, Zoom In/Out/Reset, Toggle Fullscreen |

### Help

| Item | Notes |
|------|-------|
| Biorouter Documentation | Opens `http://biorouter.ucsf.edu/docs` |
| — separator — | |
| Report a Bug… | Opens GitHub issues |
| Request a Feature… | Opens GitHub discussions or issues |
| — separator — | |
| v{version} | Disabled label showing current app version |

This spec names the bug and feature destinations only by kind. The exact URLs the implementation used are given verbatim in [the implementation plan](plan.md) under Task 5.

### Edit and Window

Kept unchanged — macOS requires these for standard system behavior (copy/paste shortcuts, window management).

## System tray

**Left click (macOS):** Show window silently — no dropdown, brings BioRouter to front immediately. Matches existing Windows behavior (already implemented via `tray.on('click', showWindow)`). Fix: add the same `tray.on('click', showWindow)` handler for `darwin`.

**Right click — context menu:**

| Item |
|------|
| Home |
| New Chat |
| Settings |
| — separator — |
| Extensions |
| Skills |
| — separator — |
| Check for Updates |
| — separator — |
| Quit |

## Navigation IPC pattern

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

## Files changed

| File | Change |
|------|--------|
| `ui/desktop/src/main.ts` | Full menu bar rebuild: Go, File, Extensions, Providers, View (+ theme items), Help. Tray left-click fix for macOS. |
| `ui/desktop/src/utils/autoUpdater.ts` | Update `updateTrayMenu()` with the new right-click context menu items. |

## Out of scope

- No changes to the renderer routing logic
- No new IPC channels (reuse `set-view`, `set-theme`, existing check/install handlers)
- Windows and Linux menu bars follow the same structure (BioRouter app menu is macOS-only; other platforms get Go/File/Extensions/Providers/View/Help)

## What has drifted since

Read the listings above as a record of the 2026-05-07 design, not as a current reference. The Go menu enumerated here omits the Knowledge and Apps sidebar routes that the app gained later, so a reader treating this page as an inventory of navigable surfaces will come up short. Consult `ui/desktop/src/main.ts` for the menu as it stands today.

## Related documentation

- [Menu bar and system tray implementation plan](plan.md) — the task-by-task companion to this spec, with the exact code, URLs and menu walkthrough used to build it.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later review of the same Electron main-process surface this spec touches.
- [Notification surface design](../notification-redesign/notification-surface-design.md) — the neighbouring design for another desktop shell surface driven from the main process.
- [Agent browser debugging](../../desktop-ui/agent-browser-debugging.md) — how to drive the running desktop app when verifying menu and tray behaviour by hand.
