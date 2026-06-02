// ui/desktop/src/components/knowledge/hooks/usePagePreview.ts
import { useEffect, useState } from 'react';
import { getPageBody } from '../../../api';

export interface UsePagePreviewResult {
  content: string | null;
  loading: boolean;
  error: string | null;
}

export function usePagePreview(kbId: string | null, path: string | null): UsePagePreviewResult {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!kbId || !path) {
      setContent(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    (async () => {
      try {
        const res = await getPageBody({ path: { id: kbId }, query: { path }, throwOnError: true });
        if (!cancelled) setContent(res.data?.content ?? null);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setContent(null);
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kbId, path]);

  return { content, loading, error };
}
