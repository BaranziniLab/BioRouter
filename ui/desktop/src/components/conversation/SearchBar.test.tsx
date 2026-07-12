import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import SearchBar from './SearchBar';

describe('SearchBar', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    Reflect.deleteProperty(window, 'matchMedia');
  });

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

  it('cancels an in-progress close when the user invokes find again', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    render(<SearchBar onSearch={vi.fn()} onClose={onClose} />);

    fireEvent.click(screen.getByTitle('Close (Esc)'));
    fireEvent.keyDown(window, { key: 'f', metaKey: true });
    act(() => vi.advanceTimersByTime(150));

    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId('conversation-search-bar').parentElement).toHaveClass(
      'search-bar-enter'
    );
  });

  it('closes immediately when reduced motion is requested', () => {
    const onClose = vi.fn();
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn(() => ({ matches: true }) as never),
    });
    render(<SearchBar onSearch={vi.fn()} onClose={onClose} />);

    fireEvent.click(screen.getByTitle('Close (Esc)'));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clears the delayed close when it unmounts', () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    const { unmount } = render(<SearchBar onSearch={vi.fn()} onClose={onClose} />);

    fireEvent.click(screen.getByTitle('Close (Esc)'));
    unmount();
    act(() => vi.advanceTimersByTime(150));

    expect(onClose).not.toHaveBeenCalled();
  });
});
