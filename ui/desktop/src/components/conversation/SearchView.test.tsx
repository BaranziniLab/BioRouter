import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SearchView } from './SearchView';

const highlighterMocks = vi.hoisted(() => ({
  highlight: vi.fn(() => []),
  clearHighlights: vi.fn(),
  destroy: vi.fn(),
  setCurrentMatch: vi.fn(),
}));

vi.mock('./SearchBar', () => ({
  default: ({ onSearch }: { onSearch: (term: string, caseSensitive: boolean) => void }) => (
    <div>
      <button onClick={() => onSearch('alpha', false)}>Search alpha</button>
      <button onClick={() => onSearch('beta', false)}>Search beta</button>
    </div>
  ),
}));

vi.mock('../../utils/searchHighlighter', () => ({
  SearchHighlighter: class {
    highlight = highlighterMocks.highlight;
    clearHighlights = highlighterMocks.clearHighlights;
    destroy = highlighterMocks.destroy;
    setCurrentMatch = highlighterMocks.setCurrentMatch;
  },
}));

describe('SearchView', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    Object.assign(window, {
      electron: {
        platform: 'darwin',
        on: vi.fn(() => vi.fn()),
      },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('cancels a pending highlight when a newer term arrives', () => {
    render(
      <SearchView>
        <p>alpha beta</p>
      </SearchView>
    );

    fireEvent.keyDown(window, { key: 'f', metaKey: true });
    fireEvent.click(screen.getByText('Search alpha'));
    fireEvent.click(screen.getByText('Search beta'));
    act(() => vi.advanceTimersByTime(150));

    expect(highlighterMocks.highlight).toHaveBeenCalledTimes(1);
    expect(highlighterMocks.highlight).toHaveBeenCalledWith('beta', false);
  });
});
