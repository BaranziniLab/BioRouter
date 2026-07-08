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
import { SessionNamePill } from './Dashboard/SessionNamePill';
import { announceSessionName, renameSession } from '../utils/sessionNameSync';
import { toastError } from '../toasts';
import { errorMessage } from '../utils/conversionUtils';
import { Greeting } from './common/Greeting';
import { navigateWithViewTransition } from '../utils/navigationUtils';
import ArtifactViewer from './artifacts/ArtifactViewer';
import InAppTerminalDock from './InAppTerminalDock';
import type { ArtifactRenderError } from './artifacts/ArtifactViewer';
import type { ArtifactSource } from './artifacts/artifactTypes';
import {
  artifactSourceFromResource,
  basenameFromPath,
  looksLikePreviewableFile,
  pathFromArtifactHref,
} from './artifacts/artifactUtils';
import type { CallToolResponse, Content, EmbeddedResource, ResourceContents } from '../api';

// Context for sharing current model info
const CurrentModelContext = createContext<{ model: string; mode: string } | null>(null);
export const useCurrentModelInfo = () => useContext(CurrentModelContext);

const ARTIFACT_PANEL_MIN_WIDTH = 360;
const ARTIFACT_PANEL_MAX_WIDTH = 860;
const ARTIFACT_PANEL_MIN_CHAT_WIDTH = 640;
const ARTIFACT_PANEL_AUTO_TUCK_WIDTH =
  ARTIFACT_PANEL_MIN_WIDTH + ARTIFACT_PANEL_MIN_CHAT_WIDTH + 48;
const ARTIFACT_PANEL_AUTO_EXPAND_PADDING = 24;
const ARTIFACT_PANEL_EXIT_MS = 180;
const SIDEBAR_COMPACT_TITLE_WIDTH = 1120;
const HEADER_ACTION_BUTTON_CLASS =
  'no-drag flex h-8 w-8 items-center justify-center rounded-md p-0 text-text-default/70 transition-colors hover:bg-background-medium hover:text-text-default';
