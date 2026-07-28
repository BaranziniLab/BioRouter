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

type Selection = {
  kb_ids: string[];
  primary_kb: string | null;
  active_kb: string | null;
  hidden_kbs: string[];
};

/**
 * What the daemon answers at each scope. `GET /active` with a `session_id` is
 * the chat's resolved selection; without one it is the machine-wide default —
 * the thing a chat inherits when it holds no pointer of its own. Tests move
 * these two apart to describe a chat that has overridden the default.
 */
const daemon: { session: Selection; machine: Selection } = {
  session: { kb_ids: [], primary_kb: null, active_kb: null, hidden_kbs: [] },
  machine: { kb_ids: [], primary_kb: null, active_kb: null, hidden_kbs: [] },
};

function Probe() {
  const {
    primaryKbId,
    hiddenKbIds,
    visibleBases,
    defaultPrimaryKb,
    canFollowDefaultPrimary,
    followDefaultPrimary,
    refreshDefaultPrimary,
    setPrimaryKbId,
    toggleKbHidden,
    refresh,
  } = useKnowledge();
  return (
    <div>
      <span data-testid="primary">{primaryKbId ?? 'none'}</span>
      <span data-testid="hidden">{hiddenKbIds.join(',') || 'none'}</span>
      <span data-testid="visible">{visibleBases.map((b) => b.id).join(',') || 'none'}</span>
      <span data-testid="default-primary">{defaultPrimaryKb?.id ?? 'none'}</span>
      <span data-testid="can-follow-default">{canFollowDefaultPrimary ? 'yes' : 'no'}</span>
      <button type="button" onClick={() => void refresh()}>
        refresh
      </button>
      <button type="button" onClick={() => void refreshDefaultPrimary()}>
        read the default
      </button>
      <button type="button" onClick={() => followDefaultPrimary()}>
        follow the default
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

function renderProvider(sessionId: string | null = 'chat-1') {
  return render(
    <KnowledgeProvider sessionId={sessionId}>
      <Probe />
    </KnowledgeProvider>
  );
}

/** Render, wait for the hydrate, then read the machine-wide default. */
async function renderWithDefault(sessionId: string | null = 'chat-1') {
  const result = renderProvider(sessionId);
  await waitFor(() => expect(mocks.getActive).toHaveBeenCalled());
  await userEvent.click(screen.getByRole('button', { name: 'read the default' }));
  await settle(() => {});
  return result;
}

beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.listBases.mockResolvedValue({ data: [base('alpha'), base('beta')] });
  daemon.session = {
    kb_ids: ['alpha'],
    primary_kb: 'alpha',
    active_kb: 'alpha',
    hidden_kbs: ['beta'],
  };
  daemon.machine = {
    kb_ids: ['alpha', 'beta'],
    primary_kb: 'alpha',
    active_kb: 'alpha',
    hidden_kbs: [],
  };
  mocks.getActive.mockImplementation((options?: { query?: { session_id?: string } }) =>
    Promise.resolve({ data: options?.query?.session_id ? daemon.session : daemon.machine })
  );
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

  // The fourth intent. `clear` writes a *durable* "this chat has no primary",
  // and deleting the base a chat had pinned installs exactly that — so without
  // a way to drop the chat's own pointer, such a chat could never follow the
  // machine-wide default again from the GUI.
  describe('following the default again', () => {
    it('reads the machine-wide default from the unscoped selection', async () => {
      daemon.machine.primary_kb = 'beta';
      daemon.machine.active_kb = 'beta';
      await renderWithDefault();

      expect(screen.getByTestId('default-primary').textContent).toBe('beta');
      const scopes = mocks.getActive.mock.calls.map(
        (call: unknown[]) =>
          (call[0] as { query?: { session_id?: string } } | undefined)?.query?.session_id ?? null
      );
      expect(scopes).toContain('chat-1');
      expect(scopes).toContain(null);
    });

    // A chat that is already showing the default has nothing to inherit, so
    // offering it the gesture is noise.
    it('offers nothing to a chat that is already on the default', async () => {
      await renderWithDefault();

      expect(screen.getByTestId('primary').textContent).toBe('alpha');
      expect(screen.getByTestId('can-follow-default').textContent).toBe('no');
    });

    it('offers the default to a chat that pinned something else', async () => {
      daemon.session = {
        kb_ids: ['alpha', 'beta'],
        primary_kb: 'beta',
        active_kb: 'beta',
        hidden_kbs: [],
      };
      await renderWithDefault();

      expect(screen.getByTestId('can-follow-default').textContent).toBe('yes');
      expect(screen.getByTestId('default-primary').textContent).toBe('alpha');
    });

    // The state a delete leaves behind: an explicit "no primary here" that
    // survives every other gesture. This is the one the affordance exists for.
    it('offers the default to a chat left with no primary at all', async () => {
      daemon.session = {
        kb_ids: ['alpha', 'beta'],
        primary_kb: null,
        active_kb: null,
        hidden_kbs: [],
      };
      await renderWithDefault();

      expect(screen.getByTestId('primary').textContent).toBe('none');
      expect(screen.getByTestId('can-follow-default').textContent).toBe('yes');
    });

    // The primary must be a member of the set, so inheriting a default this
    // chat has left out resolves to *no* primary — the offer would do nothing
    // the user can see. The membership switch is the gesture for that, and once
    // it is on the offer appears.
    it('does not offer a default this chat has left out of its set', async () => {
      daemon.session = {
        kb_ids: ['alpha'],
        primary_kb: 'alpha',
        active_kb: 'alpha',
        hidden_kbs: ['beta'],
      };
      daemon.machine.primary_kb = 'beta';
      daemon.machine.active_kb = 'beta';
      await renderWithDefault();

      expect(screen.getByTestId('default-primary').textContent).toBe('beta');
      expect(screen.getByTestId('can-follow-default').textContent).toBe('no');
    });

    // The three primary gestures are mutually exclusive on the wire: two of
    // them in one body is a 400 naming both fields, not a precedence rule.
    // The set travels separately and is left alone — a gesture that means
    // "stop overriding here" must not install a different override on the way.
    it('sends inherit_primary alone', async () => {
      daemon.session = {
        kb_ids: ['alpha', 'beta'],
        primary_kb: 'beta',
        active_kb: 'beta',
        hidden_kbs: [],
      };
      await renderWithDefault();

      await userEvent.click(screen.getByRole('button', { name: 'follow the default' }));

      await waitFor(() => expect(mocks.setActive).toHaveBeenCalled());
      const calls = mocks.setActive.mock.calls;
      const body = calls[calls.length - 1]?.[0]?.body;
      expect(body.inherit_primary).toBe(true);
      expect(body.primary_kb).toBeUndefined();
      expect(body.clear_primary).toBe(false);
      expect(body.hidden_kbs).toBeUndefined();
      expect(body.session_id).toBe('chat-1');
    });

    // Which base "the default" resolves to is the daemon's to decide — it
    // re-reads the machine pointer and filters it through this chat's set. The
    // renderer adopts that answer rather than assuming its own cached default.
    it('adopts whatever primary the daemon reports back', async () => {
      daemon.session = {
        kb_ids: ['alpha', 'beta'],
        primary_kb: 'beta',
        active_kb: 'beta',
        hidden_kbs: [],
      };
      mocks.setActive.mockResolvedValue({
        data: {
          kb_ids: ['alpha', 'beta'],
          primary_kb: 'alpha',
          active_kb: 'alpha',
          hidden_kbs: [],
        },
      });
      await renderWithDefault();

      await userEvent.click(screen.getByRole('button', { name: 'follow the default' }));

      await waitFor(() => expect(screen.getByTestId('primary')).toHaveTextContent('alpha'));
      expect(screen.getByTestId('can-follow-default').textContent).toBe('no');
    });

    // A write that never landed must not leave the chat looking like it
    // inherited: the pointer is never guessed forward, and the failure is
    // recovered by re-reading the truth, which still offers the way back.
    it('does not claim the chat inherited when the write fails', async () => {
      daemon.session = {
        kb_ids: ['alpha', 'beta'],
        primary_kb: 'beta',
        active_kb: 'beta',
        hidden_kbs: [],
      };
      const pending = deferred<unknown>();
      await renderWithDefault();
      const readsBefore = mocks.getActive.mock.calls.length;

      mocks.setActive.mockReturnValue(pending.promise);
      await userEvent.click(screen.getByRole('button', { name: 'follow the default' }));
      expect(screen.getByTestId('primary').textContent).toBe('beta');

      await settle(() => pending.reject(new Error('network down')));

      expect(mocks.getActive.mock.calls.length).toBe(readsBefore + 1);
      expect(screen.getByTestId('primary').textContent).toBe('beta');
      expect(screen.getByTestId('can-follow-default').textContent).toBe('yes');
    });

    // At machine scope there is nothing above to inherit, so the gesture
    // coincides with "this scope has no primary" — a different, destructive
    // thing. It is not offered, and asking for it writes nothing.
    it('has nothing to inherit outside a chat', async () => {
      daemon.machine.primary_kb = 'beta';
      daemon.machine.active_kb = 'beta';
      await renderWithDefault(null);

      expect(screen.getByTestId('can-follow-default').textContent).toBe('no');
      await userEvent.click(screen.getByRole('button', { name: 'follow the default' }));
      await settle(() => {});
      expect(mocks.setActive).not.toHaveBeenCalled();
    });
  });
});
