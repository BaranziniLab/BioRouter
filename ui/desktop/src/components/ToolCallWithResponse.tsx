import { ToolIconWithStatus, ToolCallStatus } from './ToolCallStatusIndicator';
import { getToolCallIcon } from '../utils/toolIconMapping';
import React, { useEffect, useRef, useState, useMemo } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useResolvedTheme, useThemeFamily } from '../contexts/ThemeContext';
import {
  CODE_FONT_FAMILY,
  CODE_FONT_SIZE,
  CODE_LINE_HEIGHT,
  codeThemesByFamily,
} from '../styles/codeTheme';
import { Button } from './ui/button';
import { ToolCallArguments, ToolCallArgumentValue } from './ToolCallArguments';
import MarkdownContent from './MarkdownContent';
import {
  ToolRequestMessageContent,
  ToolResponseMessageContent,
  NotificationEvent,
} from '../types/message';
import { cn, snakeToTitleCase } from '../utils';
import { LoadingStatus } from './ui/Dot';
import { ChevronRight, FlaskConical } from './icons/app-icons';
import MCPUIResourceRenderer from './MCPUIResourceRenderer';
import { isUIResource } from '@mcp-ui/client';
import { CallToolResponse, Content, EmbeddedResource } from '../api';
import McpAppRenderer from './McpApps/McpAppRenderer';
import type { ArtifactSource } from './artifacts/artifactTypes';
import { NotificationContent, NotificationSurface } from './alerts/NotificationSurface';
import { crossAffiliationOffer } from '../utils/crossAffiliation';
import { CrossAffiliationAcceptCard } from './privacy/CrossAffiliationAcceptCard';

/**
 * The tool card's own status vocabulary. `interrupted` extends the shared
 * `LoadingStatus` for the one case only a tool card has: the turn ended without
 * a tool response ever arriving. It renders through the (previously
 * unreachable) `pending` dot rather than claiming success.
 */
type ToolLoadingStatus = LoadingStatus | 'interrupted';

interface ToolGraphNode {
  tool: string;
  description: string;
  depends_on: number[];
}

type UiMeta = {
  ui?: {
    resourceUri?: string;
  };
};

/**
 * One executed sub-call of a coordinated `execute_code` step (#28), recorded
 * by the backend in the result meta under `biorouter/tool-calls`.
 */
type ExecutedToolCall = {
  tool: string;
  /** The exact JSON arguments string (backend-truncated at ~2 KB). */
  args?: string;
  status: 'ok' | 'error';
  /** The real error text for a failed call. */
  error?: string;
  resultBytes?: number;
};

type ResultMeta = UiMeta & {
  'biorouter/tool-calls'?: unknown;
  'biorouter/tool-calls-dropped'?: unknown;
};

type ToolResultWithMeta = {
  status?: string;
  value?: CallToolResponse & {
    _meta?: ResultMeta;
  };
};

type ToolRequestWithMeta = ToolRequestMessageContent & {
  _meta?: UiMeta;
  toolCall: {
    status: 'success';
    value: {
      name: string;
      arguments?: Record<string, unknown>;
    };
  };
};

interface ToolCallWithResponseProps {
  sessionId?: string;
  isCancelledMessage: boolean;
  toolRequest: ToolRequestMessageContent;
  toolResponse?: ToolResponseMessageContent;
  notifications?: NotificationEvent[];
  isStreamingMessage?: boolean;
  /**
   * Whether the CHAT TURN is still running (chat-level `chatState !== Idle`),
   * as opposed to `isStreamingMessage`, which only says "this message is
   * currently the last one". Tool status must key off the turn: a tool call
   * keeps executing long after its message stops being last. Defaults to
   * false so a read-only replay never spins.
   */
  turnActive?: boolean;
  append?: (value: string) => void;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
}

function recordOf(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
}

function normalizeToolGraph(value: unknown): ToolGraphNode[] {
  if (!Array.isArray(value)) return [];

  return value.flatMap((candidate, index) => {
    const node = recordOf(candidate);
    if (!node) return [];

    const dependencies = Array.isArray(node.depends_on)
      ? node.depends_on.filter(
          (dependency): dependency is number =>
            typeof dependency === 'number' && Number.isInteger(dependency) && dependency >= 0
        )
      : [];

    return [
      {
        tool:
          typeof node.tool === 'string' && node.tool.trim()
            ? node.tool
            : `Unknown tool ${index + 1}`,
        description:
          typeof node.description === 'string' && node.description.trim()
            ? node.description
            : 'No description was provided.',
        depends_on: dependencies,
      },
    ];
  });
}

/**
 * Executed sub-call telemetry from the result meta (#28). The meta is untyped
 * wire data, so anything malformed is dropped rather than crashing the card.
 */
function normalizeExecutedCalls(value: unknown): ExecutedToolCall[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((candidate): ExecutedToolCall[] => {
    const record = recordOf(candidate);
    if (!record || typeof record.tool !== 'string' || !record.tool.trim()) return [];
    return [
      {
        tool: record.tool,
        args: typeof record.args === 'string' && record.args.trim() ? record.args : undefined,
        status: record.status === 'error' ? 'error' : 'ok',
        error: typeof record.error === 'string' && record.error.trim() ? record.error : undefined,
        resultBytes: typeof record.result_bytes === 'number' ? record.result_bytes : undefined,
      },
    ];
  });
}

function normalizedToolResultValue(toolResult: unknown): Record<string, unknown> | null {
  const result = recordOf(toolResult);
  if (!result) return null;
  if (result.status === 'error') return null;
  if (result.status === 'success') return recordOf(result.value);
  return result;
}

