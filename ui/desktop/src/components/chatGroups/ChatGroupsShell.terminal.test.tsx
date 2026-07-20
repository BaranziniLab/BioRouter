import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

/**
 * The terminal is PER CHAT TAB, scoped by the shell.
 *
 * These pin the wiring that makes "each tab has its own terminal, and switching
 * tabs switches which one you see" true, without mounting the real BaseChat or a
 * real xterm dock:
 *   - BaseChat is handed its tab's id as `terminalKey`, so the terminal is tied
 *     to the tab (not the rebindable session).
 *   - one dock is rendered per open terminal, but only the ACTIVE tab's is `open`
 *     (visible); the others stay mounted-but-hidden so their shells keep running.
 *   - closed tabs' terminals are pruned via `retain`, which disposes them.
 */

let lastBaseChatProps: Record<string, unknown> = {};
vi.mock('../BaseChat', () => ({
  default: (props: Record<string, unknown>) => {
    lastBaseChatProps = props;
    return <div data-testid="basechat" />;
  },
}));

// Capture the props of every dock the shell renders (React strips `key`, so the
// docks are identified by their cwd, which is unique per tab here).
const renderedDocks: Array<{ open: boolean; workingDir?: string }> = [];
vi.mock('../InAppTerminalDock', () => ({
  default: (props: { open: boolean; workingDir?: string }) => {
    renderedDocks.push({ open: props.open, workingDir: props.workingDir });
    return <div data-testid="dock" data-open={String(props.open)} data-cwd={props.workingDir} />;
  },
}));

// Active group g1, active tab t1, with a second tab t2 in the same group.
vi.mock('../../contexts/ChatGroupsContext', () => ({
  useChatGroups: () => ({
    dispatch: vi.fn(),
    runningSessionIds: [],
    state: {
      activeGroupId: 'g1',
      layout: { kind: 'leaf', groupId: 'g1' },
      groups: {
        g1: {
          id: 'g1',
          activeTabId: 't1',
          tabs: [
            { tabId: 't1', sessionId: 'sess-1', title: 'One', userSetName: false },
            { tabId: 't2', sessionId: 'sess-2', title: 'Two', userSetName: false },
          ],
        },
      },
    },
  }),
}));

const retain = vi.fn();
let terminals: Array<{ key: string; workingDir?: string; showing: boolean }> = [];
vi.mock('../../contexts/TerminalDockContext', () => ({
  useTerminalDock: () => ({
    isOpenFor: (key: string) => terminals.some((t) => t.key === key && t.showing),
    setOpen: vi.fn(),
    remove: vi.fn(),
    retain,
    terminals,
  }),
}));

vi.mock('../ui/sidebar', () => ({ useSidebar: () => ({ state: 'expanded', isMobile: false }) }));

import ChatGroupsShell from './ChatGroupsShell';

describe('ChatGroupsShell — per-tab terminal scoping', () => {
  it('hands BaseChat the active tab id as terminalKey', () => {
    terminals = [];
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(lastBaseChatProps.terminalKey).toBe('t1');
  });

  it('shows only the active tab’s terminal; a background tab’s stays mounted-hidden', () => {
    // Both tabs have a terminal open; the active tab is t1.
    terminals = [
      { key: 't1', workingDir: '/work/one', showing: true },
      { key: 't2', workingDir: '/work/two', showing: true },
    ];
    renderedDocks.length = 0;
    render(<ChatGroupsShell onChatChange={() => {}} />);

    const active = renderedDocks.find((d) => d.workingDir === '/work/one');
    const background = renderedDocks.find((d) => d.workingDir === '/work/two');
    // One dock per open terminal — both mounted...
    expect(active).toBeDefined();
    expect(background).toBeDefined();
    // ...but only the active tab's is visible.
    expect(active!.open).toBe(true);
    expect(background!.open).toBe(false);
  });

  it('a hidden (showing:false) terminal on the active tab is not shown', () => {
    terminals = [{ key: 't1', workingDir: '/work/one', showing: false }];
    renderedDocks.length = 0;
    render(<ChatGroupsShell onChatChange={() => {}} />);
    const dock = renderedDocks.find((d) => d.workingDir === '/work/one');
    expect(dock).toBeDefined();
    expect(dock!.open).toBe(false);
  });

  it('prunes terminals down to the live tab ids', () => {
    terminals = [{ key: 't1', workingDir: '/work/one', showing: true }];
    retain.mockClear();
    render(<ChatGroupsShell onChatChange={() => {}} />);
    expect(retain).toHaveBeenCalledWith(['t1', 't2']);
  });
});

