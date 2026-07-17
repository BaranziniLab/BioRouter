import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { SearchView } from './conversation/SearchView';
import LoadingBioRouter from './LoadingBioRouter';
import ProgressiveMessageList from './ProgressiveMessageList';
import { MainPanelLayout } from './Layout/MainPanelLayout';
import ChatInput from './ChatInput';
import { ScrollArea, ScrollAreaHandle } from './ui/scroll-area';
import { useFileDrop } from '../hooks/useFileDrop';
import { selectBilledTokens } from '../utils/billedTokens';
import { mostCompleteBilledTokens } from '../utils/usageAccounting';
import { Message } from '../api';
import type { UserAttachment } from '../types/message';
import {
  getProviderMetadata,
  modelSupportedInputMimeTypes,
  modelSupportsVision,
} from './settings/models/modelInterface';
import { ChatState } from '../types/chatState';
import { ChatType } from '../types/chat';
import { useIsMobile } from '../hooks/use-mobile';
import { useSidebar } from './ui/sidebar';
import { cn } from '../utils';
import { useChatStream } from '../hooks/useChatStream';
import { useNavigation } from '../hooks/useNavigation';
import { WorkflowHeader } from './WorkflowHeader';
import { WorkflowWarningModal } from './ui/WorkflowWarningModal';
import { scanWorkflow } from '../workflow';
import { useCostTracking } from '../hooks/useCostTracking';
import { useDiverge } from '../hooks/useDiverge';
import WorkflowActivities from './workflows/WorkflowActivities';
import { useToolCount } from './alerts/useToolCount';
import { Button } from './ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { CodeAnalysis, Pipeline, ScrollText, Terminal } from './icons/app-icons';
import {
  createArtifactRenderRepairMessage,
  getThinkingMessage,
  getTextContent,
} from '../types/message';
import ParameterInputModal from './ParameterInputModal';
import { substituteParameters } from '../utils/providerUtils';
import CreateWorkflowFromSessionModal from './workflows/CreateWorkflowFromSessionModal';
import CreateEditWorkflowModal from './workflows/CreateEditWorkflowModal';
import { DiagnosticsModal } from './ui/Diagnostics';
import { toastSuccess } from '../toasts';
import { Workflow } from '../workflow';
import { createSession } from '../sessions';
import { getInitialWorkingDir } from '../utils/workingDir';
import { useConfig } from './ConfigContext';
import { useTerminalDock } from '../contexts/TerminalDockContext';
import { SessionNamePill } from './Dashboard/SessionNamePill';
import { getSessionTitlePadding } from './Layout/TitlebarControls';
import { announceSessionName, renameSession } from '../utils/sessionNameSync';
import { toastError } from '../toasts';
import { errorMessage, isConnectionError } from '../utils/conversionUtils';
import { Greeting } from './common/Greeting';
import { navigateWithViewTransition } from '../utils/navigationUtils';
import ArtifactViewer from './artifacts/ArtifactViewer';
import InAppTerminalDock from './InAppTerminalDock';
import { ChatTurnError, hasVisibleTurnErrorMessage } from './conversation/ChatTurnError';
import type { ArtifactRenderError } from './artifacts/ArtifactViewer';
import type { ArtifactSource } from './artifacts/artifactTypes';
import {
  artifactSourceFromResource,
  basenameFromPath,
  fileArtifactPathsFromToolCall,
  looksLikePreviewableFile,
  pathFromArtifactHref,
  resolveArtifactPath,
} from './artifacts/artifactUtils';
import type { CallToolResponse, Content, EmbeddedResource, ResourceContents } from '../api';
import {
  PREVIEW_MIN_WIDTH,
  PreviewPanelMode,
  previewPanelMode,
  SIDEBAR_COMPACT_WIDTH as SIDEBAR_COMPACT_TITLE_WIDTH,
} from './Layout/yieldLadder';

// Context for sharing current model info
const CurrentModelContext = createContext<{ model: string; mode: string } | null>(null);
export const useCurrentModelInfo = () => useContext(CurrentModelContext);

// Rung 2 of the yield ladder (D-32). The floor is shared with the ladder rather
// than re-declared, so the number the rule reasons about and the number the
// panel clamps to cannot drift apart.
const ARTIFACT_PANEL_MIN_WIDTH = PREVIEW_MIN_WIDTH;
const ARTIFACT_PANEL_MAX_WIDTH = 920;
// The chat's PREFERRED width — what the panel gives up first. Below this the
// panel keeps narrowing toward its own 360px floor (the clamp in
// getMaxArtifactPanelWidth), and at ARTIFACT_PANEL_MIN_WIDTH + CHAT_MIN_WIDTH
// there is nothing left to give and rung 2 fires. See previewPanelMode.
const ARTIFACT_PANEL_MIN_CHAT_WIDTH = 640;
const ARTIFACT_PANEL_DEFAULT_WIDTH_RATIO = 0.48;
const ARTIFACT_PANEL_AUTO_TUCK_WIDTH =
  ARTIFACT_PANEL_MIN_WIDTH + ARTIFACT_PANEL_MIN_CHAT_WIDTH + 48;
const ARTIFACT_PANEL_AUTO_EXPAND_PADDING = 24;
// Matches the panel's close transition (--motion-fast); exit is a tier faster
// than the --motion-base entrance so the panel unmounts as the slide completes.
const ARTIFACT_PANEL_EXIT_MS = 120;
// Imported, not re-declared — see SIDEBAR_COMPACT_WIDTH. This file held the
// third copy of 1120.
// How long after the agent last worked a render failure is still treated as part
// of the current exchange (and worth auto-fixing). A figure the agent just made
// usually errors within a second or two of finishing; a failure that surfaces
// well after this window almost always means the user is managing an artifact
// from a finished conversation — reopening an old figure, editing an app's code,
// deleting it — and must not silently resume the chat.
const ARTIFACT_REPAIR_ACTIVE_GRACE_MS = 15_000;
const HEADER_ACTION_BUTTON_CLASS =
  'no-drag flex h-8 w-8 items-center justify-center rounded-md p-0 text-text-default/70 transition-colors hover:bg-background-medium hover:text-text-default';
const PREVIEWABLE_TEXT_ARTIFACT_RE =
  /(?<![\w:/\\@])(?:file:\/\/|~[\\/]|\.{1,2}[\\/]|[a-z]:[\\/]|\/|\\\\)[^\s)\]}\x60"'<>]+\.(?:html?|png|jpe?g|gif|webp|svg|pdf|docx|xlsx|pptx|ipynb|sql|md|qmd|rmd|txt|log|json|csv|tsv|ya?ml|toml|xml|css|ts|tsx|js|jsx|py|r|rs|go|java|c|cpp|h|hpp)(?:[?#][^\s)\]}\x60"'<>]*)?(?![\w./\\])/gi;

function clampArtifactPanelWidth(value: number, max: number) {
  return Math.min(Math.max(value, ARTIFACT_PANEL_MIN_WIDTH), max);
}

export function getDefaultArtifactPanelWidth(containerWidth: number): number {
  const maxWidth = Math.max(
    ARTIFACT_PANEL_MIN_WIDTH,
    Math.min(ARTIFACT_PANEL_MAX_WIDTH, containerWidth - ARTIFACT_PANEL_MIN_CHAT_WIDTH)
  );
  return clampArtifactPanelWidth(
    Math.round(containerWidth * ARTIFACT_PANEL_DEFAULT_WIDTH_RATIO),
    maxWidth
  );
}

export function getArtifactPanelExpansionContentWidth(
  contentWidth: number,
  splitPaneWidth: number
): number | null {
  if (!Number.isFinite(contentWidth) || !Number.isFinite(splitPaneWidth)) return null;
  const deficit = ARTIFACT_PANEL_AUTO_TUCK_WIDTH - splitPaneWidth;
  if (deficit <= 0) return null;
  return Math.ceil(contentWidth + deficit + ARTIFACT_PANEL_AUTO_EXPAND_PADDING);
}

// Whether an artifact render failure should be fed back to the agent to fix.
//
// Only when the conversation is live: a turn is running (any non-idle state that
// is not merely reloading a saved session), or one finished within the grace
// window. Once the chat has been quiet longer than that it is treated as over,
// so a failure surfacing from the user reopening an old figure, editing an app's
// code, or deleting it does NOT silently resume the chat to "repair" it.
export function shouldAutoRepairArtifact(
  chatState: ChatState,
  lastAgentActiveAt: number,
  now: number
): boolean {
  if (chatState !== ChatState.Idle && chatState !== ChatState.LoadingConversation) {
    return true;
  }
  return now - lastAgentActiveAt < ARTIFACT_REPAIR_ACTIVE_GRACE_MS;
}

/**
 * Session filter for BROADCAST window events.
 *
 * Several chat events are dispatched on `window`, which every mounted BaseChat
 * hears. That is latent on /pair today (one BaseChat) but the Dashboard already
 * mounts N of them (Dashboard/ChatWindow.tsx), and tabbed chat will mount N on
 * /pair — at which point an unfiltered listener lets chat A drive chat B.
 *
 * The predicate is deliberately LENIENT, verbatim from the established idiom at
 * ChatInput.tsx:379-382 ('focus-chat-input'): an event that carries no sessionId
 * is treated as a true broadcast and handled by everyone. That keeps any
 * dispatcher we haven't updated (or one outside this repo) working exactly as it
 * does today. Every in-app dispatcher of these events now sets `sessionId`, so
 * the lenient branch is a back-compat path, not the normal one.
 *
 * @param detail the CustomEvent detail, if any
 * @param sessionId the listening BaseChat's own session
 */
export function isEventForSession(
  detail: { sessionId?: string | null } | null | undefined,
  sessionId: string
): boolean {
  if (detail?.sessionId && detail.sessionId !== sessionId) return false;
  return true;
}

/** Delay before scrolling, so appended content has rendered first. */
export const SCROLL_TO_BOTTOM_DELAY_MS = 200;

/**
 * The OS-window content width this chat needs to fit its artifact panel, or
 * null if it must not resize the window at all.
 *
 * Resizing the window is app-scoped, but BaseChat is session-scoped. When more
 * than one chat is mounted (the Dashboard does this today), only the focused one
 * may resize — otherwise a background chat opening an artifact yanks the window
 * out from under the chat the user is actually looking at.
 */
