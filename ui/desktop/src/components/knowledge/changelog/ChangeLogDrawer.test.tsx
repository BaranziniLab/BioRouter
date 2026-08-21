import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';
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

let warn: MockInstance;

beforeEach(() => {
  warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
});
afterEach(() => {
  warn.mockRestore();
  cleanup();
});

/**
 * ⚠ **This drawer warned on every open in a real browser while the whole suite
 * was green** while warnings were globally muted. The local spy and the DOM
 * assertion pin both the diagnostic and the broken reference a screen reader
 * would follow.
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
    expect(warn.mock.calls.some((call) => String(call[0]).includes('Missing `Description`'))).toBe(
      false
    );
  });
});
