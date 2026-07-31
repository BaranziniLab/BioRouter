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
    expect(screen.getByText('developer')).toBeTruthy();
    expect(screen.getByText('kb-papers')).toBeTruthy();
    expect(screen.getByText('kb-methods')).toBeTruthy();
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
});