const PREVIEWABLE_TEXT_ARTIFACT_RE =
  /(file:\/\/[^\s)\]]+|(?:~|\.{1,2}|\/)[^\s)\]]+\.(?:html?|png|jpe?g|gif|webp|svg|sql|md|txt|json|csv|ts|tsx|js|jsx|py|r)(?:[?#][^\s)\]]*)?)/gi;

function clampArtifactPanelWidth(value: number, max: number) {
  return Math.min(Math.max(value, ARTIFACT_PANEL_MIN_WIDTH), max);
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

function collectTextArtifacts(text: string): ArtifactSource[] {
  const artifacts: ArtifactSource[] = [];
  for (const match of text.matchAll(PREVIEWABLE_TEXT_ARTIFACT_RE)) {
    const href = match[0];
    if (!looksLikePreviewableFile(href)) continue;
    const path = pathFromArtifactHref(href);
    artifacts.push({
      kind: 'file',
      title: basenameFromPath(path),
      path,
    });
  }
  return artifacts;
}

export function collectArtifactsFromMessages(messages: Message[]): ArtifactSource[] {
  const artifacts: ArtifactSource[] = [];
  const seen = new Set<string>();
  const visibleToolRequestIds = new Set<string>();
  const addArtifact = (artifact: ArtifactSource | null) => {
    if (!artifact) return;
    const key = artifactKey(artifact);
    if (seen.has(key)) return;
    seen.add(key);
    artifacts.push(artifact);
  };

  for (const message of messages) {
    if (message.role !== 'assistant' || message.metadata?.userVisible === false) continue;
    for (const artifact of collectTextArtifacts(getTextContent(message))) {
      addArtifact(artifact);
    }
    for (const content of message.content) {
      if (content.type === 'toolRequest') {
        visibleToolRequestIds.add(content.id);
      }
    }
  }

  for (const message of messages) {
    for (const content of message.content) {
      if (content.type !== 'toolResponse' || !visibleToolRequestIds.has(content.id)) continue;
      for (const resultContent of getToolResultContent(content.toolResult)) {
        if (!isEmbeddedResource(resultContent)) continue;
        addArtifact(
          artifactSourceFromResource({ ...resultContent, type: 'resource' as const }, 'Artifact')
        );
      }
    }
  }

  return artifacts;
}

function formatCompactNumber(value: number) {
  if (!Number.isFinite(value) || value <= 0) return '0';
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}k`;
  return value.toLocaleString();
}

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

function SummaryMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md bg-background-medium/60 px-2.5 py-2">
      <div className="text-[11px] uppercase tracking-wide text-text-muted">{label}</div>
      <div className="mt-1 truncate text-sm font-medium text-text-default">{value}</div>
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
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [isTerminalDockOpen, setIsTerminalDockOpen] = useState(false);
  const [showEditWorkflowModal, setShowEditWorkflowModal] = useState(false);
  const splitPaneRef = useRef<HTMLDivElement>(null);
  const artifactPanelCloseTimerRef = useRef<number | null>(null);
  const artifactPanelOpenFrameRef = useRef<number | null>(null);
  const artifactPanelResizeFrameRef = useRef<number | null>(null);
  const artifactPanelEnsureFrameRef = useRef<number | null>(null);
  const pendingArtifactPanelWidthRef = useRef<number | null>(null);
  const artifactPanelWidthUserSetRef = useRef(false);
  const reportedArtifactRenderErrorsRef = useRef<Set<string>>(new Set());
  const pendingArtifactRenderFeedbackRef = useRef<Message | null>(null);
  const knownArtifactKeysRef = useRef<Set<string>>(new Set());
  const artifactInitialScanDoneRef = useRef(false);
  const composerMotionRef = useRef<HTMLDivElement>(null);
  const pendingComposerRectRef = useRef<DOMRect | null>(null);

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
  const sessionPillWrapperCls = isCompactSidebarOverlayOpen
    ? 'pl-[224px]'
    : reserveTitlebarControls
      ? `${isMacOS ? 'pl-[184px]' : 'pl-[104px]'}`
      : 'pl-4';
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
    'pr-1 pb-10',
    (isMobile || isSidebarCompact || sidebarState === 'collapsed') && 'pt-11'
  );

  // Use shared file drop
  const { droppedFiles, setDroppedFiles, handleDrop, handleDragOver } = useFileDrop();

  const onStreamFinish = useCallback(() => {}, []);

  const [isCreateWorkflowModalOpen, setIsCreateWorkflowModalOpen] = useState(false);
  const hasAutoSubmittedRef = useRef(false);

  // Reset auto-submit flag when session changes
  useEffect(() => {
    hasAutoSubmittedRef.current = false;
    setPresentedArtifact(null);
    setIsArtifactPanelOpen(false);
    setIsArtifactPanelResizing(false);
    setArtifactPanelWidth(null);
    artifactPanelWidthUserSetRef.current = false;
    setDiagnosticsOpen(false);
    setReviewOpen(false);
    setIsTerminalDockOpen(false);
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
      if (artifactPanelEnsureFrameRef.current) {
        window.cancelAnimationFrame(artifactPanelEnsureFrameRef.current);
      }
    };
  }, []);

  const ensureArtifactPanelFits = useCallback(async () => {
    if (isMobile) return;

    const splitPaneWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    const targetWidth = getArtifactPanelExpansionContentWidth(window.innerWidth, splitPaneWidth);
    if (!targetWidth || !window.electron.ensureWindowContentWidth) return;

    await window.electron.ensureWindowContentWidth(targetWidth).catch(() => undefined);
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

      artifactPanelWidthUserSetRef.current = false;

      await ensureArtifactPanelFits();

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
    [ensureArtifactPanelFits, presentedArtifact]
  );

  const handleCloseArtifactPanel = useCallback(() => {
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

  useEffect(() => {
    const splitPane = splitPaneRef.current;
    if (!splitPane) return;

    const updateArtifactFitState = () => {
      if (!presentedArtifact || isMobile) return;
      if (artifactPanelEnsureFrameRef.current !== null) return;
      artifactPanelEnsureFrameRef.current = window.requestAnimationFrame(() => {
        artifactPanelEnsureFrameRef.current = null;
        void ensureArtifactPanelFits();
      });
    };

    updateArtifactFitState();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateArtifactFitState);
      return () => window.removeEventListener('resize', updateArtifactFitState);
    }

    const resizeObserver = new ResizeObserver(updateArtifactFitState);
    resizeObserver.observe(splitPane);
    return () => resizeObserver.disconnect();
  }, [ensureArtifactPanelFits, isMobile, presentedArtifact]);

  const getMaxArtifactPanelWidth = useCallback(() => {
    const containerWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    return Math.max(
      ARTIFACT_PANEL_MIN_WIDTH,
      Math.min(ARTIFACT_PANEL_MAX_WIDTH, containerWidth - ARTIFACT_PANEL_MIN_CHAT_WIDTH)
    );
  }, []);

  const getDefaultArtifactPanelWidth = useCallback(() => {
    const containerWidth = splitPaneRef.current?.clientWidth ?? window.innerWidth;
    return clampArtifactPanelWidth(Math.round(containerWidth * 0.38), getMaxArtifactPanelWidth());
  }, [getMaxArtifactPanelWidth]);

  useEffect(() => {
    if (!presentedArtifact || isMobile || artifactPanelWidth !== null) return;
    setArtifactPanelWidth(getDefaultArtifactPanelWidth());
  }, [artifactPanelWidth, getDefaultArtifactPanelWidth, isMobile, presentedArtifact]);

  useEffect(() => {
    const splitPane = splitPaneRef.current;
    if (!splitPane || !presentedArtifact || isMobile) return;

    const updateArtifactPanelWidth = () => {
      setArtifactPanelWidth((currentWidth) => {
        const maxWidth = getMaxArtifactPanelWidth();
        if (currentWidth === null || !artifactPanelWidthUserSetRef.current) {
          return getDefaultArtifactPanelWidth();
        }
        const nextWidth = clampArtifactPanelWidth(currentWidth, maxWidth);
        return nextWidth === currentWidth ? currentWidth : nextWidth;
      });
    };

    updateArtifactPanelWidth();

    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateArtifactPanelWidth);
      return () => window.removeEventListener('resize', updateArtifactPanelWidth);
    }

    const resizeObserver = new ResizeObserver(updateArtifactPanelWidth);
    resizeObserver.observe(splitPane);
    return () => resizeObserver.disconnect();
  }, [getDefaultArtifactPanelWidth, getMaxArtifactPanelWidth, isMobile, presentedArtifact]);

  const handleArtifactPanelResizeStart = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (isMobile) return;

      event.preventDefault();
      event.stopPropagation();

      const startX = event.clientX;
      const startWidth = artifactPanelWidth ?? getDefaultArtifactPanelWidth();
      const previousCursor = document.body.style.cursor;
      const previousUserSelect = document.body.style.userSelect;

      setIsArtifactPanelResizing(true);
      artifactPanelWidthUserSetRef.current = true;
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';

      const applyPendingWidth = () => {
        artifactPanelResizeFrameRef.current = null;
        if (pendingArtifactPanelWidthRef.current === null) return;
        setArtifactPanelWidth(pendingArtifactPanelWidthRef.current);
      };

      const scheduleWidth = (nextWidth: number) => {
        pendingArtifactPanelWidthRef.current = nextWidth;
        if (artifactPanelResizeFrameRef.current !== null) return;
        artifactPanelResizeFrameRef.current = window.requestAnimationFrame(applyPendingWidth);
      };

      const handleMove = (moveEvent: PointerEvent) => {
        const nextWidth = startWidth - (moveEvent.clientX - startX);
        scheduleWidth(clampArtifactPanelWidth(nextWidth, getMaxArtifactPanelWidth()));
      };

      const handleEnd = () => {
        if (artifactPanelResizeFrameRef.current !== null) {
          window.cancelAnimationFrame(artifactPanelResizeFrameRef.current);
          artifactPanelResizeFrameRef.current = null;
        }
        if (pendingArtifactPanelWidthRef.current !== null) {
          setArtifactPanelWidth(pendingArtifactPanelWidthRef.current);
          pendingArtifactPanelWidthRef.current = null;
        }
        setIsArtifactPanelResizing(false);
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        window.removeEventListener('pointermove', handleMove);
        window.removeEventListener('pointerup', handleEnd);
        window.removeEventListener('pointercancel', handleEnd);
      };

      window.addEventListener('pointermove', handleMove);
      window.addEventListener('pointerup', handleEnd);
      window.addEventListener('pointercancel', handleEnd);
    },
    [artifactPanelWidth, getDefaultArtifactPanelWidth, getMaxArtifactPanelWidth, isMobile]
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
    sessionLoadError,
    setWorkflowUserParams,
    tokenState,
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
  const handleTitleDiverge = useCallback(() => {
    if (!canDivergeSession) return;
    void diverge(sessionId);
  }, [canDivergeSession, diverge, sessionId]);

  const submitArtifactRepairMessage = useCallback(
    (message: Message) => {
      if (chatState !== ChatState.Idle) {
        pendingArtifactRenderFeedbackRef.current = message;
        return;
      }

      pendingArtifactRenderFeedbackRef.current = null;
      void submitSystemMessage(message);
    },
    [chatState, submitSystemMessage]
  );

  const handleArtifactRenderError = useCallback(
    (error: ArtifactRenderError) => {
      const key = `${error.artifactTitle}\n${error.message}\n${error.detail ?? ''}`;
      if (reportedArtifactRenderErrorsRef.current.has(key)) return;
      reportedArtifactRenderErrorsRef.current.add(key);

      submitArtifactRepairMessage(createArtifactRenderRepairMessage(error));
    },
    [submitArtifactRepairMessage]
  );

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
  }, [session, initialMessage, searchParams, handleSubmit, navigate, location]);

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
      } catch {
        setIsCreatingSession(false);
      }
      return;
    }

    if (workflow && textValue.trim()) {
      setHasStartedUsingWorkflow(true);
    }
    handleSubmit(textValue, attachments);
  };

  const { sessionCosts } = useCostTracking({
    sessionInputTokens: session?.accumulated_input_tokens || 0,
    sessionOutputTokens: session?.accumulated_output_tokens || 0,
    localInputTokens: 0,
    localOutputTokens: 0,
    session,
  });

  const workflow = session?.workflow;

  useEffect(() => {
    if (!workflow) return;

    (async () => {
      const accepted = await window.electron.hasAcceptedWorkflowBefore(workflow);
      setHasNotAcceptedWorkflow(!accepted);

      if (!accepted) {
        const scanResult = await scanWorkflow(workflow);
        setHasWorkflowSecurityWarnings(scanResult.has_security_warnings);
      }
    })();
  }, [workflow]);

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
        scrollRef.current.scrollToBottom();
      }
    } else if (scrollRef.current?.isFollowing) {
      if (scrollRef.current?.scrollToBottom) {
        scrollRef.current.scrollToBottom();
      }
    }
  }, [messages.length]);

  const toolCount = useToolCount(sessionId);
  const sessionArtifacts = useMemo(() => collectArtifactsFromMessages(messages), [messages]);
  const sessionToolCallCount = useMemo(() => countToolRequests(messages), [messages]);
  const codeDelta = useMemo(() => collectCodeDelta(messages), [messages]);
  const totalSessionTokens =
    tokenState?.totalTokens ??
    session?.total_tokens ??
    session?.accumulated_total_tokens ??
    ((session?.accumulated_input_tokens ?? 0) + (session?.accumulated_output_tokens ?? 0) || null);
  const sessionWorkingDir = session?.working_dir || getInitialWorkingDir();

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

  // Listen for global scroll-to-bottom requests (e.g., from MCP UI prompt actions)
  useEffect(() => {
    const handleGlobalScrollRequest = () => {
      // Add a small delay to ensure content has been rendered
      setTimeout(() => {
        if (scrollRef.current?.scrollToBottom) {
          scrollRef.current.scrollToBottom();
        }
      }, 200);
    };

    window.addEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
    return () => window.removeEventListener('scroll-chat-to-bottom', handleGlobalScrollRequest);
  }, []);

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

  useEffect(() => {
    const handleMakeAgent = () => {
      setIsCreateWorkflowModalOpen(true);
    };

    window.addEventListener('make-agent-from-chat', handleMakeAgent);
    return () => window.removeEventListener('make-agent-from-chat', handleMakeAgent);
  }, []);

  useEffect(() => {
    const handleSessionForked = (event: Event) => {
      const customEvent = event as CustomEvent<{
        newSessionId: string;
        shouldStartAgent?: boolean;
        editedMessage?: string;
      }>;
      const { newSessionId, shouldStartAgent, editedMessage } = customEvent.detail;

      const params = new URLSearchParams();
      params.set('resumeSessionId', newSessionId);
      if (shouldStartAgent) {
        params.set('shouldStartAgent', 'true');
      }

      navigate(`/pair?${params.toString()}`, {
        state: {
          disableAnimation: true,
          initialMessage: editedMessage,
        },
      });
    };

    window.addEventListener('session-forked', handleSessionForked);

    return () => {
      window.removeEventListener('session-forked', handleSessionForked);
    };
  }, [location.pathname, navigate]);

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
  useEffect(() => {
    if (!session) return;
    onSessionUpdateRef.current?.({
      id: session.id,
      name: session.name,
      userSetName: session.user_set_name ?? false,
    });
  }, [session?.id, session?.name, session?.user_set_name]);

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
    setIsTerminalDockOpen((open) => !open);
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
            title={isTerminalDockOpen ? 'Close in-app terminal' : 'Open in-app terminal'}
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
                title="Review session summary"
              >
                <ScrollText className="h-4 w-4" />
              </Button>
            </PopoverTrigger>
          </TooltipTrigger>
          <TooltipContent>Review session summary</TooltipContent>
        </Tooltip>
        <PopoverContent side="bottom" align="end" className="w-80 p-3">
          <div className="space-y-3">
            <div>
              <div className="text-sm font-medium text-text-default">Session review</div>
              <div className="text-xs text-text-muted">
                {session?.name || 'Current conversation'}
              </div>
            </div>
            <div className="grid grid-cols-2 gap-2">
              <SummaryMetric label="Tool calls" value={sessionToolCallCount.toLocaleString()} />
              <SummaryMetric label="Tokens" value={formatCompactNumber(totalSessionTokens ?? 0)} />
              <SummaryMetric label="Artifacts" value={sessionArtifacts.length.toLocaleString()} />
              <div className="rounded-md bg-background-medium/60 px-2.5 py-2">
                <div className="text-[11px] uppercase tracking-wide text-text-muted">Code</div>
                <div className="mt-1 flex items-center gap-2 text-sm font-medium">
                  <span className="text-text-success">+{codeDelta.added.toLocaleString()}</span>
                  <span className="text-text-danger">-{codeDelta.removed.toLocaleString()}</span>
                </div>
              </div>
            </div>
            <div className="flex gap-2 border-t border-border-subtle pt-3">
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="min-w-0 flex-1 justify-center gap-1.5"
                onClick={handleWorkflowReviewAction}
              >
                <Pipeline className="h-3.5 w-3.5" />
                <span className="truncate">{workflow ? 'Workflow' : 'Make workflow'}</span>
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="min-w-0 flex-1 justify-center gap-1.5"
                onClick={handleDiagnosticsReviewAction}
              >
                <CodeAnalysis className="h-3.5 w-3.5" />
                <span className="truncate">Diagnostics</span>
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
    <div className="relative z-[60] h-full flex flex-col min-h-0">
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
                  className={`relative z-[60] flex h-14 flex-shrink-0 items-center gap-3 border-b border-border-subtle/35 bg-background-muted/95 pr-4 backdrop-blur ${sessionPillWrapperCls}`}
                  style={{ WebkitAppRegion: 'drag' } as React.CSSProperties}
                >
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
                  {renderSessionHeaderActions()}
                </div>
              )}
              {isCleanConversation ? (
                <div
                  className="flex-1 min-h-0 flex items-center justify-center px-4 py-10 sm:px-6 sm:py-16"
                  onDrop={handleDrop}
                  onDragOver={handleDragOver}
                  data-drop-zone="true"
                >
                  <div className="w-full max-w-[760px] flex flex-col items-center gap-6 -translate-y-10 sm:-translate-y-12">
                    <Greeting
                      key={sessionId}
                      className={cn(
                        'text-center text-2xl font-semibold tracking-tight text-text-default animate-in fade-in duration-300'
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

                    {messages.length > 0 || workflow ? (
                      <>
                        <SearchView>
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
                          />
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
                    : `px-4 sm:px-6 pb-6 pt-2 flex-shrink-0 ${disableAnimation ? '' : 'animate-[fadein_400ms_ease-in_forwards]'}`
                }
              >
                {renderWorkingStatus()}
                {renderChatInput()}
              </div>
            )}
          </div>

          {presentedArtifact && (
            <ArtifactViewer
              artifact={presentedArtifact}
              isOpen={isArtifactPanelOpen}
              isResizing={isArtifactPanelResizing}
              onClose={handleCloseArtifactPanel}
              onOpenArtifact={handleOpenArtifact}
              onResizeStart={isMobile ? undefined : handleArtifactPanelResizeStart}
              onRenderError={handleArtifactRenderError}
              style={
                isMobile
                  ? undefined
                  : {
                      width: artifactPanelWidth ?? getDefaultArtifactPanelWidth(),
                      flexBasis: artifactPanelWidth ?? getDefaultArtifactPanelWidth(),
                    }
              }
              className={
                isMobile
                  ? 'absolute inset-x-2 bottom-2 top-16 z-[70] rounded-lg border border-border-subtle'
                  : 'min-w-[360px] flex-shrink-0'
              }
            />
          )}
        </div>
        <InAppTerminalDock
          open={isTerminalDockOpen}
          workingDir={sessionWorkingDir}
          onClose={() => setIsTerminalDockOpen(false)}
        />
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
