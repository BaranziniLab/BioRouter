import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KnowledgeProvider, useKnowledge } from './KnowledgeContext';

const mocks = vi.hoisted(() => ({
  listBases: vi.fn(),
  getActive: vi.fn(),
  setActive: vi.fn(),
}));

vi.mock('../../api', () => ({
  listBases: mocks.listBases,
  getActive: mocks.getActive,
  setActive: mocks.setActive,
}));

function base(id: string) {
  return { id, name: id, color: '#cf6d47', created_at: '', schema_version: 1 };
}

function Probe() {
  const { primaryKbId, hiddenKbIds, setPrimaryKbId, toggleKbHidden } = useKnowledge();
  return (
    <div>
      <span data-testid="primary">{primaryKbId ?? 'none'}</span>
      <span data-testid="hidden">{hiddenKbIds.join(',') || 'none'}</span>
      <button type="button" onClick={() => setPrimaryKbId('beta')}>
        make beta primary
      </button>
      <button type="button" onClick={() => toggleKbHidden('alpha')}>
        toggle alpha
      </button>
    </div>
  );
}

function renderProvider() {
  return render(
    <KnowledgeProvider sessionId="chat-1">
      <Probe />
    </KnowledgeProvider>
  );
}

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.listBases.mockResolvedValue({ data: [base('alpha'), base('beta')] });
  mocks.getActive.mockResolvedValue({
    data: { kb_ids: ['alpha'], primary_kb: 'alpha', active_kb: 'alpha', hidden_kbs: ['beta'] },
  });
  mocks.setActive.mockResolvedValue({
    data: { kb_ids: ['alpha', 'beta'], primary_kb: 'beta', active_kb: 'beta', hidden_kbs: [] },
  });
});

describe('KnowledgeContext', () => {
  it('hydrates the primary and the set from the daemon', async () => {
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));
    expect(screen.getByTestId('hidden')).toHaveTextContent('beta');
  });

  // The invariant, at the UI edge: the primary must be a member of the set, so
  // "make primary" on a base that is toggled off is ONE request that does both
  // and is validated by the daemon against the state it produces.
  it('makes a base primary and adds it to the chat in the same request', async () => {
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));

    await waitFor(() => expect(mocks.setActive).toHaveBeenCalled());
    const body = mocks.setActive.mock.calls.at(-1)?.[0]?.body;
    expect(body.primary_kb).toBe('beta');
    expect(body.hidden_kbs).toEqual([]);
    expect(body.session_id).toBe('chat-1');
  });

  // The promote/clear rule lives in the daemon. If the UI re-derived it, the
  // two would drift and the chat chip would disagree with the model.
  it('adopts the primary the daemon reports back', async () => {
    mocks.setActive.mockResolvedValue({
      data: { kb_ids: ['alpha'], primary_kb: 'alpha', active_kb: 'alpha', hidden_kbs: ['beta'] },
    });
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));

    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));
  });

  // Toggling the primary's own membership off is the case the repair exists
  // for. A set-only edit must therefore state NO primary: echoing the current
  // one back would be a primary outside the resulting set, which the daemon
  // rejects with a 400 — the badge would vanish instead of moving.
  it('states no primary on a set-only edit, so the daemon can promote', async () => {
    mocks.setActive.mockResolvedValue({
      data: { kb_ids: ['beta'], primary_kb: 'beta', active_kb: 'beta', hidden_kbs: ['alpha'] },
    });
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    await userEvent.click(screen.getByRole('button', { name: 'toggle alpha' }));

    await waitFor(() => expect(mocks.setActive).toHaveBeenCalled());
    const body = mocks.setActive.mock.calls.at(-1)?.[0]?.body;
    expect(body.primary_kb).toBeUndefined();
    expect(body.clear_primary).toBe(false);
    expect(body.hidden_kbs).toEqual(['alpha', 'beta']);
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('beta'));
  });
});
