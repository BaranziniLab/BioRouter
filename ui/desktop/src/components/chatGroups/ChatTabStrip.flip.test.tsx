import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, render } from '@testing-library/react';
import { fireEvent } from '@testing-library/dom';
import { ChatTabStrip, ChatTabStripProps } from './ChatTabStrip';
import { ChatTab } from './chatGroupsTypes';

/**
 * #37 — the FLIP pass: on reorder/close, tabs get an INVERTED inline translate
 * and are released to identity with the spring easing, instead of teleporting.
 *
 * jsdom computes no layout (offsetLeft is 0 everywhere), so these tests stub
 * offsetLeft to a synthetic 100px grid — they verify the WIRING (measure →
 * invert → release → clean up), not the pixels. The visual spring itself is
 * browser-verified, per the repo's Prism/Tailwind lesson.
 */

const TAB_SLOT_PX = 100;

function tab(id: string): ChatTab {
  return { tabId: id, sessionId: `s-${id}`, title: `Chat ${id}`, userSetName: false };
}

function renderStrip(tabs: ChatTab[], over: Partial<ChatTabStripProps> = {}) {
  const props: ChatTabStripProps = {
    tabs,
    activeTabId: tabs[0]?.tabId ?? null,
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

function tabNode(container: HTMLElement, tabId: string) {
  return container.querySelector(`[data-tab-id="${tabId}"]`) as HTMLElement;
}

const originalOffsetLeft = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  'offsetLeft'
) as PropertyDescriptor;
const originalMatchMedia = window.matchMedia;

let frames: Parameters<typeof window.requestAnimationFrame>[0][] = [];

function flushFrames() {
  act(() => {
    const pending = frames;
    frames = [];
    for (const cb of pending) cb(performance.now());
  });
}

beforeEach(() => {
  frames = [];
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
    frames.push(cb);
    return frames.length;
  });
  // A synthetic layout: each tab occupies a 100px slot in its DOM order.
  Object.defineProperty(HTMLElement.prototype, 'offsetLeft', {
    configurable: true,
    get(this: HTMLElement) {
      if (!this.dataset?.tabId || !this.parentElement) return 0;
      const siblings = Array.from(this.parentElement.querySelectorAll(':scope > [data-tab-id]'));
      return siblings.indexOf(this) * TAB_SLOT_PX;
    },
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(HTMLElement.prototype, 'offsetLeft', originalOffsetLeft);
  window.matchMedia = originalMatchMedia;
});

describe('ChatTabStrip — FLIP slide on reorder (#37)', () => {
  it('applies the INVERTED translate when the order changes, then releases with the spring', () => {
    const { container, rerender, props } = renderStrip([tab('a'), tab('b')]);

    // Mount pass: a snapshot, no motion.
    expect(tabNode(container, 'a').style.transform).toBe('');
    expect(tabNode(container, 'b').style.transform).toBe('');

    rerender(<ChatTabStrip {...props} tabs={[tab('b'), tab('a')]} />);

    // First: each moved tab is inverted back to where it WAS (a: 0 → 100px slot
    // means translateX(-100px); b: 100 → 0 means translateX(100px)), with
    // transitions off so the jump is invisible.
    const a = tabNode(container, 'a');
    const b = tabNode(container, 'b');
    expect(a.style.transform).toBe(`translateX(${-TAB_SLOT_PX}px)`);
    expect(b.style.transform).toBe(`translateX(${TAB_SLOT_PX}px)`);
    expect(a.style.transition).toBe('none');
    expect(b.style.transition).toBe('none');

    // Next frame: released to identity under the spring easing.
    flushFrames();
    expect(a.style.transform).toBe('');
    expect(b.style.transform).toBe('');
    expect(a.style.transition).toBe('transform var(--motion-base) var(--ease-spring)');
    expect(b.style.transition).toBe('transform var(--motion-base) var(--ease-spring)');

    // Once the transition settles, the inline transition is handed back to the
    // stylesheet — no residue.
    fireEvent.transitionEnd(a);
    fireEvent.transitionEnd(b);
    expect(a.style.transition).toBe('');
    expect(a.style.transform).toBe('');
  });

  it('the shift after a CLOSE slides the survivors leftward', () => {
    const { container, rerender, props } = renderStrip([tab('a'), tab('b'), tab('c')]);

    rerender(<ChatTabStrip {...props} tabs={[tab('a'), tab('c')]} />);

    // a kept slot 0: untouched. c moved 200 → 100: inverted +100px.
    expect(tabNode(container, 'a').style.transform).toBe('');
    expect(tabNode(container, 'c').style.transform).toBe(`translateX(${TAB_SLOT_PX}px)`);
  });

  it('a newly opened tab appears in place — it does not slide in from nowhere', () => {
    const { container, rerender, props } = renderStrip([tab('a')]);

    rerender(<ChatTabStrip {...props} tabs={[tab('a'), tab('b')]} />);

    expect(tabNode(container, 'a').style.transform).toBe('');
    expect(tabNode(container, 'b').style.transform).toBe('');
  });

  it('respects prefers-reduced-motion: no transform is ever applied', () => {
    window.matchMedia = ((query: string) =>
      ({
        matches: query === '(prefers-reduced-motion: reduce)',
        media: query,
        onchange: null,
        addListener: () => {},
        removeListener: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList) as typeof window.matchMedia;

    const { container, rerender, props } = renderStrip([tab('a'), tab('b')]);
    rerender(<ChatTabStrip {...props} tabs={[tab('b'), tab('a')]} />);

    expect(tabNode(container, 'a').style.transform).toBe('');
    expect(tabNode(container, 'b').style.transform).toBe('');
    expect(frames).toHaveLength(0);
  });

  it('an unchanged re-render leaves every tab untouched', () => {
    const { container, rerender, props } = renderStrip([tab('a'), tab('b')]);
    rerender(<ChatTabStrip {...props} tabs={[tab('a'), tab('b')]} />);

    expect(tabNode(container, 'a').style.transform).toBe('');
    expect(tabNode(container, 'b').style.transform).toBe('');
  });

  it('the active tab still carries data-active (the accent strip is CSS-only)', () => {
    const { container } = renderStrip([tab('a'), tab('b')], { activeTabId: 'b' });
    expect(tabNode(container, 'b').dataset.active).toBe('true');
    expect(tabNode(container, 'a').dataset.active).toBeUndefined();
  });
});
