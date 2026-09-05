import { describe, expect, it } from 'vitest';
import type { Message } from '../api';
import { sessionTodoItems, todoMutationRevision } from './sessionTodos';
import { scriptedTodoExchange } from './scriptedTodoExchange.fixture';

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
  it('reads blocked items and the parent of an expanded step', () => {
    expect(
      sessionTodoItems({
        'todo.v1': {
          items: [
            { id: '1', text: 'Coarse', status: 'pending' },
            { id: '2', text: 'Step', status: 'blocked', parent: '1' },
          ],
        },
      })
    ).toEqual([
      { id: '1', text: 'Coarse', status: 'pending' },
      { id: '2', text: 'Step', status: 'blocked', parent: '1' },
    ]);
    expect(sessionTodoItems({ 'todo.v0': { content: '- [!] Waiting' } })).toEqual([
      { id: '1', text: 'Waiting', status: 'blocked' },
    ]);
  });
  it('keeps an item whose status this build does not know, as pending', () => {
    // ⚠ Deliberately NOT dropped. An unrecognised status used to remove the row
    // entirely, so a desktop build older than the backend hid every item in a
    // status it had not shipped yet — `blocked` was exactly that case. A task
    // the backend is tracking must stay visible; only its styling degrades.
    expect(
      sessionTodoItems({ 'todo.v1': { items: [{ id: '2', text: 'Bad', status: 'invented' }] } })
    ).toEqual([{ id: '2', text: 'Bad', status: 'pending' }]);
  });
  it('discards malformed entries and duplicate ids without losing valid tasks', () => {
    expect(
      sessionTodoItems({
        'todo.v1': {
          items: [
            null,
            { id: '1', text: ' Keep ' },
            { id: '1', text: 'duplicate' },
            { id: '3', text: '' },
          ],
        },
      })
    ).toEqual([{ id: '1', text: 'Keep', status: 'pending' }]);
  });
});

describe('acknowledged To Do changes', () => {
  it('invalidates for successful write/add/update only, once per id', () => {
    for (const name of ['todo_write', 'todo_add', 'todo_expand', 'todo_update']) {
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

describe('acknowledged To Do changes a script ran as sub-calls', () => {
  const write = { tool: 'todo__todo_write' };
  const add = { tool: 'todo__todo_add' };
  const update = { tool: 'todo__todo_update' };

  it('invalidates for a scripted checklist, and again for every later run', () => {
    const first = scriptedTodoExchange('run-1', [write, add, update]);
    const one = todoMutationRevision(first);
    // The bug: top-level requests alone see nothing here, so the summary never
    // refetches and a list created while it is open never appears.
    expect(one).not.toBe('[]');
    // Replay is not new work.
    expect(todoMutationRevision([...first, ...first])).toBe(one);
    // A second run is, even though nothing top-level changed. Catches a marker
    // that latches on the first scripted mutation and never moves again.
    expect(todoMutationRevision([...first, ...scriptedTodoExchange('run-2', [add])])).not.toBe(one);
    // So is a longer run under the same id. Catches keying on the id alone.
    expect(todoMutationRevision(scriptedTodoExchange('run-1', [write, add, update, add]))).not.toBe(
      one
    );
  });

  it('invalidates even when the enclosing script run itself failed', () => {
    // The sub-call's write is already persisted; a later throw does not undo it.
    expect(
      todoMutationRevision(scriptedTodoExchange('run-1', [write], { failedRun: true }))
    ).not.toBe('[]');
  });

  it.each([
    { tool: 'todo__todo_update', status: 'error' },
    { tool: 'todo__plan_write' },
    { tool: 'developer__shell' },
  ])('ignores a failed or non-checklist sub-call', (call) => {
    expect(todoMutationRevision(scriptedTodoExchange('run-1', [call]))).toBe('[]');
  });

  it('ignores a run that recorded no checklist sub-calls at all', () => {
    expect(todoMutationRevision(scriptedTodoExchange('run-1', []))).toBe('[]');
  });
});
