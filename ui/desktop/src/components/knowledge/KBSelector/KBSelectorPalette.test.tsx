import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { KBSelectorPalette } from './KBSelectorPalette';
import type { Manifest } from '../../../api/types.gen';

const mocks = vi.hoisted(() => ({
  setPrimaryKbId: vi.fn(),
  toggleKbHidden: vi.fn(),
  followDefaultPrimary: vi.fn(),
  refreshDefaultPrimary: vi.fn().mockResolvedValue(undefined),
  refresh: vi.fn().mockResolvedValue(undefined),
  onClose: vi.fn(),
}));

const state = vi.hoisted(() => ({
  primaryKbId: 'alpha' as string | null,
  defaultPrimaryKb: null as Partial<Manifest> | null,
  canFollowDefaultPrimary: false,
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => ({
    bases: [
      { id: 'alpha', name: 'Alpha', color: '#cf6d47', tier: 'private' },
      { id: 'beta', name: 'Beta', color: '#b85a32', tier: 'public' },
    ],
    primaryKbId: state.primaryKbId,
    hiddenKbIds: ['beta'],
    defaultPrimaryKb: state.defaultPrimaryKb,
    canFollowDefaultPrimary: state.canFollowDefaultPrimary,
    followDefaultPrimary: mocks.followDefaultPrimary,
    refreshDefaultPrimary: mocks.refreshDefaultPrimary,
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

beforeEach(() => {
  vi.clearAllMocks();
  state.primaryKbId = 'alpha';
  state.defaultPrimaryKb = null;
  state.canFollowDefaultPrimary = false;
});

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

  // Issue #56 DR-18. The palette is the switch, so the tier has to be legible
  // BEFORE the user switches — a badge only on the base you already chose tells
  // you what you did, not what you are about to do.
  it('the tier is visible in the palette before the user switches to a base', () => {
    render(<KBSelectorPalette onClose={mocks.onClose} />);
    const priv = screen.getByRole('option', { name: /Alpha/ });
    const pub = screen.getByRole('option', { name: /Beta/ });
    expect(within(priv).getByText(/Private/)).toBeInTheDocument();
    // Public is the ordinary state and carries no badge, so the private one
    // reads as a marking rather than as a label everything wears.
    expect(within(pub).queryByText(/Private/)).toBeNull();
  });

  describe('following the default again', () => {
    // The machine-wide default is not part of the base list, so the palette has
    // to ask for it — and it asks when it opens, so a default changed in
    // another chat is not stale here.
    it('reads the machine-wide default when it opens', () => {
      render(<KBSelectorPalette onClose={mocks.onClose} />);
      expect(mocks.refreshDefaultPrimary).toHaveBeenCalled();
    });

    // A chat that is already following the default has nothing to inherit, so
    // the palette says nothing about it.
    it('says nothing to a chat that has not overridden the default', () => {
      render(<KBSelectorPalette onClose={mocks.onClose} />);
      expect(screen.queryByTestId('knowledge-kb-follow-default')).not.toBeInTheDocument();
    });

    // The state deleting a chat's pinned base leaves behind. Naming the base
    // makes the outcome of the click concrete rather than systemic.
    it('offers a chat with no primary the way back to the default', async () => {
      state.primaryKbId = null;
      state.defaultPrimaryKb = { id: 'alpha', name: 'Alpha' };
      state.canFollowDefaultPrimary = true;
      render(<KBSelectorPalette onClose={mocks.onClose} />);

      expect(screen.getByText(/no primary knowledge base/i)).toBeInTheDocument();
      const follow = screen.getByTestId('knowledge-kb-follow-default');
      expect(follow).toHaveTextContent('Alpha');

      await userEvent.click(follow);
      expect(mocks.followDefaultPrimary).toHaveBeenCalledTimes(1);
      expect(mocks.onClose).not.toHaveBeenCalled();
    });

    it('offers a chat that pinned its own primary the way back too', () => {
      state.primaryKbId = 'beta';
      state.defaultPrimaryKb = { id: 'alpha', name: 'Alpha' };
      state.canFollowDefaultPrimary = true;
      render(<KBSelectorPalette onClose={mocks.onClose} />);

      expect(screen.getByText(/its own primary/i)).toBeInTheDocument();
      expect(screen.getByTestId('knowledge-kb-follow-default')).toBeInTheDocument();
    });

    // The way back is one control for the whole chat, not a third state on
    // every row: the row stays "in this chat" plus "make it primary".
    it('does not add a control to any row', () => {
      state.primaryKbId = null;
      state.defaultPrimaryKb = { id: 'alpha', name: 'Alpha' };
      state.canFollowDefaultPrimary = true;
      render(<KBSelectorPalette onClose={mocks.onClose} />);

      expect(screen.getAllByRole('switch')).toHaveLength(2);
      expect(screen.getAllByTestId('knowledge-kb-follow-default')).toHaveLength(1);
    });
  });
});
