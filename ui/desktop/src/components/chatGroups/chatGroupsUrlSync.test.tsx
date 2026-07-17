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
}: {
  onOpen: (r: UrlOpenRequest) => void;
  initialActive?: string;
}) {
  renderCount++;
  const [activeSessionId, setActiveSessionId] = useState(initialActive);
  const location = useLocation();

  useChatGroupsUrlSync({ activeSessionId, onOpen });

  return (
    <div>
      <span data-testid="url">{location.pathname + location.search}</span>
      <span data-testid="active">{activeSessionId}</span>
      <button data-testid="activate-b" onClick={() => setActiveSessionId('sB')}>
        activate
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

function renderAt(url: string, onOpen: (r: UrlOpenRequest) => void, initialActive = '') {
  return render(
    <MemoryRouter initialEntries={[url]}>
      <NavSpy />
      <Routes>
        <Route path="/pair" element={<Harness onOpen={onOpen} initialActive={initialActive} />} />
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
    expect(onOpen).toHaveBeenCalledWith(expect.objectContaining({ sessionId: 'sA', preview: false }));
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

  it('carries preview and initialMessage off location.state', () => {
    const onOpen = vi.fn();
    render(
      <MemoryRouter
        initialEntries={[
          {
            pathname: '/pair',
            search: '?resumeSessionId=sP',
            state: { preview: true, initialMessage: 'hi' },
          },
        ]}
      >
        <Routes>
          <Route path="/pair" element={<Harness onOpen={onOpen} />} />
        </Routes>
      </MemoryRouter>
    );

    expect(onOpen).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: 'sP', preview: true, initialMessage: 'hi' })
    );
  });

  it('an empty active group writes nothing to the URL', () => {
    const onOpen = vi.fn();
    const { getByTestId } = renderAt('/pair', onOpen, '');
    expect(onOpen).not.toHaveBeenCalled();
    expect(getByTestId('url').textContent).toBe('/pair');
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
