import { describe, expect, it, vi } from 'vitest';
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
});
