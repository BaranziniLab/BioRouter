import { catalogChanges } from '../api';
import type { CatalogChanged, CatalogDelta } from '../api';

/**
 * Issue #112. The renderer's ear on the extension catalogue.
 *
 * An install can succeed on disk while four inventories keep serving stale
 * answers — `ConfigContext.extensionsList`, the Settings list, the composer's
 * picker, and the running agent's own extension manager. Each was repaired by
 * whichever code path happened to write, so an install from *outside* the GUI
 * (`biorouter extension install` in a terminal, an agent, a deep link, a hand
 * edit) repaired none of them and the user needed a new chat or a restart.
 *
 * This is a plain function rather than a hook on purpose: the signal has to
 * reach consumers that are not React components — the `catalog:changed` window
 * event below is what Worktree 5's skill inventory listens on — and a
 * subscription that lived in a provider's state could only ever serve its own
 * tree.
 *
 * ⚠ **The revision is the contract, not the payload.** A consumer that applies
 * `changes` and never refetches drifts the first time two changes race, or the
 * first time it falls further behind than the daemon's buffer holds. Treat a
 * delta as "something moved, go and look", and treat `truncated` as an order to
 * refetch rather than as a warning.
 */

/** The window event non-React consumers listen for. */
export const CATALOG_CHANGED_EVENT = 'catalog:changed';

export interface CatalogSubscriptionOptions {
  /** Called for every delta that reports a change. */
  onChange: (delta: CatalogDelta) => void;
  /** Injected in tests. Defaults to the generated client. */
  poll?: (since: number) => Promise<CatalogDelta | undefined>;
  /** Injected in tests. */
  sleep?: (ms: number) => Promise<void>;
  /** How long to wait after a failed poll before trying again. */
  retryDelayMs?: number;
}

const DEFAULT_RETRY_MS = 5000;

const defaultPoll = async (since: number): Promise<CatalogDelta | undefined> => {
  const response = await catalogChanges({ query: { since } });
  return response.data;
};

const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Start following the catalogue. Returns a function that stops it.
 *
 * The loop is a long poll: the daemon parks the request until the revision
 * moves or ~25s elapse, so an idle app is not re-establishing a request every
 * few seconds and a change is seen within a network round trip.
 */
export function subscribeToCatalog(options: CatalogSubscriptionOptions): () => void {
  const poll = options.poll ?? defaultPoll;
  const sleep = options.sleep ?? defaultSleep;
  const retryDelayMs = options.retryDelayMs ?? DEFAULT_RETRY_MS;

  let stopped = false;
  let since = 0;

  const run = async () => {
    while (!stopped) {
      let delta: CatalogDelta | undefined;
      try {
        delta = await poll(since);
      } catch {
        // The daemon is restarting, or the machine slept. Back off rather than
        // spinning; the next successful poll re-establishes the cursor.
        await sleep(retryDelayMs);
        continue;
      }
      if (stopped) return;
      if (!delta) {
        await sleep(retryDelayMs);
        continue;
      }

      // ⚠ A revision LOWER than the one we hold means the daemon restarted and
      // its counter went back to zero. Nothing was "undone" — our cursor is
      // simply meaningless now, and holding it would park us forever on a
      // number the daemon will take minutes to climb back to.
      const restarted = delta.revision < since;
      const moved = delta.revision !== since;
      since = delta.revision;

      const changed = (delta.changes ?? []).length > 0 || delta.truncated === true;
      if (restarted || (moved && changed)) {
        options.onChange(delta);
        dispatchCatalogChanged(delta);
      }
    }
  };

  void run();
  return () => {
    stopped = true;
  };
}

function dispatchCatalogChanged(delta: CatalogDelta) {
  if (typeof window === 'undefined') return;
  window.dispatchEvent(new CustomEvent<CatalogDelta>(CATALOG_CHANGED_EVENT, { detail: delta }));
}

/**
 * Every extension key a delta touched, deduplicated.
 *
 * Callers use this to decide what to *offer* — "this chat can now use
 * BiorOffice" — never to decide what is installed. That question is answered by
 * refetching.
 */
export function changedExtensionKeys(delta: CatalogDelta): string[] {
  const keys = new Set<string>();
  for (const change of delta.changes ?? []) {
    for (const extension of change.extensions ?? []) {
      keys.add(extension.key);
    }
  }
  return [...keys];
}

/** The extensions a delta reports as newly installed and enabled. */
export function newlyInstalledExtensions(
  delta: CatalogDelta
): Array<{ key: string; name: string }> {
  const found = new Map<string, { key: string; name: string }>();
  for (const change of (delta.changes ?? []) as CatalogChanged[]) {
    for (const extension of change.extensions ?? []) {
      if (extension.change === 'added' && extension.enabled) {
        found.set(extension.key, { key: extension.key, name: extension.name });
      }
    }
  }
  return [...found.values()];
}
