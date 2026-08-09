import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Greeting } from './Greeting';

const animate = vi.fn();
vi.mock('../../hooks/use-text-animator', () => ({
  useTextAnimator: (opts: { text: string; enabled?: boolean }) => {
    animate(opts.enabled !== false);
    return { current: null };
  },
}));

beforeEach(() => animate.mockClear());

describe('Greeting', () => {
  it('renders one of the stock sentences', () => {
    render(<Greeting />);
    expect(screen.getByRole('heading').textContent).toMatch(/\?$/);
  });

  /**
   * ⚠ The rotation is deliberate product voice. It was removed once as
   * marketing register and restored on the operator's instruction: a different
   * line on each arrival is the intent, so a change that collapses this to one
   * fixed sentence should fail here rather than pass quietly.
   */
  it('draws a different sentence across arrivals', () => {
    const seen = new Set<string>();
    for (let i = 0; i < 40; i++) {
      const { container, unmount } = render(<Greeting />);
      seen.add(container.textContent ?? '');
      unmount();
    }
    expect(seen.size).toBeGreaterThan(1);
  });

  /**
   * ⚠ It unrolls on EVERY mount. `010bf68e` removed the animator outright and a
   * later pass gated it to once per chat; both were wrong the same way. Home, a
   * new window and a new chat are all arrivals, and an arrival is exactly when
   * the unroll belongs. `prefers-reduced-motion` is the accessibility answer,
   * and the animator already honours it.
   */
  it('animates on every mount, and can be switched off explicitly', () => {
    render(<Greeting />);
    expect(animate).toHaveBeenLastCalledWith(true);

    render(<Greeting />);
    expect(animate).toHaveBeenLastCalledWith(true);

    render(<Greeting animate={false} />);
    expect(animate).toHaveBeenLastCalledWith(false);
  });

  it('keeps its sentence stable across a re-render of the same instance', () => {
    // A re-render must not swap the text out from under a running animation.
    const { container, rerender } = render(<Greeting />);
    const first = container.textContent;
    rerender(<Greeting />);
    expect(container.textContent).toBe(first);
  });
});
