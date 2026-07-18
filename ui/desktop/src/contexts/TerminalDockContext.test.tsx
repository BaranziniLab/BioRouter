import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { TerminalDockProvider, useTerminalDock } from './TerminalDockContext';

function setup() {
  const { result } = renderHook(() => useTerminalDock(), { wrapper: TerminalDockProvider });
  // useTerminalDock is non-null inside the provider.
  return result as { current: NonNullable<ReturnType<typeof useTerminalDock>> };
}

describe('TerminalDockProvider — per-chat-tab terminals', () => {
  it('opens a terminal for a tab and captures its cwd', () => {
    const ctx = setup();
    expect(ctx.current.isOpenFor('tab-1')).toBe(false);
    expect(ctx.current.terminals).toHaveLength(0);

    act(() => ctx.current.setOpen('tab-1', true, '/work/a'));

    expect(ctx.current.isOpenFor('tab-1')).toBe(true);
    expect(ctx.current.terminals).toEqual([
      { key: 'tab-1', workingDir: '/work/a', showing: true },
    ]);
  });

  it('keeps each tab independent — opening one never opens another', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('tab-1', true, '/work/a'));
    act(() => ctx.current.setOpen('tab-2', true, '/work/b'));

    expect(ctx.current.isOpenFor('tab-1')).toBe(true);
    expect(ctx.current.isOpenFor('tab-2')).toBe(true);
    expect(ctx.current.terminals.map((t) => t.key)).toEqual(['tab-1', 'tab-2']);
    expect(ctx.current.terminals.map((t) => t.workingDir)).toEqual(['/work/a', '/work/b']);
  });

  it('hiding keeps the entry and its panes alive (hide is not destroy)', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('tab-1', true, '/work/a'));
    act(() => ctx.current.setOpen('tab-1', false));

    expect(ctx.current.isOpenFor('tab-1')).toBe(false);
    // Still mounted so the shell keeps its pty running.
    expect(ctx.current.terminals).toEqual([
      { key: 'tab-1', workingDir: '/work/a', showing: false },
    ]);

    // Re-showing a hidden terminal keeps its FROZEN cwd — never respawns it.
    act(() => ctx.current.setOpen('tab-1', true, '/work/CHANGED'));
    expect(ctx.current.terminals).toEqual([
      { key: 'tab-1', workingDir: '/work/a', showing: true },
    ]);
  });

  it('does not overwrite the frozen cwd of an already-open terminal', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('tab-1', true, '/work/a'));
    act(() => ctx.current.setOpen('tab-1', true, '/work/b'));
    expect(ctx.current.terminals[0].workingDir).toBe('/work/a');
  });

  it('remove destroys the terminal; a fresh open then captures a NEW cwd', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('tab-1', true, '/work/a'));
    act(() => ctx.current.remove('tab-1'));

    expect(ctx.current.isOpenFor('tab-1')).toBe(false);
    expect(ctx.current.terminals).toHaveLength(0);

    // A brand-new terminal picks up the tab's current folder, not the stale one.
    act(() => ctx.current.setOpen('tab-1', true, '/work/fresh'));
    expect(ctx.current.terminals).toEqual([
      { key: 'tab-1', workingDir: '/work/fresh', showing: true },
    ]);
  });

  it('retain drops terminals of closed tabs and keeps the rest', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('tab-1', true, '/a'));
    act(() => ctx.current.setOpen('tab-2', true, '/b'));
    act(() => ctx.current.setOpen('tab-3', true, '/c'));

    act(() => ctx.current.retain(['tab-1', 'tab-3']));

    expect(ctx.current.terminals.map((t) => t.key)).toEqual(['tab-1', 'tab-3']);
    expect(ctx.current.isOpenFor('tab-2')).toBe(false);
  });

  it('setOpen(false) on an unknown / already-hidden key is a no-op', () => {
    const ctx = setup();
    act(() => ctx.current.setOpen('missing', false));
    expect(ctx.current.terminals).toHaveLength(0);

    act(() => ctx.current.setOpen('tab-1', true, '/a'));
    const before = ctx.current.terminals;
    act(() => ctx.current.setOpen('tab-1', false));
    act(() => ctx.current.setOpen('tab-1', false)); // second hide changes nothing
    expect(ctx.current.terminals[0].showing).toBe(false);
    expect(before).not.toBe(ctx.current.terminals); // the first hide did change it
  });
});
