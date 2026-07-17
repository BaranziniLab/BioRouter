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
    ...over,
  };
}

function renderStrip(over: Partial<ChatTabStripProps> = {}) {
  const props: ChatTabStripProps = {
    tabs: [tab()],
    activeTabId: 'tab-1',
    runningSessionIds: [],
    onSelect: vi.fn(),
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

  it('NO tab is ever italic — preview tabs are retired, every tab is upright', () => {
    // .br-tab--preview still exists in main.css but has zero consumers. If this
    // fails, the preview concept has crept back into the strip.
    const { container } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2' })],
      activeTabId: 'tab-2',
    });
    for (const tabId of ['tab-1', 'tab-2']) {
      const node = tabNode(container, tabId);
      expect(node.classList.contains('br-tab--preview')).toBe(false);
      expect(node.dataset.preview).toBeUndefined();
      expect(node.querySelector('.br-tab__label')).toBeTruthy();
    }
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
  it('clicking selects; double-clicking does nothing extra (pin is retired)', () => {
    const { container, props } = renderStrip();
    const button = within(tabNode(container, 'tab-1')).getByRole('tab');

    fireEvent.click(button);
    expect(props.onSelect).toHaveBeenCalledWith('tab-1');

    // A double click is just two clicks now — it must not throw, and it must not
    // mean anything special.
    fireEvent.doubleClick(button);
    expect(props.onSelect).toHaveBeenCalledTimes(1);
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

/**
 * design.md §3.9 fixes the icon scale at 16 (inline/dense) / 20 (default) /
 * 24 (page-level). The strip had drifted to 13px (the tab glyph) and 12px (the
 * close ×) — both off-scale inventions, measured in the running app.
 *
 * Tailwind size classes are real DOM, so jsdom CAN hold this line: the classes
 * are the single source of the rendered px. (The rendered geometry itself was
 * verified by driving Electron — 13/12 -> 16/16 at stroke 1.5.)
 */
describe('ChatTabStrip — icons stay on the design.md §3.9 scale', () => {
  const ON_SCALE = ['h-4 w-4', 'h-5 w-5', 'h-6 w-6'];

  function iconClassNames(container: HTMLElement) {
    return [...container.querySelectorAll('svg')].map((s) => s.getAttribute('class') ?? '');
  }

  it('renders every strip icon at an on-scale size, never a bespoke px value', () => {
    const { container } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })],
    });
    const classes = iconClassNames(container);
    expect(classes.length).toBeGreaterThan(0);
    for (const cls of classes) {
      // No arbitrary-value sizing: h-[13px] and friends are exactly the drift.
      expect(cls).not.toMatch(/[hw]-\[/);
      expect(ON_SCALE.some((size) => cls.includes(size))).toBe(true);
    }
  });

  it('sizes the running-tab close control on-scale too', () => {
    // The running branch renders its own X; it must not drift from the idle one.
    const { container } = renderStrip({ tabs: [tab()], runningSessionIds: ['s1'] });
    const classes = iconClassNames(container);
    expect(classes.length).toBeGreaterThan(0);
    for (const cls of classes) {
      expect(cls).not.toMatch(/[hw]-\[/);
      expect(ON_SCALE.some((size) => cls.includes(size))).toBe(true);
    }
  });

  it('draws every glyph at stroke 1.5, which is what app-icons guarantees', () => {
    // Provenance check: a raw `lucide-react` import would default to stroke 2.
    // Asserting the rendered stroke is what actually catches that swap.
    const { container } = renderStrip({ tabs: [tab()], runningSessionIds: ['s1'] });
    const svgs = [...container.querySelectorAll('svg')];
    expect(svgs.length).toBeGreaterThan(0);
    for (const svg of svgs) {
      expect(svg.getAttribute('stroke-width')).toBe('1.5');
    }
  });
});
