/**
 * The Cmd+T hand-off between App.tsx and ChatGroupsProvider.
 *
 * The same seam, and deliberately the same shape, as closeActiveTabRegistry —
 * for the same reason. Cmd+T is an Electron MENU accelerator (the built menu
 * dump shows "Go > New Chat" holding CmdOrCtrl+T), so the keystroke arrives as
 * IPC and is answered at the app root; but ChatGroupsProvider, which owns the
 * tab state, is mounted only under /pair. The root cannot read tab state out of
 * context, so the provider registers a claim while it is mounted and the root
 * asks.
 *
 * It is a sibling file rather than a second command bolted into
 * closeActiveTabRegistry because the two commands differ in the part that
 * matters — their FALLBACK. "No handler" means "close the window" for Cmd+W,
 * but for Cmd+T it means "you are not on /pair yet", which is a thing to fix by
 * navigating rather than a different outcome. Hence `pendingNewTab`, which has
 * no analogue on the close side: Cmd+T from Settings must both go to /pair AND
 * arrive with a fresh tab, the way Cmd+T in a browser opens a tab from whatever
 * page you were on.
 */
export type NewTabHandler = () => void;

let handler: NewTabHandler | null = null;
let pendingNewTab = false;

export function registerNewTab(next: NewTabHandler): () => void {
  handler = next;
  // Same disposer discipline as closeActiveTabRegistry: only clear the handler
  // if it is still the one we installed, so React's mount-B-then-dispose-A
  // order under StrictMode cannot leave the registry empty.
  return () => {
    if (handler === next) handler = null;
  };
}

/**
 * True when a tab was opened; false when there was no provider to open it.
 *
 * On false the request is REMEMBERED, not dropped, and the caller is expected
 * to navigate to /pair — where the mounting provider consumes it. Returning
 * false and silently forgetting would make Cmd+T on Settings merely a
 * navigation, which is not what the key means anywhere else.
 */
export function requestNewTab(): boolean {
  if (handler) {
    handler();
    return true;
  }
  pendingNewTab = true;
  return false;
}

/**
 * Consume-once, by the provider on mount.
 *
 * It must be consume-once rather than a readable flag: StrictMode mounts,
 * unmounts and remounts the provider, running this effect twice. A plain
 * boolean read would open two blank tabs for one Cmd+T.
 */
export function consumePendingNewTab(): boolean {
  const pending = pendingNewTab;
  pendingNewTab = false;
  return pending;
}

/**
 * Non-consuming PEEK, for gates only — never a substitute for the consume.
 *
 * The empty-/pair redirect (issue #38) runs in a CHILD of ChatGroupsProvider,
 * so its effect fires BEFORE the provider's consuming effect on mount. A
 * Cmd+T-from-Settings arrives as zero tabs + a pending request; the redirect
 * must see the request and stand down WITHOUT consuming it, or the provider
 * would find nothing to cash in and the keystroke would silently downgrade to
 * a navigation (or worse, bounce straight back Home).
 */
export function hasPendingNewTab(): boolean {
  return pendingNewTab;
}

/** Tests only — the singleton must not leak across cases. */
export function resetNewTabRegistry(): void {
  handler = null;
  pendingNewTab = false;
}