function getToolResultContent(toolResult: unknown): Content[] {
  const value = normalizedToolResultValue(toolResult);
  if (!Array.isArray(value?.content)) {
    return [];
  }

  return value.content.filter((item): item is Content => {
    if (!recordOf(item)) return false;
    const annotations = (item as { annotations?: { audience?: string[] } }).annotations;
    return !annotations?.audience || annotations.audience.includes('user');
  });
}

function displayError(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error;
  const record = recordOf(error);
  if (typeof record?.message === 'string' && record.message.trim()) return record.message;
  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== '{}') return serialized;
  } catch {
    return 'The tool returned an unreadable error.';
  }
  return 'The tool call failed without an error message.';
}

function getToolResultError(toolResult: unknown): string | null {
  const result = recordOf(toolResult);
  if (!result) return null;
  if (result.status === 'error') return displayError(result.error);

  const value = normalizedToolResultValue(result);
  // `isError` is the wire spelling: rmcp serialises CallToolResult with
  // rename_all = "camelCase", so every live and persisted tool result uses it.
  // `is_error` is kept for tolerance of any snake_case producer.
  if (value?.isError !== true && value?.is_error !== true) return null;
  const text = getToolResultContent(result)
    .flatMap((content) =>
      'text' in content && typeof content.text === 'string' ? [content.text] : []
    )
    .join('\n')
    .trim();
  if (text) return text;
  // No user-audience error text exists. Content marked assistant-only was
  // deliberately kept out of the user's view by the tool, so the audience
  // filter holds even on the error path: falling back to assistant-only text
  // here would leak content the user was never meant to see. Backends that
  // want a specific message must supply user-visible error text.
  return 'The tool reported that it could not complete the request.';
}

function isEmbeddedResource(content: Content): content is EmbeddedResource {
  return 'resource' in content && typeof (content as Record<string, unknown>).resource === 'object';
}

interface McpAppWrapperProps {
  toolRequest: ToolRequestMessageContent;
  toolResponse?: ToolResponseMessageContent;
  sessionId: string;
  append?: (value: string) => void;
}

function McpAppWrapper({
  toolRequest,
  toolResponse,
  sessionId,
  append,
}: McpAppWrapperProps): React.ReactNode {
  const requestWithMeta = toolRequest as ToolRequestWithMeta;
  let resourceUri = requestWithMeta._meta?.ui?.resourceUri;

  if (!resourceUri && toolResponse) {
    const resultWithMeta = toolResponse.toolResult as ToolResultWithMeta;
    if (resultWithMeta?.status === 'success' && resultWithMeta.value) {
      resourceUri = resultWithMeta.value._meta?.ui?.resourceUri;
    }
  }

  // Tool names are formatted as "{extension_name}__{tool_name}".
  // Extension names can contain underscores (special chars like parentheses are normalized to "_"),
  // so we must use lastIndexOf to find the delimiter.
  // e.g., "my_server(local)" -> "my_server_local_" -> "my_server_local___get_time"
  const toolCallName =
    requestWithMeta.toolCall.status === 'success' ? requestWithMeta.toolCall.value.name : '';
  const delimiterIndex = toolCallName.lastIndexOf('__');
  const extensionName = delimiterIndex === -1 ? '' : toolCallName.substring(0, delimiterIndex);

  const toolArguments =
    requestWithMeta.toolCall.status === 'success'
      ? requestWithMeta.toolCall.value.arguments
      : undefined;

  // Memoize toolInput to prevent unnecessary re-renders
  const toolInput = useMemo(() => ({ arguments: toolArguments || {} }), [toolArguments]);

  // Memoize toolResult to prevent unnecessary re-renders
  const toolResult = useMemo(() => {
    if (!toolResponse) return undefined;
    const resultWithMeta = toolResponse.toolResult as ToolResultWithMeta;
    if (resultWithMeta?.status === 'success' && resultWithMeta.value) {
      return resultWithMeta.value;
    }
    return undefined;
  }, [toolResponse]);

  if (!resourceUri) return null;
  if (requestWithMeta.toolCall.status !== 'success') return null;

  return (
    <div className="mt-3">
      <McpAppRenderer
        resourceUri={resourceUri}
        toolInput={toolInput}
        toolResult={toolResult}
        extensionName={extensionName}
        sessionId={sessionId}
        append={append}
      />
      <div className="mt-3 p-4 py-3 border border-border-subtle rounded-lg bg-background-muted flex items-center">
        <FlaskConical className="mr-2" size={20} />
        <div className="text-sm font-sans">
          MCP Apps are experimental and may change at any time.
        </div>
      </div>
    </div>
  );
}

interface ToolCallRenderBoundaryProps {
  children: React.ReactNode;
  resetValue: unknown;
}

class ToolCallRenderBoundary extends React.Component<
  ToolCallRenderBoundaryProps,
  { error: Error | null }
> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidUpdate(previousProps: ToolCallRenderBoundaryProps) {
    if (this.state.error && previousProps.resetValue !== this.props.resetValue) {
      this.setState({ error: null });
    }
  }

  render() {
    if (!this.state.error) return this.props.children;

    return (
      <NotificationSurface
        status="error"
        role="alert"
        title="Tool details unavailable"
        message="This tool returned an unexpected response. The conversation can continue."
      >
        <details className="mt-2.5 text-xs text-text-muted">
          <summary className="w-fit cursor-pointer select-none font-medium text-text-default">
            Technical details
          </summary>
          <div className="mt-2 whitespace-pre-wrap break-words font-mono [overflow-wrap:anywhere]">
            {this.state.error.message}
          </div>
        </details>
      </NotificationSurface>
    );
  }
}

export default function ToolCallWithResponse(props: ToolCallWithResponseProps) {
  return (
    <ToolCallRenderBoundary resetValue={props.toolResponse ?? props.toolRequest}>
      <ToolCallWithResponseContent {...props} />
    </ToolCallRenderBoundary>
  );
}

