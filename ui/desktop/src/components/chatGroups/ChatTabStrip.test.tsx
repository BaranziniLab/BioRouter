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

/**
 * Rung 3 of the yield ladder (D-32) — the ▾ overflow menu.
 *
 * WHAT THESE TESTS ARE AND ARE NOT. jsdom computes no layout, so scrollWidth and
 * clientWidth are 0 here and the real overflow can never occur: these stub the
 * MEASUREMENT and test the WIRING — that the strip asks the rule, believes it,
 * and that the menu can reach a scrolled-out tab. Whether the strip actually
 * overflows at 640px is geometry, and geometry is verified by driving the real
 * app and measuring. The rule itself is unit-tested in Layout/yieldLadder.test.ts.
 */
describe('ChatTabStrip — rung 3: the ▾ overflow menu', () => {
  /** Pretend the strip's content is `content` px wide inside a `box` px box. */
  function stubStripMetrics(content: number, box: number) {
    const scroll = vi.spyOn(HTMLElement.prototype, 'scrollWidth', 'get');
    const client = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get');
    scroll.mockImplementation(function (this: HTMLElement) {
      return this.dataset.testid === 'chat-tab-strip' ? content : 0;
    });
    client.mockImplementation(function (this: HTMLElement) {
      return this.dataset.testid === 'chat-tab-strip' ? box : 0;
    });
    return () => {
      scroll.mockRestore();
      client.mockRestore();
    };
  }

  const manyTabs = Array.from({ length: 6 }, (_, i) =>
    tab({ tabId: `tab-${i + 1}`, sessionId: `s${i + 1}`, title: `Chat ${i + 1}` })
  );

  it('stays away while the tabs fit', () => {
    const restore = stubStripMetrics(300, 300);
    try {
      const { queryByTestId } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      expect(queryByTestId('chat-tab-overflow-trigger')).toBeNull();
    } finally {
      restore();
    }
  });

  it('appears once tabs are scrolled out of sight', () => {
    const restore = stubStripMetrics(900, 300);
    try {
      const { queryByTestId } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      expect(queryByTestId('chat-tab-overflow-trigger')).toBeTruthy();
    } finally {
      restore();
    }
  });

  it('NEVER WRAPS: the strip stays a single nowrap scroll box even when it overflows', () => {
    // The spec is explicit — a wrapped second row moves every tab under the
    // cursor. The overflow answer must be the menu, never a taller strip.
    const restore = stubStripMetrics(900, 300);
    try {
      const { getByTestId } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      const strip = getByTestId('chat-tab-strip');
      // The nowrap itself lives in main.css (`.br-tabstrip`), which jsdom does
      // not apply. What IS provable here: the strip never grows a second row's
      // worth of markup, and it keeps the class that carries the rule.
      expect(strip.className).toContain('br-tabstrip');
      expect(strip.className).not.toMatch(/flex-wrap|wrap/);
    } finally {
      restore();
    }
  });

  it('keeps the ▾ OUTSIDE the strip, or it would latch its own overflow alive', () => {
    // The load-bearing structural claim of rung 3: a button inside the scroll box
    // counts toward scrollWidth, so the overflow that summoned it could never
    // clear. Outside, both directions are monotone. See shouldShowTabOverflowMenu.
    const restore = stubStripMetrics(900, 300);
    try {
      const { getByTestId } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      const trigger = getByTestId('chat-tab-overflow-trigger');
      const strip = getByTestId('chat-tab-strip');
      expect(strip.contains(trigger)).toBe(false);
      expect(trigger.closest('.br-tabstrip-wrap')).toBeTruthy();
    } finally {
      restore();
    }
  });

  it('never offers a menu that could only list the tab you are already on', () => {
    const restore = stubStripMetrics(900, 20);
    try {
      const { queryByTestId } = renderStrip({ tabs: [tab()], activeTabId: 'tab-1' });
      expect(queryByTestId('chat-tab-overflow-trigger')).toBeNull();
    } finally {
      restore();
    }
  });

  it('reaches a scrolled-out tab: selecting from the menu activates it', () => {
    const restore = stubStripMetrics(900, 300);
    try {
      const { getByTestId, props } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      fireEvent.pointerDown(getByTestId('chat-tab-overflow-trigger'));
      fireEvent.click(getByTestId('chat-tab-overflow-item-tab-6'));
      expect(props.onSelect).toHaveBeenCalledWith('tab-6');
    } finally {
      restore();
    }
  });

  it('lists every tab, so the menu is the whole strip and not just its tail', () => {
    const restore = stubStripMetrics(900, 300);
    try {
      const { getByTestId } = renderStrip({ tabs: manyTabs, activeTabId: 'tab-1' });
      fireEvent.pointerDown(getByTestId('chat-tab-overflow-trigger'));
      for (const t of manyTabs) {
        expect(getByTestId(`chat-tab-overflow-item-${t.tabId}`)).toBeTruthy();
      }
    } finally {
      restore();
    }
  });
});

/**
 * The MERGE caret — a tab being dragged in from ANOTHER window.
 *
 * It needs its own prop because this window receives no pointer events at all
 * while a cross-window drag is in flight: the OS delivers them to the window the
 * drag started in (measured with real OS input, tear-off design §8 Phase 0). So
 * `dragOverTabId` is empty for the whole gesture and the caret is driven by IPC.
 */
describe('ChatTabStrip — the cross-window merge caret', () => {
  const twoTabs = [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Second' })];

  it('draws the SAME hairline the local reorder draws', () => {
    // Merging into a strip IS inserting into a strip. A second visual for it
    // would be a second answer to a question the strip already answers.
    const { container } = renderStrip({ tabs: twoTabs, remoteDropBeforeTabId: 'tab-2' });
    expect(tabNode(container, 'tab-2')).toHaveAttribute('data-dropbefore', 'true');
    expect(tabNode(container, 'tab-1')).not.toHaveAttribute('data-dropbefore');
  });

  it('shows nothing when the caret belongs after the last tab', () => {
    // `null` is what the bridge answers for "append": there is no tab to hang a
    // leading-edge hairline on, which is exactly how the local reorder behaves.
    const { container } = renderStrip({ tabs: twoTabs, remoteDropBeforeTabId: null });
    expect(container.querySelectorAll('[data-dropbefore]')).toHaveLength(0);
  });

  it('defaults to no caret, so every existing caller is untouched', () => {
    const { container } = renderStrip({ tabs: twoTabs });
    expect(container.querySelectorAll('[data-dropbefore]')).toHaveLength(0);
  });

  it('names a tab this strip does not have without marking anything', () => {
    // A stale caret from a window whose layout moved under the preview.
    const { container } = renderStrip({ tabs: twoTabs, remoteDropBeforeTabId: 'tab-99' });
    expect(container.querySelectorAll('[data-dropbefore]')).toHaveLength(0);
  });
});
