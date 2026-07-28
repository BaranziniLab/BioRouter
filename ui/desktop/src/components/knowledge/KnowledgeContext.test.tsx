import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KnowledgeProvider, useKnowledge } from './KnowledgeContext';

/** A promise the test resolves by hand, so "after the response settled" is a fact, not a race. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Resolve/reject a pending sync and let React apply every queued state update. */
async function settle(action: () => void) {
  await act(async () => {
    action();
    await Promise.resolve();
    await Promise.resolve();
  });
}

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
  const { primaryKbId, hiddenKbIds, visibleBases, setPrimaryKbId, toggleKbHidden } = useKnowledge();
  return (
    <div>
      <span data-testid="primary">{primaryKbId ?? 'none'}</span>
      <span data-testid="hidden">{hiddenKbIds.join(',') || 'none'}</span>
      <span data-testid="visible">{visibleBases.map((b) => b.id).join(',') || 'none'}</span>
      <button type="button" onClick={() => setPrimaryKbId('beta')}>
        make beta primary
      </button>
      <button type="button" onClick={() => setPrimaryKbId('alpha')}>
        make alpha primary
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
    const calls = mocks.setActive.mock.calls;
    const body = calls[calls.length - 1]?.[0]?.body;
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
    const calls = mocks.setActive.mock.calls;
    const body = calls[calls.length - 1]?.[0]?.body;
    expect(body.primary_kb).toBeUndefined();
    expect(body.clear_primary).toBe(false);
    expect(body.hidden_kbs).toEqual(['alpha', 'beta']);
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('beta'));
  });

  // Mixed versions: a new renderer against a daemon that predates `primary_kb`.
  // Its POST answer carries only the deprecated `active_kb` mirror, and reading
  // just `primary_kb` turns a *successful* write into "there is no primary" —
  // which the design forbids inventing back, so the pointer would stay lost.
  // Every other reader already falls back to the mirror; this one must too.
  it('reads the deprecated active_kb mirror out of a successful POST', async () => {
    const pending = deferred<unknown>();
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    mocks.setActive.mockReturnValue(pending.promise);
    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));
    await waitFor(() => expect(mocks.setActive).toHaveBeenCalled());

    await settle(() =>
      pending.resolve({
        data: { kb_ids: ['alpha', 'beta'], active_kb: 'beta', hidden_kbs: [] },
      })
    );

    expect(screen.getByTestId('primary')).toHaveTextContent('beta');
  });
});
