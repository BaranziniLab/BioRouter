import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import SearchBar from './SearchBar';

describe('SearchBar', () => {
  it('renders as a centered floating search surface', () => {
    render(<SearchBar onSearch={vi.fn()} onClose={vi.fn()} />);

    const surface = screen.getByTestId('conversation-search-bar');

    // A floating search panel is a popover: 16px radius, --shadow-popover, and an
    // opaque fill (no backdrop blur). See design.md §3.4, §3.5 and the anti-patterns.
    expect(surface).toHaveClass('max-w-[720px]');
    expect(surface).toHaveClass('rounded-2xl');
    expect(surface).toHaveClass('shadow-popover');
    expect(surface).not.toHaveClass('backdrop-blur-md');
  });

  it('keeps search behavior while using the compact surface', () => {
    const onSearch = vi.fn();
    render(<SearchBar onSearch={onSearch} onClose={vi.fn()} />);

    fireEvent.change(screen.getByPlaceholderText('Search conversation...'), {
      target: { value: 'agent' },
    });

    expect(screen.getByDisplayValue('agent')).toBeInTheDocument();
  });
});
