import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Greeting, resetGreetingAnimationForTests } from './Greeting';

const animate = vi.fn();
vi.mock('../../hooks/use-text-animator', () => ({
  useTextAnimator: (opts: { text: string; enabled?: boolean }) => {
    animate(opts.enabled !== false);
    return { current: null };
  },
}));

beforeEach(() => {
  animate.mockClear();
  resetGreetingAnimationForTests();
});

describe('Greeting', () => {
  it('renders the fixed line', () => {
    render(<Greeting />);
    expect(screen.getByRole('heading')).toHaveTextContent('What do you want to work on?');
  });

  /**
   * ⚠ This is the rule `010bf68e` was reaching for and overshot.
   *
   * That commit removed the animation outright because `BaseChat` renders
   * `<Greeting key={sessionId}>`, so every remount replayed it: reopening a
   * saved chat, a renderer reload, switching back to a tab. An animation firing
   * when nothing new happened reads as a glitch.
   *
   * The answer is the gate, not the removal. First mount of a chat animates;
   * every later mount of that same chat does not.
   */
  it('unrolls once for a new chat and stays still on every remount of it', () => {
    render(<Greeting animateOnceFor="chat-a" />);
    expect(animate).toHaveBeenLastCalledWith(true);

    // Same chat again: reopened, reloaded, or tabbed back to.
    render(<Greeting animateOnceFor="chat-a" />);
    expect(animate).toHaveBeenLastCalledWith(false);

    // A genuinely different chat is new, so it plays.
    render(<Greeting animateOnceFor="chat-b" />);
    expect(animate).toHaveBeenLastCalledWith(true);
  });

  it('never animates when no chat is named', () => {
    render(<Greeting />);
    expect(animate).toHaveBeenLastCalledWith(false);
  });

  /**
   * ⚠ The greeting is FIXED. It used to pick at random from fifteen lines in a
   * product-page register ("unlock", "bring us closer to a cure", "medical
   * mystery"), on the first screen the user sees. Two assertions, because the
   * two failure modes differ: prose drifting back into that voice one word at a
   * time, and the randomness returning via a refactor that means no harm.
   */
  it('is one fixed line, with no marketing register and no randomness', () => {
    const randomSpy = vi.spyOn(Math, 'random');
    const first = render(<Greeting />).container.textContent;
    const second = render(<Greeting />).container.textContent;

    expect(second).toBe(first);
    expect(randomSpy).not.toHaveBeenCalled();

    for (const word of ['unlock', 'breakthrough', 'cure', 'mystery', 'journey', 'uncover', 'hidden']) {
      expect(first?.toLowerCase()).not.toContain(word);
    }
    randomSpy.mockRestore();
  });
});
