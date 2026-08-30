import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { ChatSummary } from './ChatSummary';
import type { TodoItem } from '../utils/sessionTodos';

const items: TodoItem[] = [
  { id: '1', text: 'Compare clinic options', status: 'completed' },
  { id: '2', text: 'Verify arithmetic', status: 'in_progress' },
  { id: '3', text: 'Present the comparison', status: 'pending' },
];
const props = {
  name: 'Clinic comparison',
  toolCalls: '6',
  billedTokens: '211k',
  artifacts: '1',
  codeDelta: { added: 0, removed: 0 },
  hasWorkflow: false,
  onWorkflow: vi.fn(),
  onDiagnostics: vi.fn(),
  todos: { items: [], loading: false, error: false, refresh: vi.fn() },
};

describe('compact chat summary', () => {
  it('does not flash a To Do section while an empty summary refreshes', () => {
    render(<ChatSummary {...props} todos={{ ...props.todos, loading: true }} />);
    expect(screen.queryByText(/To Do/)).not.toBeInTheDocument();
  });
  it('uses compact label/value typography and hides absent progress', () => {
    render(<ChatSummary {...props} />);
    expect(screen.getByText('6')).toHaveClass('text-sm');
    expect(screen.getByText('Tool calls')).toHaveClass('text-xs');
    expect(screen.getAllByRole('definition')).toHaveLength(4);
    expect(screen.queryByRole('region', { name: 'To Do progress' })).not.toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
  it('shows ordered, explicitly labelled steps and accessible progress', () => {
    render(<ChatSummary {...props} todos={{ ...props.todos, items }} />);
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '1');
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuemax', '3');
    const rows = within(screen.getByRole('list', { name: 'To Do tasks' })).getAllByRole('listitem');
    expect(rows.map((row) => row.textContent)).toEqual([
      'Compare clinic optionsComplete',
      'Verify arithmeticIn progress',
      'Present the comparisonPending',
    ]);
  });
  it('updates completion, reopening, renaming, replacement and clearing without stale rows', () => {
    const { rerender } = render(<ChatSummary {...props} todos={{ ...props.todos, items }} />);
    rerender(
      <ChatSummary
        {...props}
        todos={{ ...props.todos, items: items.map((item) => ({ ...item, status: 'completed' })) }}
      />
    );
    expect(screen.getByText('3 of 3 complete')).toBeInTheDocument();
    rerender(
      <ChatSummary
        {...props}
        todos={{
          ...props.todos,
          items: [{ id: '1', text: 'Recheck assumptions', status: 'in_progress' }],
        }}
      />
    );
    expect(screen.getByText('0 of 1 complete')).toBeInTheDocument();
    expect(screen.queryByText('Verify arithmetic')).not.toBeInTheDocument();
    expect(screen.getByText('Recheck assumptions')).toBeInTheDocument();
    rerender(<ChatSummary {...props} />);
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
  it('contains long lists, wraps labels and never executes task markup', () => {
    const text = '検証 🧬 <img src=x onerror=alert(1)> '.repeat(20);
    render(
      <ChatSummary
        {...props}
        todos={{
          ...props.todos,
          items: Array.from({ length: 200 }, (_, i) => ({
            id: String(i),
            text: `${i} ${text}`,
            status: 'pending',
          })),
        }}
      />
    );
    expect(screen.getAllByRole('listitem')).toHaveLength(200);
    expect(screen.getByRole('list')).toHaveClass('max-h-60', 'overflow-y-auto');
    expect(screen.queryByRole('img')).not.toBeInTheDocument();
  });
  it('retains actions, loading feedback and recoverable refresh errors', () => {
    const { rerender } = render(
      <ChatSummary {...props} todos={{ ...props.todos, items, loading: true }} />
    );
    expect(screen.getByRole('status')).toHaveTextContent('Refreshing To Do');
    fireEvent.click(screen.getByRole('button', { name: 'Make workflow' }));
    fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }));
    expect(props.onWorkflow).toHaveBeenCalled();
    expect(props.onDiagnostics).toHaveBeenCalled();
    rerender(<ChatSummary {...props} hasWorkflow todos={{ ...props.todos, items, error: true }} />);
    expect(screen.getByRole('alert')).toHaveTextContent('may be out of date');
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(props.todos.refresh).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: 'Workflow' })).toBeInTheDocument();
  });
});
