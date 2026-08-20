import { render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useTextAnimator } from './use-text-animator';

/**
 * The unroll's contract is about the FIRST FRAME, so that is what these pin.
 *
 * The greeting is rendered with its text at full opacity and the animator is
 * what hides the characters so they can arrive. That makes *when* the animator
 * runs the whole behaviour: a turn taken after the browser has painted shows
 * the finished sentence, then blanks it, then unrolls it — the answer, a blink
 * of nothing, then the answer again. A layout effect runs before the paint, so
 * the characters are already hidden in the first frame the user ever sees.
 *
 * jsdom never paints, so none of this can be observed the way a user observes
 * it. What jsdom *can* answer exactly is the question underneath: had the work
 * happened by the time the commit finished, or only later? These assert against
 * un-advanced fake timers, so "later" fails whether it is one frame later or a
 * hundred milliseconds later.
 */

const started: Array<{ el: Element; delay: number }> = [];

function Harness({ text, enabled = true }: { text: string; enabled?: boolean }) {
  const ref = useTextAnimator({ text, enabled });
  return (
    <h1>
      <span ref={ref}>{text}</span>
    </h1>
  );
}

beforeEach(() => {
  started.length = 0;
  vi.useFakeTimers();
  // jsdom implements no Web Animations API. The stub records what was started
  // and returns just enough of an `Animation` for the animator to drive.
  Element.prototype.animate = function (
    this: Element,
    _keyframes: unknown,
    options: { delay?: number }
  ) {
    started.push({ el: this, delay: options?.delay ?? 0 });
    return { cancel: () => {}, onfinish: null } as unknown as globalThis.Animation;
  } as unknown as Element['animate'];
  window.matchMedia = ((q: string) => ({
    matches: false,
    media: q,
    addEventListener: () => {},
    removeEventListener: () => {},
  })) as unknown as typeof window.matchMedia;
});

afterEach(() => {
  vi.useRealTimers();
});

describe('useTextAnimator', () => {
  it('has hidden the characters and started the unroll before the first paint', () => {
    const { container } = render(<Harness text="Hello there" />);

    // NOTHING is advanced between the render and these assertions. Anything the
    // animator defers — a `setTimeout`, or simply being a `useEffect` that the
    // browser paints ahead of — leaves both of these unsatisfied.
    expect(started.length).toBeGreaterThan(0);

    const chars = container.querySelectorAll<HTMLElement>('.char');
    expect(chars.length).toBeGreaterThan(0);
    expect([...chars].every((c) => c.style.opacity === '0')).toBe(true);
  });

  it('staggers the characters rather than revealing them together', () => {
    render(<Harness text="Hello there" />);
    const delays = started.map((s) => s.delay);
    expect(new Set(delays).size).toBeGreaterThan(1);
    expect(delays).toEqual([...delays].sort((a, b) => a - b));
  });

  /**
   * ⚠ The two ways out of this hook must leave the text VISIBLE, which is why
   * the hiding lives in the animator and not in the rendered markup. A
   * `style={{opacity: 0}}` on the span would kill the flash just as well and
   * would make these two cases render nothing at all, permanently.
   */
  it('leaves the text alone when animation is switched off at the call site', () => {
    const { container } = render(<Harness text="Hello there" enabled={false} />);
    expect(started).toHaveLength(0);
    expect(container.querySelector('span')?.style.opacity).not.toBe('0');
    expect(container.textContent).toBe('Hello there');
  });

  it('leaves the text alone under prefers-reduced-motion', () => {
    window.matchMedia = ((q: string) => ({
      matches: true,
      media: q,
      addEventListener: () => {},
      removeEventListener: () => {},
    })) as unknown as typeof window.matchMedia;

    const { container } = render(<Harness text="Hello there" />);
    expect(started).toHaveLength(0);
    expect(container.querySelector('span')?.style.opacity).not.toBe('0');
    expect(container.textContent).toBe('Hello there');
  });
});
