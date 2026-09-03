import { useEffect, useMemo, useState } from 'react';
import { getSession, type Message, type Session } from '../api';
import { userActionHeaders } from '../utils/userAction';
import { sessionTodoItems, todoMutationRevision, type TodoItem } from '../utils/sessionTodos';

type TodoSnapshot = { sessionId: string; items: TodoItem[]; loading: boolean; error: boolean };

export function useSessionTodos(
  sessionId: string,
  session: Session | undefined,
  messages: Message[],
  open: boolean
) {
  const revision = useMemo(() => (open ? todoMutationRevision(messages) : ''), [messages, open]);
  const extensionData = session?.id === sessionId ? session.extension_data : undefined;
  const initialItems = useMemo(() => sessionTodoItems(extensionData), [extensionData]);
  const [retry, setRetry] = useState(0);
  const [snapshot, setSnapshot] = useState<TodoSnapshot>({
    sessionId: '',
    items: [],
    loading: false,
    error: false,
  });

  useEffect(() => {
    if (!open || !sessionId) return;
    const controller = new AbortController();
    setSnapshot((previous) => ({
      sessionId,
      items: previous.sessionId === sessionId ? previous.items : initialItems,
      loading: true,
      error: false,
    }));
    // Coalesce parallel To Do results; no polling and no fetches for streamed prose.
    const timer = setTimeout(async () => {
      try {
        const headers = await userActionHeaders();
        if (controller.signal.aborted) return;
        const response = await getSession({
          path: { session_id: sessionId },
          query: { metadata_only: true },
          headers,
          signal: controller.signal,
        });
        if (controller.signal.aborted) return;
        if (response.error || !response.data || response.data.id !== sessionId)
          throw new Error('Summary unavailable');
        setSnapshot({
          sessionId,
          items: sessionTodoItems(response.data.extension_data),
          loading: false,
          error: false,
        });
      } catch {
        if (!controller.signal.aborted)
          setSnapshot((previous) => ({ ...previous, loading: false, error: true }));
      }
    }, 80);
    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [sessionId, open, revision, initialItems, retry]);

  return {
    ...(snapshot.sessionId === sessionId
      ? snapshot
      : { items: initialItems, loading: open && !!sessionId, error: false }),
    refresh: () => setRetry((value) => value + 1),
  };
}
