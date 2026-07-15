import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { BottomMenuKnowledgeSelection } from './BottomMenuKnowledgeSelection';

const mocks = vi.hoisted(() => ({
  toggleKbHidden: vi.fn(),
  hideAllKnowledgeBases: vi.fn(),
  showAllKnowledgeBases: vi.fn(),
}));

vi.mock('../knowledge/KnowledgeContext', () => ({
  useKnowledge: () => ({
    bases: [
      { id: 'soul', name: 'Soul' },
      { id: 'brainstorm', name: 'brainstorm' },
    ],
    visibleBases: [
      { id: 'soul', name: 'Soul' },
      { id: 'brainstorm', name: 'brainstorm' },
    ],
    hiddenKbIds: [],
    toggleKbHidden: mocks.toggleKbHidden,
    hideAllKnowledgeBases: mocks.hideAllKnowledgeBases,
    showAllKnowledgeBases: mocks.showAllKnowledgeBases,
  }),
}));

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('BottomMenuKnowledgeSelection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('uses the same compact searchable menu layout as skills', async () => {
    const user = userEvent.setup();
    render(<BottomMenuKnowledgeSelection />);

    const trigger = screen.getByRole('button', { name: 'Manage knowledge bases (2 visible)' });
    expect(trigger).not.toHaveAttribute('title');

    await user.hover(trigger);
    expect(await screen.findByRole('tooltip')).toHaveTextContent('Manage knowledge bases');
    expect(screen.queryByText('2 KBs visible')).not.toBeInTheDocument();

    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false });

    const menu = await screen.findByRole('menu');
    expect(menu).toHaveClass('w-64', 'font-sans');
    expect(screen.getByPlaceholderText('search knowledge bases...')).toHaveClass('h-8', 'text-sm');
    expect(screen.queryByText('Chat knowledge discovery')).not.toBeInTheDocument();

    const soul = screen.getByRole('menuitemcheckbox', { name: /Soul/ });
    expect(soul).toHaveClass('px-2', 'py-2');
    expect(screen.getByText('Soul')).toHaveClass('text-sm', 'font-medium');

    fireEvent.change(screen.getByPlaceholderText('search knowledge bases...'), {
      target: { value: 'brain' },
    });
    expect(screen.queryByText('Soul')).not.toBeInTheDocument();
    expect(screen.getByText('brainstorm')).toBeInTheDocument();
  });
});