function ToolCallWithResponseContent({
  sessionId,
  isCancelledMessage,
  toolRequest,
  toolResponse,
  notifications,
  isStreamingMessage,
  turnActive = false,
  append,
  onOpenArtifact,
  workingDir,
}: ToolCallWithResponseProps) {
  // Handle both the wrapped ToolResult format and the unwrapped format
  // The server serializes ToolResult<T> as { status: "success", value: T } or { status: "error", error: string }
  const toolCallData = toolRequest.toolCall as Record<string, unknown>;
  const toolCall =
    toolCallData?.status === 'success'
      ? (toolCallData.value as { name: string; arguments: Record<string, unknown> })
      : (toolCallData as { name: string; arguments: Record<string, unknown> });

  if (!toolCall || !toolCall.name) {
    return null;
  }

  const requestWithMeta = toolRequest as ToolRequestWithMeta;
  const resultWithMeta = toolResponse?.toolResult as ToolResultWithMeta;
  const hasMcpAppResourceURI = Boolean(
    requestWithMeta._meta?.ui?.resourceUri || resultWithMeta?.value?._meta?.ui?.resourceUri
  );
  const isError = getToolResultError(toolResponse?.toolResult) !== null;

  return (
    <>
      {/* D-17: a tool call is a LINE in the transcript, not a card. An outline
          around every one of them reads as a focus ring and turns a quiet
          transcript into a stack of boxes. Failure is signalled by colour — the
          status icon and label — plus a faint danger wash, never by an outline. */}
      <div
        className={cn(
          'w-full overflow-hidden rounded-md text-sm font-sans transition-colors',
          isError && 'bg-background-danger/5'
        )}
      >
        <ToolCallView
          {...{
            isCancelledMessage,
            sessionId,
            toolCall,
            toolResponse,
            notifications,
            isStreamingMessage,
            turnActive,
            onOpenArtifact,
            workingDir,
          }}
        />
      </div>
      {/* MCP UI — Inline */}
      {!hasMcpAppResourceURI &&
        toolResponse?.toolResult &&
        getToolResultContent(toolResponse.toolResult).map((content, index) => {
          const resourceContent = isEmbeddedResource(content)
            ? { ...content, type: 'resource' as const }
            : null;
          if (resourceContent && isUIResource(resourceContent)) {
            return (
              <div key={index} className="mt-3">
                <MCPUIResourceRenderer
                  content={resourceContent}
                  sessionId={sessionId}
                  appendPromptToChat={append}
                  onOpenArtifact={onOpenArtifact}
                />
              </div>
            );
          } else {
            return null;
          }
        })}

      {hasMcpAppResourceURI && sessionId && (
        <McpAppWrapper
          toolRequest={toolRequest}
          toolResponse={toolResponse}
          sessionId={sessionId}
          append={append}
        />
      )}
    </>
  );
}

interface ToolCallExpandableProps {
  label: string | React.ReactNode;
  isStartExpanded?: boolean;
  isForceExpand?: boolean;
  children: React.ReactNode;
  className?: string;
}

function ToolCallExpandable({
  label,
  isStartExpanded = false,
  isForceExpand,
  children,
  className = '',
}: ToolCallExpandableProps) {
  const [isExpandedState, setIsExpanded] = React.useState<boolean | null>(null);
  const isExpanded = isExpandedState === null ? isStartExpanded : isExpandedState;
  const toggleExpand = () => setIsExpanded(!isExpanded);
  React.useEffect(() => {
    if (isForceExpand) setIsExpanded(true);
  }, [isForceExpand]);

  return (
    <div className={className}>
      <Button
        onClick={toggleExpand}
        className="group h-6 min-h-0 max-w-full justify-start !px-0 py-0 text-left transition-colors rounded-md hover:bg-transparent focus-visible:bg-transparent"
        variant="ghost"
        size="xs"
      >
        <span className="flex min-w-0 max-w-full items-center overflow-hidden font-sans text-sm leading-6">
          {label}
        </span>
        <ChevronRight
          className={cn(
            'opacity-0 group-hover:opacity-70 group-focus-visible:opacity-70 transition-[opacity,transform] duration-[var(--motion-fast)] size-3.5 ml-1.5 text-text-muted',
            isExpanded && 'opacity-70',
            isExpanded && 'rotate-90'
          )}
        />
      </Button>
      {isExpanded && <div>{children}</div>}
    </div>
  );
}

interface ToolCallViewProps {
  isCancelledMessage: boolean;
  /**
   * Issue #56 DR-26 / Task 57: the chat a cross-institutional acceptance would
   * be keyed on. Absent on read-only surfaces that render a transcript with no
   * live session, where there is nothing to accept against.
   */
  sessionId?: string;
  toolCall: {
    name: string;
    arguments: Record<string, unknown>;
  };
  toolResponse?: ToolResponseMessageContent;
  notifications?: NotificationEvent[];
  /**
   * Retained for callers, but deliberately NOT read when deriving status: "this
   * message is currently last" is exactly the signal that used to paint pending
   * tool calls green. Use `turnActive` instead.
   */
  isStreamingMessage?: boolean;
  turnActive?: boolean;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
}

interface Progress {
  progress: number;
  progressToken: string;
  total?: number;
  message?: string;
}

export type ToolCallSummaryInput = {
  name: string;
  arguments?: Record<string, unknown>;
};

const MAX_SUMMARY_VALUE_LENGTH = 96;

const humanize = (value: string): string => snakeToTitleCase(value.replace(/[-.]/g, '_'));

const stringifySummaryValue = (value: unknown): string => {
  if (typeof value === 'string') return value;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  if (value === null || value === undefined) return '';
  return JSON.stringify(value);
};