export function artifactPanelTargetContentWidth(opts: {
  isMobile: boolean;
  allowWindowResize: boolean;
  windowWidth: number;
  splitPaneWidth: number;
}): number | null {
  if (opts.isMobile) return null;
  if (!opts.allowWindowResize) return null;
  return getArtifactPanelExpansionContentWidth(opts.windowWidth, opts.splitPaneWidth) || null;
}

/**
 * Builds the 'scroll-chat-to-bottom' listener for one chat. Extracted from the
 * effect so two of them can be registered on a real window in a test without
 * mounting two whole BaseChats — the exported factory IS what the effect uses,
 * so the guard under test cannot drift from the guard that ships.
 */
export function createScrollToBottomHandler(deps: {
  sessionId: string;
  scrollToBottom: () => void;
}): (event: Event) => void {
  return (event: Event) => {
    const detail = (event as CustomEvent<{ sessionId?: string | null }>).detail;
    if (!isEventForSession(detail, deps.sessionId)) return;
    setTimeout(() => deps.scrollToBottom(), SCROLL_TO_BOTTOM_DELAY_MS);
  };
}

export interface SessionDivergedDetail {
  /** The session diverged FROM — see createSessionDivergedHandler. */
  sessionId?: string | null;
  newSessionId: string;
  shouldStartAgent?: boolean;
  editedMessage?: string;
}

/**
 * Builds the 'session-diverged' listener for one chat.
 *
 * The predicate matches on the ORIGIN session (detail.sessionId), NOT on
 * detail.newSessionId: diverging mints a brand-new session, so newSessionId
 * belongs to no currently-mounted BaseChat and matching on it would select
 * nobody. The chat that should navigate is the one the user diverged from,
 * which is the dispatching ChatStreamController's own session
 * (chatStreamStore.tsx, ChatStreamController.onMessageUpdate).
 *
 * Without the guard, every mounted BaseChat calls navigate() to the same URL —
 * N racing navigations once more than one chat is on screen.
 */
export function createSessionDivergedHandler(deps: {
  sessionId: string;
  navigate: (to: string, options: { state: Record<string, unknown> }) => void;
}): (event: Event) => void {
  return (event: Event) => {
    const detail = (event as CustomEvent<SessionDivergedDetail>).detail;
    if (!isEventForSession(detail, deps.sessionId)) return;
    const { newSessionId, shouldStartAgent, editedMessage } = detail;

    const params = new URLSearchParams();
    params.set('resumeSessionId', newSessionId);
    if (shouldStartAgent) {
      params.set('shouldStartAgent', 'true');
    }

    deps.navigate(`/pair?${params.toString()}`, {
      state: {
        disableAnimation: true,
        initialMessage: editedMessage,
      },
    });
  };
}

function isEmbeddedResource(content: Content): content is EmbeddedResource {
  return 'resource' in content && typeof (content as Record<string, unknown>).resource === 'object';
}

function getToolResultContent(toolResult: Record<string, unknown>): Content[] {
  const wrapped = toolResult as {
    status?: string;
    value?: CallToolResponse;
  };
  const response =
    wrapped.status === 'success'
      ? wrapped.value
      : (toolResult as unknown as CallToolResponse | undefined);
  if (!response || !Array.isArray(response.content)) return [];
  return response.content.filter((item) => {
    const annotations = (item as { annotations?: { audience?: string[] } }).annotations;
    return !annotations?.audience || annotations.audience.includes('user');
  });
}

function artifactKey(artifact: ArtifactSource) {
  switch (artifact.kind) {
    case 'html':
      return `html:${artifact.title}:${artifact.html.length}:${artifact.html.slice(0, 80)}`;
    case 'externalUrl':
      return `url:${artifact.url}`;
    case 'file':
      return `file:${artifact.path}`;
    case 'mcpResource': {
      const resource = artifact.resource as ResourceContents & { blob?: string };
      const textLength = 'text' in resource ? resource.text.length : 0;
      const blobLength = typeof resource.blob === 'string' ? resource.blob.length : 0;
      return `resource:${resource.uri}:${resource.mimeType ?? ''}:${textLength}:${blobLength}`;
    }
  }
}

function collectTextArtifacts(text: string, workingDir?: string): ArtifactSource[] {
  const artifacts: ArtifactSource[] = [];
  for (const match of text.matchAll(PREVIEWABLE_TEXT_ARTIFACT_RE)) {
    const href = match[0];
    if (!looksLikePreviewableFile(href)) continue;
    const rawPath = pathFromArtifactHref(href);
    const path = resolveArtifactPath(rawPath, workingDir) ?? rawPath;
    artifacts.push({
      kind: 'file',
      title: basenameFromPath(path),
      path,
    });
  }
  return artifacts;
}

function toolCallOf(content: {
  toolCall: Record<string, unknown>;
}): { name: string; arguments: unknown } | null {
  const call = content.toolCall as {
    status?: string;
    value?: { name?: string; arguments?: unknown };
  };
  if (call.status !== 'success' || typeof call.value?.name !== 'string') return null;
  return { name: call.value.name, arguments: call.value.arguments };
}

function isSuccessfulToolResult(toolResult: Record<string, unknown>): boolean {
  const wrapped = toolResult as { status?: string; value?: { is_error?: boolean } };
  if (wrapped.status && wrapped.status !== 'success') return false;
  return wrapped.value?.is_error !== true;
}

// Auto Visualiser combined reports are served as `ui://dashboard/<slug>` resources
// (slug derived from the report title). Used to collapse a report the agent
// re-renders within one turn down to its final version.
const DASHBOARD_URI_PREFIX = 'ui://dashboard/';

export function collectArtifactsFromMessages(
  messages: Message[],
  workingDir?: string
): ArtifactSource[] {
  const artifacts: ArtifactSource[] = [];
  const seen = new Set<string>();
  const visibleToolCalls = new Map<string, { name: string; arguments: unknown }>();
  const addArtifact = (artifact: ArtifactSource | null) => {
    if (!artifact) return;
    const key = artifactKey(artifact);
    if (seen.has(key)) return;
    seen.add(key);
    artifacts.push(artifact);
  };

  for (const message of messages) {
    if (message.role !== 'assistant' || message.metadata?.userVisible === false) continue;
    for (const artifact of collectTextArtifacts(getTextContent(message), workingDir)) {
      addArtifact(artifact);
    }
    for (const content of message.content) {
      if (content.type !== 'toolRequest') continue;
      const call = toolCallOf(content);
      if (call) visibleToolCalls.set(content.id, call);
    }
  }

  // A report the agent re-renders within one turn — a double `render_dashboard`
  // call, or an in-turn refine — must surface as ONE card (its final version), not
  // a stack of superseded drafts. `addArtifact` only dedupes byte-identical content
  // (artifactKey is content-based), so a refined re-render — same `ui://dashboard/`
  // URI, different bytes — slips through as a second artifact, inflating the
  // Artifacts count and flipping the panel from the draft to the final. Collapse
  // per (turn, dashboard URI), last render wins. Scoped to the same turn so a
  // dashboard the user deliberately refines in a LATER turn stays its own entry.
  let turnIndex = 0;
  const dashboardSlotByKey = new Map<string, number>();

  for (const message of messages) {
    if (message.role === 'user' && message.metadata?.userVisible !== false) turnIndex += 1;
    for (const content of message.content) {
      if (content.type !== 'toolResponse') continue;
      const call = visibleToolCalls.get(content.id);
      if (!call) continue;

      // A `ui://` resource carries its own preview; a written file only leaves
      // its path behind, in the arguments of the call that created it.
      if (isSuccessfulToolResult(content.toolResult)) {
        for (const path of fileArtifactPathsFromToolCall(call.name, call.arguments, workingDir)) {
          addArtifact({ kind: 'file', title: basenameFromPath(path), path });
        }
      }

      for (const resultContent of getToolResultContent(content.toolResult)) {
        if (!isEmbeddedResource(resultContent)) continue;
        const artifact = artifactSourceFromResource(
          { ...resultContent, type: 'resource' as const },
          'Artifact'
        );
        if (!artifact) continue;

        const uri = (resultContent.resource as { uri?: string } | undefined)?.uri;
        if (uri?.startsWith(DASHBOARD_URI_PREFIX)) {
          const slotKey = `${turnIndex}:${uri}`;
          const slot = dashboardSlotByKey.get(slotKey);
          if (slot !== undefined) {
            // Same report, re-rendered this turn: replace the earlier draft in place.
            seen.delete(artifactKey(artifacts[slot]));
            artifacts[slot] = artifact;
            seen.add(artifactKey(artifact));
            continue;
          }
          const before = artifacts.length;
          addArtifact(artifact);
          // Only claim the slot if it actually landed (a cross-turn byte-identical
          // copy is dropped by `seen`, and must not shadow a later real render).
          if (artifacts.length > before) dashboardSlotByKey.set(slotKey, artifacts.length - 1);
          continue;
        }

        addArtifact(artifact);
      }
    }
  }

  return artifacts;
}

/**
 * Failure handling for the pre-session `createSession` submit. The composer wipes
 * its text synchronously on submit (ChatInput.performSubmit), so when the backend
 * is unreachable the awaited createSession rejects *after* the text is already
 * gone — and the bare catch used to show nothing, so the message silently
 * vanished. Restore the typed text (via a `restore-chat-input` event the composer
 * listens for) and surface a visible toast. Connection detection only picks the
 * wording; the toast + restore fire on ANY rejection, so no silent path remains.
 * Exported so it can be unit-tested without Electron.
 */
export function handleCreateSessionError(
  err: unknown,
  ctx: { textValue: string; attachments: UserAttachment[]; sessionId?: string | null }
): void {
  // Put the user's text back so it is not lost when the backend is down.
  window.dispatchEvent(
    new CustomEvent('restore-chat-input', {
      detail: {
        sessionId: ctx.sessionId ?? null,
        value: ctx.textValue,
        attachments: ctx.attachments,
      },
    })
  );
  const connection = isConnectionError(err);
  toastError({
    title: connection ? 'Backend disconnected' : 'Failed to start session',
    msg: connection
      ? 'Biorouter could not reach its backend. Your message was kept - try again in a moment.'
      : errorMessage(err),
  });
}

