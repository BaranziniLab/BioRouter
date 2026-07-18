import { useState, useEffect } from 'react';
import { getTools } from '../../api';

// TODO(Douwe): return this as part of the start agent request
/**
 * The agent's tool count for a session.
 *
 * `agentReady` is load-bearing, not decorative. Tools come from extensions, and
 * with progressive conversation loading the transcript paints ~4.6s before the
 * extensions are up — so a fetch keyed on `sessionId` alone reliably reads zero
 * tools and, with no other dependency to re-trigger it, caches that zero for the
 * life of the chat. The "too many tools" alert would then simply never fire.
 * Refetching when readiness flips is what keeps the count true.
 */
export const useToolCount = (sessionId: string, agentReady: boolean = true) => {
  const [toolCount, setToolCount] = useState<number | null>(null);

  useEffect(() => {
    // Nothing to count until the extensions that provide the tools exist.
    if (!agentReady) return;

    let cancelled = false;
    const fetchTools = async () => {
      try {
        const response = await getTools({ query: { session_id: sessionId } });
        if (cancelled) return;
        setToolCount(response.error || !response.data ? 0 : response.data.length);
      } catch (err) {
        if (cancelled) return;
        console.error('Error fetching tools:', err);
        setToolCount(0);
      }
    };

    fetchTools();
    // Closing the tab while extensions are still starting must not land a
    // setState on an unmounted component.
    return () => {
      cancelled = true;
    };
  }, [sessionId, agentReady]);

  return toolCount;
};
