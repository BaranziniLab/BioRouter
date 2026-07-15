import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Greeting } from './Greeting';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Greeting', () => {
  it('renders stable text without per-character animation wrappers', () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);
    const { container } = render(<Greeting />);

    expect(screen.getByRole('heading')).toHaveTextContent(
      'What insights will your data reveal today?'
    );
    expect(container.querySelector('.char')).toBeNull();
  });
});
