import type { Message } from '../api';

export type TodoItem = {
  id: string;
  text: string;
  status: 'pending' | 'in_progress' | 'completed';
};

function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

export function sessionTodoItems(extensionData: unknown): TodoItem[] {
  const data = record(extensionData);
  const current = record(data?.['todo.v1']);
  if (current && current.items === undefined) return [];
  if (current && Array.isArray(current.items)) {
    const seen = new Set<string>();
    return current.items.flatMap((value): TodoItem[] => {
      const item = record(value);
      if (
        !item ||
        typeof item.id !== 'string' ||
        !item.id ||
        seen.has(item.id) ||
        typeof item.text !== 'string' ||
        !item.text.trim()
      )
        return [];
      const status = item.status ?? 'pending';
      if (status !== 'pending' && status !== 'in_progress' && status !== 'completed') return [];
      seen.add(item.id);
      return [{ id: item.id, text: item.text.trim(), status }];
    });
  }
  const legacy = record(data?.['todo.v0'])?.content;
  if (typeof legacy !== 'string') return [];
  return legacy
    .split('\n')
    .flatMap((line): TodoItem[] => {
      const match = line.trimStart().match(/^[-*+] \[([ xX~-])\] (.+)$/);
      if (!match || !match[2].trim()) return [];
      return [
        {
          id: '',
          text: match[2].trim(),
          status: /[xX]/.test(match[1])
            ? 'completed'
            : /[~-]/.test(match[1])
              ? 'in_progress'
              : 'pending',
        },
      ];
    })
    .map((item, index) => ({ ...item, id: String(index + 1) }));
}

/** Only acknowledged mutations invalidate the persisted checklist, never proposed arguments. */
export function todoMutationRevision(messages: readonly Message[]): string {
  const requests = new Set<string>();
  const completed = new Set<string>();
  for (const message of messages) {
    for (const content of message.content) {
      if (content.type === 'toolRequest') {
        const call = record(content.toolCall.value);
        if (
          content.toolCall.status === 'success' &&
          ['todo__todo_write', 'todo__todo_add', 'todo__todo_update'].includes(String(call?.name))
        ) {
          requests.add(content.id);
        }
      }
    }
  }
  for (const message of messages) {
    for (const content of message.content) {
      if (content.type !== 'toolResponse' || !requests.has(content.id)) continue;
      const result = record(content.toolResult.value);
      if (
        content.toolResult.status === 'success' &&
        result &&
        result.isError !== true &&
        result.is_error !== true
      ) {
        completed.add(content.id);
      }
    }
  }
  return JSON.stringify([...completed].sort());
}
