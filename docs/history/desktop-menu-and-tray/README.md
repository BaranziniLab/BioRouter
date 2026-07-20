# Desktop menu bar and system tray

This folder documents one completed piece of work: the 2026-05-07 replacement of Electron's patched default application menu with a single custom template (BioRouter/Go/File/Extensions/Providers/View/Help), plus a fix making the macOS tray icon show the window on left-click instead of opening a dropdown. It did happen — the design was written and the plan was executed, and `buildApplicationMenu()` is present in `ui/desktop/src/main.ts` in the shipping app. Both files are kept for the record, not as current guidance: the live menu has drifted from the listings here, which omit the Knowledge and Apps routes the app gained later, and every line number the plan cites points at the 2026-05-07 tree. **For the menu as it stands today, read `ui/desktop/src/main.ts`.**

Come here when you want the reasoning behind the menu's shape — why the tray left-click behaves as it does, why navigation reuses the existing `set-view` IPC channel rather than adding new ones, or what the original menu inventory was before it drifted. Go elsewhere if you want anything broader: neighbouring folders under `docs/history/` cover other desktop shell work of the same period ([notification-redesign](../notification-redesign/notification-surface-design.md) for toasts and inline alerts, [desktop-ui-fixes](../desktop-ui-fixes/v1-72-1-bug-fix-batch.md) for the v1.72.1 batch), and live developer documentation for the desktop app lives in [`docs/desktop-ui/`](../../desktop-ui/README.md), not here.

## Documents

| Document | What it covers |
|----------|----------------|
| [Menu bar and system tray design](design.md) | The design spec for the custom Electron application menu and for the tray's left-click and right-click behaviour, with the full item-and-shortcut listing as designed on 2026-05-07. |
| [Menu bar and system tray implementation plan](plan.md) | The task-by-task build plan (Tasks 1–5) that turned the design into code, carrying the exact edits, URLs, and the manual menu walkthrough used to verify it. |

Read the design first for what the finished menu contains and why; the plan is its operational companion.

## Related documentation

- [History index](../README.md) — the full catalogue of completed and abandoned work, for locating the neighbouring folders named above.
- [Desktop reliability defects](../subsystem-reviews-2026/desktop-reliability-defects.md) — a later review of the same Electron main-process surface, and the best guide to what has changed since.
- [Agent browser debugging](../../desktop-ui/agent-browser-debugging.md) — how to drive the running desktop app, if you need to check current menu or tray behaviour by hand.
- [Notification surface design](../notification-redesign/notification-surface-design.md) — the neighbouring design for another desktop shell surface driven from the main process.
