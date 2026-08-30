import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import { sessionTodoItems, todoMutationRevision } from './sessionTodos';

export function todoExchange(
  id: string,
  options: { name?: string; error?: boolean; pending?: boolean; refused?: boolean } = {}
): Message[] {
  return [
    {
      role: 'assistant',
      created: 0,
      metadata: { agentVisible: true, userVisible: true },
      content: [
        {
          type: 'toolRequest',
          id,
          toolCall: {
            status: 'success',
            value: {
              name: options.name ?? 'todo__todo_update',
              arguments: { id: '1', status: 'completed' },
            },
          },
        },
        ...(options.pending
          ? []
          : [
              {
                type: 'toolResponse',
                id,
                toolResult: {
                  status: options.refused ? 'error' : 'success',
                  value: { content: [], isError: options.error ?? false },
                },
              },
            ]),
      ],
    },
  ] as Message[];
}

describe('persisted session checklist', () => {
  it.each([undefined, {}, { 'todo.v1': { plan: 'Only a plan', items: [] } }])(
    'hides an absent checklist',
    (data) => {
      expect(sessionTodoItems(data)).toEqual([]);
    }
  );
  it('reads canonical order, multiple active steps, and reopened work', () => {
    const items = [
      { id: '1', text: 'Compare', status: 'completed' },
      { id: '2', text: 'Verify', status: 'in_progress' },
      { id: '3', text: 'Present', status: 'in_progress' },
    ];
    expect(sessionTodoItems({ 'todo.v1': { items } })).toEqual(items);
    items[0].status = 'pending';
    expect(sessionTodoItems({ 'todo.v1': { items } })[0].status).toBe('pending');
  });
  it('never resurrects a cleared list from legacy state', () => {
    expect(
      sessionTodoItems({ 'todo.v1': { items: [] }, 'todo.v0': { content: '- [x] Old' } })
    ).toEqual([]);
  });
  it('treats a v1 plan-only state as empty even when a legacy list remains', () => {
    expect(
      sessionTodoItems({
        'todo.v1': { plan: 'Only a plan' },
        'todo.v0': { content: '- [x] Obsolete' },
      })
    ).toEqual([]);
  });
  it('supports legacy checklists but not freeform plans', () => {
    expect(
      sessionTodoItems({ 'todo.v0': { content: '- [ ] A\n* [X] B\n  + [-] C\nnot a task' } }).map(
        (item) => item.status
      )
    ).toEqual(['pending', 'completed', 'in_progress']);
    expect(sessionTodoItems({ 'todo.v0': { content: 'Plan prose' } })).toEqual([]);
  });
  it('discards malformed entries and duplicate ids without losing valid tasks', () => {
    expect(
      sessionTodoItems({
        'todo.v1': {
          items: [
            null,
            { id: '1', text: ' Keep ' },
            { id: '1', text: 'duplicate' },
            { id: '2', text: 'Bad', status: 'invented' },
            { id: '3', text: '' },
          ],
        },
      })
    ).toEqual([{ id: '1', text: 'Keep', status: 'pending' }]);
  });
});

describe('acknowledged To Do changes', () => {
  it('invalidates for successful write/add/update only, once per id', () => {
    for (const name of ['todo_write', 'todo_add', 'todo_update']) {
      const messages = todoExchange('a', { name: `todo__${name}` });
      expect(todoMutationRevision([...messages, ...messages])).toBe('["a"]');
    }
  });
  it.each([
    { error: true },
    { pending: true },
    { refused: true },
    { name: 'todo__plan_write' },
    { name: 'developer__shell' },
  ])('does not treat an unconfirmed or unrelated call as an update', (options) => {
    expect(todoMutationRevision(todoExchange('a', options))).toBe('[]');
  });
});
