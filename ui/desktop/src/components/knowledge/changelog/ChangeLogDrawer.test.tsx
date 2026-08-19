import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ChangeLogDrawer } from './ChangeLogDrawer';

vi.mock('../KnowledgeContext', () => ({
  useKnowledge: () => ({ primaryKbId: 'kb-1', triggerGraphRefresh: vi.fn() }),
}));

vi.mock('../hooks/useHistory', () => ({
  useHistory: () => ({
    history: [],
    loading: false,
    error: null,
    refresh: vi.fn(),
    restore: vi.fn(),
  }),
}));

beforeEach(() => {
  vi.mocked(console.warn).mockClear();
});
afterEach(cleanup);

/**
 * ⚠ **This drawer warned on every open in a real browser while the whole suite
 * was green**, because `src/test/setup.ts` replaces `console.warn` with a
 * `vi.fn()` for the entire run — Radix's *"Missing `Description` or
 * `aria-describedby={undefined}` for {DialogContent}"* was swallowed, not
 * absent. The mock is read back here on purpose, and the DOM attribute is
 * asserted beside it because that is the thing a screen reader follows.
 *
 * The header is a bare title, so there is no visible line to promote into a
 * description; the drawer takes Radix's own opt-out rather than invent copy that
 * exists only in the accessibility tree.
 */
describe('ChangeLogDrawer — the description contract', () => {
  it('leaves Radix no aria-describedby to dangle', () => {
    render(
      <ChangeLogDrawer
        open
        onOpenChange={() => undefined}
        onPreview={() => undefined}
        onRestored={() => undefined}
      />
    );

    const drawer = screen.getByRole('dialog');
    expect(drawer).toHaveAccessibleName('Change log');
    expect(drawer.hasAttribute('aria-describedby')).toBe(false);
    expect(
      vi
        .mocked(console.warn)
        .mock.calls.some((call) => String(call[0]).includes('Missing `Description`'))
    ).toBe(false);
  });
});
