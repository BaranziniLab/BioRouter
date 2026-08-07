import { describe, it, expect, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
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

afterEach(cleanup);

/**
 * The styling for `data-detach` lands with the rest of the tear-off visuals
 * (design §8 Phase 4), so this asserts only the hook the CSS will select on.
 * jsdom applies no stylesheet, so it could not assert anything else anyway.
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
