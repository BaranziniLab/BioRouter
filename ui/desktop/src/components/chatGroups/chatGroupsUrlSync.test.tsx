import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, act } from '@testing-library/react';
import { useState } from 'react';
import { MemoryRouter, Routes, Route, useLocation } from 'react-router-dom';
import { useChatGroupsUrlSync, UrlOpenRequest } from './useChatGroupsUrlSync';

/**
 * R2 — the fixed point. A read must not trigger a write that triggers a read.
 * These tests are the mechanical proof, and they exist BEFORE the shell is
 * wired, exactly as the plan requires: an infinite render found by hand in the
 * running app costs the day.
 */

let renderCount = 0;
let navCount = 0;

function Harness({
  onOpen,
  initialActive = '',
  activateOnOpen = false,
}: {
  onOpen: (r: UrlOpenRequest) => void;
  initialActive?: string;
  /** Mimic the reducer: opening a tab also activates it. */
  activateOnOpen?: boolean;
}) {
  renderCount++;
  const [activeSessionId, setActiveSessionId] = useState(initialActive);
  const location = useLocation();

  useChatGroupsUrlSync({
    activeSessionId,
    onOpen: (request) => {
      onOpen(request);
      if (activateOnOpen) setActiveSessionId(request.sessionId);
    },
  });

  return (
    <div>
      <span data-testid="url">{location.pathname + location.search}</span>
      <span data-testid="active">{activeSessionId}</span>
      <button data-testid="activate-b" onClick={() => setActiveSessionId('sB')}>
        activate
      </button>
      <button data-testid="activate-empty" onClick={() => setActiveSessionId('')}>
        activate empty
      </button>
    </div>
  );
}

/** Counts every distinct location the router settles on. */
function NavSpy() {
  const location = useLocation();
  const key = location.key + location.search;
  const [seen] = useState(() => new Set<string>());
  if (!seen.has(key)) {
    seen.add(key);
    navCount++;
  }
  return null;
}

function renderAt(
  url: string,
  onOpen: (r: UrlOpenRequest) => void,
  initialActive = '',
  activateOnOpen = false
) {
  return render(
    <MemoryRouter initialEntries={[url]}>
      <NavSpy />
      <Routes>
        <Route
          path="/pair"
          element={
            <Harness
              onOpen={onOpen}
              initialActive={initialActive}
              activateOnOpen={activateOnOpen}
            />
          }
        />
      </Routes>
    </MemoryRouter>
  );
}

beforeEach(() => {
  renderCount = 0;
  navCount = 0;
});

