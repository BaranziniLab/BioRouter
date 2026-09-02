// Shared test fixture, deliberately NOT a `.test.ts` file.
// Importing one test file from another re-registers its suites in the importer,
// so the same 20 tests ran twice (28 -> 48) when this lived in sessionTodos.test.ts.
import type { Message } from '../api';

/**
 * A coordinated `execute_code` step: ONE top-level tool call whose result meta
 * records the `todo__*` sub-calls the script actually ran. This is how a
 * checklist reaches the session when the model drives the Todo tools from a
 * script — the transcript carries no top-level `todo__*` request at all.
 */
export function scriptedTodoExchange(
  id: string,
  calls: { tool: string; status?: string }[],
  options: { failedRun?: boolean } = {}
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
            value: { name: 'code_execution__execute_code', arguments: { code: 'await …' } },
          },
        },
        {
          type: 'toolResponse',
          id,
          toolResult: {
            status: 'success',
            value: {
              content: [],
              isError: options.failedRun ?? false,
              _meta: {
                'biorouter/tool-calls': calls.map((call) => ({
                  tool: call.tool,
                  args: '{}',
                  status: call.status ?? 'ok',
                })),
              },
            },
          },
        },
      ],
    },
  ] as Message[];
}