const compactValue = (value: unknown, maxLength = MAX_SUMMARY_VALUE_LENGTH): string => {
  const text = stringifySummaryValue(value).replace(/\s+/g, ' ').trim();
  if (text.length <= maxLength) return text;
  const keep = Math.max(12, Math.floor((maxLength - 1) / 2));
  return `${text.slice(0, keep)}…${text.slice(-keep)}`;
};

const basename = (value: unknown): string => {
  const text = compactValue(value);
  const parts = text.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || text;
};

const firstPresent = (args: Record<string, unknown>, keys: string[]): unknown => {
  for (const key of keys) {
    if (args[key] !== undefined && args[key] !== null && args[key] !== '') {
      return args[key];
    }
  }
  return undefined;
};

/** Longest task snippet a delegation label carries before it is elided. */
const MAX_DELEGATION_LABEL = 64;

/** The opening sentence of `text`, trimmed to a label-sized snippet. */
const headline = (text: string): string => {
  const firstSentence = text.split(/(?<=[.!?])\s/)[0] ?? text;
  // A very short leading fragment ("Do this.") says less than the whole
  // instruction would, so fall back to the full text in that case.
  const candidate = firstSentence.length >= 12 ? firstSentence : text;
  if (candidate.length <= MAX_DELEGATION_LABEL) return candidate;
  return `${candidate.slice(0, MAX_DELEGATION_LABEL - 1).trimEnd()}…`;
};

/**
 * A label for a `subagent` call that names THIS delegation.
 *
 * Every subagent call carries the same argument *keys*, so the generic
 * "<Tool> with <keys>" fallback rendered each one as the byte-identical
 * "Subagent with instructions". Tool cards start collapsed, so a turn that fans
 * out to five subagents was five identical rows: the user could not tell which
 * result came from which without expanding all of them — the one thing a
 * transcript of delegated work has to get right.
 */
const summarizeDelegation = (args: Record<string, unknown>): string => {
  const workflow = compactValue(firstPresent(args, ['subworkflow']) ?? '');
  const instructions = stringifySummaryValue(firstPresent(args, ['instructions']) ?? '')
    .replace(/\s+/g, ' ')
    .trim();
  const task = instructions ? headline(instructions) : '';
  if (workflow && task) return `Delegating ${workflow}: ${task}`;
  if (workflow) return `Delegating ${workflow}`;
  if (task) return `Delegating: ${task}`;
  return 'Delegating to a subagent';
};

const summarizeSearchQuery = (value: unknown): string | null => {
  if (!value) return null;
  if (typeof value === 'string') return compactValue(value);
  if (Array.isArray(value)) {
    // A plain list of terms (e.g. search_modules' `terms: ["fetch", "http"]`)
    // reads best joined, not JSON-stringified.
    if (
      value.length > 0 &&
      value.every((item) => typeof item === 'string' || typeof item === 'number')
    ) {
      return compactValue(value.join(', '));
    }
    const first = value[0] as Record<string, unknown> | undefined;
    return first ? compactValue(first.q ?? first.query ?? first.search_query ?? value) : null;
  }
  if (typeof value === 'object') {
    const record = value as Record<string, unknown>;
    return compactValue(record.q ?? record.query ?? record.search_query ?? value);
  }
  return compactValue(value);
};

