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
        arguments: { search_query: [{ q: 'BioRouter agent drafter guardrails' }] },
      })
    ).toBe('Searching for BioRouter agent drafter guardrails');

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
    expect(screen.getByText(/Finished/)).toBeInTheDocument();
    expect(screen.queryByText('cmd')).not.toBeInTheDocument();

    fireEvent.click(trigger);
    fireEvent.click(screen.getByText('View tool details').closest('button') as HTMLElement);

    expect(screen.getByText('cmd')).toBeInTheDocument();
    expect(screen.getByText('npm run typecheck')).toBeInTheDocument();
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