/**
 * The fix that this file exists to guard: a terminal is scoped to ITS pane, not
 * to the whole window. In a split, each pane shows its own active tab's terminal
 * — two can be open at once — and no terminal renders in a pane it does not
 * belong to. The old shell-level dock could show only the globally-active tab's
 * terminal, as a full-width bar below every pane; a split could never show two.
 */
describe('ChatGroupsShell — terminals are scoped per split pane', () => {
  it('shows each pane its OWN terminal, and two panes can show terminals at once', async () => {
    vi.resetModules();
    const dockRenders: Array<{ open: boolean; cwd?: string }> = [];
    vi.doMock('../BaseChat', () => ({ default: () => <div data-testid="basechat" /> }));
    vi.doMock('../InAppTerminalDock', () => ({
      default: (props: { open: boolean; workingDir?: string }) => {
        dockRenders.push({ open: props.open, cwd: props.workingDir });
        return <div data-testid="dock" data-open={String(props.open)} data-cwd={props.workingDir} />;
      },
    }));
    // Two groups side by side: gA (tab tA active) | gB (tab tB active). Each tab
    // has a terminal open; the globally active group is gA.
    vi.doMock('../../contexts/ChatGroupsContext', () => ({
      useChatGroups: () => ({
        dispatch: vi.fn(),
        runningSessionIds: [],
        state: {
          activeGroupId: 'gA',
          layout: {
            kind: 'branch',
            dir: 'row',
            sizes: [0.5, 0.5],
            children: [
              { kind: 'leaf', groupId: 'gA' },
              { kind: 'leaf', groupId: 'gB' },
            ],
          },
          groups: {
            gA: { id: 'gA', activeTabId: 'tA', tabs: [{ tabId: 'tA', sessionId: 's-a', title: 'A', userSetName: false }] },
            gB: { id: 'gB', activeTabId: 'tB', tabs: [{ tabId: 'tB', sessionId: 's-b', title: 'B', userSetName: false }] },
          },
        },
      }),
    }));
    vi.doMock('../../contexts/TerminalDockContext', () => ({
      useTerminalDock: () => ({
        isOpenFor: () => true,
        setOpen: vi.fn(),
        remove: vi.fn(),
        retain: vi.fn(),
        terminals: [
          { key: 'tA', workingDir: '/work/a', showing: true },
          { key: 'tB', workingDir: '/work/b', showing: true },
        ],
      }),
    }));
    vi.doMock('../ui/sidebar', () => ({ useSidebar: () => ({ state: 'expanded', isMobile: false }) }));

    const { default: Shell } = await import('./ChatGroupsShell');
    render(<Shell onChatChange={() => {}} />);

    const dockA = dockRenders.find((d) => d.cwd === '/work/a');
    const dockB = dockRenders.find((d) => d.cwd === '/work/b');
    // Each pane rendered exactly its own terminal (no cross-pane leakage) ...
    expect(dockRenders.filter((d) => d.cwd === '/work/a')).toHaveLength(1);
    expect(dockRenders.filter((d) => d.cwd === '/work/b')).toHaveLength(1);
    // ... and BOTH are open at once — the split can show two terminals, each
    // scoped to its pane's own session working directory.
    expect(dockA?.open).toBe(true);
    expect(dockB?.open).toBe(true);
    vi.doUnmock('../BaseChat');
    vi.doUnmock('../InAppTerminalDock');
    vi.doUnmock('../../contexts/ChatGroupsContext');
    vi.doUnmock('../../contexts/TerminalDockContext');
    vi.doUnmock('../ui/sidebar');
  });
});
