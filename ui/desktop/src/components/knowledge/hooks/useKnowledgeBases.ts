import { useCallback } from 'react';
import { createBase as apiCreate, deleteBase as apiDelete } from '../../../api';
import { useKnowledge } from '../KnowledgeContext';
import type { Manifest } from '../../../api/types.gen';

export function useKnowledgeBases() {
  const { refresh, setActiveKbId, activeKbId } = useKnowledge();

  const create = useCallback(
    async (id: string, name: string, color?: string): Promise<Manifest | undefined> => {
      const res = await apiCreate({
        throwOnError: true,
        body: { id, name, ...(color ? { color } : {}) },
      });
      await refresh();
      return res.data;
    },
    [refresh]
  );

  const remove = useCallback(
    async (id: string): Promise<void> => {
      await apiDelete({ throwOnError: true, path: { id } });
      if (activeKbId === id) setActiveKbId(null);
      await refresh();
    },
    [refresh, activeKbId, setActiveKbId]
  );

  return { create, remove };
}
