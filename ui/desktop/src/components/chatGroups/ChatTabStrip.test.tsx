import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent, within } from '@testing-library/react';
import { ChatTabStrip, ChatTabStripProps } from './ChatTabStrip';
import { ChatTab } from './chatGroupsTypes';

function tab(over: Partial<ChatTab> = {}): ChatTab {
  return {
    tabId: 'tab-1',
    sessionId: 's1',
    title: 'Cohort query',
    userSetName: false,
    preview: false,
    ...over,
  };
}

function renderStrip(over: Partial<ChatTabStripProps> = {}) {
  const props: ChatTabStripProps = {
    tabs: [tab()],
    activeTabId: 'tab-1',
    runningSessionIds: [],
    onSelect: vi.fn(),
    onPin: vi.fn(),
    onClose: vi.fn(),
    onReorder: vi.fn(),
    reserveTitlebar: false,
    isCompactSidebarOverlayOpen: false,
    ...over,
  };
  const utils = render(<ChatTabStrip {...props} />);
  return { ...utils, props };
}

/** The .br-tab contract is reached via .closest, exactly as ArtifactViewer's
 *  tests do — so both tab surfaces assert against the same styled node and
 *  cannot drift apart. */
function tabNode(container: HTMLElement, tabId: string) {
  return container.querySelector(`[data-tab-id="${tabId}"]`) as HTMLElement;
}

describe('ChatTabStrip — the .br-tab contract', () => {
  it('reuses the shared tab language rather than inventing a second one', () => {
    const { container } = renderStrip();
    const node = tabNode(container, 'tab-1');
    expect(node.closest('.br-tab')).toBe(node);
    expect(container.querySelector('.br-tabstrip')).toBeTruthy();
  });

  it('marks exactly one tab data-active', () => {
    const { container } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })],
      activeTabId: 'tab-2',
    });
    const active = container.querySelectorAll('.br-tab[data-active="true"]');
    expect(active).toHaveLength(1);
    expect((active[0] as HTMLElement).dataset.tabId).toBe('tab-2');
  });

  it('a preview tab is italic via .br-tab--preview (its first consumer)', () => {
    const { container } = renderStrip({ tabs: [tab({ preview: true })] });
    const node = tabNode(container, 'tab-1');
    expect(node.classList.contains('br-tab--preview')).toBe(true);
    expect(node.querySelector('.br-tab__label')).toBeTruthy();
  });

  it('a pinned tab is NOT italic', () => {
    const { container } = renderStrip({ tabs: [tab({ preview: false })] });
    expect(tabNode(container, 'tab-1').classList.contains('br-tab--preview')).toBe(false);
  });

  it('every tab declares WebkitAppRegion no-drag (it sits inside a drag header)', () => {
    // R1: the strip lives inside BaseChat's 52px WebkitAppRegion:'drag' header.
    // Without no-drag on the tab, macOS moves the window instead of letting the
    // click through, and the whole strip becomes inert.
    const { container } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2' })],
    });
    for (const id of ['tab-1', 'tab-2']) {
      // WebkitAppRegion is a real, load-bearing style that the
      // CSSStyleDeclaration lib type does not declare, hence the cast.
      const style = tabNode(container, id).style as CSSStyleDeclaration & {
        WebkitAppRegion?: string;
      };
      expect(style.WebkitAppRegion).toBe('no-drag');
    }
  });
});

describe('ChatTabStrip — the running pulse replaces the close control', () => {
  it('a running chat shows the coral pulse dot', () => {
    const { container, getByTestId } = renderStrip({ runningSessionIds: ['s1'] });
    expect(getByTestId('chat-tab-running-tab-1')).toBeTruthy();
    expect(container.querySelector('.br-tab__dot')).toBeTruthy();
  });

  it('an idle chat shows NO pulse dot', () => {
    const { container, queryByTestId } = renderStrip({ runningSessionIds: [] });
    expect(queryByTestId('chat-tab-running-tab-1')).toBeNull();
    expect(container.querySelector('.br-tab__dot')).toBeNull();
  });

  it('the dot hides on hover and the x returns in its place', () => {
    const { getByTestId } = renderStrip({ runningSessionIds: ['s1'] });
    const dot = getByTestId('chat-tab-running-tab-1');
    const close = getByTestId('chat-tab-close-tab-1');

    // The swap is class-driven (group-hover), which jsdom does not evaluate —
    // so assert the MECHANISM: the dot is hidden on hover, the x is shown on
    // hover, and they occupy the same slot.
    expect(dot.className).toContain('group-hover:hidden');
    expect(close.className).toContain('hidden');
    expect(close.className).toContain('group-hover:block');
    expect(dot.parentElement).toBe(close.parentElement);
  });

  it('the pulse only marks the tab whose OWN session is running', () => {
    const { queryByTestId } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })],
      runningSessionIds: ['s2'],
    });
    expect(queryByTestId('chat-tab-running-tab-1')).toBeNull();
    expect(queryByTestId('chat-tab-running-tab-2')).toBeTruthy();
  });
});

describe('ChatTabStrip — interaction', () => {
  it('clicking selects; double-clicking pins', () => {
    const { container, props } = renderStrip({ tabs: [tab({ preview: true })] });
    const button = within(tabNode(container, 'tab-1')).getByRole('tab');

    fireEvent.click(button);
    expect(props.onSelect).toHaveBeenCalledWith('tab-1');

    fireEvent.doubleClick(button);
    expect(props.onPin).toHaveBeenCalledWith('tab-1');
  });

  it('the close control closes without also selecting', () => {
    const { getByTestId, props } = renderStrip();
    fireEvent.click(getByTestId('chat-tab-close-tab-1'));
    expect(props.onClose).toHaveBeenCalledWith('tab-1');
    expect(props.onSelect).not.toHaveBeenCalled();
  });
});

describe('ChatTabStrip — accessibility', () => {
  it('is a tablist with a roving tabindex', () => {
    const { container, getAllByRole } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })],
      activeTabId: 'tab-1',
    });
    expect(container.querySelector('[role="tablist"]')).toBeTruthy();

    const tabs = getAllByRole('tab');
    expect(tabs.map((t) => t.getAttribute('tabindex'))).toEqual(['0', '-1']);
    expect(tabs.map((t) => t.getAttribute('aria-selected'))).toEqual(['true', 'false']);
  });

  it('arrow keys move between tabs and wrap', () => {
    const { getAllByRole, props } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })],
      activeTabId: 'tab-1',
    });
    fireEvent.keyDown(getAllByRole('tab')[0], { key: 'ArrowRight' });
    expect(props.onSelect).toHaveBeenCalledWith('tab-2');

    fireEvent.keyDown(getAllByRole('tab')[0], { key: 'ArrowLeft' });
    expect(props.onSelect).toHaveBeenCalledWith('tab-2'); // wraps to the end
  });

  it('every close control is labelled', () => {
    const { getByTestId } = renderStrip();
    expect(getByTestId('chat-tab-close-tab-1').getAttribute('aria-label')).toBe(
      'Close Cohort query'
    );
  });
});
