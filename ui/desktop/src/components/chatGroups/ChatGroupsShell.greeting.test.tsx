import { render } from '@testing-library/react';
import { describe, expect, it, vi, beforeEach } from 'vitest';

/**
 * The placeholder pane must not draw a greeting it is about to throw away.
 *
 * A tabless `/pair` is never a resting state: `useEmptyPairRedirect` either
 * finds cargo in flight (a resume id, a parked launcher message, a workflow
 * deeplink, a session mid-create) or navigates to Home. So the pane rendered
 * while `activeTab` is undefined exists only until a real tab lands.
 *
 * `ChatGroupsShell` keys `BaseChat` on the tab id, so that landing unmounts the
 * placeholder and mounts a fresh `<Greeting>` — which draws a NEW random
 * sentence, by design. The unroll takes about a second and the awaited
 * `createSession` on the new-window paths takes about as long, so the
 * placeholder had time to finish before being discarded: the user saw a heading
 * arrive, vanish, and a different heading arrive after it. That is the reported
 * "flash then re-animate", reached by a second route. The first-frame fix in
 * `use-text-animator` cannot touch it, because this is a whole extra mount
 * rather than a flash inside one.
 *
 * ⚠ It suppresses the GREETING and not the empty state. `suppressEmptyState`
 * would take the composer with it, and the composer is the one thing that must
 * survive here in case the tab that was coming never arrives.
 */

let tabs: Array<{ tabId: string; sessionId: string; title: string; userSetName: boolean }> = [];
let activeTabId: string | null = null;
let lastChatProps: Record<string, unknown> = {};

vi.mock('../BaseChat', () => ({
  default: (props: Record<string, unknown>) => {
    lastChatProps = props;
    return <div data-testid="basechat" />;
  },
}));

vi.mock('./ChatTabStrip', () => ({ ChatTabStrip: () => <div data-testid="strip" /> }));

vi.mock('../../utils/sessionListCache', () => ({
  getCachedSessionList: () => null,
  subscribeSessionList: () => () => {},
  preloadSessionList: () => {},
}));

vi.mock('../../contexts/ChatGroupsContext', () => ({
  useChatGroups: () => ({
    dispatch: vi.fn(),
    state: {
      activeGroupId: 'g1',
      layout: { kind: 'leaf', groupId: 'g1' },
      groups: { g1: { id: 'g1', activeTabId, tabs } },
    },
  }),
}));

vi.mock('../ui/sidebar', () => ({ useSidebar: () => ({ state: 'expanded', isMobile: false }) }));

import ChatGroupsShell from './ChatGroupsShell';

describe('ChatGroupsShell — the placeholder pane', () => {
  beforeEach(() => {
    lastChatProps = {};
  });

  it('suppresses the greeting while there is no tab, because that pane is replaced and not filled', () => {
    tabs = [];
    activeTabId = null;
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(lastChatProps.suppressGreeting).toBe(true);
    // ⚠ And ONLY the greeting. Taking the empty state would take the composer.
    expect(lastChatProps.suppressEmptyState).toBe(false);
  });

  it('draws the greeting for a real tab, which is the arrival it belongs to', () => {
    tabs = [{ tabId: 't1', sessionId: 'sess-1', title: 'Chat', userSetName: false }];
    activeTabId = 't1';
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(lastChatProps.suppressGreeting).toBe(false);
    expect(lastChatProps.suppressEmptyState).toBe(false);
  });
});