export function formatCompactNumber(value: number) {
  if (!Number.isFinite(value) || value <= 0) return '0';
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return value.toLocaleString();
}

// Billed (accumulated) per-conversation token selection lives in
// utils/billedTokens (imported at the top of this file) and is re-exported so
// tests can keep importing pure BaseChat helpers from one place
// (BaseChat.artifacts.test.ts precedent).
export { selectBilledTokens };

function countToolRequests(messages: Message[]) {
  return messages.reduce(
    (count, message) =>
      count + message.content.filter((content) => content.type === 'toolRequest').length,
    0
  );
}

function visitStrings(
  value: unknown,
  visitor: (text: string) => void,
  seen = new WeakSet<object>()
) {
  if (typeof value === 'string') {
    visitor(value);
    return;
  }
  if (!value || typeof value !== 'object') return;
  if (seen.has(value)) return;
  seen.add(value);
  if (Array.isArray(value)) {
    value.forEach((item) => visitStrings(item, visitor, seen));
    return;
  }
  Object.values(value as Record<string, unknown>).forEach((item) =>
    visitStrings(item, visitor, seen)
  );
}

function countPatchLines(text: string) {
  let added = 0;
  let removed = 0;
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('+++') || line.startsWith('---')) continue;
    if (line.startsWith('+')) added += 1;
    if (line.startsWith('-')) removed += 1;
  }
  return { added, removed };
}

function collectCodeDelta(messages: Message[]) {
  let added = 0;
  let removed = 0;
  const seenMatches = new Set<string>();
  const compactDiffRe = /\+([0-9][\d,]*)\s+[-−]([0-9][\d,]*)/g;
  const gitDiffRe = /([0-9][\d,]*)\s+insertions?\(\+\)(?:,\s*([0-9][\d,]*)\s+deletions?\(-\))?/gi;
  const codeFenceRe = /```[^\n]*\n([\s\S]*?)```/g;

  for (const message of messages) {
    visitStrings(message.content, (text) => {
      if (!text.trim()) return;

      for (const match of text.matchAll(compactDiffRe)) {
        const key = `compact:${match[0]}`;
        if (seenMatches.has(key)) continue;
        seenMatches.add(key);
        added += Number(match[1].replace(/,/g, '')) || 0;
        removed += Number(match[2].replace(/,/g, '')) || 0;
      }

      for (const match of text.matchAll(gitDiffRe)) {
        const key = `git:${match[0]}`;
        if (seenMatches.has(key)) continue;
        seenMatches.add(key);
        added += Number(match[1].replace(/,/g, '')) || 0;
        removed += Number(match[2]?.replace(/,/g, '') ?? 0) || 0;
      }

      if (text.includes('*** Begin Patch') || text.includes('diff --git')) {
        const delta = countPatchLines(text);
        added += delta.added;
        removed += delta.removed;
      }

      for (const match of text.matchAll(codeFenceRe)) {
        const key = `fence:${match[0].slice(0, 120)}:${match[0].length}`;
        if (seenMatches.has(key)) continue;
        seenMatches.add(key);
        added += match[1].split(/\r?\n/).filter((line) => line.trim()).length;
      }
    });
  }

  return { added, removed };
}

/**
 * A metric readout, per design.md §4.13: a 30/34 mono-light value over an
 * 11px caps label. It used to be a filled `rounded-md bg-background-medium/60`
 * tile with a 14px semibold sans value — four boxes nested inside an already
 * rounded popover, which is the "box inside a box" this pass exists to remove.
 * The fill goes; the number does the work. `children` lets a caller compose a
 * richer value (e.g. the +/- code diff) without duplicating this component.
 */
function SummaryMetric({
  label,
  value,
  children,
}: {
  label: string;
  value?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="min-w-0 py-2">
      <div className="text-[11px] font-medium uppercase tracking-[0.08em] text-text-muted">
        {label}
      </div>
      <div className="mt-0.5 truncate font-mono text-[30px] font-light leading-[34px] tracking-[-0.02em] text-text-default tabular-nums">
        {children ?? value}
      </div>
    </div>
  );
}

interface BaseChatProps {
  setChat: (chat: ChatType) => void;
  onMessageSubmit?: (message: string) => void;
  renderHeader?: () => React.ReactNode;
  customChatInputProps?: Record<string, unknown>;
  customMainLayoutProps?: Record<string, unknown>;
  contentClassName?: string;
  disableSearch?: boolean;
  showPopularTopics?: boolean;
  suppressEmptyState: boolean;
  sessionId: string;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
  /** Render messages + input as a single coherent surface (default true). */
  coherent?: boolean;
  /** Optional: overrides the default rename behavior (which calls biorouterd updateSessionName). */
  onRenameSession?: (newName: string) => void;
  /** Notify parent when the underlying session object changes (e.g., biorouterd renamed it). */
  onSessionUpdate?: (session: { id: string; name: string; userSetName: boolean } | null) => void;
  /** Optional accent dot color (dashboard windows pass theirs). */
  accentColor?: string;
  /** Hide the SessionNamePill at the top of the chat. Dashboard windows pass this
   * because their own WindowTitleBar already shows the editable name. */
  hideSessionNamePill?: boolean;
  /** Fires when the inner chat transitions between idle and any non-idle state
   * (streaming, thinking, tool-running, etc.). Used by DashboardContext to
   * drive the per-window busy indicator on folded cards. */
  onBusyChange?: (busy: boolean) => void;
  /** Fires whenever the last assistant message text changes (including
   * mid-stream). Receives a tail-truncated string for hover previews on
   * dashboard folded cards. Null when there's no assistant message yet. */
  onLatestMessage?: (text: string | null) => void;
  /** Monotonically increments when the parent wants the chat input refocused
   * and the conversation scrolled back to the bottom. Used by dashboard
   * ChatWindow on unfold, because BaseChat stays mounted while folded so
   * mount-time autofocus / auto-scroll never re-fire on visibility change. */
  focusTrigger?: number;
  /** Whether this chat may resize the OS window to fit its artifact panel
   * (default true). A BaseChat is a session-scoped component, but
   * ensureArtifactPanelFits reaches for an app-scoped effect: with N chats
   * mounted, a BACKGROUND chat opening an artifact would resize the whole
   * Electron window out from under the focused one. Callers that mount more
   * than one BaseChat pass false for every chat that isn't focused. */
  allowWindowResize?: boolean;
  /**
   * Whether this chat may render its artifact preview panel (default true).
   *
   * "The preview panel follows the ACTIVE group" (spec §4). With N groups
   * mounted, every background group would otherwise render its own panel and the
   * window would fill with previews of chats nobody is looking at. Callers that
   * mount more than one BaseChat pass true for the focused chat only.
   *
   * This GATES THE RENDER, it does not clear the state: `presentedArtifact` and
   * the panel's tab stack live on untouched while the group is in the
   * background, so switching back restores the panel exactly as it was. That is
   * the whole reason the panel stays per-pane instead of being hoisted to a
   * window-level surface keyed by session — a hoist drops the artifact tab stack
   * on every group switch, and that regression is not acceptable.
   */
  artifactPanelEnabled?: boolean;
  /**
   * Renders the left-hand content of the existing 52px session header in place
   * of the SessionNamePill. The chat tab strip comes through HERE rather than
   * being mounted above BaseChat, and that is deliberate:
   *
   * renderSessionHeaderActions() (below, in this same header row) closes over
   * isTerminalDockOpen / reviewOpen / session — BaseChat-local state — and
   * cannot be hoisted cheaply. Threading the strip through the seam means the
   * actions never have to move. Mount a strip ABOVE BaseChat instead and you
   * get a strip row AND an actions row: two 52px bars.
   *
   * The header is WebkitAppRegion:'drag'; anything interactive rendered here
   * must declare no-drag on itself (R1, measured 2026-07-16 — the gesture does
   * reach the DOM, but only for no-drag children).
   */
  renderSessionTitle?: () => React.ReactNode;
}

