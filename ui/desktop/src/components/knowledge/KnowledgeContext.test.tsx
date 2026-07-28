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
  const { primaryKbId, hiddenKbIds, visibleBases, setPrimaryKbId, toggleKbHidden, refresh } =
    useKnowledge();
  return (
    <div>
      <span data-testid="primary">{primaryKbId ?? 'none'}</span>
      <span data-testid="hidden">{hiddenKbIds.join(',') || 'none'}</span>
      <span data-testid="visible">{visibleBases.map((b) => b.id).join(',') || 'none'}</span>
      <button type="button" onClick={() => void refresh()}>
        refresh
      </button>
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

  // The primary is the KB-less write target. Between the click that removes it
  // from the chat and the daemon's repair there must be no window in which the
  // renderer still names it — IngestPanel passes `primaryKbId` explicitly, so a
  // stale one aims a digest at a base this session no longer includes.
  it('never keeps a primary the user just removed from the set', async () => {
    const pending = deferred<unknown>();
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    mocks.setActive.mockReturnValue(pending.promise);
    await userEvent.click(screen.getByRole('button', { name: 'toggle alpha' }));
    await waitFor(() => expect(mocks.setActive).toHaveBeenCalled());

    expect(screen.getByTestId('hidden').textContent).toBe('alpha,beta');
    expect(screen.getByTestId('primary').textContent).toBe('none');

    await settle(() =>
      pending.resolve({
        data: { kb_ids: ['beta'], primary_kb: 'beta', active_kb: 'beta', hidden_kbs: ['alpha'] },
      })
    );

    // …and then the whole repair is adopted, set included, so the primary is a
    // member of the visible set again rather than of a set only the daemon has.
    expect(screen.getByTestId('primary').textContent).toBe('beta');
    expect(screen.getByTestId('hidden').textContent).toBe('alpha');
    expect(screen.getByTestId('visible').textContent).toBe('beta');
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

  // The optimistic value is a guess about what the daemon will do. When the
  // write does not land, keeping the guess leaves the chip, the Knowledge view
  // and the ingest target describing a selection the daemon never applied — and
  // nothing later corrects it. Re-read the truth instead.
  it('re-reads the daemon selection when the write is rejected', async () => {
    const pending = deferred<unknown>();
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));
    expect(mocks.getActive).toHaveBeenCalledTimes(1);

    mocks.setActive.mockReturnValue(pending.promise);
    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));
    expect(screen.getByTestId('primary').textContent).toBe('beta');

    await settle(() => pending.reject(new Error('network down')));

    expect(mocks.getActive).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('primary').textContent).toBe('alpha');
    expect(screen.getByTestId('hidden').textContent).toBe('beta');
  });

  // Same divergence by the other door: the client resolves, but with an error
  // envelope instead of a selection. That is a write that did not land.
  it('re-reads the daemon selection when the write returns no selection', async () => {
    const pending = deferred<unknown>();
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    mocks.setActive.mockReturnValue(pending.promise);
    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));

    await settle(() => pending.resolve({ error: { message: 'primary_kb is not a member' } }));

    expect(mocks.getActive).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId('primary').textContent).toBe('alpha');
    expect(screen.getByTestId('hidden').textContent).toBe('beta');
  });

  // Two clicks, two writes, answers out of order. The older answer describes a
  // selection the user has already moved on from; adopting it silently undoes
  // the newer click.
  it('ignores a superseded response that lands after a newer one', async () => {
    const first = deferred<unknown>();
    const second = deferred<unknown>();
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));

    mocks.setActive.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    await userEvent.click(screen.getByRole('button', { name: 'make beta primary' }));
    await userEvent.click(screen.getByRole('button', { name: 'make alpha primary' }));
    expect(mocks.setActive).toHaveBeenCalledTimes(2);

    await settle(() =>
      second.resolve({
        data: {
          kb_ids: ['alpha', 'beta'],
          primary_kb: 'alpha',
          active_kb: 'alpha',
          hidden_kbs: [],
        },
      })
    );
    expect(screen.getByTestId('primary').textContent).toBe('alpha');

    await settle(() =>
      first.resolve({
        data: { kb_ids: ['beta'], primary_kb: 'beta', active_kb: 'beta', hidden_kbs: ['alpha'] },
      })
    );
    expect(screen.getByTestId('primary').textContent).toBe('alpha');
    expect(screen.getByTestId('hidden').textContent).toBe('none');
  });

  // The prune drops ids naming bases that no longer exist. An empty base list is
  // not evidence that nothing exists — on mount it only means the list has not
  // arrived — and pruning against it writes the session's whole set away.
  it('does not prune the set before the base list has arrived', async () => {
    mocks.listBases.mockReturnValue(new Promise(() => {}));
    renderProvider();
    await waitFor(() => expect(mocks.getActive).toHaveBeenCalled());
    await settle(() => {});

    expect(mocks.setActive).not.toHaveBeenCalled();
    expect(screen.getByTestId('hidden').textContent).toBe('beta');
  });

  // Same, by the other door: a list request that fails is not a list of zero
  // bases. Emptying the list on failure hands the prune the same false evidence.
  it('keeps the base list when a refresh fails', async () => {
    renderProvider();
    await waitFor(() => expect(screen.getByTestId('visible').textContent).toBe('alpha'));

    mocks.listBases.mockRejectedValue(new Error('daemon down'));
    await userEvent.click(screen.getByRole('button', { name: 'refresh' }));
    await settle(() => {});

    expect(screen.getByTestId('visible').textContent).toBe('alpha');
    expect(screen.getByTestId('hidden').textContent).toBe('beta');
    expect(mocks.setActive).not.toHaveBeenCalled();
  });
});
