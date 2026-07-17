/**
 * The Cmd+W hand-off between App.tsx and ChatGroupsProvider.
 *
 * Cmd+W has exactly one listener per renderer, and it lives at the app root
 * (App.tsx) because it must answer even on routes that have no tabs at all —
 * Settings, Extensions, the Hub — where Cmd+W must still close the WINDOW like
 * any other macOS app. ChatGroupsProvider, meanwhile, is mounted only under
 * /pair, so the root cannot simply read the tab state out of context.
 *
 * So the provider registers a claim function while it is mounted, and the root
 * asks. `true` means "I closed a tab, the keystroke is spent"; `false` means
 * "nothing here to close" and the root closes the window instead.
 *
 * A module singleton is the right scope: one renderer is one window, and the
 * provider is mounted at most once within it. The register call returns its own
 * disposer and only clears the handler if it is still the one it installed, so a
 * StrictMode double-mount (new handler installed, old one disposed after) cannot
 * leave the registry empty.
 */
export type CloseActiveTabHandler = () => boolean;

let handler: CloseActiveTabHandler | null = null;

export function registerCloseActiveTab(next: CloseActiveTabHandler): () => void {
  handler = next;
  return () => {
    if (handler === next) handler = null;
  };
}

/** True when a tab was closed; false when there was nothing to close. */
export function closeActiveTab(): boolean {
  return handler ? handler() : false;
}

/** Tests only — the singleton must not leak across cases. */
export function resetCloseActiveTabRegistry(): void {
  handler = null;
}
