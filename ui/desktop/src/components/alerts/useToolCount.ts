import { useState, useEffect } from 'react';
import { getTools } from '../../api';
import { CATALOG_CHANGED_EVENT } from '../../utils/catalogSubscription';

// TODO(Douwe): return this as part of the start agent request
/**
 * The agent's tool count for a session.
 *
 * `agentReady` is load-bearing, not decorative. Tools come from extensions, and
 * with progressive conversation loading the transcript paints ~4.6s before the
 * extensions are up — so a fetch keyed on `sessionId` alone reliably reads zero
 * tools and, with no other dependency to re-trigger it, caches that zero for the
 * life of the chat. The "too many tools" alert would then simply never fire.
 * Refetch when readiness or the live catalog changes; discard superseded
 * responses so a slow pre-install query cannot overwrite the current count.
 */
export const useToolCount = (sessionId: string, agentReady: boolean = true) => {
  const [toolCount, setToolCount] = useState<number | null>(null);

  useEffect(() => {
    setToolCount(null);
    // Nothing to count until the extensions that provide the tools exist.
    if (!agentReady || !sessionId) return;

    let cancelled = false;
    let revision = 0;
    let controller: AbortController | undefined;
    const fetchTools = async () => {
      const requestRevision = ++revision;
      controller?.abort();
      controller = new AbortController();
      try {
        const response = await getTools({
          query: { session_id: sessionId },
          signal: controller.signal,
        });
        if (cancelled || requestRevision !== revision) return;
        setToolCount(response.error || !response.data ? 0 : response.data.length);
      } catch (err) {
        if (cancelled || requestRevision !== revision) return;
        console.error('Error fetching tools:', err);
        setToolCount(0);
      }
    };

    fetchTools();
    window.addEventListener(CATALOG_CHANGED_EVENT, fetchTools);
    // Closing the tab while extensions are still starting must not land a
    // setState on an unmounted component.
    return () => {
      cancelled = true;
      controller?.abort();
      window.removeEventListener(CATALOG_CHANGED_EVENT, fetchTools);
    };
  }, [sessionId, agentReady]);

  return toolCount;
};
