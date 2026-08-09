import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SubagentTabHeader } from './SubagentTabHeader';

const props = {
  sessionId: 'child-1',
  parentSessionId: 'parent-1',
  parentSessionName: 'Planning chat',
  spawnContext: '## Subagent spawn context\ntask: count the files',
  extensions: ['developer', 'todo'],
  knowledgeBases: ['kb-papers', 'kb-methods'],
  running: true,
  onOpenParent: vi.fn(),
  onStop: vi.fn(),
};

describe('SubagentTabHeader', () => {
  // `props` is module scope and its handlers are live spies, so without this a
  // call-count assertion depends on which earlier test happened to click what.
  afterEach(() => vi.clearAllMocks());

  it('shows lineage, grants, and an expandable spawn context', () => {
    render(<SubagentTabHeader {...props} />);
    expect(screen.getByText(/spawned by/i)).toBeTruthy();
    expect(screen.getByText(/Planning chat/)).toBeTruthy();
    // ⚠ Counts on the band, names in the `title`. The header used to print one
    // chip per grant in an uncapped flex-wrap, so a subagent tab grew taller
    // than an ordinary one with every extension — and, before the backend fix,
    // the names it printed were the user's whole globally-enabled set rather
    // than the child's grants. The names must stay REACHABLE, which is what the
    // title asserts; they must not set the band's height.
    // `getByTitle`, not `getByText`: the count is interpolated
    // (`{n} extension{s}`) so React splits it across text nodes, and the title
    // is the thing that actually has to carry the names.
    const exts = screen.getByTitle('developer, todo');
    expect(exts.textContent).toBe('2 extensions');
    const kbs = screen.getByTitle('kb-papers, kb-methods');
    expect(kbs.textContent).toBe('2 knowledge bases');
    // Collapsed by default; expanding reveals the spawn context.
    expect(screen.queryByText(/count the files/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /spawn context/i }));
    expect(screen.getByText(/count the files/)).toBeTruthy();
  });

  it('Stop is offered while running and confirms through onStop', () => {
    render(<SubagentTabHeader {...props} />);
    fireEvent.click(screen.getByRole('button', { name: /stop subagent/i }));
    expect(props.onStop).toHaveBeenCalledOnce();
  });

  it('hides Stop when the child is idle', () => {
    render(<SubagentTabHeader {...props} running={false} />);
    expect(screen.queryByRole('button', { name: /stop subagent/i })).toBeNull();
  });

  it('the spawned-by name is the control that opens the parent', () => {
    // The lineage link is the whole point of the "spawned by" line, and it is
    // what BaseChat wires to the reducer's open-or-focus dispatch. Nothing else
    // in the suite ever clicks it.
    render(<SubagentTabHeader {...props} />);
    fireEvent.click(screen.getByRole('button', { name: 'Planning chat' }));
    expect(props.onOpenParent).toHaveBeenCalledOnce();
  });

  it('falls back to the parent session id when the parent has no name yet', () => {
    render(<SubagentTabHeader {...props} parentSessionName={undefined} />);
    fireEvent.click(screen.getByRole('button', { name: 'parent-1' }));
    expect(props.onOpenParent).toHaveBeenCalledOnce();
  });

  it('offers no spawn-context disclosure when there is no record to disclose', () => {
    // Reachable: `persist_spawn_context` is best-effort on the backend (a
    // failure only warns), and sessions spawned before it landed have no such
    // record either. The header still renders — lineage and Stop are the point —
    // but a toggle that can only ever open onto nothing is a dead control.
    render(<SubagentTabHeader {...props} spawnContext={undefined} />);
    expect(screen.getByText(/spawned by/i)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /spawn context/i })).toBeNull();
  });

  it('names the region the disclosure controls', () => {
    render(<SubagentTabHeader {...props} />);
    const toggle = screen.getByRole('button', { name: /spawn context/i });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    const controls = toggle.getAttribute('aria-controls');
    expect(controls).toBeTruthy();

    fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(document.getElementById(controls!)?.textContent).toContain('count the files');
  });
});
