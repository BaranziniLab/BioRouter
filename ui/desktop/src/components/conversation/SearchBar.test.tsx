import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import SearchBar from './SearchBar';

describe('SearchBar', () => {
  it('renders as a centered floating search surface', () => {
    render(<SearchBar onSearch={vi.fn()} onClose={vi.fn()} />);

    const surface = screen.getByTestId('conversation-search-bar');

    expect(surface).toHaveClass('max-w-[720px]');
    expect(surface).toHaveClass('rounded-xl');
    expect(surface).toHaveClass(
      'shadow-[0_18px_44px_-34px_rgba(32,25,15,0.42),0_1px_0_rgba(32,25,15,0.04)]'
    );
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
