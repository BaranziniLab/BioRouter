import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import React from 'react';
import { useChatGroupsUrlSync, type UrlOpenRequest } from './useChatGroupsUrlSync';

/**
 * A tab opened from Recents must be born with the session's name.
 *
 * The bug: clicking a chat in the history opened a tab titled "New Session",
 * which sat there for about a second and then popped to the real name once
 * BaseChat had fetched the session and fired `onSessionUpdate` -> `renameTab`.
 * The name was never missing — the sidebar was RENDERING it on the row being
 * clicked. It just wasn't handed over, so the tab round-tripped to the server
 * for a string that was already on screen.
 *
 * The fix threads the known title through route state:
 *   RecentChats.onOpen(id, name) -> AppSidebar.handleOpenChat -> navigate state
 *   -> useChatGroupsUrlSync -> UrlOpenRequest.title -> openTab payload.
 *
 * This pins the URL-sync link of that chain, which is the one that silently
 * drops data: the hook reads `location.state` as a loosely-typed bag, so a
 * field that isn't explicitly copied across vanishes with no type error.
 *
 * The late rename is deliberately NOT removed and is not tested away here —
 * see the sibling ChatGroupsShell.sessionName test. It still has to run for
 * deep links, fresh chats, and renames, and it carries `userSetName`, which
 * the session LIST payload does not include.
 */
describe('useChatGroupsUrlSync — the opener hands over the name it already has', () => {
  const wrapperFor = (entry: { pathname: string; search: string; state?: unknown }) => {
    const Wrapper = ({ children }: { children: React.ReactNode }) => (
      <MemoryRouter initialEntries={[entry]}>{children}</MemoryRouter>
    );
    return Wrapper;
  };

  let onOpen: (r: UrlOpenRequest) => void;
  let calls: UrlOpenRequest[];

  beforeEach(() => {
    calls = [];
    onOpen = vi.fn((r: UrlOpenRequest) => calls.push(r));
  });

  it('forwards a title supplied in route state, so the tab never shows the placeholder', () => {
    renderHook(() => useChatGroupsUrlSync({ activeSessionId: '', onOpen }), {
      wrapper: wrapperFor({
        pathname: '/pair',
        search: '?resumeSessionId=sess-1',
        state: { title: 'CRISPR screen analysis' },
      }),
    });

    expect(calls).toHaveLength(1);
    expect(calls[0].sessionId).toBe('sess-1');
    // The assertion that matters: had the hook not copied `title` across, this
    // is undefined and the reducer falls back to "New Session" — the flash.
    expect(calls[0].title).toBe('CRISPR screen analysis');
  });

  it('leaves the title undefined for a deep link that carries no state', () => {
    renderHook(() => useChatGroupsUrlSync({ activeSessionId: '', onOpen }), {
      wrapper: wrapperFor({ pathname: '/pair', search: '?resumeSessionId=sess-2' }),
    });

    expect(calls).toHaveLength(1);
    expect(calls[0].sessionId).toBe('sess-2');
    // Not a placeholder invented here: undefined lets the reducer's own default
    // apply, and the late rename fills in the real name once the session loads.
    expect(calls[0].title).toBeUndefined();
  });

  it('still forwards an initial message alongside the title', () => {
    renderHook(() => useChatGroupsUrlSync({ activeSessionId: '', onOpen }), {
      wrapper: wrapperFor({
        pathname: '/pair',
        search: '?resumeSessionId=sess-3',
        state: { title: 'Named', initialMessage: 'hello' },
      }),
    });

    expect(calls[0].title).toBe('Named');
    expect(calls[0].initialMessage).toBe('hello');
  });
});
