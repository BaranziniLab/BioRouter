import { describe, expect, it, vi, afterEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';

const mocks = vi.hoisted(() => ({
  getSession: vi.fn(),
  getSessionExtensions: vi.fn(),
  cancelTurn: vi.fn(),
}));

vi.mock('../../api', async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>;
  return {
    ...actual,
    getSession: mocks.getSession,
    getSessionExtensions: mocks.getSessionExtensions,
    cancelTurn: mocks.cancelTurn,
  };
});

import { useSubagentSession } from './useSubagentSession';

describe('useSubagentSession', () => {
  afterEach(() => vi.clearAllMocks());

  it('loads lineage, grants, and the spawn-context record for sub_agent sessions', async () => {
    mocks.getSession.mockResolvedValue({
      data: {
        id: 'child-1',
        session_type: 'sub_agent',
        parent_session_id: 'parent-1',
        conversation: [
          {
            role: 'user',
            created: 1,
            content: [{ type: 'text', text: '## Subagent spawn context\ntask: count' }],
            metadata: {
              userVisible: true,
              agentVisible: false,
              provenance: { kind: 'spawn_context' },
            },
          },
        ],
      },
    });
    mocks.getSessionExtensions.mockResolvedValue({
      data: { extensions: [{ type: 'platform', name: 'developer' }] },
    });

    const { result } = renderHook(() => useSubagentSession('child-1'));
    await waitFor(() => expect(result.current.isSubagent).toBe(true));
    expect(result.current.parentSessionId).toBe('parent-1');
    expect(result.current.extensions).toEqual(['developer']);
    expect(result.current.spawnContext).toContain('count');

    // Stop posts the addressable cancel — the chain Task 33 made real.
    await result.current.stop();
    expect(mocks.cancelTurn).toHaveBeenCalledWith(
      expect.objectContaining({ body: { session_id: 'child-1' } })
    );
  });

  it('is inert for ordinary sessions', async () => {
    mocks.getSession.mockResolvedValue({ data: { id: 's', session_type: 'user' } });
    const { result } = renderHook(() => useSubagentSession('s'));
    await waitFor(() => expect(mocks.getSession).toHaveBeenCalled());
    expect(result.current.isSubagent).toBe(false);
    expect(mocks.getSessionExtensions).not.toHaveBeenCalled();
  });
});
