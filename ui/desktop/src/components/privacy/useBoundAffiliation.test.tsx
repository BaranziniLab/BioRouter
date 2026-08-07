import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useBoundAffiliation } from './useBoundAffiliation';

const mocks = vi.hoisted(() => ({
  getProviders: vi.fn(),
  currentProvider: 'versa_azure' as string | null,
}));

vi.mock('../ConfigContext', () => ({
  useConfig: () => ({ getProviders: mocks.getProviders }),
}));
vi.mock('../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({ currentProvider: mocks.currentProvider }),
}));

const ucsfRow = {
  name: 'versa_azure',
  is_configured: true,
  provider_type: 'Preferred',
  metadata: { name: 'versa_azure', tier: 'private' },
  affiliation: { kind: 'institutions', institutions: [{ id: 'ucsf', display_name: 'UCSF' }] },
};
const publicRow = {
  name: 'openai',
  is_configured: true,
  provider_type: 'Builtin',
  metadata: { name: 'openai', tier: 'public' },
  affiliation: null,
};

/**
 * The chat-name pill's data path (issue #56, DR-26). `BaseChat` calls this hook
 * and passes the result to `SessionNamePill`; without a test here that door of
 * the capability is wired but unguarded, which is how a badge ships rendering
 * nothing on the one surface nobody opened while testing.
 */
describe('useBoundAffiliation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentProvider = 'versa_azure';
    mocks.getProviders.mockResolvedValue([ucsfRow, publicRow]);
  });

  it('resolves the bound provider’s institution', async () => {
    const { result } = renderHook(() => useBoundAffiliation());
    await waitFor(() =>
      expect(result.current).toEqual({
        kind: 'institutions',
        institutions: [{ id: 'ucsf', display_name: 'UCSF' }],
      })
    );
  });

  /**
   * ⚠ It reads the ROW, not `row.metadata`. The affiliation is served beside the
   * metadata precisely because `ProviderMetadata` is the type-level claim; a
   * hook that looked inside it would find nothing and silently render no badge.
   */
  it('reads the row, not the metadata', async () => {
    // The two carry DIFFERENT affiliations, so this discriminates instead of
    // racing: a hook that read `row.metadata` would settle on `institutions`,
    // and one that reads the row settles on `local`. Asserting a value through
    // `waitFor` — rather than asserting null after the fetch was merely called —
    // is what makes the wrong answer a failure instead of a timing accident.
    mocks.getProviders.mockResolvedValue([
      {
        ...ucsfRow,
        affiliation: { kind: 'local', institutions: [] },
        metadata: { ...ucsfRow.metadata, affiliation: ucsfRow.affiliation },
      },
    ]);
    const { result } = renderHook(() => useBoundAffiliation());
    await waitFor(() => expect(result.current).toEqual({ kind: 'local', institutions: [] }));
  });

  it('says nothing for a public model, which has no affiliation', async () => {
    mocks.currentProvider = 'openai';
    const { result } = renderHook(() => useBoundAffiliation());
    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });

  it('says nothing when no provider is bound, and never asks', async () => {
    mocks.currentProvider = null;
    const { result } = renderHook(() => useBoundAffiliation());
    expect(result.current).toBeNull();
    expect(mocks.getProviders).not.toHaveBeenCalled();
  });

  /**
   * A catalog it cannot read is not evidence of an affiliation. Failing safe on
   * this axis means saying nothing, because every value here is a claim about
   * whose agreements cover a transcript.
   */
  it('asserts nothing when the provider catalog cannot be read', async () => {
    mocks.getProviders.mockRejectedValue(new Error('daemon down'));
    const { result } = renderHook(() => useBoundAffiliation());
    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });

  it('says nothing when the bound provider is not in the catalog at all', async () => {
    mocks.currentProvider = 'a-provider-this-install-does-not-publish';
    const { result } = renderHook(() => useBoundAffiliation());
    await waitFor(() => expect(mocks.getProviders).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });
});
