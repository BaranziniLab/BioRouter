import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import ToolCallWithResponse, { summarizeToolCall } from './ToolCallWithResponse';
import type { ToolRequestMessageContent } from '../types/message';

describe('summarizeToolCall', () => {
  it('summarizes file editing tools as natural reading and editing actions', () => {
    expect(
      summarizeToolCall({
        name: 'developer__text_editor',
        arguments: { command: 'view', path: '/Users/wgu/Desktop/biorouter/package.json' },
      })
    ).toBe('Reading package.json');

    expect(
      summarizeToolCall({
        name: 'developer__text_editor',
        arguments: { command: 'str_replace', path: 'ui/desktop/src/components/ChatInput.tsx' },
      })
    ).toBe('Editing ChatInput.tsx');
  });

  it('summarizes shell and browser-style tools without exposing raw parameter boxes', () => {
    expect(
      summarizeToolCall({
        name: 'developer__exec_command',
        arguments: { cmd: 'npm run typecheck' },
      })
    ).toBe('Running npm run typecheck');

    expect(
      summarizeToolCall({
        name: 'browser__screenshot',
        arguments: { fullPage: false },
      })
    ).toBe('Capturing a screenshot');
  });

  it('summarizes search and unknown MCP tools using the most helpful visible target', () => {
    expect(
      summarizeToolCall({
        name: 'web__search_query',
        arguments: { search_query: [{ q: 'Biorouter agent drafter guardrails' }] },
      })
    ).toBe('Searching for Biorouter agent drafter guardrails');

    expect(
      summarizeToolCall({
        name: 'ucsfomopagent__cohort_lookup',
        arguments: { cohort_id: 42, table: 'condition_occurrence' },
      })
    ).toBe('Cohort Lookup with cohort_id, table');
  });

  it('summarizes multi-step tool graphs as coordinated work', () => {
    expect(
      summarizeToolCall({
        name: 'multi_tool_use__execute_code',
        arguments: {
          tool_graph: [
            { tool: 'read', description: 'Read the manifest', depends_on: [] },
            { tool: 'search', description: 'Find matching routes', depends_on: [0] },
            { tool: 'edit', description: 'Patch the UI', depends_on: [1] },
          ],
        },
      })
    ).toBe('Coordinating 3 tool steps');
  });

  it('renders the friendly summary before raw details and reveals details on demand', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-1',
      toolCall: {
        status: 'success',
        value: {
          name: 'developer__text_editor',
          arguments: { command: 'view', path: '/Users/wgu/Desktop/biorouter/package.json' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        isStreamingMessage={false}
      />
    );

    expect(screen.getByText(/Reading package\.json/)).toBeInTheDocument();
    expect(screen.queryByText('command')).not.toBeInTheDocument();
    expect(screen.queryByText('/Users/wgu/Desktop/biorouter/package.json')).not.toBeInTheDocument();

    fireEvent.click(screen.getByText(/Reading package\.json/).closest('button') as HTMLElement);
    fireEvent.click(screen.getByText('View tool details').closest('button') as HTMLElement);

    expect(screen.getByText('command')).toBeInTheDocument();
    expect(screen.getByText('view')).toBeInTheDocument();
    expect(screen.getByText('/Users/wgu/Desktop/biorouter/package.json')).toBeInTheDocument();
  });

  it('keeps the tool call trigger compact while preserving click-to-expand details', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-2',
      toolCall: {
        status: 'success',
        value: {
          name: 'developer__exec_command',
          arguments: { cmd: 'npm run typecheck' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        isStreamingMessage={false}
      />
    );

    const trigger = screen.getByText(/Running npm run typecheck/).closest('button') as HTMLElement;

    expect(trigger).toHaveClass('h-6');
    expect(trigger).toHaveClass('min-h-0');
    expect(trigger).toHaveClass('!px-0');
    expect(trigger.querySelector('svg:last-child')).toHaveClass('opacity-0');
    // No tool response ever arrived and the turn is not running, so the card
    // reports the truth ("No result") rather than the old fabricated "Finished".
    expect(screen.getByText(/No result/)).toBeInTheDocument();
    expect(screen.queryByText(/Finished/)).not.toBeInTheDocument();
    expect(screen.queryByText('cmd')).not.toBeInTheDocument();

    fireEvent.click(trigger);
    fireEvent.click(screen.getByText('View tool details').closest('button') as HTMLElement);

    expect(screen.getByText('cmd')).toBeInTheDocument();
    expect(screen.getByText('npm run typecheck')).toBeInTheDocument();
  });

  it('expands a partial execute-code graph when dependency metadata is missing', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-partial-graph',
      toolCall: {
        status: 'success',
        value: {
          name: 'multi_tool_use__execute_code',
          arguments: {
            tool_graph: [
              {
                tool: 'developer/shell',
                description: 'Attempt the command',
              },
            ],
          },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        toolResponse={{
          type: 'toolResponse',
          id: 'tool-partial-graph',
          toolResult: { status: 'error', error: 'The command could not be started' },
        }}
      />
    );

    fireEvent.click(screen.getByText('Attempt the command').closest('button') as HTMLElement);

    expect(screen.getByText('1. developer/shell: Attempt the command')).toBeInTheDocument();
    expect(screen.getByText('Tool call failed')).toBeInTheDocument();
    expect(screen.getByText('The command could not be started')).toBeInTheDocument();
    expect(screen.queryByText('Tool details unavailable')).not.toBeInTheDocument();
  });

  it('treats a successful wrapper with missing content as an empty result', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-partial-result',
      toolCall: {
        status: 'success',
        value: {
          name: 'developer__exec_command',
          arguments: { cmd: 'partially-started-command' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        toolResponse={{
          type: 'toolResponse',
          id: 'tool-partial-result',
          toolResult: {
            status: 'success',
            value: { is_error: false },
          } as never,
        }}
      />
    );

    expect(screen.getByText(/Running partially-started-command/)).toBeInTheDocument();
    expect(screen.queryByText('Tool details unavailable')).not.toBeInTheDocument();
  });

  it('shows MCP is_error text as an inline tool failure', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-mcp-error',
      toolCall: {
        status: 'success',
        value: {
          name: 'example__lookup',
          arguments: { id: 'missing-record' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        toolResponse={{
          type: 'toolResponse',
          id: 'tool-mcp-error',
          toolResult: {
            status: 'success',
            value: {
              is_error: true,
              content: [{ type: 'text', text: 'Record missing-record was not found' }],
            },
          },
        }}
      />
    );

    fireEvent.click(screen.getByText(/Problem with/).closest('button') as HTMLElement);

    expect(screen.getByText('Tool call failed')).toBeInTheDocument();
    expect(screen.getByText('Record missing-record was not found')).toBeInTheDocument();
  });

  /**
   * R1-02 — the REAL wire spelling. rmcp serialises CallToolResult with
   * `rename_all = "camelCase"`, so every live and persisted tool result carries
   * `isError`, not `is_error`. Before the fix only the snake_case spelling was
   * checked, so genuinely failed calls (e.g. search_modules "No matches found")
   * rendered as green successes — three in a row in the loop-guard repro.
   */
  it('shows a camelCase isError result (the real MCP wire shape) as a failure', () => {
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-mcp-error-camel',
      toolCall: {
        status: 'success',
        value: {
          name: 'code_execution__search_modules',
          arguments: { terms: 'web search' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        toolResponse={{
          type: 'toolResponse',
          id: 'tool-mcp-error-camel',
          toolResult: {
            status: 'success',
            value: {
              isError: true,
              content: [{ type: 'text', text: 'Error: No matches found for: web search' }],
              structuredContent: {
                error: { kind: 'tool_failure', retryable: false },
              },
            },
          },
        }}
      />
    );

    fireEvent.click(screen.getByText(/Problem with/).closest('button') as HTMLElement);

    expect(screen.getByText('Tool call failed')).toBeInTheDocument();
    expect(screen.getByText('Error: No matches found for: web search')).toBeInTheDocument();
  });

  it('contains an unexpected tool-card render failure instead of crashing the chat', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-boundary',
      toolCall: {
        status: 'success',
        value: {
          name: 'example__lookup',
          arguments: { id: 'record-1' },
        },
      },
    };

    try {
      render(
        <ToolCallWithResponse
          isCancelledMessage={false}
          toolRequest={toolRequest}
          notifications={[
            {
              type: 'Notification',
              request_id: 'broken-notification',
              message: undefined,
            } as never,
          ]}
        />
      );

      expect(screen.getByRole('alert')).toHaveTextContent('Tool details unavailable');
      expect(screen.getByRole('alert')).toHaveTextContent('The conversation can continue');
    } finally {
      consoleError.mockRestore();
    }
  });

  it('opens generated files named in tool output in the side panel', () => {
    const onOpenArtifact = vi.fn();
    const toolRequest: ToolRequestMessageContent = {
      type: 'toolRequest',
      id: 'tool-3',
      toolCall: {
        status: 'success',
        value: {
          name: 'developer__exec_command',
          arguments: { cmd: 'build-site' },
        },
      },
    };

    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={toolRequest}
        toolResponse={{
          type: 'toolResponse',
          id: 'tool-3',
          toolResult: {
            status: 'success',
            value: {
              is_error: false,
              content: [{ type: 'text', text: 'Created `dist/index.html`' }],
            },
          },
        }}
        workingDir="/Users/wgu/Desktop/weather-website"
        onOpenArtifact={onOpenArtifact}
      />
    );

    fireEvent.click(screen.getByText(/Running build-site/).closest('button') as HTMLElement);
    fireEvent.click(screen.getByText('View output').closest('button') as HTMLElement);
    fireEvent.click(screen.getByRole('button', { name: 'dist/index.html' }));

    expect(onOpenArtifact).toHaveBeenCalledWith({
      kind: 'file',
      title: 'index.html',
      path: '/Users/wgu/Desktop/weather-website/dist/index.html',
    });
  });
});

describe('ToolCallWithResponse status derivation', () => {
  const pendingToolRequest: ToolRequestMessageContent = {
    type: 'toolRequest',
    id: 'tool-status-1',
    toolCall: {
      status: 'success',
      value: {
        name: 'developer__exec_command',
        arguments: { cmd: 'npm run typecheck' },
      },
    },
  };

  it('shows a response-less tool call as loading while the turn is still running', () => {
    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={pendingToolRequest}
        toolResponse={undefined}
        turnActive={true}
      />
    );

    expect(screen.getByLabelText('Tool status: loading')).toBeInTheDocument();
    expect(screen.getByText(/Working on/)).toBeInTheDocument();
    expect(screen.queryByText(/Finished/)).not.toBeInTheDocument();
  });

  it('stays loading for a response-less tool call that is no longer the last message', () => {
    // The regression this guards: status used to be derived from "am I the last
    // streaming message", so a still-running sibling painted green the instant
    // any later message arrived.
    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={pendingToolRequest}
        toolResponse={undefined}
        isStreamingMessage={false}
        turnActive={true}
      />
    );

    expect(screen.getByLabelText('Tool status: loading')).toBeInTheDocument();
    expect(screen.queryByLabelText('Tool status: success')).not.toBeInTheDocument();
  });

  it('marks a response-less tool call as interrupted once the turn has ended', () => {
    render(
      <ToolCallWithResponse
        isCancelledMessage={false}
        toolRequest={pendingToolRequest}
        toolResponse={undefined}
        turnActive={false}
      />
    );

    expect(screen.getByLabelText('Tool status: pending')).toBeInTheDocument();
    expect(screen.getByText(/No result/)).toBeInTheDocument();
    expect(screen.queryByText(/Finished/)).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Tool status: success')).not.toBeInTheDocument();
  });
});

// SUB-01: a turn that fans out to several subagents renders one collapsed card
// per delegation. Before this, every card read "Subagent with instructions" —
// the generic "<Tool> with <argument keys>" fallback — so the cards were
// byte-identical and nothing in the transcript said which result came from
// which child.
describe('summarizeToolCall — subagent delegation', () => {
  const delegation = (args: Record<string, unknown>) =>
    summarizeToolCall({ name: 'subagent', arguments: args });

  it('gives parallel delegations distinct labels', () => {
    const labels = [
      'Count the .rs files under crates/ and report the number.',
      'Read Cargo.toml and report the workspace version.',
      'List every crate in the workspace.',
    ].map((instructions) => delegation({ instructions }));

    expect(new Set(labels).size).toBe(3);
    expect(labels[0]).toContain('Count the .rs files');
    expect(labels[1]).toContain('Cargo.toml');
    expect(labels[2]).toContain('List every crate');
  });

  it('never falls back to the argument-key label', () => {
    expect(delegation({ instructions: 'Do the thing thoroughly.' })).not.toContain(
      'with instructions'
    );
    expect(
      delegation({
        instructions: 'Do the thing thoroughly.',
        extensions: ['developer'],
        settings: { model: 'm' },
        summary: true,
      })
    ).not.toContain('with instructions');
  });

  it('names the subworkflow when one was used', () => {
    expect(delegation({ subworkflow: 'lint', parameters: { path: 'src' } })).toBe(
      'Delegating lint'
    );
    expect(delegation({ subworkflow: 'lint', instructions: 'Only check the new files.' })).toBe(
      'Delegating lint: Only check the new files.'
    );
  });

  it('elides a long instruction instead of dropping it', () => {
    const label = delegation({
      instructions:
        'Read every provider module under crates/biorouter/src/providers and report which of them implement streaming completions',
    });
    expect(label.startsWith('Delegating: Read every provider module')).toBe(true);
    expect(label.endsWith('…')).toBe(true);
    expect(label.length).toBeLessThan(90);
  });

  it('still says something when a delegation carries no readable task', () => {
    expect(delegation({})).toBe('Delegating to a subagent');
  });
});

describe('ToolCallWithResponse renders delegations distinguishably', () => {
  const delegationRequest = (id: string, instructions: string): ToolRequestMessageContent => ({
    type: 'toolRequest',
    id,
    toolCall: {
      status: 'success',
      value: { name: 'subagent', arguments: { instructions } },
    },
  });

  it('labels two collapsed subagent cards by their own tasks', () => {
    render(
      <>
        <ToolCallWithResponse
          isCancelledMessage={false}
          toolRequest={delegationRequest('d1', 'Count the .rs files under crates/.')}
          toolResponse={undefined}
          turnActive={true}
        />
        <ToolCallWithResponse
          isCancelledMessage={false}
          toolRequest={delegationRequest('d2', 'Read Cargo.toml and report the version.')}
          toolResponse={undefined}
          turnActive={true}
        />
      </>
    );

    expect(screen.getByText(/Count the \.rs files/)).toBeInTheDocument();
    expect(screen.getByText(/Read Cargo\.toml/)).toBeInTheDocument();
    expect(screen.queryByText(/Subagent with instructions/)).not.toBeInTheDocument();
  });
});
