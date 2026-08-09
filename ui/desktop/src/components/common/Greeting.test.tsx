import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Greeting } from './Greeting';

describe('Greeting', () => {
  it('renders stable text without per-character animation wrappers', () => {
    const { container } = render(<Greeting />);

    expect(screen.getByRole('heading')).toHaveTextContent('What do you want to work on?');
    expect(container.querySelector('.char')).toBeNull();
  });

  /**
   * ⚠ The greeting is FIXED. It used to pick at random from fifteen lines in a
   * product-page register ("unlock", "bring us closer to a cure", "medical
   * mystery"), on the first screen the user sees. Two independent assertions,
   * because the two failure modes are different: prose can drift back into that
   * voice one word at a time, and the randomness can be reintroduced by a
   * refactor that means no harm.
   */
  it('is one fixed line, with no marketing register and no randomness', () => {
    const randomSpy = vi.spyOn(Math, 'random');
    const first = render(<Greeting />).container.textContent;
    const second = render(<Greeting />).container.textContent;

    expect(second).toBe(first);
    expect(randomSpy).not.toHaveBeenCalled();

    for (const word of [
      'unlock',
      'breakthrough',
      'cure',
      'mystery',
      'journey',
      'uncover',
      'hidden',
    ]) {
      expect(first?.toLowerCase()).not.toContain(word);
    }

    randomSpy.mockRestore();
  });
});
