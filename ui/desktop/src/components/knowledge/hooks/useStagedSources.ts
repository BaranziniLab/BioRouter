import { useCallback, useState } from 'react';

export type StagedSource =
  | {
      kind: 'file';
      id: string;
      file: File;
      label?: string;
      status: 'pending' | 'ingesting' | 'done' | 'error';
      error?: string;
    }
  | {
      kind: 'path';
      id: string;
      path: string;
      label: string;
      status: 'pending' | 'ingesting' | 'done' | 'error';
      error?: string;
    }
  | {
      kind: 'url';
      id: string;
      url: string;
      status: 'pending' | 'ingesting' | 'done' | 'error';
      error?: string;
    }
  | {
      kind: 'text';
      id: string;
      text: string;
      title?: string;
      status: 'pending' | 'ingesting' | 'done' | 'error';
      error?: string;
    };

export function useStagedSources() {
  const [items, setItems] = useState<StagedSource[]>([]);

  const add = useCallback((s: StagedSource) => setItems((xs) => [...xs, s]), []);

  const remove = useCallback((id: string) => setItems((xs) => xs.filter((s) => s.id !== id)), []);

  const update = useCallback(
    (id: string, patch: Partial<StagedSource>) =>
      setItems((xs) => xs.map((s) => (s.id === id ? ({ ...s, ...patch } as StagedSource) : s))),
    []
  );

  const clear = useCallback(() => setItems([]), []);

  return { items, add, remove, update, clear };
}
