import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { KBSelectorMenu } from './KBSelectorMenu';
import type { Manifest } from '../../../api/types.gen';

/**
 * The DR-12 primary-repair contract, ported wholesale from
 * `KBSelectorPalette.test.tsx` when §4.2's file split retired that file.
 *
 * ⚠ These cases are PORTED, not re-derived. Six of the old palette's cases
 * encoded "following the default again", which is the behaviour that makes a
 * chat whose pinned base was deleted recoverable; deleting the file and writing
 * fresh tests for the two new ones would have dropped that contract silently.
 */

const mocks = vi.hoisted(() => ({
  setPrimaryKbId: vi.fn(),
  followDefaultPrimary: vi.fn(),
  refreshDefaultPrimary: vi.fn().mockResolvedValue(undefined),
  refresh: vi.fn().mockResolvedValue(undefined),
  onClose: vi.fn(),
  onManage: vi.fn(),
  onCreate: vi.fn(),
}));

const state = vi.hoisted(() => ({
  primaryKbId: 'alpha' as string | null,
  defaultPrimaryKb: null as Partial<Manifest> | null,
  canFollowDefaultPrimary: false,
}));

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => ({
    visibleBases: [
      { id: 'alpha', name: 'Alpha', color: '#cf6d47', tier: 'private' },
      { id: 'beta', name: 'Beta', color: '#b85a32', tier: 'public' },
    ],
    primaryKbId: state.primaryKbId,
    defaultPrimaryKb: state.defaultPrimaryKb,
    canFollowDefaultPrimary: state.canFollowDefaultPrimary,
    followDefaultPrimary: mocks.followDefaultPrimary,
    refreshDefaultPrimary: mocks.refreshDefaultPrimary,
    refresh: mocks.refresh,
    setPrimaryKbId: mocks.setPrimaryKbId,
  }),
}));

function renderMenu() {
  return render(
    <KBSelectorMenu onClose={mocks.onClose} onManage={mocks.onManage} onCreate={mocks.onCreate} />
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  state.primaryKbId = 'alpha';
  state.defaultPrimaryKb = null;
  state.canFollowDefaultPrimary = false;
});

describe('KBSelectorMenu', () => {
  // The picker is the "which base am I pointed at" surface, so picking one is a
  // navigation and the popover closes behind it. (The manager, which is where
  // the collection is edited, deliberately does NOT close — see its own test.)
  it('makes a base primary and closes', async () => {
    renderMenu();
    await userEvent.click(screen.getByText('Beta'));
    expect(mocks.setPrimaryKbId).toHaveBeenCalledWith('beta');
    expect(mocks.onClose).toHaveBeenCalledTimes(1);
  });

  // Issue #56 DR-18. The picker is the switch, so the tier has to be legible
  // BEFORE the user switches — Public is quiet so the private marking stays a
  // marking.
  it('marks a private base before the user switches to it', () => {
    renderMenu();
    const priv = screen.getByRole('option', { name: /Alpha/ });
    const pub = screen.getByRole('option', { name: /Beta/ });
    expect(within(priv).queryByTestId('privacy-badge')).not.toBeNull();
    expect(within(pub).queryByTestId('privacy-badge')).toBeNull();
  });

  it('routes management and creation out of the picker', async () => {
    renderMenu();
    await userEvent.click(screen.getByTestId('knowledge-kb-open-manager'));
    expect(mocks.onManage).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByTestId('knowledge-kb-open-create'));
    expect(mocks.onCreate).toHaveBeenCalledTimes(1);
  });

  describe('following the default again', () => {
    // The machine-wide default is not part of the base list, so the picker has
    // to ask for it — and it asks when it opens, so a default changed in
    // another chat is not stale here.
    it('reads the machine-wide default when it opens', () => {
      renderMenu();
      expect(mocks.refreshDefaultPrimary).toHaveBeenCalled();
    });

    // A chat that is already following the default has nothing to inherit, so
    // the picker says nothing about it.
    it('says nothing to a chat that has not overridden the default', () => {
      renderMenu();
      expect(screen.queryByTestId('knowledge-kb-follow-default')).not.toBeInTheDocument();
    });

    // The state deleting a chat's pinned base leaves behind. Naming the base
    // makes the outcome of the click concrete rather than systemic.
    it('offers a chat with no primary the way back to the default', async () => {
      state.primaryKbId = null;
      state.defaultPrimaryKb = { id: 'alpha', name: 'Alpha' };
      state.canFollowDefaultPrimary = true;
      renderMenu();

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
      renderMenu();

      expect(screen.getByText(/its own primary/i)).toBeInTheDocument();
      expect(screen.getByTestId('knowledge-kb-follow-default')).toBeInTheDocument();
    });

    // The way back is one control for the whole chat, not a third state on
    // every row: the row stays "pick this base", and nothing else.
    it('does not add a control to any row', () => {
      state.primaryKbId = null;
      state.defaultPrimaryKb = { id: 'alpha', name: 'Alpha' };
      state.canFollowDefaultPrimary = true;
      renderMenu();

      expect(screen.getAllByTestId('knowledge-kb-follow-default')).toHaveLength(1);
      for (const row of screen.getAllByRole('option')) {
        expect(within(row).queryByRole('button')).toBeNull();
      }
    });
  });
});
