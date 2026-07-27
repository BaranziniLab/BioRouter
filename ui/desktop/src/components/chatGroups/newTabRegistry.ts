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
/** Cmd+T arrived with NO provider mounted; the next mounting provider consumes it. */
let pendingNewTab = false;
/**
 * Cmd+T was dispatched to a LIVE provider but its tab has not committed yet.
 *
 * Dispatching is not arriving: `handler()` queues a reducer update, and until
 * React commits it the layout still reads zero tabs. The empty-pair redirect
 * (issue #38) can run in exactly that gap — its effect from a zero-tab commit
 * calls hasPendingNewTab() at effect run time, after the dispatch — so the
 * handled request must stay observable or the keystroke is bounced Home and
 * the freshly dispatched tab dies with the unmounting provider. The provider
 * acknowledges once it commits a nonzero tab count (acknowledgeNewTabCommit),
 * which is the moment the request has become a real tab.
 */
let handledNewTab = false;

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
    // Mark BEFORE dispatching: the redirect gate must already see the request
    // if a stale zero-tab effect runs between this dispatch and the commit
    // that lands the tab (see handledNewTab above).
    handledNewTab = true;
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
 *
 * A still-unacknowledged handled request folds in as a safety net: a dispatch
 * that raced the provider's unmount died with the discarded reducer state, so
 * the next mounting provider honours it exactly as if no provider had been
 * there — the user pressed Cmd+T and a tab must appear. In the ordinary flow
 * this arm is dead: the provider that took the dispatch commits the tab and
 * acknowledges long before another mount's consume could run.
 */
export function consumePendingNewTab(): boolean {
  const pending = pendingNewTab || handledNewTab;
  pendingNewTab = false;
  handledNewTab = false;
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
 * a navigation (or worse, bounce straight back Home). A request dispatched to
 * a live provider stays visible here too, until the provider commits the tab.
 */
export function hasPendingNewTab(): boolean {
  return pendingNewTab || handledNewTab;
}

/**
 * The provider reports every committed tab count; a nonzero count means any
 * dispatched Cmd+T has become a real tab and stops being observable. Called
 * from an effect keyed on the provider's state, so it runs on every commit —
 * AFTER the child redirect effect from the same commit has taken its peek.
 */
export function acknowledgeNewTabCommit(tabCount: number): void {
  if (tabCount > 0) handledNewTab = false;
}

/** Tests only — the singleton must not leak across cases. */
export function resetNewTabRegistry(): void {
  handler = null;
  pendingNewTab = false;
  handledNewTab = false;
}