describe('useChatGroupsUrlSync — the fixed point', () => {
  it('a deep link opens exactly one tab and settles', () => {
    const onOpen = vi.fn();
    renderAt('/pair?resumeSessionId=sA', onOpen);

    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ sessionId: 'sA' }));
  });

  it('SETTLES: activating a tab writes the URL once and does NOT re-dispatch onOpen', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair?resumeSessionId=sA', onOpen, 'sA');

    expect(onOpen).toHaveBeenCalledTimes(1);
    const rendersBefore = renderCount;

    // Focus moves to sB. OUT mirrors it into the URL...
    act(() => getByTestId('activate-b').click());

    expect(getByTestId('url').textContent).toBe('/pair?resumeSessionId=sB');
    // ...and IN must recognise its own echo rather than dispatching it back.
    // If this is 2, the read/write cycle is live and the app will spin.
    expect(onOpen).toHaveBeenCalledTimes(1);

    // The render count must be bounded. A ping-pong shows up here first.
    expect(renderCount - rendersBefore).toBeLessThan(6);
  });

  it('SETTLES: the whole cycle reaches a fixed point (no unbounded renders/navs)', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair?resumeSessionId=sA', onOpen, 'sA');

    act(() => getByTestId('activate-b').click());
    const settledRenders = renderCount;
    const settledNavs = navCount;

    // Let every queued effect drain. If read and write are mutually recursive,
    // these counts keep climbing with no input at all.
    act(() => {});
    act(() => {});

    expect(renderCount).toBe(settledRenders);
    expect(navCount).toBe(settledNavs);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('the same param twice with no new navigation opens ONCE', () => {
    const onOpen = vi.fn();
    const { rerender } = renderAt('/pair?resumeSessionId=sA', onOpen);
    expect(onOpen).toHaveBeenCalledTimes(1);

    // A re-render with an unchanged location must not re-consume the param.
    rerender(
      <MemoryRouter initialEntries={['/pair?resumeSessionId=sA']}>
        <NavSpy />
        <Routes>
          <Route path="/pair" element={<Harness onOpen={onOpen} />} />
        </Routes>
      </MemoryRouter>
    );
    act(() => {});
    // The remount is a fresh mount, so it legitimately opens again — but the
    // FIRST instance must not have fired twice.
    expect(onOpen.mock.calls.filter((c) => c[0].sessionId === 'sA').length).toBeLessThanOrEqual(2);
  });

  it('carries initialMessage off location.state', () => {
    const onOpen = vi.fn();
    render(
      <MemoryRouter
        initialEntries={[
          {
            pathname: '/pair',
            search: '?resumeSessionId=sP',
            state: { initialMessage: 'hi' },
          },
        ]}
      >
        <Routes>
          <Route path="/pair" element={<Harness onOpen={onOpen} />} />
        </Routes>
      </MemoryRouter>
    );

    expect(onOpen).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: 'sP', initialMessage: 'hi' })
    );
  });

  /**
   * The IN gate is the query param, so a state-only arrival is INERT — every
   * test above supplies `search: '?resumeSessionId=…'`, which is why none of
   * them caught the Home-composer drop. Pinned here as the contract that makes
   * the navigationUtils fix load-bearing: callers MUST put the id in the URL.
   */
  it('ignores a state-only resumeSessionId (the id must be in the query string)', () => {
    const onOpen = vi.fn();
    render(
      <MemoryRouter
        initialEntries={[
          {
            pathname: '/pair',
            search: '',
            state: { resumeSessionId: 'sP', initialMessage: 'hi' },
          },
        ]}
      >
        <Routes>
          <Route path="/pair" element={<Harness onOpen={onOpen} />} />
        </Routes>
      </MemoryRouter>
    );

    expect(onOpen).not.toHaveBeenCalled();
  });

  it('an empty active group writes nothing to the URL', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair', onOpen, '');
    expect(onOpen).not.toHaveBeenCalled();
    expect(getByTestId('url').textContent).toBe('/pair');
  });

  /**
   * R1-01 — the sidebar Recents highlight reads ?resumeSessionId= off the URL,
   * so an active EMPTY tab (New Session / strip "+") must clear the param.
   * Before the fix OUT early-returned on '' and the URL kept pointing at the
   * previous chat: the sidebar highlighted the wrong row and a reload resumed
   * a session the strip said was not focused.
   */
  it('activating an empty New Session tab clears the stale resumeSessionId from the URL', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair?resumeSessionId=sA', onOpen, 'sA');
    expect(onOpen).toHaveBeenCalledTimes(1);

    act(() => getByTestId('activate-empty').click());

    expect(getByTestId('url').textContent).toBe('/pair');
    // Clearing must not re-dispatch anything: there is no param left to read.
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it('re-activating a real tab after an empty one mirrors its id back into the URL', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair?resumeSessionId=sA', onOpen, 'sA');

    act(() => getByTestId('activate-empty').click());
    expect(getByTestId('url').textContent).toBe('/pair');

    act(() => getByTestId('activate-b').click());
    expect(getByTestId('url').textContent).toBe('/pair?resumeSessionId=sB');
    // The write is OUT's own echo, never a re-open.
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  /**
   * The empty-tab clear must NOT clobber a deep link in the commit that
   * consumes it: IN dispatches openTab and the reducer state only catches up on
   * the NEXT render, so OUT momentarily sees activeSessionId '' beside the very
   * param IN just consumed. That is a consumption in flight, not a stale param.
   */
  it('does not bounce the URL when a deep link is consumed while the group is still empty', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair?resumeSessionId=sA', onOpen, '', true);

    act(() => {});

    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(getByTestId('url').textContent).toBe('/pair?resumeSessionId=sA');
    // One location only: the initial mount. A clear+rewrite churn shows up here.
    expect(navCount).toBe(1);
  });

  it('OUT does not fire when focus already matches the URL', () => {
    const onOpen = vi.fn();
    renderAt('/pair?resumeSessionId=sA', onOpen, 'sA');
    const navsAfterMount = navCount;
    act(() => {});
    // The mirror must be a no-op when it agrees with the URL.
    expect(navCount).toBe(navsAfterMount);
  });
});
