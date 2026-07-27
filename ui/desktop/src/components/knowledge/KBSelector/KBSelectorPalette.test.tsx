import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { KBSelectorPalette } from './KBSelectorPalette';

const mocks = vi.hoisted(() => ({
  setPrimaryKbId: vi.fn(),
  toggleKbHidden: vi.fn(),
  refresh: vi.fn().mockResolvedValue(undefined),
  onClose: vi.fn(),
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => ({
    bases: [
      { id: 'alpha', name: 'Alpha', color: '#cf6d47' },
      { id: 'beta', name: 'Beta', color: '#b85a32' },
    ],
    primaryKbId: 'alpha',
    hiddenKbIds: ['beta'],
    refresh: mocks.refresh,
    setPrimaryKbId: mocks.setPrimaryKbId,
    toggleKbHidden: mocks.toggleKbHidden,
  }),
}));

vi.mock('../hooks/useKnowledgeBases', () => ({
  useKnowledgeBases: () => ({
    create: vi.fn(),
    exportArchive: vi.fn(),
    importArchive: vi.fn(),
    remove: vi.fn(),
    rename: vi.fn(),
  }),
}));

beforeAll(() => {
  // The palette pulls in Radix primitives that observe their trigger.
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterAll(() => vi.unstubAllGlobals());

beforeEach(() => vi.clearAllMocks());

describe('KBSelectorPalette', () => {
  // Two states per row, never three. Under the merged model membership and
  // the primary are the only two things a base can be, and the row body is
  // the "make primary" affordance.
  it('offers exactly one membership switch per row', () => {
    render(<KBSelectorPalette onClose={mocks.onClose} />);
    expect(screen.getByLabelText('Include Alpha in this chat')).toBeInTheDocument();
    expect(screen.getByLabelText('Include Beta in this chat')).toBeInTheDocument();
    expect(screen.getAllByRole('switch')).toHaveLength(2);
  });

  // Picking a primary used to close the palette, which made the selector feel
  // like a radio group over a single-active model. It is now a place you stay.
  it('makes a base primary without closing the palette', async () => {
    render(<KBSelectorPalette onClose={mocks.onClose} />);
    await userEvent.click(screen.getByText('Beta'));
    expect(mocks.setPrimaryKbId).toHaveBeenCalledWith('beta');
    expect(mocks.onClose).not.toHaveBeenCalled();
  });

  it('marks the primary', () => {
    render(<KBSelectorPalette onClose={mocks.onClose} />);
    expect(screen.getByText('Primary')).toBeInTheDocument();
  });
});
