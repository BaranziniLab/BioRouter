import { describe, it, expect, afterEach } from 'vitest';
import { act, render, screen, cleanup } from '@testing-library/react';
import { ChatTabGhost } from './ChatDropOverlay';
import { DragGhost } from './useTabDragReorder';

const GHOST: DragGhost = {
  tabId: 'tab-1',
  title: 'Volcano plot',
  x: 120,
  y: 40,
  grabOffsetX: 12,
  grabOffsetY: 8,
};

afterEach(() => {
  cleanup();
  delete (window as { electron?: unknown }).electron;
});

/**
 * Install a minimal `window.electron` whose `on` records subscribers, and hand
 * back a way to push a main-process message into them.
 */
function stubDesktopBridge() {
  const listeners = new Map<string, Array<(event: unknown, ...args: unknown[]) => void>>();
  (window as unknown as { electron: unknown }).electron = {
    on: (channel: string, callback: (event: unknown, ...args: unknown[]) => void) => {
      const forChannel = listeners.get(channel) ?? [];
      forChannel.push(callback);
      listeners.set(channel, forChannel);
      return () =>
        listeners.set(
          channel,
          forChannel.filter((entry) => entry !== callback)
        );
    },
  };
  return {
    send(channel: string, payload: unknown) {
      act(() => {
        for (const callback of listeners.get(channel) ?? []) callback({}, payload);
      });
    },
    count: (channel: string) => listeners.get(channel)?.length ?? 0,
  };
}

/**
 * This asserts only the hook the CSS selects on — the flat, dashed-outline look
 * itself is `.br-tab-ghost[data-detach='true']` in `main.css`. jsdom applies no
 * stylesheet, so it could not assert anything else anyway.
 */
describe('ChatTabGhost detach state', () => {
  it('marks the ghost detached when the drag has left the window', () => {
    render(<ChatTabGhost ghost={GHOST} detached />);
    expect(screen.getByTestId('chat-tab-ghost')).toHaveAttribute('data-detach', 'true');
  });

  it('leaves the attribute ABSENT — not "false" — for an ordinary in-window drag', () => {
    // `[data-detach='true']` would not match `data-detach="false"` either, but an
    // absent attribute is the one state that cannot be mistaken for a set one by
    // a later `[data-detach]` selector.
    render(<ChatTabGhost ghost={GHOST} />);
    expect(screen.getByTestId('chat-tab-ghost')).not.toHaveAttribute('data-detach');
  });

  it('still renders the tab title in both states', () => {
    render(<ChatTabGhost ghost={GHOST} detached />);
    expect(screen.getByTestId('chat-tab-ghost')).toHaveTextContent('Volcano plot');
  });
});

/**
 * The renderer's half of the cross-desktop ghost (issue #75, Phase 4b).
 *
 * ⚠ THE FEATURE ITSELF IS NOT TESTABLE HERE AND IS NOT TESTED HERE. Whether a
 * real `BrowserWindow` follows the cursor onto the desktop, and whether opening
 * it steals the focus that would end the drag, needs a second window, a screen
 * and real pointer capture — jsdom has none of the three. What this file can
 * cover is the contract between the two halves: the numbers main reads off this
 * element, and this element getting out of the way when main says its window is
 * up.
 */
describe('ChatTabGhost and the OS ghost window', () => {
  it('publishes the grab offset main needs to place the OS ghost', () => {
    // `tab-drag:move` carries only `{screenX, screenY}`, so these attributes are
    // the ONLY way the offset reaches the main process. They are also the same
    // offset `tornOffWindowBounds` subtracts, which is what makes the ghost and
    // the window it becomes land at one origin.
    const el = render(<ChatTabGhost ghost={GHOST} detached />).getByTestId('chat-tab-ghost');
    expect(el).toHaveAttribute('data-grab-x', '12');
    expect(el).toHaveAttribute('data-grab-y', '8');
  });

  it('keeps the class main probes for', () => {
    // `GHOST_PROBE_SCRIPT` in dragGhostWindow.ts selects `.br-tab-ghost`.
    expect(render(<ChatTabGhost ghost={GHOST} />).getByTestId('chat-tab-ghost')).toHaveClass(
      'br-tab-ghost'
    );
  });

  it('hides itself while the OS ghost is up, and comes back when it goes', () => {
    const bridge = stubDesktopBridge();
    render(<ChatTabGhost ghost={GHOST} detached />);
    const el = screen.getByTestId('chat-tab-ghost');
    expect(el.style.visibility).toBe('');

    bridge.send('tab-drag:ghost-window', { active: true });
    // Two tabs in the air at once — one pinned at the window edge, one on the
    // desktop — reads as a duplication bug, which is why this is not optional.
    expect(el.style.visibility).toBe('hidden');
    expect(el).toHaveAttribute('data-os-ghost', 'true');

    bridge.send('tab-drag:ghost-window', { active: false });
    expect(el.style.visibility).toBe('');
    expect(el).not.toHaveAttribute('data-os-ghost');
  });

  it('stays in the DOM while hidden rather than unmounting', () => {
    // Hidden, not gone: coming back inside the window restores it in the same
    // frame, and main can still read its measurements.
    const bridge = stubDesktopBridge();
    render(<ChatTabGhost ghost={GHOST} detached />);
    bridge.send('tab-drag:ghost-window', { active: true });
    expect(screen.getByTestId('chat-tab-ghost')).toBeInTheDocument();
  });

  it('unsubscribes on unmount, so the next drag does not start hidden', () => {
    const bridge = stubDesktopBridge();
    const view = render(<ChatTabGhost ghost={GHOST} detached />);
    expect(bridge.count('tab-drag:ghost-window')).toBe(1);
    view.unmount();
    expect(bridge.count('tab-drag:ghost-window')).toBe(0);
  });

  it('renders with no desktop bridge at all', () => {
    // The browser harness and every test above run without `window.electron`.
    expect(() => render(<ChatTabGhost ghost={GHOST} detached />)).not.toThrow();
    expect(screen.getByTestId('chat-tab-ghost').style.visibility).toBe('');
  });

  it('ignores a malformed payload instead of hiding on it', () => {
    const bridge = stubDesktopBridge();
    render(<ChatTabGhost ghost={GHOST} detached />);
    bridge.send('tab-drag:ghost-window', undefined);
    bridge.send('tab-drag:ghost-window', 'yes');
    expect(screen.getByTestId('chat-tab-ghost').style.visibility).toBe('');
  });
});
