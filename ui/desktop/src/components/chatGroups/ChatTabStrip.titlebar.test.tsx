import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import { ChatTabStrip } from './ChatTabStrip';
import { ChatTab, firstLeaf, GroupLayout } from './chatGroupsTypes';
import { getTitlebarControlReserve, TITLEBAR_CONTROL_RESERVE_PROPERTY } from '../Layout/TitlebarControls';

/**
 * The reserve test the spec demands (card Pi).
 *
 * This reserve is what stops the tab strip sliding under the macOS traffic
 * lights when the sidebar is collapsed — and when it breaks, it breaks
 * SILENTLY: no error, no crash, just tabs the user cannot click because the OS
 * owns those 100px. It has failed silently once already, which is why the card
 * exists and why this file does.
 */

const TAB: ChatTab = {
  tabId: 'tab-1',
  sessionId: 's1',
  title: 'Cohort',
  userSetName: false,
};

function renderStrip(reserveTitlebar: boolean, isCompactSidebarOverlayOpen = false) {
  return render(
    <ChatTabStrip
      tabs={[TAB]}
      activeTabId="tab-1"
      runningSessionIds={[]}
      onSelect={vi.fn()}
      onClose={vi.fn()}
      onReorder={vi.fn()}
      reserveTitlebar={reserveTitlebar}
      isCompactSidebarOverlayOpen={isCompactSidebarOverlayOpen}
    />
  );
}

describe('ChatTabStrip — the 172px titlebar reserve', () => {
  it('macOS + sidebar collapsed: the strip reserves the control strip width', () => {
    const { getByTestId } = renderStrip(true);
    const strip = getByTestId('chat-tab-strip');

    // 100 (traffic lights) + 64 (two 32px controls) + 8 (gap) = 172.
    expect(getTitlebarControlReserve(true)).toBe(172);
    // Derived from the same function AppLayout sets the property from, never a
    // literal — that is exactly how a hardcoded 172 drifted before.
    expect(strip.style.paddingLeft).toBe(`var(${TITLEBAR_CONTROL_RESERVE_PROPERTY}, 172px)`);
  });

  it('no reserve: the strip falls back to the plain 16px inset', () => {
    const { getByTestId } = renderStrip(false);
    expect(getByTestId('chat-tab-strip').style.paddingLeft).toBe('16px');
  });

  it('the compact sidebar overlay wins over the titlebar reserve', () => {
    const { getByTestId } = renderStrip(true, true);
    expect(getByTestId('chat-tab-strip').style.paddingLeft).toBe('calc(var(--sidebar-width) + 8px)');
  });
});

describe('firstLeaf — the reserve is aimed by a TREE WALK, never an index', () => {
  it('a row split reserves for the LEFT group only', () => {
    const layout: GroupLayout = {
      kind: 'branch',
      dir: 'row',
      sizes: [0.5, 0.5],
      children: [
        { kind: 'leaf', groupId: 'left' },
        { kind: 'leaf', groupId: 'right' },
      ],
    };
    expect(firstLeaf(layout)).toBe('left');
  });

  it('a COL split reserves for the TOP group only — the case an index gets wrong', () => {
    // Both groups sit at x=0 in a column split. An x-based or index-based
    // predicate would reserve for both (or the wrong one); only the TOP strip
    // actually collides with the traffic lights.
    const layout: GroupLayout = {
      kind: 'branch',
      dir: 'col',
      sizes: [0.5, 0.5],
      children: [
        { kind: 'leaf', groupId: 'top' },
        { kind: 'leaf', groupId: 'bottom' },
      ],
    };
    expect(firstLeaf(layout)).toBe('top');
    expect(firstLeaf(layout)).not.toBe('bottom');
  });

  it('a DEPTH-2 tree: the reserve follows children[0] all the way down', () => {
    // col[ row[a, b], c ] — the layout you get by splitting right, then
    // splitting the whole thing down. `a` is the only strip against the traffic
    // lights: `b` is to its right, `c` is below both.
    //
    // This is the shape that kills every shortcut. leafGroupIds(layout)[0] would
    // happen to be right here, so that is not what makes the test useful — what
    // makes it useful is `c`: a predicate that looked at the ROOT's first child
    // and stopped (or that reserved for every leaf at x=0) gets `c` wrong,
    // because `c` is at x=0 too and must NOT reserve.
    const layout: GroupLayout = {
      kind: 'branch',
      dir: 'col',
      sizes: [0.5, 0.5],
      children: [
        {
          kind: 'branch',
          dir: 'row',
          sizes: [0.5, 0.5],
          children: [
            { kind: 'leaf', groupId: 'a' },
            { kind: 'leaf', groupId: 'b' },
          ],
        },
        { kind: 'leaf', groupId: 'c' },
      ],
    };
    expect(firstLeaf(layout)).toBe('a');
    expect(firstLeaf(layout)).not.toBe('b');
    // `c` sits at x=0, exactly like `a`. It must not reserve.
    expect(firstLeaf(layout)).not.toBe('c');
  });

  it('a branch with no children throws rather than silently reserving nothing', () => {
    // A corrupt or half-written persisted tree must be LOUD here. Returning a
    // sentinel would mean no strip matches the reserved id and the reserve
    // silently never applies — which is the exact failure mode card Pi is about.
    const layout: GroupLayout = { kind: 'branch', dir: 'row', sizes: [], children: [] };
    expect(() => firstLeaf(layout)).toThrow(/no children/);
  });

  it('renders the reserve for the first leaf and NOT for its sibling', () => {
    // End-to-end through the component: the shell aims the reserve with
    // firstLeaf, so simulate both groups' strips and assert only one reserves.
    const layout: GroupLayout = {
      kind: 'branch',
      dir: 'col',
      sizes: [0.5, 0.5],
      children: [
        { kind: 'leaf', groupId: 'top' },
        { kind: 'leaf', groupId: 'bottom' },
      ],
    };
    const reserved = firstLeaf(layout);

    const top = renderStrip(reserved === 'top');
    expect(top.getByTestId('chat-tab-strip').style.paddingLeft).toContain('172px');
    top.unmount();

    const bottom = renderStrip(reserved === 'bottom');
    expect(bottom.getByTestId('chat-tab-strip').style.paddingLeft).toBe('16px');
  });
});