function BaseChatContent({
  setChat,
  renderHeader,
  customChatInputProps = {},
  customMainLayoutProps = {},
  sessionId,
  initialMessage,
  initialAttachments,
  suppressEmptyState,
  coherent = true,
  onRenameSession,
  onSessionUpdate,
  accentColor,
  hideSessionNamePill = false,
  onBusyChange,
  onLatestMessage,
  focusTrigger,
  allowWindowResize = true,
  artifactPanelEnabled = true,
  renderSessionTitle,
}: BaseChatProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const scrollRef = useRef<ScrollAreaHandle>(null);
  const { extensionsList, getProviders } = useConfig();
  // Per-session vision capability. The global ModelAndProviderContext tracks
  // the user's default model, but each chat session (especially in dashboard
  // mode) can be bound to a different provider/model. Look up vision support
  // against the session's own provider/model so attach gating reflects what
  // the session will actually use.
  const [sessionSupportsVision, setSessionSupportsVision] = React.useState<boolean | null>(null);
  const [sessionSupportedInputMimeTypes, setSessionSupportedInputMimeTypes] = React.useState<
    string[] | null | undefined
  >(undefined);

  const disableAnimation = location.state?.disableAnimation || false;
  const [hasStartedUsingWorkflow, setHasStartedUsingWorkflow] = React.useState(false);
  const [hasNotAcceptedWorkflow, setHasNotAcceptedWorkflow] = useState<boolean>();
  const [hasWorkflowSecurityWarnings, setHasWorkflowSecurityWarnings] = useState(false);
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const [presentedArtifact, setPresentedArtifact] = useState<ArtifactSource | null>(null);
  const [isArtifactPanelOpen, setIsArtifactPanelOpen] = useState(false);
  const [isArtifactPanelResizing, setIsArtifactPanelResizing] = useState(false);
  const [artifactPanelWidth, setArtifactPanelWidth] = useState<number | null>(null);
  // Rung 2 of the yield ladder. Derived from the PANE's measured width through
  // previewPanelMode — never stored as a raw width, so the state only changes on
  // the 720px crossing and a splitter drag does not re-render the whole chat
  // once per frame.
  const [previewMode, setPreviewMode] = useState<PreviewPanelMode>('side');
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [showEditWorkflowModal, setShowEditWorkflowModal] = useState(false);

  // ---- Terminal dock seam -------------------------------------------------
  // With /pair's shell mounted there is ONE dock for every chat group, and this
  // chat drives it through the context. Everywhere else (Dashboard, /extensions,
  // the Hub) there is no provider, useTerminalDock() returns null, and BaseChat
  // keeps its own local dock exactly as before — same state, same render site,
  // byte-identical behaviour.
  const terminalDock = useTerminalDock();
  const [localTerminalDockOpen, setLocalTerminalDockOpen] = useState(false);
  const isTerminalDockOpen = terminalDock ? terminalDock.isOpen : localTerminalDockOpen;
  // sessionWorkingDir is computed far below (it needs `session`), but the setter
  // must be stable and defined here. A ref, assigned at the definition site, is
  // the seam: the dock only ever reads a cwd at the instant it opens.
  const sessionWorkingDirRef = useRef<string | undefined>(undefined);
  const setIsTerminalDockOpen = useCallback(
    (next: boolean) => {
      if (terminalDock) terminalDock.setOpen(next, sessionWorkingDirRef.current);
      else setLocalTerminalDockOpen(next);
    },
    [terminalDock]
  );
  const splitPaneRef = useRef<HTMLDivElement>(null);
  const artifactPanelCloseTimerRef = useRef<number | null>(null);
  const artifactPanelOpenFrameRef = useRef<number | null>(null);
  const artifactPanelResizeFrameRef = useRef<number | null>(null);
  const pendingArtifactPanelWidthRef = useRef<number | null>(null);
  const artifactPanelResizeCleanupRef = useRef<(() => void) | null>(null);
  const artifactPanelWidthUserSetRef = useRef(false);
  const reportedArtifactRenderErrorsRef = useRef<Set<string>>(new Set());
  const pendingArtifactRenderFeedbackRef = useRef<Message | null>(null);
  // Wall-clock of the last moment the agent was actively working. Used to decide
  // whether an artifact render failure belongs to the current exchange (auto-fix
  // it) or surfaced long after the conversation went quiet (leave it alone).
  const lastAgentActiveAtRef = useRef(0);
  const knownArtifactKeysRef = useRef<Set<string>>(new Set());
  const artifactInitialScanDoneRef = useRef(false);
  const composerMotionRef = useRef<HTMLDivElement>(null);
  const pendingComposerRectRef = useRef<ReturnType<HTMLDivElement['getBoundingClientRect']> | null>(
    null
  );

  const isMobile = useIsMobile();
  const { state: sidebarState } = useSidebar();
  const [isSidebarCompact, setIsSidebarCompact] = useState(() => {
    return typeof window !== 'undefined' && window.innerWidth < SIDEBAR_COMPACT_TITLE_WIDTH;
  });
  const isMacOS = (window?.electron?.platform || 'darwin') === 'darwin';
  const { diverge } = useDiverge();
  const isCompactSidebarOverlayOpen = isSidebarCompact && !isMobile && sidebarState !== 'collapsed';
  const reserveTitlebarControls =
    isMacOS && (isMobile || isSidebarCompact || sidebarState === 'collapsed');
  const sessionPillPaddingLeft = getSessionTitlePadding(
    isCompactSidebarOverlayOpen,
    reserveTitlebarControls
  );
  const setView = useNavigation();

  useEffect(() => {
    const updateSidebarCompact = () => {
      setIsSidebarCompact(window.innerWidth < SIDEBAR_COMPACT_TITLE_WIDTH);
    };

    updateSidebarCompact();
    window.addEventListener('resize', updateSidebarCompact);
    return () => window.removeEventListener('resize', updateSidebarCompact);
  }, []);

  const contentClassName = cn(
    // `px-1`, not `pr-1`: this padding exists to keep the transcript off the
    // scrollbar, but as a right-only inset it made the scroll viewport 4px
    // narrower on ONE side, so the `mx-auto` readable column centred 4px left of
    // true centre. Padding the gutter symmetrically costs 4px of width and buys
    // an actually centred column. Measured in Electron: 198/202 -> 200/200.
    'px-1 pb-10',
    (isMobile || isSidebarCompact || sidebarState === 'collapsed') && 'pt-11'
  );

  // Use shared file drop
  const { droppedFiles, setDroppedFiles, handleDrop, handleDragOver } = useFileDrop();

  const onStreamFinish = useCallback(() => {}, []);

  const [isCreateWorkflowModalOpen, setIsCreateWorkflowModalOpen] = useState(false);
  const hasAutoSubmittedRef = useRef(false);

  // Reset auto-submit flag when session changes
  useEffect(() => {
    artifactPanelResizeCleanupRef.current?.();
    hasAutoSubmittedRef.current = false;
    setPresentedArtifact(null);
    setIsArtifactPanelOpen(false);
    setIsArtifactPanelResizing(false);
    setArtifactPanelWidth(null);
    artifactPanelWidthUserSetRef.current = false;
    setDiagnosticsOpen(false);
    setReviewOpen(false);
    // Only the LOCAL dock. The global dock is shared by every group, so one
    // group changing session must not close the terminal another group is using.
    setLocalTerminalDockOpen(false);
    setShowEditWorkflowModal(false);
    knownArtifactKeysRef.current.clear();
    artifactInitialScanDoneRef.current = false;
  }, [sessionId]);

  useEffect(() => {
    return () => {
      if (artifactPanelCloseTimerRef.current) {
        window.clearTimeout(artifactPanelCloseTimerRef.current);
      }
      if (artifactPanelOpenFrameRef.current) {
        window.cancelAnimationFrame(artifactPanelOpenFrameRef.current);
      }
      if (artifactPanelResizeFrameRef.current) {
        window.cancelAnimationFrame(artifactPanelResizeFrameRef.current);
      }
      artifactPanelResizeCleanupRef.current?.();
    };
  }, []);

  const ensureArtifactPanelFits = useCallback(async () => {
    const targetWidth = artifactPanelTargetContentWidth({
      isMobile,
      allowWindowResize,
      windowWidth: window.innerWidth,
      splitPaneWidth: splitPaneRef.current?.clientWidth ?? window.innerWidth,
    });
    if (!targetWidth || !window.electron.ensureWindowContentWidth) return;

    await window.electron.ensureWindowContentWidth(targetWidth).catch(() => undefined);
  }, [isMobile, allowWindowResize]);

  /**
   * Rung 2: ask the PANE, not the window, whether the preview panel can have a
   * column.
   *
   * The shipped rule was `isMobile` — window < 930 — which is right for one group
   * and blind the moment there are two: in a split the pane is decoupled from the
   * window, so a 2-up split in a 1000px window left the panel's 360px floor
   * sitting next to a 140px transcript. isMobile survives inside previewPanelMode
   * as an override, so nothing between 720 and 930 changes.
   */
  const measurePreviewMode = useCallback(() => {
    const paneWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    // Same value → React bails out. This runs from a ResizeObserver, so it must
    // be free at rest.
    setPreviewMode(previewPanelMode({ isMobile, paneWidth }));
  }, [isMobile]);

  const handleOpenArtifact = useCallback(
    async (artifact: ArtifactSource) => {
      if (artifactPanelCloseTimerRef.current) {
        window.clearTimeout(artifactPanelCloseTimerRef.current);
        artifactPanelCloseTimerRef.current = null;
      }
      if (artifactPanelOpenFrameRef.current) {
        window.cancelAnimationFrame(artifactPanelOpenFrameRef.current);
        artifactPanelOpenFrameRef.current = null;
      }

      if (!presentedArtifact) {
        artifactPanelWidthUserSetRef.current = false;
        await ensureArtifactPanelFits();
      }

      // Prime the rung BEFORE the panel exists: the observer below only starts
      // once presentedArtifact is set, so without this the first frame would
      // paint the panel in whichever mode the last artifact left behind. Measured
      // after ensureArtifactPanelFits, which may have grown the OS window.
      measurePreviewMode();
      setPresentedArtifact(artifact);

      if (presentedArtifact) {
        setIsArtifactPanelOpen(true);
        return;
      }

      setIsArtifactPanelOpen(false);
      artifactPanelOpenFrameRef.current = window.requestAnimationFrame(() => {
        artifactPanelOpenFrameRef.current = null;
        setIsArtifactPanelOpen(true);
      });
    },
    [ensureArtifactPanelFits, measurePreviewMode, presentedArtifact]
  );

  const handleCloseArtifactPanel = useCallback(() => {
    artifactPanelResizeCleanupRef.current?.();
    setIsArtifactPanelResizing(false);
    setIsArtifactPanelOpen(false);

    if (artifactPanelCloseTimerRef.current) {
      window.clearTimeout(artifactPanelCloseTimerRef.current);
    }

    artifactPanelCloseTimerRef.current = window.setTimeout(() => {
      artifactPanelCloseTimerRef.current = null;
      setPresentedArtifact(null);
      setIsArtifactPanelResizing(false);
    }, ARTIFACT_PANEL_EXIT_MS);
  }, []);

  const getMaxArtifactPanelWidth = useCallback(() => {
    const containerWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    return Math.max(
      ARTIFACT_PANEL_MIN_WIDTH,
      Math.min(ARTIFACT_PANEL_MAX_WIDTH, containerWidth - ARTIFACT_PANEL_MIN_CHAT_WIDTH)
    );
  }, []);

  const getInitialArtifactPanelWidth = useCallback(() => {
    const containerWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    return getDefaultArtifactPanelWidth(containerWidth);
  }, []);

  useEffect(() => {
    if (!presentedArtifact || isMobile || artifactPanelWidth !== null) return;
    setArtifactPanelWidth(getInitialArtifactPanelWidth());
  }, [artifactPanelWidth, getInitialArtifactPanelWidth, isMobile, presentedArtifact]);

  useEffect(() => {
    const splitPane = splitPaneRef.current;
    // Gated on `presentedArtifact` only. `isMobile` used to gate this too, but
    // rung 2 has to keep watching the PANE at every window width — a split can
    // starve the transcript in a window the mobile rule calls roomy. The width
    // bookkeeping below stays a no-op in overlay mode, where the panel takes no
    // width from anything.
    if (!splitPane || !presentedArtifact) return;

    const sampleSplitPane = () => {
      measurePreviewMode();
      if (isMobile) return;
      setArtifactPanelWidth((currentWidth) => {
        const maxWidth = getMaxArtifactPanelWidth();
        if (currentWidth === null || !artifactPanelWidthUserSetRef.current) {
          return getInitialArtifactPanelWidth();
        }
        const nextWidth = clampArtifactPanelWidth(currentWidth, maxWidth);
        return nextWidth === currentWidth ? currentWidth : nextWidth;
      });
    };

    sampleSplitPane();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', sampleSplitPane);
      return () => window.removeEventListener('resize', sampleSplitPane);
    }

    // No feedback loop: this observes the split pane, which is sized by the
    // GROUP above it, and rung 2 only ever moves the panel between a column and
    // an overlay INSIDE that pane. Neither mode can change the observed box, so
    // the callback cannot retrigger itself — no hysteresis needed, and none
    // added on spec.
    const resizeObserver = new ResizeObserver(sampleSplitPane);
    resizeObserver.observe(splitPane);
    return () => resizeObserver.disconnect();
  }, [
    getInitialArtifactPanelWidth,
    getMaxArtifactPanelWidth,
    isMobile,
    measurePreviewMode,
    presentedArtifact,
  ]);

  const handleArtifactPanelResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      // A yielded panel has no column to resize. The handle is already unwired at
      // the render site; this is the second lock on the same door.
      if (previewMode === 'overlay') return;

      event.preventDefault();
      event.stopPropagation();

      artifactPanelResizeCleanupRef.current?.();

      const startX = event.clientX;
      const startWidth = artifactPanelWidth ?? getInitialArtifactPanelWidth();
      const previousCursor = document.body.style.cursor;
      const previousUserSelect = document.body.style.userSelect;
      const resizeHandle = event.currentTarget;
      const pointerId = event.pointerId;
      let finished = false;

      try {
        resizeHandle.setPointerCapture(pointerId);
      } catch {
        // Global listeners below still keep the resize usable when capture is unavailable.
      }

      setIsArtifactPanelResizing(true);
      artifactPanelWidthUserSetRef.current = true;
      pendingArtifactPanelWidthRef.current = null;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';

      const applyPendingWidth = () => {
        artifactPanelResizeFrameRef.current = null;
        const nextWidth = pendingArtifactPanelWidthRef.current;
        pendingArtifactPanelWidthRef.current = null;
        if (nextWidth !== null) setArtifactPanelWidth(nextWidth);
      };

      const scheduleWidth = (nextWidth: number) => {
        pendingArtifactPanelWidthRef.current = nextWidth;
        if (artifactPanelResizeFrameRef.current !== null) return;
        artifactPanelResizeFrameRef.current = window.requestAnimationFrame(applyPendingWidth);
      };

      const handleMove = (moveEvent: globalThis.PointerEvent) => {
        if (moveEvent.pointerId !== pointerId) return;
        const nextWidth = startWidth - (moveEvent.clientX - startX);
        scheduleWidth(clampArtifactPanelWidth(nextWidth, getMaxArtifactPanelWidth()));
      };

      const finishResize = (commitPendingWidth: boolean, updateState: boolean) => {
        if (finished) return;
        finished = true;
        if (artifactPanelResizeFrameRef.current !== null) {
          window.cancelAnimationFrame(artifactPanelResizeFrameRef.current);
          artifactPanelResizeFrameRef.current = null;
        }
        if (commitPendingWidth && pendingArtifactPanelWidthRef.current !== null) {
          setArtifactPanelWidth(pendingArtifactPanelWidthRef.current);
        }
        pendingArtifactPanelWidthRef.current = null;
        if (updateState) setIsArtifactPanelResizing(false);
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        window.removeEventListener('pointermove', handleMove);
        window.removeEventListener('pointerup', handleEnd);
        window.removeEventListener('pointercancel', handleEnd);
        window.removeEventListener('blur', handleWindowBlur);
        resizeHandle.removeEventListener('lostpointercapture', handleLostPointerCapture);
        try {
          if (resizeHandle.hasPointerCapture(pointerId))
            resizeHandle.releasePointerCapture(pointerId);
        } catch {
          // The element may have left the document while the pointer was outside the window.
        }
        artifactPanelResizeCleanupRef.current = null;
      };

      const handleEnd = (endEvent: globalThis.PointerEvent) => {
        if (endEvent.pointerId !== pointerId) return;
        finishResize(true, true);
      };

      const handleWindowBlur = () => finishResize(true, true);
      const handleLostPointerCapture = (lostEvent: globalThis.PointerEvent) => {
        if (lostEvent.pointerId === pointerId) finishResize(true, true);
      };

      artifactPanelResizeCleanupRef.current = () => finishResize(false, false);

      window.addEventListener('pointermove', handleMove);
      window.addEventListener('pointerup', handleEnd);
      window.addEventListener('pointercancel', handleEnd);
      window.addEventListener('blur', handleWindowBlur);
      resizeHandle.addEventListener('lostpointercapture', handleLostPointerCapture);
    },
    [artifactPanelWidth, getInitialArtifactPanelWidth, getMaxArtifactPanelWidth, previewMode]
  );

  const {
    session,
    messages,
    chatState,
    setChatState,
    handleSubmit,
    submitSystemMessage,
    submitElicitationResponse,
    stopStreaming,
    steer,
    sessionLoadError,
    turnError,
    setWorkflowUserParams,
    tokenState,
    agentReady,
    notifications: toolCallNotifications,
    onMessageUpdate,
  } = useChatStream({
    sessionId,
    onStreamFinish,
  });

  const canDivergeSession = useMemo(
    () => messages.some((message) => message.role === 'assistant'),
    [messages]
  );
  const handleTitleDiverge = useCallback(async () => {
    if (!canDivergeSession) return;
    await diverge(sessionId);
  }, [canDivergeSession, diverge, sessionId]);

  const submitArtifactRepairMessage = useCallback(
    (message: Message): boolean => {
      if (!shouldAutoRepairArtifact(chatState, lastAgentActiveAtRef.current, Date.now())) {
        // Conversation is over — do not auto-resume it to repair a failure the
        // user almost certainly caused by editing or deleting the artifact.
        return false;
      }

      if (chatState === ChatState.Idle) {
        // A turn just wrapped up (grace window): fold the fix in now.
        pendingArtifactRenderFeedbackRef.current = null;
        void submitSystemMessage(message);
      } else {
        // A turn is still running: queue the fix so it lands when the turn ends
        // instead of interrupting it mid-flight.
        pendingArtifactRenderFeedbackRef.current = message;
      }
      return true;
    },
    [chatState, submitSystemMessage]
  );

  const handleArtifactRenderError = useCallback(
    (error: ArtifactRenderError) => {
      const key = `${error.artifactTitle}\n${error.message}\n${error.detail ?? ''}`;
      if (reportedArtifactRenderErrorsRef.current.has(key)) return;

      // Only mark the failure as handled if we actually acted on it. Otherwise a
      // failure seen while the chat is dormant would block the agent from fixing
      // the same artifact if it re-renders during a later, active turn.
      if (submitArtifactRepairMessage(createArtifactRenderRepairMessage(error))) {
        reportedArtifactRenderErrorsRef.current.add(key);
      }
    },
    [submitArtifactRepairMessage]
  );

  // Stamp the last-active time whenever the agent is working, so the grace window
  // above measures from the end of real activity (not from session load).
  useEffect(() => {
    if (chatState !== ChatState.Idle && chatState !== ChatState.LoadingConversation) {
      lastAgentActiveAtRef.current = Date.now();
    }
  }, [chatState]);

  useEffect(() => {
    if (chatState !== ChatState.Idle || !pendingArtifactRenderFeedbackRef.current) return;
    const message = pendingArtifactRenderFeedbackRef.current;
    pendingArtifactRenderFeedbackRef.current = null;
    void submitSystemMessage(message);
  }, [chatState, submitSystemMessage]);

  const stageComposerMotion = useCallback(() => {
    const rect = composerMotionRef.current?.getBoundingClientRect();
    if (rect) {
      pendingComposerRectRef.current = rect;
    }
  }, []);

  // Pipe chatState transitions to the parent (dashboard window). Busy = any
  // non-idle state (Thinking, Streaming, WaitingForUserInput, Compacting, etc.).
  // ChatState.LoadingConversation counts as busy too — the session is still
  // resolving and the user should see that as activity.
  useEffect(() => {
    if (!onBusyChange) return;
    onBusyChange(chatState !== ChatState.Idle);
  }, [chatState, onBusyChange]);

  // Propagate a tail of the most recent assistant message to the parent so
  // dashboard folded cards can show a hover preview. Updates on every message
  // mutation — including mid-stream — so the user can watch the AI work
  // through the popup without expanding the card.
  useEffect(() => {
    if (!onLatestMessage) return;
    const lastAssistant = [...messages].reverse().find((m) => m.role === 'assistant');
    if (!lastAssistant) {
      onLatestMessage(null);
      return;
    }
    const text = getTextContent(lastAssistant).trim();
    if (!text) {
      onLatestMessage(null);
      return;
    }
    // Keep the tail short — the popup is ~6 lines of small text.
    const TAIL = 220;
    const tail = text.length > TAIL ? '…' + text.slice(-TAIL) : text;
    onLatestMessage(tail);
  }, [messages, onLatestMessage]);

  // Generate command history from user messages (most recent first)
  const commandHistory = useMemo(() => {
    return messages
      .reduce<string[]>((history, message) => {
        if (message.role === 'user' && message.metadata?.userVisible !== false) {
          const text = getTextContent(message).trim();
          if (text) {
            history.push(text);
          }
        }
        return history;
      }, [])
      .reverse();
  }, [messages]);

  useEffect(() => {
    if (!session || hasAutoSubmittedRef.current) {
      return;
    }

    const shouldStartAgent = searchParams.get('shouldStartAgent') === 'true';

    if (initialMessage) {
      hasAutoSubmittedRef.current = true;
      handleSubmit(initialMessage, initialAttachments);
      // Clear initialMessage + attachments from navigation state to prevent re-sending on refresh
      navigate(location.pathname + location.search, {
        replace: true,
        state: { ...location.state, initialMessage: undefined, initialAttachments: undefined },
      });
    } else if (shouldStartAgent) {
      hasAutoSubmittedRef.current = true;
      handleSubmit('');
    }
  }, [session, initialMessage, initialAttachments, searchParams, handleSubmit, navigate, location]);

  // Resolve session-scoped vision capability whenever the session's bound
  // provider or model changes. Falls back to null (== use global context) if
  // either piece isn't loaded yet.
  React.useEffect(() => {
    const sessionProvider = session?.provider_name;
    const sessionModel = session?.model_config?.model_name;
    if (!sessionProvider || !sessionModel) {
      setSessionSupportsVision(null);
      setSessionSupportedInputMimeTypes(undefined);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const metadata = await getProviderMetadata(sessionProvider, getProviders);
        if (!cancelled) {
          setSessionSupportsVision(modelSupportsVision(metadata, sessionModel));
          setSessionSupportedInputMimeTypes(modelSupportedInputMimeTypes(metadata, sessionModel));
        }
      } catch {
        if (!cancelled) {
          setSessionSupportsVision(false);
          setSessionSupportedInputMimeTypes(null);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [session?.provider_name, session?.model_config?.model_name, getProviders]);

  const handleFormSubmit = async (e: React.FormEvent) => {
    const customEvent = e as unknown as CustomEvent;
    const textValue = customEvent.detail?.value || '';
    const attachments = customEvent.detail?.attachments ?? [];
    if (textValue.trim() || (Array.isArray(attachments) && attachments.length > 0)) {
      stageComposerMotion();
    }

    // If no session exists, create one and navigate with the initial message
    const hasAttachments = Array.isArray(attachments) && attachments.length > 0;
    if (!session && !sessionId && (textValue.trim() || hasAttachments) && !isCreatingSession) {
      setIsCreatingSession(true);
      try {
        const newSession = await createSession(getInitialWorkingDir(), {
          allExtensions: extensionsList,
        });
        navigateWithViewTransition(
          navigate,
          `/pair?resumeSessionId=${newSession.id}`,
          {
            resumeSessionId: newSession.id,
            initialMessage: textValue,
            initialAttachments: attachments,
          },
          { replace: true }
        );
      } catch (err) {
        setIsCreatingSession(false);
        handleCreateSessionError(err, { textValue, attachments, sessionId });
      }
      return;
    }

    if (workflow && textValue.trim()) {
      setHasStartedUsingWorkflow(true);
    }
    handleSubmit(textValue, attachments);
  };

  const { sessionCosts, modelRows } = useCostTracking({ session });

  const workflow = session?.workflow;

  useEffect(() => {
    if (!workflow) return;

    // Only prompt to trust a workflow when it is about to run for the FIRST
    // time — not when merely opening/browsing a scheduled-job's past
    // conversation. Scheduled-job sessions always carry a `workflow` and, once
    // they have run, have a non-zero message_count; re-popping the click-eating
    // "Trust and Execute" modal over a completed transcript is what made viewing
    // that history feel like the whole app had frozen. A resumed conversation
    // has already executed, so treat it as accepted for viewing.
    if ((session?.message_count ?? 0) > 0) {
      setHasNotAcceptedWorkflow(false);
      return;
    }

    (async () => {
      const accepted = await window.electron.hasAcceptedWorkflowBefore(workflow);
      setHasNotAcceptedWorkflow(!accepted);

      if (!accepted) {
        const scanResult = await scanWorkflow(workflow);
        setHasWorkflowSecurityWarnings(scanResult.has_security_warnings);
      }
    })();
  }, [workflow, session?.message_count]);

  const handleWorkflowAccept = async (accept: boolean) => {
    if (workflow && accept) {
      await window.electron.recordWorkflowHash(workflow);
      setHasNotAcceptedWorkflow(false);
    } else {
      setView('chat');
    }
  };

  // Track if this is the initial render for session resuming
  const initialRenderRef = useRef(true);

  // Auto-scroll when messages are loaded (for session resuming)
  const handleRenderingComplete = React.useCallback(() => {
    // Only force scroll on the very first render
    if (initialRenderRef.current && messages.length > 0) {
      initialRenderRef.current = false;
      if (scrollRef.current?.scrollToBottom) {
        // Land instantly on resume — a smooth scroll across a full transcript is
        // a multi-second animation that makes the session feel slow to open.
        scrollRef.current.scrollToBottom('auto');
      }
    } else if (scrollRef.current?.isFollowing) {
      if (scrollRef.current?.scrollToBottom) {
        scrollRef.current.scrollToBottom();
      }
    }
  }, [messages.length]);

  // Gated on agentReady: tools do not exist until the extensions do, and this
  // hook has no other reason to refetch.
  const toolCount = useToolCount(sessionId, agentReady);
  const sessionWorkingDir = session?.working_dir || getInitialWorkingDir();
  // Feed the terminal-dock seam declared at the top of this component. Assigned
  // on every render; only ever READ at the instant the dock opens.
  sessionWorkingDirRef.current = sessionWorkingDir;
  // The working dir anchors relative paths a tool call names (`results/plot.png`).
  const sessionArtifacts = useMemo(
    () => collectArtifactsFromMessages(messages, sessionWorkingDir),
    [messages, sessionWorkingDir]
  );
  const sessionToolCallCount = useMemo(() => countToolRequests(messages), [messages]);
  const codeDelta = useMemo(() => collectCodeDelta(messages), [messages]);
  // The per-model ledger can supersede live counters only when every row has a
  // certified billed total; incomplete historical rows stay visibly unknown.
  const totalSessionTokens = mostCompleteBilledTokens(
    selectBilledTokens(tokenState, session),
    modelRows
  );

  useEffect(() => {
    if (!session) return;
    if (!artifactInitialScanDoneRef.current) {
      if ((session.message_count ?? 0) > 0 && messages.length === 0) return;
      knownArtifactKeysRef.current = new Set(sessionArtifacts.map(artifactKey));
      artifactInitialScanDoneRef.current = true;
      return;
    }

    const newArtifacts = sessionArtifacts.filter((artifact) => {
      const key = artifactKey(artifact);
      if (knownArtifactKeysRef.current.has(key)) return false;
      knownArtifactKeysRef.current.add(key);
      return true;
    });

    const latestArtifact = newArtifacts[newArtifacts.length - 1];
    if (latestArtifact) {
      handleOpenArtifact(latestArtifact);
    }
  }, [handleOpenArtifact, messages.length, session, sessionArtifacts]);

  // Listen for scroll-to-bottom requests (e.g. from MCP UI prompt actions).
  // Dispatched by MCPUIResourceRenderer / McpAppRenderer, both of which render
  // INSIDE a chat — so match by sessionId, or an artifact in chat A scrolls
  // chat B.
  useEffect(() => {
    const handleGlobalScrollRequest = createScrollToBottomHandler({
      sessionId,
      scrollToBottom: () => scrollRef.current?.scrollToBottom?.(),
    });

    window.addEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
    return () => window.removeEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
  }, [sessionId]);

  // When the parent bumps focusTrigger (e.g. dashboard card unfolded), wait
  // one animation frame for the display:none→display:flex transition to
  // settle, then scroll the chat to bottom and ask ChatInput to refocus.
  // BaseChat stays mounted while folded, so its mount-time autofocus and
  // auto-scroll-to-bottom never re-fire — without this hook the input would
  // stay unfocused and the conversation would stay at its pre-fold scroll
  // position (often leaving the input row visually cut off).
  useEffect(() => {
    if (!focusTrigger) return;
    const raf = requestAnimationFrame(() => {
      scrollRef.current?.scrollToBottom?.();
      window.dispatchEvent(new CustomEvent('focus-chat-input', { detail: { sessionId } }));
    });
    return () => cancelAnimationFrame(raf);
  }, [focusTrigger, sessionId]);

  // NOTE: as of this writing 'make-agent-from-chat' has NO dispatcher anywhere in
  // ui/desktop (only this listener and the one in hooks/useWorkflowManager.ts:284)
  // — it appears to be dead code. It is session-scoped here for consistency with
  // the other two broadcast listeners rather than deleted, because removing it is
  // out of scope; without a dispatcher the filter is inert either way.
  useEffect(() => {
    const handleMakeAgent = (event: Event) => {
      const detail = (event as CustomEvent<{ sessionId?: string | null }>).detail;
      if (!isEventForSession(detail, sessionId)) return;
      setIsCreateWorkflowModalOpen(true);
    };

    window.addEventListener('make-agent-from-chat', handleMakeAgent);
    return () => window.removeEventListener('make-agent-from-chat', handleMakeAgent);
  }, [sessionId]);

  // Match on the ORIGIN session, not detail.newSessionId. Diverging creates a
  // brand-new session, so newSessionId belongs to no mounted BaseChat — filtering
  // on it would match nobody. detail.sessionId is the session the user actually
  // diverged FROM (ChatStreamController.sessionId, chatStreamStore.tsx), i.e. the
  // one chat that should navigate. Without this every mounted BaseChat fires its
  // own navigate() to the same URL: N racing navigations.
  useEffect(() => {
    const handleSessionDiverged = createSessionDivergedHandler({ sessionId, navigate });

    window.addEventListener('session-diverged', handleSessionDiverged);

    return () => {
      window.removeEventListener('session-diverged', handleSessionDiverged);
    };
  }, [location.pathname, navigate, sessionId]);

  const handleWorkflowCreated = (workflow: Workflow) => {
    toastSuccess({
      title: 'Workflow created successfully!',
      msg: `"${workflow.title}" has been saved and is ready to use.`,
    });
  };

  const chat: ChatType = {
    messages,
    workflow,
    sessionId,
    name: session?.name || 'No Session',
  };

  // Update the global chat context when session name changes
  const lastSetNameRef = useRef<string>('');

  useEffect(() => {
    const currentSessionName = session?.name;
    if (currentSessionName && currentSessionName !== lastSetNameRef.current) {
      lastSetNameRef.current = currentSessionName;
      setChat({
        messages,
        workflow,
        sessionId,
        name: currentSessionName,
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.name, setChat]);

  // Keep the latest onSessionUpdate in a ref so changes to its identity (e.g., a
  // new arrow on every render) don't refire this effect. The effect must fire
  // only when the session id/name actually changes — otherwise we'd loop through
  // setState in parent, re-render this child, get a new callback identity, fire
  // again, ad infinitum.
  const onSessionUpdateRef = useRef(onSessionUpdate);
  useEffect(() => {
    onSessionUpdateRef.current = onSessionUpdate;
  }, [onSessionUpdate]);
  const sessionUpdateId = session?.id;
  const sessionUpdateName = session?.name;
  const sessionUpdateUserSetName = session?.user_set_name;
  useEffect(() => {
    if (!sessionUpdateId || sessionUpdateName === undefined) return;
    onSessionUpdateRef.current?.({
      id: sessionUpdateId,
      name: sessionUpdateName,
      userSetName: sessionUpdateUserSetName ?? false,
    });
  }, [sessionUpdateId, sessionUpdateName, sessionUpdateUserSetName]);

  const handleRename = async (newName: string) => {
    if (onRenameSession) {
      onRenameSession(newName);
      return;
    }
    if (!sessionId) return;
    // Optimistic announce so the pill, the chat-context display, history, and
    // any other open window snap to the new name immediately. `renameSession`
    // will re-announce on API success (idempotent — no-op if name matches).
    const previous = session;
    announceSessionName({
      sessionId,
      name: newName,
      userSetName: true,
      origin: 'user',
    });
    try {
      await renameSession(sessionId, newName, 'user');
    } catch (err) {
      // Roll back to whatever the session held before the click. Using
      // `sync` as the origin so listeners treat it as authoritative.
      if (previous?.name) {
        announceSessionName({
          sessionId,
          name: previous.name,
          userSetName: previous.user_set_name ?? false,
          origin: 'sync',
        });
      }
      toastError({
        title: 'Failed to rename session',
        msg: errorMessage(err),
      });
    }
  };

  const handleOpenTerminal = () => {
    setIsTerminalDockOpen(!isTerminalDockOpen);
  };

  const handleWorkflowReviewAction = () => {
    setReviewOpen(false);
    if (workflow) {
      setShowEditWorkflowModal(true);
    } else {
      setIsCreateWorkflowModalOpen(true);
    }
  };

  const handleDiagnosticsReviewAction = () => {
    setReviewOpen(false);
    setDiagnosticsOpen(true);
  };

  // Only use initialMessage for the prompt if it hasn't been submitted yet
  // If we have a workflow prompt and user workflow values, substitute parameters
  let workflowPrompt = '';
  if (messages.length === 0 && workflow?.prompt) {
    workflowPrompt = session?.user_workflow_values
      ? substituteParameters(workflow.prompt, session.user_workflow_values)
      : workflow.prompt;
  }

  const initialPrompt = workflowPrompt;
  const isCleanConversation =
    !suppressEmptyState &&
    messages.length === 0 &&
    !workflow &&
    !initialPrompt &&
    chatState === ChatState.Idle &&
    !isCreatingSession;

  const renderSessionHeaderActions = () => (
    <div
      className="ml-auto flex flex-shrink-0 items-center gap-1"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            onClick={handleOpenTerminal}
            variant="ghost"
            size="sm"
            shape="round"
            className={cn(
              HEADER_ACTION_BUTTON_CLASS,
              isTerminalDockOpen && 'bg-background-medium text-text-default'
            )}
            aria-label={isTerminalDockOpen ? 'Close in-app terminal' : 'Open in-app terminal'}
          >
            <Terminal className="h-4 w-4" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{isTerminalDockOpen ? 'Close terminal' : 'Open terminal'}</TooltipContent>
      </Tooltip>

      <Popover open={reviewOpen} onOpenChange={setReviewOpen}>
        <Tooltip>
          <TooltipTrigger asChild>
            <PopoverTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                shape="round"
                className={HEADER_ACTION_BUTTON_CLASS}
                aria-label="Review session summary"
              >
                <ScrollText className="h-4 w-4" />
              </Button>
            </PopoverTrigger>
          </TooltipTrigger>
          <TooltipContent>Review session summary</TooltipContent>
        </Tooltip>
        <PopoverContent side="bottom" align="end" className="w-96 p-3">
          <div className="space-y-3">
            <div>
              <div className="text-sm font-medium text-text-default">Session review</div>
              <div className="text-xs text-text-muted">
                {session?.name || 'Current conversation'}
              </div>
            </div>
            <div className="grid grid-cols-2 gap-x-4">
              <SummaryMetric label="Tool calls" value={sessionToolCallCount.toLocaleString()} />
              <SummaryMetric
                label="Billed tokens"
                value={
                  totalSessionTokens === null ? 'N/A' : formatCompactNumber(totalSessionTokens)
                }
              />
              <SummaryMetric label="Artifacts" value={sessionArtifacts.length.toLocaleString()} />
              {/* Composed rather than copy-pasted: this used to duplicate
                  SummaryMetric's markup, so the two diverged on every edit. */}
              <SummaryMetric label="Code">
                <span className="text-text-success">+{codeDelta.added.toLocaleString()}</span>{' '}
                <span className="text-text-danger">-{codeDelta.removed.toLocaleString()}</span>
              </SummaryMetric>
            </div>
            {/* `secondary`, not `outline`: a 1px box drawn around the quietest
                actions was the heaviest line in the panel. design.md §4.1
                already specifies a fill here — outline is only for a secondary
                action on an already-tinted ground. */}
            <div className="flex gap-2 border-t border-border-subtle pt-3">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="min-w-0 flex-1 justify-center gap-1.5"
                onClick={handleWorkflowReviewAction}
              >
                <Pipeline className="h-3.5 w-3.5 shrink-0" />
                <span className="whitespace-nowrap">{workflow ? 'Workflow' : 'Make workflow'}</span>
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                className="min-w-0 flex-1 justify-center gap-1.5"
                onClick={handleDiagnosticsReviewAction}
              >
                <CodeAnalysis className="h-3.5 w-3.5 shrink-0" />
                <span className="whitespace-nowrap">Diagnostics</span>
              </Button>
            </div>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );

  useLayoutEffect(() => {
    if (isCleanConversation) return;

    const from = pendingComposerRectRef.current;
    const element = composerMotionRef.current;
    pendingComposerRectRef.current = null;

    const isReducedMotion =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    if (!from || !element || isReducedMotion) {
      return;
    }

    const to = element.getBoundingClientRect();
    const deltaX = from.left - to.left;
    const deltaY = from.top - to.top;
    const scaleX = to.width > 0 ? from.width / to.width : 1;

    if (Math.abs(deltaX) < 1 && Math.abs(deltaY) < 1 && Math.abs(scaleX - 1) < 0.01) {
      return;
    }

    const animation = element.animate(
      [
        {
          opacity: 0.96,
          transform: `translate3d(${deltaX}px, ${deltaY}px, 0) scaleX(${scaleX})`,
        },
        { opacity: 1, transform: 'translate3d(0, 0, 0) scaleX(1)' },
      ],
      {
        duration: 420,
        easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
      }
    );

    return () => animation.cancel();
  }, [isCleanConversation]);

  const renderChatInput = () => (
    <div
      ref={composerMotionRef}
      data-composer-shell="true"
      className={cn(
        'w-full max-w-[760px] mx-auto biorouter-chat-composer biorouter-composer-motion',
        'biorouter-composer-view-transition'
      )}
    >
      <ChatInput
        sessionId={sessionId}
        handleSubmit={handleFormSubmit}
        chatState={chatState}
        setChatState={setChatState}
        onStop={stopStreaming}
        onSteer={steer}
        commandHistory={commandHistory}
        initialValue={initialPrompt}
        setView={setView}
        totalTokens={tokenState?.totalTokens ?? session?.total_tokens ?? undefined}
        accumulatedInputTokens={
          tokenState?.accumulatedInputTokens ?? session?.accumulated_input_tokens ?? undefined
        }
        accumulatedOutputTokens={
          tokenState?.accumulatedOutputTokens ?? session?.accumulated_output_tokens ?? undefined
        }
        droppedFiles={droppedFiles}
        onFilesProcessed={() => setDroppedFiles([])} // Clear dropped files after processing
        messages={messages}
        disableAnimation={disableAnimation}
        sessionCosts={sessionCosts}
        modelCostRows={modelRows}
        workflow={workflow}
        workflowAccepted={!hasNotAcceptedWorkflow}
        initialPrompt={initialPrompt}
        toolCount={toolCount || 0}
        supportsVisionOverride={sessionSupportsVision ?? undefined}
        supportedInputMimeTypesOverride={sessionSupportedInputMimeTypes}
        {...customChatInputProps}
      />
    </div>
  );

  const renderWorkingStatus = () => {
    if (chatState === ChatState.Idle) return null;

    return (
      <div className="w-full max-w-[760px] mx-auto mb-2.5 pl-2 pointer-events-none">
        <LoadingBioRouter
          chatState={chatState}
          message={
            messages.length > 0 ? getThinkingMessage(messages[messages.length - 1]) : undefined
          }
        />
      </div>
    );
  };

  if (sessionLoadError) {
    return (
      <div className="h-full flex flex-col min-h-0">
        <MainPanelLayout
          backgroundColor={'bg-background-muted'}
          removeTopPadding={true}
          {...customMainLayoutProps}
        >
          {renderHeader && renderHeader()}
          <div className="flex flex-col flex-1 mb-0.5 min-h-0 relative">
            <div className="flex-1 bg-background-default rounded-b-2xl flex items-center justify-center">
              <div className="flex flex-col items-center justify-center p-8">
                <div className="text-text-danger bg-background-danger/10 border border-border-danger/40 p-4 rounded-lg mb-4 max-w-md">
                  <h3 className="font-semibold mb-2">Failed to Load Session</h3>
                  <p className="text-sm">{sessionLoadError}</p>
                </div>
                <button
                  onClick={() => {
                    setView('chat');
                  }}
                  className="px-4 py-2 text-center cursor-pointer text-text-default border border-border-subtle hover:bg-background-medium rounded-lg transition-all duration-150"
                >
                  Go home
                </button>
              </div>
            </div>
          </div>
        </MainPanelLayout>
      </div>
    );
  }

  return (
    // `w-full min-w-0 flex-1` is load-bearing, not decoration. BaseChat used to
    // mount only inside AppLayout's flex COLUMN, where align-items:stretch gave
    // it the full width for free. The chat-groups shell mounts it inside a flex
    // ROW (ChatGroupPane), where width is the MAIN axis: without flex-grow the
    // root defaulted to `flex: 0 1 auto` and hugged its content — 808px (the
    // 760px readable column + 48px of px-6) — then sat at the group's left edge.
    // The transcript's `mx-auto` centred inside THAT 808px box, not the group,
    // so the column read ~134px left-of-centre in a 942px group. min-w-0 is what
    // lets it shrink when the artifact panel takes its share of the row.
    <div className="relative z-[60] h-full flex flex-col min-h-0 w-full min-w-0 flex-1">
      <MainPanelLayout
        backgroundColor={'bg-background-muted'}
        removeTopPadding={true}
        {...customMainLayoutProps}
      >
        {/* Custom header */}
        {renderHeader && renderHeader()}

        <div ref={splitPaneRef} className="relative flex flex-1 min-h-0 min-w-0">
          <div className="flex min-w-0 flex-1 flex-col">
            {/* Chat container with sticky workflow header */}
            <div
              className={
                coherent
                  ? 'flex flex-col flex-1 min-h-0 relative rounded-t-2xl overflow-hidden bg-background-muted'
                  : 'flex flex-col flex-1 mx-4 mt-4 mb-3 min-h-0 relative rounded-2xl overflow-hidden'
              }
            >
              {!hideSessionNamePill && (
                <div
                  // Opaque, not frosted. The artifact panel's header sits flush
                  // beside this one; a translucent, blurred fill made the two
                  // bottom hairlines read at different weights so they never
                  // visually aligned (D-18).
                  className="relative z-[var(--z-sticky)] flex h-[52px] flex-shrink-0 items-center gap-3 border-b border-border-subtle bg-background-muted pr-4"
                  style={
                    {
                      WebkitAppRegion: 'drag',
                      // The tab strip owns its own left reserve (it is the only
                      // consumer of getSessionTitlePadding once it renders), so
                      // the header must not apply the reserve twice.
                      paddingLeft: renderSessionTitle ? 0 : sessionPillPaddingLeft,
                    } as React.CSSProperties
                  }
                >
                  {/* The strip renders HERE, in place of the pill. Do not move
                      renderSessionHeaderActions() out of this row — it closes
                      over BaseChat-local state, and hoisting the strip above
                      BaseChat instead would produce two 52px bars. */}
                  {renderSessionTitle ? (
                    renderSessionTitle()
                  ) : (
                    <div className="min-w-0 flex-1">
                      <SessionNamePill
                        name={session?.name || 'New Session'}
                        onRename={handleRename}
                        onDiverge={handleTitleDiverge}
                        canDiverge={canDivergeSession}
                        accentColor={accentColor}
                        className="w-fit max-w-[min(520px,calc(100%-16px))]"
                      />
                    </div>
                  )}
                  {renderSessionHeaderActions()}
                </div>
              )}
              {isCleanConversation ? (
                <div
                  className="biorouter-clean-conversation flex-1 min-h-0 flex items-center justify-center overflow-y-auto px-4 py-10 sm:px-6 sm:py-16"
                  onDrop={handleDrop}
                  onDragOver={handleDragOver}
                  data-drop-zone="true"
                >
                  <div className="biorouter-clean-conversation-content w-full max-w-[760px] flex flex-col items-center gap-6 -translate-y-10 sm:-translate-y-12">
                    <Greeting
                      key={sessionId}
                      className={cn(
                        'text-center text-2xl font-semibold tracking-tight text-text-default'
                      )}
                    />
                    {renderChatInput()}
                  </div>
                </div>
              ) : (
                <ScrollArea
                  ref={scrollRef}
                  className={
                    coherent
                      ? `flex-1 min-h-0 relative ${contentClassName}`
                      : `flex-1 bg-background-default rounded-2xl min-h-0 relative ${contentClassName}`
                  }
                  autoScroll
                  onDrop={handleDrop}
                  onDragOver={handleDragOver}
                  data-drop-zone="true"
                  paddingX={6}
                  paddingY={0}
                >
                  <div className="biorouter-chat-column mx-auto w-full max-w-[760px]">
                    {workflow?.title && (
                      <div className="sticky top-0 z-10 bg-background-muted mb-4 pt-2">
                        <WorkflowHeader title={workflow.title} />
                      </div>
                    )}

                    {workflow && (
                      <div className={hasStartedUsingWorkflow ? 'mb-6' : ''}>
                        <WorkflowActivities
                          append={(text: string) => handleSubmit(text)}
                          activities={
                            Array.isArray(workflow.activities) ? workflow.activities : null
                          }
                          title={workflow.title}
                          parameterValues={session?.user_workflow_values || {}}
                        />
                      </div>
                    )}

                    {messages.length > 0 || workflow || turnError ? (
                      <>
                        <SearchView>
                          <>
                            <ProgressiveMessageList
                              messages={messages}
                              chat={{ sessionId }}
                              toolCallNotifications={toolCallNotifications}
                              append={(text: string) => handleSubmit(text)}
                              isUserMessage={(m: Message) => m.role === 'user'}
                              isStreamingMessage={chatState !== ChatState.Idle}
                              onRenderingComplete={handleRenderingComplete}
                              onMessageUpdate={onMessageUpdate}
                              submitElicitationResponse={submitElicitationResponse}
                              onOpenArtifact={handleOpenArtifact}
                              workingDir={sessionWorkingDir}
                            />
                            {turnError && !hasVisibleTurnErrorMessage(turnError, messages) && (
                              <ChatTurnError error={turnError} />
                            )}
                          </>
                        </SearchView>

                        <div className="block h-8" />
                      </>
                    ) : null}
                  </div>
                </ScrollArea>
              )}
            </div>

            {!isCleanConversation && (
              <div
                className={
                  coherent
                    ? 'biorouter-chat-composer-bar flex-shrink-0 px-4 sm:px-6 pb-6 pt-2 bg-background-muted'
                    : `px-4 sm:px-6 pb-6 pt-2 flex-shrink-0 ${disableAnimation ? '' : 'animate-[appear_200ms_var(--ease-out)_forwards]'}`
                }
              >
                {renderWorkingStatus()}
                {renderChatInput()}
              </div>
            )}
          </div>

          {presentedArtifact && artifactPanelEnabled && (
            <ArtifactViewer
              artifact={presentedArtifact}
              isOpen={isArtifactPanelOpen}
              isResizing={isArtifactPanelResizing}
              onClose={handleCloseArtifactPanel}
              onOpenArtifact={handleOpenArtifact}
              // Rung 2. 'overlay' means the panel has YIELDED ITS COLUMN: it is
              // absolutely positioned inside the split pane, so it takes no width
              // from the transcript at all and there is no edge to drag. The
              // pane's full width stays the chat's, and closing the panel reveals
              // it untouched. This is the treatment a mobile-width window has
              // always got; rung 2 only changes WHEN it applies — a pane that
              // cannot seat a 360px chat beside a 360px panel, at any window size.
              onResizeStart={
                previewMode === 'overlay' ? undefined : handleArtifactPanelResizeStart
              }
              onRenderError={handleArtifactRenderError}
              style={
                previewMode === 'overlay'
                  ? undefined
                  : {
                      width: artifactPanelWidth ?? getInitialArtifactPanelWidth(),
                      flexBasis: artifactPanelWidth ?? getInitialArtifactPanelWidth(),
                    }
              }
              className={
                previewMode === 'overlay'
                  ? 'absolute inset-x-2 bottom-2 top-16 z-[70] rounded-lg border border-border-subtle'
                  : 'min-w-[360px] flex-shrink-0'
              }
            />
          )}
        </div>
        {/* The global dock renders ONCE in the shell, below all the groups. When
            it exists this chat must not render a second one — N groups would
            mean N docks stacked inside N panes, which is the opposite of "the
            panel stays global, spans all groups". */}
        {!terminalDock && (
          <InAppTerminalDock
            open={isTerminalDockOpen}
            workingDir={sessionWorkingDir}
            onClose={() => setIsTerminalDockOpen(false)}
          />
        )}
      </MainPanelLayout>

      {workflow && (
        <WorkflowWarningModal
          isOpen={!!hasNotAcceptedWorkflow}
          onConfirm={() => handleWorkflowAccept(true)}
          onCancel={() => handleWorkflowAccept(false)}
          workflowDetails={{
            title: workflow.title,
            description: workflow.description,
            instructions: workflow.instructions || undefined,
          }}
          hasSecurityWarnings={hasWorkflowSecurityWarnings}
        />
      )}

      {workflow?.parameters && workflow.parameters.length > 0 && !session?.user_workflow_values && (
        <ParameterInputModal
          parameters={workflow.parameters}
          onSubmit={setWorkflowUserParams}
          onClose={() => setView('chat')}
          initialValues={
            (window.appConfig?.get('workflowParameters') as Record<string, string> | undefined) ||
            undefined
          }
        />
      )}

      <CreateWorkflowFromSessionModal
        isOpen={isCreateWorkflowModalOpen}
        onClose={() => setIsCreateWorkflowModalOpen(false)}
        sessionId={chat.sessionId}
        onWorkflowCreated={handleWorkflowCreated}
      />

      {sessionId && diagnosticsOpen && (
        <DiagnosticsModal
          isOpen={diagnosticsOpen}
          onClose={() => setDiagnosticsOpen(false)}
          sessionId={sessionId}
        />
      )}

      {workflow && showEditWorkflowModal && (
        <CreateEditWorkflowModal
          isOpen={showEditWorkflowModal}
          onClose={() => setShowEditWorkflowModal(false)}
          workflow={workflow}
        />
      )}
    </div>
  );
}

export default function BaseChat(props: BaseChatProps) {
  return <BaseChatContent {...props} />;
}