export function summarizeToolCall(toolCall: ToolCallSummaryInput): string {
  const args = toolCall.arguments ?? {};
  const toolName = getToolName(toolCall.name);
  const displayName = humanize(toolName);
  const target = firstPresent(args, [
    'path',
    'file',
    'file_path',
    'filename',
    'name',
    'uri',
    'url',
    'ref_id',
    'documentId',
    'spreadsheetId',
    'threadId',
    'id',
  ]);
  const targetName = target ? basename(target) : null;
  const command = firstPresent(args, ['cmd', 'command', 'script']);
  const operation = firstPresent(args, ['operation', 'action', 'method']);

  // Before the generic name-matching chain: a delegation must be identifiable
  // by its task, not by the fact that it is a delegation.
  if (toolName === 'subagent') return summarizeDelegation(args);

  // code_execution's module tools and the skills loader carry their targets
  // under argument names (`module_path`, `terms`, `name`) the generic chains
  // don't know, so their labels degraded to the opaque "Read Module" /
  // "Search Modules" verbs the transcript audit flagged (#27). Name the exact
  // target in the collapsed row. Matched on the FULL prefixed name (the
  // built-in extensions register as `code_execution` / `skills`), so an
  // unrelated extension's same-named tool keeps its own generic label.
  if (toolCall.name === 'code_execution__read_module') {
    const modulePath = compactValue(args.module_path ?? '');
    return modulePath ? `Reading module ${modulePath}` : 'Reading a module';
  }
  if (toolCall.name === 'code_execution__search_modules') {
    const terms = summarizeSearchQuery(args.terms);
    return terms ? `Searching modules for ${terms}` : 'Searching modules';
  }
  if (toolCall.name === 'skills__loadSkill') {
    const skillName = compactValue(args.name ?? '');
    return skillName ? `Loading skill ${skillName}` : 'Loading a skill';
  }

  if (toolName === 'text_editor' || toolName.includes('editor')) {
    if (args.command === 'view' && targetName) return `Reading ${targetName}`;
    if (args.command === 'write' && targetName) return `Writing ${targetName}`;
    if (args.command === 'str_replace' && targetName) return `Editing ${targetName}`;
    if (targetName) return `Updating ${targetName}`;
  }

  if (toolName === 'shell' || toolName === 'exec_command' || toolName.includes('command')) {
    return command ? `Running ${compactValue(command)}` : `Running a command`;
  }

  if (toolName === 'apply_patch') return `Applying a code patch`;
  if (toolName === 'view_image')
    return targetName ? `Inspecting ${targetName}` : `Inspecting an image`;
  if (toolName.includes('screenshot')) return `Capturing a screenshot`;
  if (toolName.includes('imagegen')) return `Generating an image`;

  if (toolName.includes('read') || toolName.includes('open') || toolName.includes('fetch')) {
    return targetName ? `Reading ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('write') || toolName.includes('update') || toolName.includes('edit')) {
    return targetName ? `Updating ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('create')) {
    return targetName ? `Creating ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('delete') || toolName.includes('remove')) {
    return targetName ? `Removing ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('list')) {
    return targetName ? `Listing ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('search') || toolName.includes('query')) {
    const query = summarizeSearchQuery(
      firstPresent(args, ['q', 'query', 'search_query', 'image_query', 'pattern', 'name'])
    );
    return query ? `Searching for ${query}` : `${displayName}`;
  }

  if (toolName.includes('download')) {
    return targetName ? `Downloading ${targetName}` : `${displayName}`;
  }

  if (toolName.includes('send') || toolName.includes('post')) {
    return targetName ? `Sending to ${targetName}` : `${displayName}`;
  }

  if (toolName === 'sheets_tool' || toolName.includes('sheet')) {
    return operation ? `${humanize(compactValue(operation))} in a spreadsheet` : displayName;
  }

  if (toolName === 'docs_tool' || toolName.includes('doc')) {
    return operation ? `${humanize(compactValue(operation))} in a document` : displayName;
  }

  if (toolName === 'execute_code') {
    const toolGraph = normalizeToolGraph(args.tool_graph);
    if (toolGraph.length > 0) {
      if (toolGraph.length === 1) return compactValue(toolGraph[0].description);
      return `Coordinating ${toolGraph.length} tool steps`;
    }
    return `Executing code`;
  }

  if (targetName) return `${displayName} for ${targetName}`;

  const keys = Object.keys(args).filter((key) => !['content', 'text', 'body'].includes(key));
  if (keys.length > 0) {
    return `${displayName} with ${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '…' : ''}`;
  }

  return displayName;
}

export const logToString = (logMessage: NotificationEvent) => {
  const message = logMessage.message as { method: string; params: unknown };
  const params = message.params as Record<string, unknown>;

  // Special case for the developer system shell logs
  if (
    params &&
    params.data &&
    typeof params.data === 'object' &&
    'output' in params.data &&
    'stream' in params.data
  ) {
    return `[${params.data.stream}] ${params.data.output}`;
  }

  // Issue #72: the developer shell's foreground heartbeat. A command that prints
  // nothing while it works — a big `find`, a slow query — used to leave the card
  // saying "Working through the tool call" indefinitely, which is
  // indistinguishable from a hung agent. Render its ready-made sentence rather
  // than the raw JSON the fallback below would produce.
  if (
    params &&
    params.data &&
    typeof params.data === 'object' &&
    'message' in params.data &&
    typeof (params.data as { message: unknown }).message === 'string'
  ) {
    return (params.data as { message: string }).message;
  }

  return typeof params.data === 'string' ? params.data : JSON.stringify(params.data);
};

const notificationToProgress = (notification: NotificationEvent): Progress => {
  const message = notification.message as { method: string; params: unknown };
  return message.params as Progress;
};

// Helper function to extract toolcall name
const getToolName = (toolCallName: string): string => {
  const lastIndex = toolCallName.lastIndexOf('__');
  if (lastIndex === -1) return toolCallName;

  return toolCallName.substring(lastIndex + 2);
};

function ToolCallView({
  isCancelledMessage,
  sessionId,
  toolCall,
  toolResponse,
  notifications,
  turnActive = false,
  onOpenArtifact,
  workingDir,
}: ToolCallViewProps) {
  const [responseStyle, setResponseStyle] = useState(() => localStorage.getItem('response_style'));

  useEffect(() => {
    const handleStorageChange = () => {
      setResponseStyle(localStorage.getItem('response_style'));
    };

    window.addEventListener('storage', handleStorageChange);

    window.addEventListener('responseStyleChanged', handleStorageChange);

    return () => {
      window.removeEventListener('storage', handleStorageChange);
      window.removeEventListener('responseStyleChanged', handleStorageChange);
    };
  }, []);

  const isToolDetails = toolCall?.arguments && Object.entries(toolCall.arguments).length > 0;

  const toolError = getToolResultError(toolResponse?.toolResult);

  // Issue #56, DR-26 / Task 57. A cross-institutional refusal Gate C would clear
  // on a grant — the ONE refusal in this feature whose way out is a thing the
  // user does rather than a model they switch to. `null` for every other
  // failure, including the two sites that compose the same refusal without
  // consulting a grant.
  const acceptOffer = crossAffiliationOffer(toolError);

  // Executed sub-call telemetry (#28): what a coordinated execute_code step
  // actually ran, with exact inputs and per-call status. Shown alongside the
  // declared tool_graph plan — never force-matched to it, since loops and
  // conditionals mean plan and execution can legitimately differ.
  const resultMeta = (toolResponse?.toolResult as ToolResultWithMeta | undefined)?.value?._meta;
  const executedCalls = normalizeExecutedCalls(resultMeta?.['biorouter/tool-calls']);
  const droppedExecutedCalls =
    typeof resultMeta?.['biorouter/tool-calls-dropped'] === 'number'
      ? resultMeta['biorouter/tool-calls-dropped']
      : 0;

  // Status is derived from FACTS — is there a result, and is the turn still
  // running — never from "am I the last message". The old derivation keyed off
  // `!isStreamingMessage`, which flips false for a still-executing tool call the
  // instant any later message arrives, painting a pending card green "Finished".
  //
  // The response-less-after-the-turn-ended case still must not spin forever
  // (some backends never emit a tool response: cancelled turns, BioRouterMode
  // ::Chat skips, crashed extensions), so it routes to a distinct `interrupted`
  // state rather than a fabricated success.
  const loadingStatus: ToolLoadingStatus = toolResponse
    ? toolError
      ? 'error'
      : 'success'
    : turnActive
      ? 'loading'
      : 'interrupted';

  const toolResults =
    loadingStatus === 'success' && toolResponse?.toolResult
      ? getToolResultContent(toolResponse.toolResult)
      : [];

  const logs = notifications
    ?.filter((notification) => {
      const message = notification.message as { method?: string };
      return message.method === 'notifications/message';
    })
    .map(logToString);

  const progress = notifications
    ?.filter((notification) => {
      const message = notification.message as { method?: string };
      return message.method === 'notifications/progress';
    })
    .map(notificationToProgress)
    .reduce((map, item) => {
      const key = item.progressToken;
      if (!map.has(key)) {
        map.set(key, []);
      }
      map.get(key)!.push(item);
      return map;
    }, new Map<string, Progress[]>());

  const progressEntries = [...(progress?.values() || [])].map(
    (entries) => entries.sort((a, b) => b.progress - a.progress)[0]
  );

  // Map ToolLoadingStatus to ToolCallStatus
  const getToolCallStatus = (loadingStatus: ToolLoadingStatus): ToolCallStatus => {
    switch (loadingStatus) {
      case 'success':
        return 'success';
      case 'error':
        return 'error';
      case 'loading':
        return 'loading';
      default:
        return 'pending';
    }
  };

  const toolCallStatus = getToolCallStatus(loadingStatus);
  const toolSummary = summarizeToolCall(toolCall);
  const latestProgress = progressEntries[0]?.message;
  const latestLog = logs && logs.length > 0 ? logs[logs.length - 1] : undefined;
  const liveDetail =
    loadingStatus === 'loading'
      ? compactValue(latestProgress || latestLog || 'Working through the tool call')
      : loadingStatus === 'error'
        ? 'Tool call failed'
        : loadingStatus === 'interrupted'
          ? 'No result'
          : toolResults.length > 0
            ? `${toolResults.length} result${toolResults.length === 1 ? '' : 's'} ready`
            : null;

  const toolLabel = (
    <span className="flex items-center gap-2 min-w-0 text-left leading-6">
      <ToolIconWithStatus
        ToolIcon={getToolCallIcon(toolCall.name)}
        status={toolCallStatus}
        className="mt-px"
      />
      <span className="min-w-0 flex-1 truncate text-text-muted">
        <span>
          {loadingStatus === 'loading'
            ? 'Working on'
            : loadingStatus === 'error'
              ? 'Problem with'
              : loadingStatus === 'interrupted'
                ? 'Stopped'
                : 'Ran'}{' '}
          <span className="text-text-default">{toolSummary}</span>
        </span>
        {liveDetail && (
          <span
            className={cn('text-text-muted/70', loadingStatus === 'loading' && 'animate-pulse')}
          >
            {' '}
            · {liveDetail}
          </span>
        )}
      </span>
    </span>
  );
  return (
    // ⚠ A grantable cross-institutional refusal starts EXPANDED (issue #56, Task
    // 57). A failed tool call is otherwise a collapsed line, and a way out that
    // only exists behind a disclosure most people never open is the hard block
    // this task exists to remove, one click further away. Every other failure
    // keeps the quiet default.
    <ToolCallExpandable
      isStartExpanded={acceptOffer !== null}
      isForceExpand={false}
      label={toolLabel}
    >
      {(() => {
        const toolName = toolCall.name.substring(toolCall.name.lastIndexOf('__') + 2);
        const toolGraph = normalizeToolGraph(toolCall.arguments?.tool_graph);
        const code = toolCall.arguments?.code as unknown as string | undefined;
        const hasToolGraph = toolName === 'execute_code' && toolGraph.length > 0;

        if (hasToolGraph) {
          return (
            <div className="border-t border-border-subtle">
              <ToolGraphView toolGraph={toolGraph} code={code} />
            </div>
          );
        }

        if (isToolDetails) {
          return (
            <div className="border-t border-border-subtle">
              <ToolDetailsView toolCall={toolCall} isStartExpanded={false} />
            </div>
          );
        }

        return null;
      })()}

      {executedCalls.length > 0 && (
        <div className="border-t border-border-subtle">
          <ExecutedCallsView calls={executedCalls} dropped={droppedExecutedCalls} />
        </div>
      )}

      {toolError && (
        <div className="border-t border-border-subtle px-3 py-3">
          <NotificationContent status="error" title="Tool call failed" message={toolError} />
          {/*
            Issue #56, DR-26 / Task 57 — the accept control, on the surface the
            refusal lands on and directly under the daemon's own words. DR-26
            rules warn-then-allow-if-the-user-insists; without this the refusal
            arrived, the model was told to ask, and the person at the keyboard
            had nothing to press.
          */}
          {acceptOffer && (
            <CrossAffiliationAcceptCard sessionId={sessionId} extension={acceptOffer.extension} />
          )}
        </div>
      )}

      {logs && logs.length > 0 && (
        <div className="border-t border-border-subtle">
          <ToolLogsView
            logs={logs}
            working={loadingStatus === 'loading'}
            isStartExpanded={
              loadingStatus === 'loading' || responseStyle === 'detailed' || responseStyle === null
            }
          />
        </div>
      )}

      {toolResults.length === 0 &&
        progressEntries.length > 0 &&
        progressEntries.map((entry, index) => (
          <div className="p-3 border-t border-border-subtle" key={index}>
            <ProgressBar progress={entry.progress} total={entry.total} message={entry.message} />
          </div>
        ))}

      {/* Tool Output */}
      {!isCancelledMessage && (
        <>
          {toolResults.map((result, index) => (
            <div key={index} className={cn('border-t border-border-subtle')}>
              <ToolResultView
                result={result}
                isStartExpanded={false}
                onOpenArtifact={onOpenArtifact}
                workingDir={workingDir}
              />
            </div>
          ))}
        </>
      )}
    </ToolCallExpandable>
  );
}

interface ToolDetailsViewProps {
  toolCall: {
    name: string;
    arguments: Record<string, unknown>;
  };
  isStartExpanded: boolean;
}

function ToolDetailsView({ toolCall, isStartExpanded }: ToolDetailsViewProps) {
  return (
    <ToolCallExpandable
      label={<span className="pl-2 font-sans text-sm text-text-muted">View tool details</span>}
      isStartExpanded={isStartExpanded}
    >
      <div className="pr-4 pl-6 pb-2">
        {toolCall.arguments && (
          <ToolCallArguments args={toolCall.arguments as Record<string, ToolCallArgumentValue>} />
        )}
      </div>
    </ToolCallExpandable>
  );
}

interface ToolGraphViewProps {
  toolGraph: ToolGraphNode[];
  code?: string;
}

function ToolGraphView({ toolGraph, code }: ToolGraphViewProps) {
  // The one shared palette (styles/codeTheme.ts) — the same source chat code
  // blocks and the artifact panel read, so the generated-code view can never
  // drift from them. Both hooks are non-throwing outside a provider.
  const codeStyle = codeThemesByFamily[useThemeFamily()][useResolvedTheme()];

  const renderGraph = () => {
    if (toolGraph.length === 0) return null;

    const lines: string[] = [];

    toolGraph.forEach((node, index) => {
      const deps =
        node.depends_on.length > 0 ? ` (uses ${node.depends_on.map((d) => d + 1).join(', ')})` : '';
      lines.push(`${index + 1}. ${node.tool}: ${node.description}${deps}`);
    });

    return lines.join('\n');
  };

  return (
    <div className="px-4 py-2">
      <pre className="font-sans text-sm text-text-muted whitespace-pre-wrap overflow-x-auto">
        {renderGraph()}
      </pre>
      {code && (
        <div className="border-t border-border-subtle -mx-4 mt-2">
          <ToolCallExpandable
            label={
              <span className="pl-2 font-sans text-sm text-text-muted">View generated code</span>
            }
            isStartExpanded={false}
          >
            {/* bg-background-code: the ground the syntax palette is verified
                against (see MarkdownContent's CodeBlock). No line numbers, so
                long lines may wrap (never combine wrapping with line numbers
                in react-syntax-highlighter). */}
            <div className="w-full overflow-x-auto bg-background-code">
              <SyntaxHighlighter
                style={codeStyle}
                language="javascript"
                PreTag="div"
                customStyle={{
                  margin: 0,
                  padding: '12px',
                  background: 'transparent',
                  width: '100%',
                  maxWidth: '100%',
                }}
                codeTagProps={{
                  style: {
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                    overflowWrap: 'break-word',
                    fontFamily: CODE_FONT_FAMILY,
                    fontSize: CODE_FONT_SIZE,
                    lineHeight: CODE_LINE_HEIGHT,
                  },
                }}
                showLineNumbers={false}
                wrapLines={false}
              >
                {code}
              </SyntaxHighlighter>
            </div>
          </ToolCallExpandable>
        </div>
      )}
    </div>
  );
}

/** Parse a recorded args JSON string; null if truncated/malformed. */
function parsedCallArguments(args?: string): Record<string, ToolCallArgumentValue> | null {
  if (!args) return null;
  try {
    const parsed: unknown = JSON.parse(args);
    const record = recordOf(parsed);
    return record ? (record as Record<string, ToolCallArgumentValue>) : null;
  } catch {
    return null;
  }
}

/**
 * The EXECUTED sub-calls of a coordinated step (#28): which tools actually
 * ran, with which exact inputs, and which one failed — so a failed step is
 * attributable to a specific call instead of reading as "the step failed".
 */
function ExecutedCallsView({ calls, dropped }: { calls: ExecutedToolCall[]; dropped: number }) {
  return (
    <ToolCallExpandable
      label={
        <span className="pl-2 font-sans text-sm text-text-muted">
          View executed calls ({calls.length})
        </span>
      }
      isStartExpanded={false}
    >
      <div className="pl-4 pr-4 pb-2">
        {calls.map((call, index) => (
          <ExecutedCallRow key={index} call={call} index={index} />
        ))}
        {dropped > 0 && (
          <div className="py-1 font-sans text-xs text-text-muted">
            …and {dropped} more call{dropped === 1 ? '' : 's'} not recorded.
          </div>
        )}
      </div>
    </ToolCallExpandable>
  );
}

/**
 * Executed-call telemetry is untrusted wire data replayed from the result
 * meta, so keys and values render as PLAIN TEXT only — never through
 * `ToolCallArguments`, whose expanded strings go through `MarkdownContent`
 * and would turn crafted arguments into live links or remote-image fetches.
 */
function ExecutedCallArguments({ args }: { args: Record<string, ToolCallArgumentValue> }) {
  return (
    <div className="my-2">
      {Object.entries(args).map(([key, value]) => (
        <div key={key} className="mb-2 flex flex-row font-sans text-sm">
          <span className="min-w-[140px] shrink-0 text-text-muted">{key}</span>
          <pre className="min-w-0 max-w-full flex-1 overflow-x-auto whitespace-pre-wrap break-words font-sans text-sm text-text-muted">
            {typeof value === 'string' ? value : JSON.stringify(value, null, 2)}
          </pre>
        </div>
      ))}
    </div>
  );
}

function ExecutedCallRow({ call, index }: { call: ExecutedToolCall; index: number }) {
  const parsedArgs = parsedCallArguments(call.args);
  return (
    <ToolCallExpandable
      label={
        <span className="flex min-w-0 items-center gap-2 text-left leading-6">
          <ToolIconWithStatus
            ToolIcon={getToolCallIcon(call.tool)}
            status={call.status === 'error' ? 'error' : 'success'}
            className="mt-px"
          />
          <span className="min-w-0 flex-1 truncate text-text-muted">
            <span className="text-text-default">
              {index + 1}. {call.tool}
            </span>
            {call.status === 'error' && ' · failed'}
          </span>
        </span>
      }
      isStartExpanded={false}
    >
      <div className="pl-6 pr-2 pb-2">
        {parsedArgs && Object.keys(parsedArgs).length > 0 ? (
          <ExecutedCallArguments args={parsedArgs} />
        ) : call.args ? (
          // Truncated/malformed JSON still shows the exact recorded text.
          <pre className="whitespace-pre-wrap break-all font-mono text-xs text-text-muted">
            {call.args}
          </pre>
        ) : (
          <div className="font-sans text-xs text-text-muted">No arguments recorded.</div>
        )}
        {call.error && (
          <div className="mt-2">
            <NotificationContent
              status="error"
              title={`${call.tool} failed`}
              message={call.error}
            />
          </div>
        )}
      </div>
    </ToolCallExpandable>
  );
}

interface ToolResultViewProps {
  result: Content;
  isStartExpanded: boolean;
  onOpenArtifact?: (artifact: ArtifactSource) => void;
  workingDir?: string;
}

function ToolResultView({
  result,
  isStartExpanded,
  onOpenArtifact,
  workingDir,
}: ToolResultViewProps) {
  const hasText = (c: Content): c is Content & { text: string } =>
    'text' in c && typeof (c as Record<string, unknown>).text === 'string';

  const hasImage = (c: Content): c is Content & { data: string; mimeType: string } => {
    if (!('data' in c && 'mimeType' in c)) return false;
    const mimeType = (c as Record<string, unknown>).mimeType;
    return typeof mimeType === 'string' && mimeType.startsWith('image');
  };

  const hasResource = (c: Content): c is Content & { resource: unknown } => 'resource' in c;

  return (
    <ToolCallExpandable
      label={<span className="pl-2 py-1 font-sans text-sm text-text-muted">View output</span>}
      isStartExpanded={isStartExpanded}
    >
      <div className="pl-4 pr-4 py-4">
        {hasText(result) && (
          <MarkdownContent
            content={result.text}
            className="whitespace-pre-wrap max-w-full overflow-x-auto"
            onOpenArtifact={onOpenArtifact}
            workingDir={workingDir}
          />
        )}
        {hasImage(result) && (
          <img
            src={`data:${result.mimeType};base64,${result.data}`}
            alt="Tool result"
            className="max-w-full h-auto rounded-md my-2"
            onError={(e) => {
              console.error('Failed to load image');
              e.currentTarget.style.display = 'none';
            }}
          />
        )}
        {hasResource(result) && (
          <pre className="font-sans text-sm whitespace-pre-wrap break-all overflow-x-auto max-w-full">
            {JSON.stringify(result, null, 2)}
          </pre>
        )}
      </div>
    </ToolCallExpandable>
  );
}

function ToolLogsView({
  logs,
  working,
  isStartExpanded,
}: {
  logs: string[];
  working: boolean;
  isStartExpanded?: boolean;
}) {
  const boxRef = useRef<HTMLDivElement>(null);

  // Whenever logs update, jump to the newest entry
  useEffect(() => {
    if (boxRef.current) {
      boxRef.current.scrollTop = boxRef.current.scrollHeight;
    }
  }, [logs.length]);
  // normally we do not want to put .length on an array in react deps:
  //
  // if the objects inside the array change but length doesn't change you want updates
  //
  // in this case, this is array of strings which once added do not change so this cuts
  // down on the possibility of unwanted runs

  return (
    <ToolCallExpandable
      label={
        <span className="pl-2 py-1 font-sans text-sm flex items-center text-text-muted">
          <span>View live logs</span>
          {working && (
            <div className="mx-2 inline-block">
              <span
                className="inline-block animate-spin rounded-full border-2 border-t-transparent border-current"
                style={{ width: 8, height: 8 }}
                role="status"
                aria-label="Loading spinner"
              />
            </div>
          )}
        </span>
      }
      isStartExpanded={isStartExpanded}
    >
      <div
        ref={boxRef}
        className={`flex flex-col items-start space-y-2 overflow-y-auto p-4 ${working ? 'max-h-[4rem]' : 'max-h-[20rem]'}`}
      >
        {logs.map((log, i) => (
          <span
            key={i}
            className="w-full whitespace-pre-wrap break-words font-sans text-sm text-text-muted"
          >
            {log}
          </span>
        ))}
      </div>
    </ToolCallExpandable>
  );
}

const ProgressBar = ({ progress, total, message }: Omit<Progress, 'progressToken'>) => {
  const isDeterminate = typeof total === 'number';
  const percent = isDeterminate ? Math.min((progress / total!) * 100, 100) : 0;

  return (
    <div className="w-full space-y-2">
      {message && <div className="font-sans text-sm text-text-muted">{message}</div>}

      <div className="w-full bg-background-muted rounded-md h-4 overflow-hidden relative">
        {isDeterminate ? (
          <div
            className="bg-background-accent h-full w-full origin-left transition-transform duration-[var(--motion-base)] ease-[var(--ease-out)]"
            style={{ transform: `scaleX(${Math.min(Math.max(percent / 100, 0), 1)})` }}
          />
        ) : (
          <div className="absolute inset-0 animate-pulse bg-background-accent" />
        )}
      </div>
    </div>
  );
};
