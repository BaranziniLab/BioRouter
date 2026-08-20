import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

/**
 * A tab must adopt its session's REAL name once the session loads.
 *
 * The bug this pins was found by driving the app, not by any test: the rename
 * mirror in ChatGroupsContext only listens to `announceSessionName`, and
 * BaseChat only announces on an explicit RENAME. Nothing announces when a
 * session merely LOADS — so a tab opened from the sidebar or a deep link kept
 * its creation-time placeholder and sat there reading "New Session" while
 * document.title showed the real name two inches away.
 *
 * The fix is to feed BaseChat's existing `onSessionUpdate` back into a
 * `renameTab`. This asserts the WIRING (the shell hands BaseChat a callback
 * that dispatches), which is the part that was missing — the reducer's
 * renameTab was already correct and already tested.
 */

const dispatch = vi.fn();

// Capture the props the shell hands to BaseChat without mounting the real
// 1800-line component (it needs ~10 providers; no existing test mounts it).
let lastBaseChatProps: Record<string, unknown> = {};
vi.mock('../BaseChat', () => ({
  default: (props: Record<string, unknown>) => {
    lastBaseChatProps = props;
    return <div data-testid="basechat" />;
  },
}));

vi.mock('../../contexts/ChatGroupsContext', () => ({
  useChatGroups: () => ({
    dispatch,
    state: {
      activeGroupId: 'g1',
      layout: { kind: 'leaf', groupId: 'g1' },
      groups: {
        g1: {
          id: 'g1',
          activeTabId: 't1',
          tabs: [
            { tabId: 't1', sessionId: 'sess-1', title: 'New Session', userSetName: false },
          ],
        },
      },
    },
  }),
}));

vi.mock('../ui/sidebar', () => ({ useSidebar: () => ({ state: 'expanded', isMobile: false }) }));

import ChatGroupsShell from './ChatGroupsShell';

describe('ChatGroupsShell — the tab adopts the loaded session name', () => {
  it('dispatches renameTab when BaseChat reports the session it loaded', () => {
    dispatch.mockClear();
    render(<ChatGroupsShell onChatChange={() => {}} />);

    // BaseChat reports the session it actually loaded.
    const onSessionUpdate = lastBaseChatProps.onSessionUpdate as
      | ((s: { id: string; name: string; userSetName: boolean } | null) => void)
      | undefined;

    // The wiring itself is the thing that was missing.
    expect(onSessionUpdate).toBeTypeOf('function');

    onSessionUpdate!({ id: 'sess-1', name: 'Multiple sclerosis knowledge graph', userSetName: false });

    expect(dispatch).toHaveBeenCalledWith({
      type: 'renameTab',
      sessionId: 'sess-1',
      title: 'Multiple sclerosis knowledge graph',
      userSetName: false,
    });
  });

  /**
   * The same callback also carries the session's DIRECTORY, and that half had
   * no writer at all: `ChatTab.cwd` was declared, `payloadFromTab` handled it
   * and `main.ts` read it, but nothing ever set it. A torn-off window
   * therefore fell back to `os.homedir()`, and every new chat opened in it was
   * created in `~` instead of the project the user tore off from.
   */
  it('records the session directory on the tab, so a tear-off can carry it', () => {
    dispatch.mockClear();
    render(<ChatGroupsShell onChatChange={() => {}} />);
    const onSessionUpdate = lastBaseChatProps.onSessionUpdate as (s: {
      id: string;
      name: string;
      userSetName: boolean;
      workingDir?: string;
    }) => void;

    onSessionUpdate({
      id: 'sess-1',
      name: 'Cohort query',
      userSetName: false,
      workingDir: '/Users/wgu/Desktop/project',
    });

    expect(dispatch).toHaveBeenCalledWith({
      type: 'setTabCwd',
      sessionId: 'sess-1',
      cwd: '/Users/wgu/Desktop/project',
    });
  });

  /**
   * ⚠ Recorded even when the session has no name yet, which is exactly a
   * freshly created one. Folding the directory into the rename dispatch would
   * inherit its `!session.name` guard and lose it for the case that needs it
   * most: a brand-new chat the user immediately tears off.
   */
  it('records the directory for an unnamed session, which the rename guard would drop', () => {
    dispatch.mockClear();
    render(<ChatGroupsShell onChatChange={() => {}} />);
    const onSessionUpdate = lastBaseChatProps.onSessionUpdate as (s: {
      id: string;
      name: string;
      userSetName: boolean;
      workingDir?: string;
    }) => void;

    onSessionUpdate({ id: 'sess-2', name: '', userSetName: false, workingDir: '/w/fresh' });

    expect(dispatch).toHaveBeenCalledWith({
      type: 'setTabCwd',
      sessionId: 'sess-2',
      cwd: '/w/fresh',
    });
    // ...and still no rename, which is the behaviour the next test pins.
    expect(dispatch).not.toHaveBeenCalledWith(
      expect.objectContaining({ type: 'renameTab', sessionId: 'sess-2' })
    );
  });

  it('ignores a null or unnamed session rather than blanking the tab', () => {
    dispatch.mockClear();
    render(<ChatGroupsShell onChatChange={() => {}} />);
    const onSessionUpdate = lastBaseChatProps.onSessionUpdate as (s: unknown) => void;

    onSessionUpdate(null);
    onSessionUpdate({ id: 'sess-1', name: '', userSetName: false });

    expect(dispatch).not.toHaveBeenCalled();
  });
});
