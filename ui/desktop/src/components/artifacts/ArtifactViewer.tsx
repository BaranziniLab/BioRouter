import { UIResourceRenderer } from '@mcp-ui/client';
import {
  type CSSProperties,
  type PointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useTheme, useThemeFamily } from '../../contexts/ThemeContext';
import { CODE_FONT_FAMILY, codeThemesByFamily } from '../../styles/codeTheme';
import { cn } from '../../utils';
import { injectArtifactBrowserCsp } from '../../utils/artifactSecurity';
import {
  isTabCycleEvent,
  tabCycleOffset,
  nextTabIndex,
  isWithinArtifactPanel,
  ARTIFACT_PANEL_ATTR,
} from '../../utils/tabCycle';
import {
  ChevronDown,
  ChevronRight,
  Code,
  ExternalLink,
  Eye,
  File,
  FileText,
  FileX,
  Folder,
  Github,
  Globe,
  Image,
  Lock,
  Maximize2,
  Search,
  X,
} from '../icons/app-icons';
import MarkdownContent from '../MarkdownContent';
import { useTabStripOverflow } from '../Layout/useTabStripOverflow';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import DocumentPreview from './DocumentPreview';
import NotebookPreview from './NotebookPreview';
import type {
  ArtifactFileEntry,
  ArtifactFilePreview,
  ArtifactGitEntry,
  ArtifactGitStatus,
  ArtifactSource,
} from './artifactTypes';
import {
  basenameFromPath,
  dirnameFromPath,
  extensionFromPath,
  isDelimitedPath,
  isMarkdownPath,
  languageFromPath,
  languageLabel,
  parseDelimitedTable,
  splitPathForStrip,
  STRIP_IDENT_CLASS,
  STRIP_LABEL_CLASS,
  STRIP_META_CLASS,
  withHostTheme,
} from './artifactUtils';

// Enough rows to see the shape of the data; a 200k-row CSV must not lock up the
// renderer just because the agent wrote it.
const MAX_TABLE_ROWS = 500;

// Line numbers stop helping once a file is long enough that nobody is counting.
const MAX_LINE_NUMBERED_LINES = 5_000;

// Text files conventionally end with a newline. Left in, it renders a phantom
// last line — numbered, empty, and one more than the file actually has.
function stripTrailingNewline(text: string): string {
  return text.replace(/\r?\n$/, '');
}

function countLines(text: string): number {
  const body = stripTrailingNewline(text);
  return body === '' ? 0 : body.split('\n').length;
}

/** The one mono stack (design.md §3.2), shared with chat code blocks and the terminal. */
const CODE_FONT = CODE_FONT_FAMILY;

// The panel's status-strip typography (STRIP_LABEL/IDENT/META_CLASS) and
// splitPathForStrip now live in artifactUtils so every preview — including
// NotebookPreview — shares the one status-strip voice and can never drift.

// Geometry mirrored from BaseChat's HEADER_ACTION_BUTTON_CLASS so the panel's
// expand/close read as the same control as the chat header's actions. That file
// is owned elsewhere; these values are kept in sync by hand.
const HEADER_ACTION_BUTTON_CLASS =
  'no-drag flex h-8 w-8 items-center justify-center rounded-md p-0 text-text-default/70 transition-colors hover:bg-background-medium hover:text-text-default';

interface ArtifactViewerProps {
  artifact: ArtifactSource | null;
  isOpen?: boolean;
  isResizing?: boolean;
  onClose: () => void;
  onOpenArtifact: (artifact: ArtifactSource) => void;
  onResizeStart?: (event: PointerEvent<HTMLDivElement>) => void;
  onRenderError?: (error: ArtifactRenderError) => void;
  className?: string;
  style?: CSSProperties;
}

export interface ArtifactRenderError {
  artifactTitle: string;
  message: string;
  detail?: string;
  href?: string;
}

type HtmlPreview = { kind: 'html'; html: string };
type ExternalPreview = { kind: 'externalUrl'; url: string };
type FilePreview = { kind: 'file'; preview: ArtifactFilePreview };
type McpPreview = { kind: 'mcpResource' };
type LoadingPreview = { kind: 'loading' };
type ErrorPreview = { kind: 'error'; message: string };
type PreviewState =
  | HtmlPreview
  | ExternalPreview
  | FilePreview
  | McpPreview
  | LoadingPreview
  | ErrorPreview;

type ArtifactTab = {
  id: string;
  artifact: ArtifactSource;
};

type ArtifactTabsState = {
  tabs: ArtifactTab[];
  activeTabId: string | null;
};

type ArtifactTabsAction =
  | { type: 'open'; artifact: ArtifactSource; newTabId: string }
  | { type: 'activate'; tabId: string }
  | { type: 'close'; tabId: string }
  | { type: 'reorder'; draggedTabId: string; targetTabId: string };

let artifactTabSequence = 0;

function nextArtifactTabId() {
  artifactTabSequence += 1;
  return `artifact-tab-${artifactTabSequence}`;
}

function artifactSourceKey(artifact: ArtifactSource) {
  if (artifact.kind === 'file') return `file:${artifact.path}`;
  if (artifact.kind === 'externalUrl') return `url:${artifact.url}`;
  if (artifact.kind === 'html') {
    return `html:${artifact.title}:${artifact.html.length}:${artifact.html.slice(0, 80)}`;
  }
  const resource = artifact.resource as {
    uri?: string;
    mimeType?: string;
    text?: string;
    blob?: string;
  };
  return `resource:${resource.uri ?? ''}:${resource.mimeType ?? ''}:${resource.text?.length ?? 0}:${resource.blob?.length ?? 0}`;
}

function artifactHoverTitle(artifact: ArtifactSource) {
  if (artifact.kind === 'file') return artifact.path;
  if (artifact.kind === 'externalUrl') return artifact.url;
  if (artifact.kind === 'mcpResource') return artifact.resource.uri;
  return artifact.title;
}

function createArtifactTabsState(artifact: ArtifactSource | null): ArtifactTabsState {
  if (!artifact) return { tabs: [], activeTabId: null };
  const id = nextArtifactTabId();
  return { tabs: [{ id, artifact }], activeTabId: id };
}

function artifactTabsReducer(
  state: ArtifactTabsState,
  action: ArtifactTabsAction
): ArtifactTabsState {
  if (action.type === 'open') {
    const key = artifactSourceKey(action.artifact);
    const existing = state.tabs.find((tab) => artifactSourceKey(tab.artifact) === key);
    if (existing) {
      return {
        tabs: state.tabs.map((tab) =>
          tab.id === existing.id ? { ...tab, artifact: action.artifact } : tab
        ),
        activeTabId: existing.id,
      };
    }
    return {
      tabs: [...state.tabs, { id: action.newTabId, artifact: action.artifact }],
      activeTabId: action.newTabId,
    };
  }

  if (action.type === 'activate') return { ...state, activeTabId: action.tabId };

  if (action.type === 'reorder') {
    const draggedIndex = state.tabs.findIndex((tab) => tab.id === action.draggedTabId);
    const targetIndex = state.tabs.findIndex((tab) => tab.id === action.targetTabId);
    if (draggedIndex < 0 || targetIndex < 0 || draggedIndex === targetIndex) return state;

    const tabs = [...state.tabs];
    const [draggedTab] = tabs.splice(draggedIndex, 1);
    tabs.splice(targetIndex, 0, draggedTab);
    return { ...state, tabs };
  }

  const closingIndex = state.tabs.findIndex((tab) => tab.id === action.tabId);
  if (closingIndex < 0) return state;
  const tabs = state.tabs.filter((tab) => tab.id !== action.tabId);
  if (state.activeTabId !== action.tabId) return { ...state, tabs };
  const nextTab = tabs[Math.min(closingIndex, tabs.length - 1)] ?? null;
  return { tabs, activeTabId: nextTab?.id ?? null };
}

// The artifact preview shares the chat renderer's palette rather than maintaining
// a second, divergent one. Both come from styles/codeTheme.ts (design.md §5.1),
// selected by the active theme family + mode via codeThemesByFamily.

/// A render-error field is forwarded into an agent prompt, so bound its length
/// rather than letting a figure paste an arbitrarily long payload into context.
const RENDER_ERROR_TEXT_LIMIT = 2000;

function clampRenderErrorText(value: string) {
  return value.length > RENDER_ERROR_TEXT_LIMIT
    ? `${value.slice(0, RENDER_ERROR_TEXT_LIMIT)}…`
    : value;
}

function iconForArtifact(artifact: ArtifactSource | null) {
  if (!artifact) return File;
  if (artifact.kind === 'externalUrl') return Globe;
  if (artifact.kind === 'file') {
    const ext = artifact.path.split('.').pop()?.toLowerCase();
    if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'].includes(ext || '')) return Image;
    if (['html', 'htm'].includes(ext || '')) return Globe;
    if (['js', 'ts', 'tsx', 'jsx', 'py', 'rs', 'sql', 'json', 'yaml', 'yml'].includes(ext || '')) {
      return Code;
    }
    return FileText;
  }
  return Globe;
}

function formatBytes(value?: number) {
  if (value === undefined) return '';
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export default function ArtifactViewer({
  artifact,
  isOpen = true,
  isResizing = false,
  onClose,
  onOpenArtifact,
  onResizeStart,
  onRenderError,
  className,
  style,
}: ArtifactViewerProps) {
  const { resolvedTheme } = useTheme();
  const [tabState, dispatchTabAction] = useReducer(
    artifactTabsReducer,
    artifact,
    createArtifactTabsState
  );
  const [preview, setPreview] = useState<PreviewState>({ kind: 'loading' });
  const [previewSourceKey, setPreviewSourceKey] = useState<string | null>(null);
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);
  const [dragOverTabId, setDragOverTabId] = useState<string | null>(null);
  const lastRenderErrorKeyRef = useRef<string | null>(null);
  const trustedFrameRef = useRef<HTMLIFrameElement | null>(null);
  const pendingNavigationKeyRef = useRef<string | null>(null);
  const draggedTabIdRef = useRef<string | null>(null);
  const tabPointerGestureRef = useRef<{
    tabId: string;
    startX: number;
    startY: number;
  } | null>(null);
  const dragOverTabIdRef = useRef<string | null>(null);
  const suppressTabClickRef = useRef(false);
  const suppressTabClickTimerRef = useRef<number | null>(null);
  const activeTabButtonRef = useRef<HTMLButtonElement | null>(null);
  const tabListRef = useRef<HTMLDivElement | null>(null);
  /**
   * Rung 3 (D-32) for the panel's strip, through the SAME rule the chat strip
   * uses — card Z's "one tab, three surfaces" is only true if the rule is shared
   * too. Called up here with the other hooks: the component early-returns below
   * when there is no active artifact.
   */
  const showTabOverflowMenu = useTabStripOverflow(tabListRef, tabState.tabs.length);
  const activeTab = tabState.tabs.find((tab) => tab.id === tabState.activeTabId) ?? null;
  const activeArtifact = activeTab?.artifact ?? null;
  const activeSourceKey = activeArtifact ? artifactSourceKey(activeArtifact) : null;

  const openArtifactInTab = useCallback(
    (nextArtifact: ArtifactSource) => {
      pendingNavigationKeyRef.current = artifactSourceKey(nextArtifact);
      dispatchTabAction({
        type: 'open',
        artifact: nextArtifact,
        newTabId: nextArtifactTabId(),
      });
      onOpenArtifact(nextArtifact);
    },
    [onOpenArtifact]
  );

  const closeTab = useCallback(
    (tabId: string) => {
      if (tabState.tabs.length === 1) {
        onClose();
        return;
      }

      const closingIndex = tabState.tabs.findIndex((tab) => tab.id === tabId);
      const closingActiveTab = tabState.activeTabId === tabId;
      const remainingTabs = tabState.tabs.filter((tab) => tab.id !== tabId);
      dispatchTabAction({ type: 'close', tabId });

      if (closingActiveTab) {
        const nextTab = remainingTabs[Math.min(closingIndex, remainingTabs.length - 1)];
        if (nextTab) {
          pendingNavigationKeyRef.current = artifactSourceKey(nextTab.artifact);
          onOpenArtifact(nextTab.artifact);
        }
      }
    },
    [onClose, onOpenArtifact, tabState]
  );

  useEffect(() => {
    const handleBrowserTabShortcuts = (event: KeyboardEvent) => {
      if (!isOpen || !tabState.activeTabId) return;

      if (
        event.key.toLowerCase() === 'w' &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey
      ) {
        event.preventDefault();
        event.stopPropagation();
        closeTab(tabState.activeTabId);
        return;
      }

      if (!isTabCycleEvent(event)) return;

      // Ctrl+Tab belongs to whichever strip has focus. Without this the panel
      // cycled previews from ANYWHERE the moment it was open — including with
      // the cursor in the composer, where the user means their chat tabs. The
      // chat strip consults the same predicate and takes the other branch, so
      // the two cannot both answer regardless of listener order.
      if (!isWithinArtifactPanel(event.target)) return;

      const activeIndex = tabState.tabs.findIndex((tab) => tab.id === tabState.activeTabId);
      const nextIndex = nextTabIndex(tabState.tabs.length, activeIndex, tabCycleOffset(event));
      if (nextIndex === null) return;

      event.preventDefault();
      event.stopPropagation();
      const nextTab = tabState.tabs[nextIndex];
      pendingNavigationKeyRef.current = artifactSourceKey(nextTab.artifact);
      dispatchTabAction({ type: 'activate', tabId: nextTab.id });
      onOpenArtifact(nextTab.artifact);
    };

    window.addEventListener('keydown', handleBrowserTabShortcuts, true);
    return () => window.removeEventListener('keydown', handleBrowserTabShortcuts, true);
  }, [closeTab, isOpen, onOpenArtifact, tabState.activeTabId, tabState.tabs]);

  useEffect(() => {
    activeTabButtonRef.current?.scrollIntoView?.({
      behavior: 'smooth',
      block: 'nearest',
      inline: 'nearest',
    });
  }, [tabState.activeTabId, tabState.tabs.length]);

  useEffect(() => {
    const moveTabPointer = (event: globalThis.PointerEvent) => {
      const gesture = tabPointerGestureRef.current;
      if (!gesture) return;

      if (!draggedTabIdRef.current) {
        const distance = Math.hypot(event.clientX - gesture.startX, event.clientY - gesture.startY);
        if (distance < 5) return;
        draggedTabIdRef.current = gesture.tabId;
        suppressTabClickRef.current = true;
        setDraggedTabId(gesture.tabId);
      }

      event.preventDefault();
      const target = document
        .elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>('[data-artifact-tab-id]');
      const targetTabId = target?.dataset.artifactTabId ?? null;
      const nextDragOverTabId = targetTabId === gesture.tabId ? null : targetTabId;
      dragOverTabIdRef.current = nextDragOverTabId;
      setDragOverTabId(nextDragOverTabId);
    };

    const finishTabPointer = () => {
      const sourceTabId = draggedTabIdRef.current;
      const targetTabId = dragOverTabIdRef.current;
      if (sourceTabId && targetTabId) {
        dispatchTabAction({ type: 'reorder', draggedTabId: sourceTabId, targetTabId });
      }
      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
      suppressTabClickTimerRef.current = window.setTimeout(() => {
        suppressTabClickRef.current = false;
        suppressTabClickTimerRef.current = null;
      }, 0);
      tabPointerGestureRef.current = null;
      draggedTabIdRef.current = null;
      dragOverTabIdRef.current = null;
      setDraggedTabId(null);
      setDragOverTabId(null);
    };

    window.addEventListener('pointermove', moveTabPointer);
    window.addEventListener('pointerup', finishTabPointer);
    window.addEventListener('pointercancel', finishTabPointer);
    return () => {
      window.removeEventListener('pointermove', moveTabPointer);
      window.removeEventListener('pointerup', finishTabPointer);
      window.removeEventListener('pointercancel', finishTabPointer);
      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
    };
  }, []);

  useEffect(() => {
    if (!artifact) return;
    const key = artifactSourceKey(artifact);
    if (pendingNavigationKeyRef.current === key) {
      pendingNavigationKeyRef.current = null;
      return;
    }
    dispatchTabAction({ type: 'open', artifact, newTabId: nextArtifactTabId() });
  }, [artifact]);

  useEffect(() => {
    let cancelled = false;

    async function loadPreview() {
      if (!activeArtifact || !activeSourceKey) return;
      setPreviewSourceKey(activeSourceKey);
      setPreview({ kind: 'loading' });

      if (activeArtifact.kind === 'html') {
        try {
          const prepared = await window.electron.prepareArtifactHtml({
            html: activeArtifact.html,
          });
          if (!cancelled) setPreview({ kind: 'html', html: prepared.html });
        } catch {
          if (!cancelled) setPreview({ kind: 'html', html: activeArtifact.html });
        }
        return;
      }

      if (activeArtifact.kind === 'externalUrl') {
        setPreview({ kind: 'externalUrl', url: activeArtifact.url });
        return;
      }

      if (activeArtifact.kind === 'mcpResource') {
        setPreview({ kind: 'mcpResource' });
        return;
      }

      try {
        // No cast: the preload IPC contract and the shared ArtifactFilePreview
        // union are kept in lockstep, so the result assigns structurally.
        const response: ArtifactFilePreview = await window.electron.readArtifactFile(
          activeArtifact.path
        );
        if (cancelled) return;
        if (response.kind === 'html') {
          // An HTML file the agent wrote gets a Preview/Raw toggle, like markdown:
          // keep it a file preview (so `text` remains the raw source) and attach a
          // security-prepared copy for the rendered Preview. A `ui://` figure
          // resource is `artifact.kind === 'html'` and took the branch above, so it
          // still renders figure-only with no raw source — only real files land here.
          let preparedHtml = response.text;
          try {
            const prepared = await window.electron.prepareArtifactHtml({
              html: response.text,
            });
            preparedHtml = prepared.html;
          } catch {
            // Preparation failed; fall back to the raw HTML for the Preview.
          }
          if (!cancelled) setPreview({ kind: 'file', preview: { ...response, preparedHtml } });
          return;
        }
        setPreview({ kind: 'file', preview: response });
      } catch (error) {
        if (!cancelled) {
          setPreview({
            kind: 'error',
            message: error instanceof Error ? error.message : 'Could not open artifact.',
          });
        }
      }
    }

    loadPreview();
    return () => {
      cancelled = true;
    };
  }, [activeArtifact, activeSourceKey]);

  useEffect(() => {
    if (!activeArtifact || !onRenderError) return;

    const handleMessage = (event: MessageEvent) => {
      // A render-error report becomes a hidden, agent-visible prompt. Only the
      // srcDoc frame we generated may send one: an externalUrl artifact, an
      // mcp-ui frame, or any other window would otherwise be able to inject
      // instructions into a session that holds shell and file tools.
      const trustedSource = trustedFrameRef.current?.contentWindow;
      if (!trustedSource || event.source !== trustedSource) return;

      const data = event.data as
        | {
            type?: string;
            payload?: {
              message?: unknown;
              detail?: unknown;
              href?: unknown;
            };
          }
        | undefined;

      if (!data || data.type !== 'biorouter-viz-render-error') return;

      const message =
        typeof data.payload?.message === 'string'
          ? clampRenderErrorText(data.payload.message)
          : 'This visualization could not be rendered.';
      const detail =
        typeof data.payload?.detail === 'string'
          ? clampRenderErrorText(data.payload.detail)
          : undefined;
      const href =
        typeof data.payload?.href === 'string'
          ? clampRenderErrorText(data.payload.href)
          : undefined;
      const key = `${activeArtifact.title}\n${message}\n${detail ?? ''}`;
      if (lastRenderErrorKeyRef.current === key) return;
      lastRenderErrorKeyRef.current = key;

      onRenderError({
        artifactTitle: activeArtifact.title,
        message,
        detail,
        href,
      });
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [activeArtifact, onRenderError]);

  const visiblePreview: PreviewState =
    activeSourceKey && previewSourceKey === activeSourceKey ? preview : { kind: 'loading' };

  if (!activeArtifact || !activeTab) return null;

  const activateTab = (tab: ArtifactTab) => {
    pendingNavigationKeyRef.current = artifactSourceKey(tab.artifact);
    dispatchTabAction({ type: 'activate', tabId: tab.id });
    onOpenArtifact(tab.artifact);
  };

  const beginTabPointerDrag = (event: PointerEvent<HTMLButtonElement>, tabId: string) => {
    if (event.button !== 0) return;
    tabPointerGestureRef.current = {
      tabId,
      startX: event.clientX,
      startY: event.clientY,
    };
  };

  const activateTabFromPointer = (tab: ArtifactTab) => {
    if (suppressTabClickRef.current) {
      suppressTabClickRef.current = false;
      window.clearTimeout(suppressTabClickTimerRef.current ?? undefined);
      suppressTabClickTimerRef.current = null;
      return;
    }
    activateTab(tab);
  };

  const openStandalone = async () => {
    if (activeArtifact.kind === 'html') {
      // Expand opens the offline preview in the user's default browser. Live
      // Agent Drafter apps use the explicit launch link returned by the tool.
      await window.electron.openArtifactInBrowser({
        html: visiblePreview.kind === 'html' ? visiblePreview.html : activeArtifact.html,
        title: activeArtifact.title,
        theme: resolvedTheme,
      });
      return;
    }
    if (activeArtifact.kind === 'externalUrl') {
      await window.electron.openExternal(activeArtifact.url);
      return;
    }
    if (activeArtifact.kind === 'file') {
      await window.electron.openDirectoryInExplorer(activeArtifact.path);
    }
  };

  return (
    <aside
      data-testid="artifact-viewer"
      // The anchor Ctrl+Tab arbitrates on: a keystroke landing inside this
      // subtree is aimed at the preview's tabs, anything else at the chat's.
      // Deliberately not the testid above — behaviour must not hang off a
      // promise we only made to tests.
      {...{ [ARTIFACT_PANEL_ATTR]: '' }}
      style={{
        ...style,
        contain: 'layout paint',
        // Only transform + opacity are GPU-composited. width/flex-basis are layout
        // props the compositor cannot promote — hinting them was ineffective and
        // held speculative layer state permanently.
        willChange: 'transform, opacity',
      }}
      className={cn(
        'no-drag relative isolate flex h-full min-h-0 w-full flex-col overflow-hidden border-l border-border-subtle bg-background-muted',
        // Animate only transform + opacity — width tracks instantly (drag is
        // transition-none; window-resize should snap, not lag the edge by 180ms).
        // Exit is a tier faster than entrance (--motion-fast vs --motion-base).
        isResizing ? 'transition-none' : 'transition-[opacity,transform] ease-[var(--ease-out)]',
        !isResizing && (isOpen ? 'duration-[var(--motion-base)]' : 'duration-[var(--motion-fast)]'),
        isOpen ? 'translate-x-0 opacity-100' : 'translate-x-3 opacity-0',
        className
      )}
    >
      {onResizeStart && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize artifact panel"
          onPointerDown={onResizeStart}
          className="group absolute inset-y-0 left-0 z-30 w-2 cursor-col-resize"
        >
          <div className="h-full w-px bg-transparent transition-colors group-hover:bg-border-strong" />
        </div>
      )}

      {/* The tab strip is `br-tabstrip` (shared, styles/main.css): its ground is the
          sidebar colour, so the window's whole 52px top edge is one continuous
          surface. Height is set here; the paint belongs to the class. */}
      <div className="br-tabstrip no-drag relative z-50 h-[52px] flex-shrink-0">
        {/* The tablist nests inside the strip for a11y, so it repeats the strip's
            own 3px gap: `.br-tab + .br-tab::before` hangs its divider at -2px and
            only lands in the gap if the tabs are spaced the way the class expects. */}
        <div
          ref={tabListRef}
          role="tablist"
          aria-label="Open artifact previews"
          aria-keyshortcuts="Meta+W Control+W Control+Tab Control+Shift+Tab"
          // Rung 3 of the yield ladder (D-32): shrink to the floor, then SCROLL,
          // then collapse into a ▾ — never wrap. This was `overflow-hidden`, so
          // the panel's tabs did neither: past the floor they were clipped and
          // simply unreachable, which is the failure the rung exists to prevent
          // and which the panel feels first — it is the narrowest strip in the
          // window, and rung 2 makes it narrower still.
          className="br-tabstrip__scroll flex min-w-0 flex-1 items-center gap-[3px] overflow-x-auto"
        >
          {tabState.tabs.map((tab) => {
            const TabIcon = iconForArtifact(tab.artifact);
            const isActive = tab.id === tabState.activeTabId;
            return (
              // `br-tab` (shared) paints the Safari tab: only the active one is a
              // filled pill, none carry a border, and the divider between two tabs
              // is drawn by `.br-tab + .br-tab::before` — never added here.
              <div
                key={tab.id}
                data-artifact-tab-id={tab.id}
                data-active={isActive ? 'true' : undefined}
                data-dragging={draggedTabId === tab.id ? 'true' : undefined}
                data-dragover={dragOverTabId === tab.id ? 'true' : undefined}
                className={cn(
                  'br-tab group',
                  // Drag affordances only — state feedback, not base styling.
                  draggedTabId === tab.id && 'opacity-50',
                  dragOverTabId === tab.id && 'bg-background-medium'
                )}
              >
                <button
                  ref={isActive ? activeTabButtonRef : undefined}
                  type="button"
                  role="tab"
                  aria-selected={isActive}
                  aria-controls="artifact-preview-content"
                  onPointerDown={(event) => beginTabPointerDrag(event, tab.id)}
                  onClick={() => activateTabFromPointer(tab)}
                  title={artifactHoverTitle(tab.artifact)}
                  className="flex h-full min-w-0 flex-1 cursor-grab items-center gap-1.5 text-left active:cursor-grabbing"
                >
                  <TabIcon className="h-4 w-4 shrink-0" aria-hidden="true" />
                  <span className="br-tab__label min-w-0 flex-1 truncate">
                    {tab.artifact.title}
                  </span>
                </button>
                <button
                  type="button"
                  onClick={() => closeTab(tab.id)}
                  aria-label={`Close ${tab.artifact.title}`}
                  title={`Close ${tab.artifact.title}`}
                  className={cn(
                    'inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-sm text-text-subtle transition-[background-color,color,opacity] hover:bg-background-medium hover:text-text-default',
                    isActive
                      ? 'opacity-100'
                      : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
                  )}
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                </button>
              </div>
            );
          })}
        </div>
        {showTabOverflowMenu && (
          // Outside the scroll box, for the reason ChatTabStrip's wrap documents:
          // inside it, the button's own width would keep alive the overflow that
          // summoned it.
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label="Show all previews"
                data-testid="artifact-tab-overflow-trigger"
                className="br-tabstrip__overflow"
              >
                <ChevronDown className="h-4 w-4" aria-hidden="true" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="max-h-[60vh] w-56 overflow-y-auto">
              {tabState.tabs.map((tab) => {
                const TabIcon = iconForArtifact(tab.artifact);
                return (
                  <DropdownMenuItem
                    key={tab.id}
                    data-testid={`artifact-tab-overflow-item-${tab.id}`}
                    onSelect={() => activateTab(tab)}
                    className={cn('gap-2', tab.id === tabState.activeTabId && 'font-medium')}
                  >
                    <TabIcon className="h-4 w-4 flex-none" aria-hidden="true" />
                    <span className="min-w-0 flex-1 truncate">{tab.artifact.title}</span>
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
        {activeArtifact.kind !== 'mcpResource' && (
          <button
            type="button"
            onClick={openStandalone}
            className={cn(HEADER_ACTION_BUTTON_CLASS, 'relative z-50 ml-0.5 shrink-0')}
            aria-label="Open active artifact outside preview"
            title="Open active artifact outside preview"
          >
            {activeArtifact.kind === 'html' ? (
              <Maximize2 className="h-4 w-4" aria-hidden="true" />
            ) : (
              <ExternalLink className="h-4 w-4" aria-hidden="true" />
            )}
          </button>
        )}
        <button
          type="button"
          onClick={onClose}
          className={cn(HEADER_ACTION_BUTTON_CLASS, 'relative z-50 shrink-0')}
          aria-label="Close artifact viewer"
          title="Close preview panel"
        >
          <X className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>

      {/* De-boxed (design spec H): no gutter, no card, no border, no shadow. The
          preview sits directly on the panel ground — panel → strip → content. */}
      <div id="artifact-preview-content" className="relative z-0 min-h-0 flex-1 overflow-hidden">
        {isResizing && (
          <div
            data-testid="artifact-resize-shield"
            aria-hidden="true"
            className="absolute inset-0 z-50 cursor-col-resize"
          />
        )}
        <ArtifactPreviewBody
          preview={visiblePreview}
          artifact={activeArtifact}
          resolvedTheme={resolvedTheme}
          isResizing={isResizing}
          trustedFrameRef={trustedFrameRef}
          onOpenArtifactInTab={openArtifactInTab}
        />
      </div>
    </aside>
  );
}

/**
 * Friendly empty-state for a file that could not be read (#36). The main
 * process already maps errno codes to human-readable messages (see
 * utils/artifactFileErrors.ts); this renders them as a proper centered state
 * — icon, short title, message, muted path — instead of a bare gray line of
 * error text.
 */
function ArtifactErrorState({
  message,
  path,
  code,
}: {
  message: string;
  path?: string;
  code?: string;
}) {
  const Icon = code === 'EACCES' || code === 'EPERM' ? Lock : FileX;
  return (
    <div data-testid="artifact-error-state" className="flex h-full items-center justify-center p-6">
      <div className="w-full max-w-sm text-center">
        <Icon className="mx-auto mb-3 h-6 w-6 text-text-muted" aria-hidden="true" />
        <div className="text-sm font-medium text-text-default">File not available</div>
        <p className="mt-1 text-sm leading-relaxed text-text-muted">{message}</p>
        {path && <div className="mt-2 break-all text-xs text-text-muted/70">{path}</div>}
      </div>
    </div>
  );
}

function ArtifactPreviewBody({
  preview,
  artifact,
  resolvedTheme,
  isResizing,
  trustedFrameRef,
  onOpenArtifactInTab,
}: {
  preview: PreviewState;
  artifact: ArtifactSource;
  resolvedTheme: 'light' | 'dark';
  isResizing: boolean;
  trustedFrameRef: React.RefObject<HTMLIFrameElement | null>;
  onOpenArtifactInTab: (artifact: ArtifactSource) => void;
}) {
  if (preview.kind === 'loading') {
    return (
      <div className="flex h-full items-center justify-center text-sm text-text-muted">Loading</div>
    );
  }

  if (preview.kind === 'error') {
    return <ArtifactErrorState message={preview.message} />;
  }

  if (preview.kind === 'html') {
    // `allow-popups` is withheld: with it, figure HTML can window.open() a
    // `data:` URL, which the main window's open handler turns into a real
    // BrowserWindow that inherits the preload IPC bridge.
    return (
      <iframe
        name="biorouter-artifact-preview"
        ref={trustedFrameRef}
        aria-label={artifact.title}
        // Inject the app theme so this preview matches the expanded/opened view,
        // which loads with an explicit `?theme=`; a srcdoc iframe has no query.
        srcDoc={injectArtifactBrowserCsp(withHostTheme(preview.html, resolvedTheme))}
        sandbox="allow-scripts allow-downloads"
        className={cn('h-full w-full bg-white', isResizing && 'pointer-events-none')}
      />
    );
  }

  if (preview.kind === 'externalUrl') {
    return (
      <div className="flex h-full items-center justify-center bg-background-medium p-6">
        <div className="w-full max-w-lg rounded-xl border border-border-subtle bg-background-default p-5 text-center shadow-popover">
          <Globe className="mx-auto mb-3 h-6 w-6 text-text-muted" aria-hidden="true" />
          <div className="text-sm font-medium text-text-default">External preview</div>
          <div className="mt-1 break-all text-xs text-text-muted">{preview.url}</div>
          <button
            type="button"
            onClick={() => void window.electron.openExternal(preview.url)}
            className="mt-4 inline-flex h-8 items-center gap-2 rounded-lg border border-border-subtle bg-background-default px-3 text-xs font-medium text-text-default transition-colors hover:bg-background-medium"
          >
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            Open in default browser
          </button>
        </div>
      </div>
    );
  }

  if (preview.kind === 'mcpResource' && artifact.kind === 'mcpResource') {
    return (
      <UIResourceRenderer
        resource={artifact.resource}
        supportedContentTypes={['rawHtml']}
        htmlProps={{
          autoResizeIframe: { height: false, width: false },
          style: { width: '100%', height: '100%', minHeight: '100%', border: 'none' },
          iframeRenderData: { host: 'biorouter', theme: resolvedTheme },
        }}
      />
    );
  }

  if (preview.kind !== 'file') return null;
  const file = preview.preview;

  if (file.kind === 'error') {
    return <ArtifactErrorState message={file.error} path={file.path} code={file.code} />;
  }

  if (file.kind === 'image') {
    // The image is the content, so it sits on the panel ground with a gutter —
    // no tinted well behind it, no radius pretending it is a card.
    return (
      <div className="flex h-full items-center justify-center overflow-auto p-4">
        <img src={file.dataUrl} alt={file.title} className="max-h-full max-w-full object-contain" />
      </div>
    );
  }

  if (file.kind === 'document') {
    return <DocumentPreview file={file} resolvedTheme={resolvedTheme} isResizing={isResizing} />;
  }

  if ((file.kind === 'text' || file.kind === 'html') && extensionFromPath(file.path) === 'ipynb') {
    return <NotebookPreview file={file} resolvedTheme={resolvedTheme} />;
  }

  if (file.kind === 'gitDirectory' || file.kind === 'directory') {
    return (
      <DirectoryTreePreview
        key={file.path}
        directory={file}
        resolvedTheme={resolvedTheme}
        isResizing={isResizing}
        onOpenArtifactInTab={onOpenArtifactInTab}
      />
    );
  }

  if (file.kind === 'binary') {
    // Text-decodable files already fall through to the plain-text preview below
    // (see isTextArtifact in the main process). Reaching here means the bytes are
    // genuinely binary (or the file is too large), so there is nothing to show —
    // tell the user plainly and offer to open it in its default app.
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center text-sm text-text-muted">
        <File className="h-8 w-8" aria-hidden="true" />
        <div className="font-medium text-text-default">{basenameFromPath(file.path)}</div>
        <div>
          {file.mimeType} · {formatBytes(file.size)}
        </div>
        <p className="max-w-xs leading-relaxed">
          This file can’t be previewed here. Open it in the app your system uses for this file type.
        </p>
        <button
          type="button"
          onClick={() => window.electron.openDirectoryInExplorer(file.path)}
          className="inline-flex items-center gap-1.5 rounded-md border border-border-strong bg-transparent px-3 py-1.5 text-text-default transition-[background-color,color,transform,scale] duration-[var(--motion-fast)] active:scale-[0.97] hover:bg-background-medium"
        >
          <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          Open the file
        </button>
      </div>
    );
  }

  // Keyed by path so the Preview/Raw toggle resets when a different file opens —
  // the panel stays mounted across artifacts, so the state would otherwise stick
  // and show a CSV as raw text just because the last markdown was.
  if (file.kind === 'text' || file.kind === 'html') {
    return (
      <TextFilePreview
        key={file.path}
        file={file}
        resolvedTheme={resolvedTheme}
        onOpenArtifact={onOpenArtifactInTab}
      />
    );
  }
  return null;
}

const gitStatusPresentation: Record<
  ArtifactGitStatus,
  { label: string; textClass: string; dotClass: string }
> = {
  untracked: {
    label: 'Untracked',
    textClass: 'text-text-info',
    dotClass: 'bg-background-info',
  },
  modified: {
    label: 'Modified',
    textClass: 'text-text-warning',
    dotClass: 'bg-background-warning',
  },
  staged: {
    label: 'Staged',
    textClass: 'text-text-success',
    dotClass: 'bg-background-success',
  },
  committed: {
    label: 'Committed',
    textClass: 'text-text-danger',
    dotClass: 'bg-background-danger',
  },
  pushed: {
    label: 'Pushed',
    textClass: 'text-text-muted',
    dotClass: 'bg-text-muted',
  },
};

type ArtifactTreeDirectory = Extract<ArtifactFilePreview, { kind: 'directory' | 'gitDirectory' }>;
type ArtifactTreeEntry = ArtifactFileEntry | ArtifactGitEntry;

function DirectoryTreePreview({
  directory,
  resolvedTheme,
  isResizing,
  onOpenArtifactInTab,
}: {
  directory: ArtifactTreeDirectory;
  resolvedTheme: 'light' | 'dark';
  isResizing: boolean;
  onOpenArtifactInTab: (artifact: ArtifactSource) => void;
}) {
  const isGitRepository = directory.kind === 'gitDirectory';
  const treeEntries = useMemo<ArtifactTreeEntry[]>(
    () =>
      directory.entries.map((entry) => {
        const incoming = entry as ArtifactTreeEntry & {
          relativePath?: string;
          parentPath?: string;
        };
        return {
          ...entry,
          relativePath: incoming.relativePath ?? entry.name,
          parentPath: incoming.parentPath ?? '',
        } as ArtifactTreeEntry;
      }),
    [directory.entries]
  );
  const [filter, setFilter] = useState('');
  const [expandedDirectories, setExpandedDirectories] = useState<Set<string>>(
    () =>
      new Set(
        treeEntries
          .filter((entry) => entry.isDirectory && entry.relativePath.split('/').length <= 2)
          .map((entry) => entry.relativePath)
      )
  );
  const [selectedEntry, setSelectedEntry] = useState<ArtifactTreeEntry | null>(null);
  const [selectedPreview, setSelectedPreview] = useState<ArtifactFilePreview | null>(null);
  const selectedRequestRef = useRef(0);
  const selectedFrameRef = useRef<HTMLIFrameElement | null>(null);

  const childEntries = useMemo(() => {
    const children = new Map<string, ArtifactTreeEntry[]>();
    for (const entry of treeEntries) {
      const siblings = children.get(entry.parentPath) ?? [];
      siblings.push(entry);
      children.set(entry.parentPath, siblings);
    }
    return children;
  }, [treeEntries]);

  const visibleEntries = useMemo(() => {
    const query = filter.trim().toLocaleLowerCase();
    const includedPaths = new Set<string>();
    if (query) {
      for (const entry of treeEntries) {
        if (!entry.relativePath.toLocaleLowerCase().includes(query)) continue;
        includedPaths.add(entry.relativePath);
        const segments = entry.relativePath.split('/');
        for (let index = 1; index < segments.length; index += 1) {
          includedPaths.add(segments.slice(0, index).join('/'));
        }
      }
    }

    const result: ArtifactTreeEntry[] = [];
    const appendChildren = (parentPath: string) => {
      for (const entry of childEntries.get(parentPath) ?? []) {
        if (query && !includedPaths.has(entry.relativePath)) continue;
        result.push(entry);
        if (entry.isDirectory && (query || expandedDirectories.has(entry.relativePath))) {
          appendChildren(entry.relativePath);
        }
      }
    };
    appendChildren('');
    return result;
  }, [childEntries, expandedDirectories, filter, treeEntries]);

  const openTreeFile = useCallback(async (entry: ArtifactTreeEntry) => {
    const request = selectedRequestRef.current + 1;
    selectedRequestRef.current = request;
    setSelectedEntry(entry);
    setSelectedPreview(null);
    try {
      let response: ArtifactFilePreview = await window.electron.readArtifactFile(entry.path);
      if (response.kind === 'html') {
        try {
          const prepared = await window.electron.prepareArtifactHtml({
            html: response.text,
          });
          response = { ...response, preparedHtml: prepared.html };
        } catch {
          response = { ...response, preparedHtml: response.text };
        }
      }
      if (selectedRequestRef.current === request) setSelectedPreview(response);
    } catch (cause) {
      if (selectedRequestRef.current !== request) return;
      setSelectedPreview({
        kind: 'error',
        title: entry.name,
        path: entry.path,
        error: cause instanceof Error ? cause.message : 'Could not open this folder file.',
        found: false,
      });
    }
  }, []);

  return (
    // The rail is real structure, so it keeps its hairline — but not a second
    // ground: both halves sit on the panel's, divided by the one border.
    <div className="flex h-full min-h-0">
      <div className="flex w-[42%] min-w-[205px] max-w-[280px] shrink-0 flex-col border-r border-border-subtle">
        <div className="border-b border-border-subtle px-2.5 py-2">
          <div className="flex min-w-0 items-center gap-2 px-1 pb-2">
            {isGitRepository ? (
              <Github className="h-3.5 w-3.5 shrink-0 text-text-muted" aria-hidden="true" />
            ) : (
              <Folder className="h-3.5 w-3.5 shrink-0 text-text-muted" aria-hidden="true" />
            )}
            <span className="min-w-0 flex-1 truncate text-xs font-semibold text-text-default">
              {directory.title}
            </span>
            {directory.kind === 'gitDirectory' && (
              <span className={cn(STRIP_IDENT_CLASS, 'max-w-24 truncate rounded-sm px-1.5 py-0.5')}>
                {directory.branch}
              </span>
            )}
          </div>
          <label className="flex h-8 items-center gap-2 rounded-md border border-border-subtle bg-background-default px-2 focus-within:border-border-focus">
            <Search className="h-3.5 w-3.5 shrink-0 text-text-muted" aria-hidden="true" />
            <input
              type="search"
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Filter files…"
              aria-label={isGitRepository ? 'Filter repository files' : 'Filter folder files'}
              className="min-w-0 flex-1 bg-transparent text-xs text-text-default outline-none placeholder:text-text-subtle"
            />
          </label>
        </div>
        <div
          role="tree"
          aria-label={`${directory.title} ${isGitRepository ? 'repository' : 'folder'} files`}
          className="min-h-0 flex-1 overflow-auto p-1.5"
        >
          {visibleEntries.map((entry) => {
            const depth = entry.relativePath.split('/').length - 1;
            const status = isGitRepository
              ? gitStatusPresentation[(entry as ArtifactGitEntry).status]
              : null;
            const selected = selectedEntry?.relativePath === entry.relativePath;
            const expanded = filter.trim() !== '' || expandedDirectories.has(entry.relativePath);
            return (
              <button
                type="button"
                role="treeitem"
                aria-expanded={entry.isDirectory ? expanded : undefined}
                aria-selected={selected}
                aria-current={selected ? 'true' : undefined}
                aria-label={selected ? `${entry.name}, currently viewing` : entry.name}
                key={entry.relativePath}
                title={status ? `${entry.relativePath} · ${status.label}` : entry.relativePath}
                onClick={(event) => {
                  if (event.detail > 1) return;
                  if (!entry.isDirectory) {
                    void openTreeFile(entry);
                    return;
                  }
                  setExpandedDirectories((current) => {
                    const next = new Set(current);
                    if (next.has(entry.relativePath)) next.delete(entry.relativePath);
                    else next.add(entry.relativePath);
                    return next;
                  });
                }}
                onDoubleClick={() => {
                  if (entry.isDirectory) return;
                  onOpenArtifactInTab({
                    kind: 'file',
                    title: entry.name,
                    path: entry.path,
                  });
                }}
                className={cn(
                  // A row in a list, not a card: selection is a fill, never a lift.
                  'group flex h-7 w-full min-w-0 items-center gap-1 rounded-md px-1.5 text-left text-xs transition-colors',
                  selected ? 'bg-background-focus font-medium' : 'hover:bg-background-medium'
                )}
                style={{ paddingLeft: `${6 + depth * 13}px` }}
              >
                {entry.isDirectory ? (
                  expanded ? (
                    <ChevronDown className="h-3 w-3 shrink-0 text-text-subtle" aria-hidden="true" />
                  ) : (
                    <ChevronRight
                      className="h-3 w-3 shrink-0 text-text-subtle"
                      aria-hidden="true"
                    />
                  )
                ) : (
                  <span
                    className={cn(
                      'shrink-0',
                      status ? `h-1.5 w-1.5 rounded-full ${status.dotClass}` : 'h-3 w-3'
                    )}
                  />
                )}
                {entry.isDirectory ? (
                  <Folder
                    className={cn('h-3.5 w-3.5 shrink-0', status?.textClass ?? 'text-text-muted')}
                    aria-hidden="true"
                  />
                ) : (
                  <FileText
                    className={cn('h-3.5 w-3.5 shrink-0', status?.textClass ?? 'text-text-muted')}
                    aria-hidden="true"
                  />
                )}
                <span
                  className={cn(
                    'min-w-0 flex-1 truncate',
                    status?.textClass ?? 'text-text-default'
                  )}
                >
                  {entry.name}
                </span>
                {selected && (
                  <>
                    <Eye className="h-3.5 w-3.5 shrink-0 text-text-default" aria-hidden="true" />
                    <span className="sr-only">Currently viewing</span>
                  </>
                )}
              </button>
            );
          })}
          {visibleEntries.length === 0 && (
            <div className="px-2 py-6 text-center text-xs text-text-muted">
              {filter.trim() ? 'No matching files' : 'This folder is empty'}
            </div>
          )}
        </div>
        {isGitRepository && (
          <div className="grid grid-cols-2 gap-x-2 gap-y-1 border-t border-border-subtle px-3 py-2">
            {(Object.keys(gitStatusPresentation) as ArtifactGitStatus[]).map((statusKey) => {
              const status = gitStatusPresentation[statusKey];
              return (
                <span key={statusKey} className={cn(STRIP_META_CLASS, 'flex items-center gap-1.5')}>
                  <span className={cn('h-1.5 w-1.5 rounded-full', status.dotClass)} />
                  {status.label}
                </span>
              );
            })}
          </div>
        )}
      </div>
      <div className="min-w-0 flex-1">
        {selectedEntry ? (
          <ArtifactPreviewBody
            preview={
              selectedPreview ? { kind: 'file', preview: selectedPreview } : { kind: 'loading' }
            }
            artifact={{ kind: 'file', title: selectedEntry.name, path: selectedEntry.path }}
            resolvedTheme={resolvedTheme}
            isResizing={isResizing}
            trustedFrameRef={selectedFrameRef}
            onOpenArtifactInTab={onOpenArtifactInTab}
          />
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
            {isGitRepository ? (
              <Github className="h-6 w-6 text-text-subtle" aria-hidden="true" />
            ) : (
              <Folder className="h-6 w-6 text-text-subtle" aria-hidden="true" />
            )}
            <div className="text-sm font-medium text-text-default">
              {isGitRepository ? 'Select a repository file' : 'Select a file'}
            </div>
            <p className="max-w-xs text-xs leading-relaxed text-text-muted">
              {isGitRepository
                ? `This tree is locked to ${directory.title}. Git colors show each file’s current state.`
                : `This tree is locked to ${directory.title}. Expand folders to browse without leaving this root.`}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

function CodeBlock({
  text,
  language,
  resolvedTheme,
}: {
  text: string;
  language: string;
  resolvedTheme: 'light' | 'dark';
}) {
  const lineCount = countLines(text);
  const codeStyle = codeThemesByFamily[useThemeFamily()][resolvedTheme];
  return (
    <SyntaxHighlighter
      style={codeStyle}
      language={language}
      PreTag="div"
      showLineNumbers={lineCount > 1 && lineCount <= MAX_LINE_NUMBERED_LINES}
      lineNumberStyle={{
        minWidth: '2.6em',
        paddingRight: '1.1em',
        textAlign: 'right',
        opacity: 0.35,
        userSelect: 'none',
      }}
      // No `wrapLongLines`: combined with `showLineNumbers` the highlighter makes
      // every line `display: flex` (highlight.js:106), which turns each token into
      // a flex item and shreds the line across the panel's width. Long lines scroll
      // horizontally instead, which is what a code viewer should do anyway — and it
      // keeps indentation honest.
      customStyle={{
        margin: 0,
        padding: '14px 16px',
        minHeight: '100%',
        background: 'transparent',
      }}
      codeTagProps={{
        style: {
          fontFamily: CODE_FONT,
          whiteSpace: 'pre',
        },
      }}
    >
      {stripTrailingNewline(text)}
    </SyntaxHighlighter>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const timeoutRef = useRef<number | null>(null);

  useEffect(() => () => window.clearTimeout(timeoutRef.current ?? undefined), []);

  return (
    <button
      type="button"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(text);
          setCopied(true);
          window.clearTimeout(timeoutRef.current ?? undefined);
          timeoutRef.current = window.setTimeout(() => setCopied(false), 1600);
        } catch {
          // Clipboard unavailable (denied permission); leave the label alone.
        }
      }}
      className="rounded-sm px-2 py-0.5 text-[11px] text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
    >
      {copied ? 'Copied' : 'Copy'}
    </button>
  );
}

// A written `.md` report and a written `.csv` table are the agent's output, not
// its source code — showing raw markup would make the user read the syntax to
// find the content. Both stay one click from the raw text. Everything else is a
// script, and gets highlighted, line-numbered and labelled.
function TextFilePreview({
  file,
  resolvedTheme,
  onOpenArtifact,
}: {
  file: Extract<ArtifactFilePreview, { kind: 'text' | 'html' }>;
  resolvedTheme: 'light' | 'dark';
  onOpenArtifact?: (artifact: ArtifactSource) => void;
}) {
  const markdown = isMarkdownPath(file.path);
  const delimited = isDelimitedPath(file.path);
  const html = file.kind === 'html';
  const renderable = markdown || delimited || html;
  const [showRaw, setShowRaw] = useState(false);

  const lineCount = useMemo(() => countLines(file.text), [file.text]);
  const showingCode = showRaw || !renderable;

  const code = (
    <CodeBlock
      text={file.text}
      language={languageFromPath(file.path, file.mimeType)}
      resolvedTheme={resolvedTheme}
    />
  );

  const { directory, name } = splitPathForStrip(file.path);

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* The one status strip (design spec H): 34px, a bottom hairline, and the
          content below it sits on the panel ground — no sub-header, no card. */}
      <div
        data-testid="artifact-status-strip"
        className="flex h-[34px] flex-shrink-0 items-center gap-2.5 border-b border-border-subtle px-3.5"
      >
        <span className={cn(STRIP_LABEL_CLASS, 'shrink-0')}>
          {languageLabel(file.path, file.mimeType)}
        </span>
        <span className={cn(STRIP_IDENT_CLASS, 'min-w-0 truncate')} title={file.path}>
          <span className="text-text-subtle">{directory}</span>
          <span className="text-text-default">{name}</span>
        </span>
        {showingCode && (
          <span className={cn(STRIP_IDENT_CLASS, 'shrink-0 tabular-nums')}>
            {lineCount.toLocaleString()} line{lineCount === 1 ? '' : 's'}
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <CopyButton text={file.text} />
          {renderable && (
            <div className="inline-flex overflow-hidden rounded-md border border-border-subtle">
              {[
                { label: delimited ? 'Table' : 'Preview', raw: false },
                { label: 'Raw', raw: true },
              ].map((option) => (
                <button
                  key={option.label}
                  type="button"
                  onClick={() => setShowRaw(option.raw)}
                  aria-pressed={showRaw === option.raw}
                  className={cn(
                    'px-2 py-0.5 text-[11px] transition-colors',
                    showRaw === option.raw
                      ? 'bg-background-medium text-text-default'
                      : 'text-text-muted hover:text-text-default'
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
      {/* `bg-background-code` when the code view is showing, for the same reason
          MarkdownContent does it: the syntax palette is measured against
          --background-code, but the panel root paints --background-muted, and the
          highlighter renders transparent — so the reader was seeing the palette
          on a surface it was never verified against. In Parchment dark that put
          `comment` at 4.14:1, under AA. Only the code view switches ground; the
          markdown and preview branches keep the panel's own surface. */}
      <div className={cn('min-h-0 flex-1 overflow-auto', showingCode && 'bg-background-code')}>
        {showingCode ? (
          code
        ) : markdown ? (
          <div className="px-4 py-3">
            {/* Anchor relative image/link paths against the FILE's own directory
                (not the app cwd), and let sibling-file links open in this panel. */}
            <MarkdownContent
              content={file.text}
              workingDir={dirnameFromPath(file.path)}
              onOpenArtifact={onOpenArtifact}
            />
          </div>
        ) : html ? (
          // Same sandbox + theme injection as the figure preview above. `allow-popups`
          // is withheld so the framed HTML can't window.open() into a real BrowserWindow
          // that would inherit the preload IPC bridge.
          <iframe
            name="biorouter-artifact-preview"
            aria-label={file.title}
            srcDoc={injectArtifactBrowserCsp(
              withHostTheme(file.preparedHtml ?? file.text, resolvedTheme)
            )}
            sandbox="allow-scripts allow-downloads"
            className="h-full w-full bg-white"
          />
        ) : (
          <DelimitedTable
            text={file.text}
            delimiter={extensionFromPath(file.path) === 'tsv' ? '\t' : ','}
          />
        )}
      </div>
    </div>
  );
}

function DelimitedTable({ text, delimiter }: { text: string; delimiter: string }) {
  const rows = parseDelimitedTable(text, delimiter);
  if (rows.length === 0) {
    return <div className="p-4 text-sm text-text-muted">This file has no rows.</div>;
  }

  const [header, ...body] = rows;
  const shown = body.slice(0, MAX_TABLE_ROWS);
  const hidden = body.length - shown.length;

  return (
    <div className="h-full overflow-auto">
      <table className="w-full border-collapse text-left text-[13px]">
        <thead className="sticky top-0 bg-background-muted">
          <tr>
            {header.map((cell, index) => (
              <th
                key={index}
                className="whitespace-nowrap border-b border-border-subtle px-3 py-2 font-medium text-text-default"
              >
                {cell}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((row, rowIndex) => (
            <tr key={rowIndex} className="even:bg-background-muted/40">
              {header.map((_, cellIndex) => (
                <td
                  key={cellIndex}
                  className="border-b border-border-subtle px-3 py-1.5 text-text-muted"
                >
                  {row[cellIndex] ?? ''}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {hidden > 0 && (
        <div className="px-3 py-2 text-xs text-text-muted">
          {hidden.toLocaleString()} more row{hidden === 1 ? '' : 's'} not shown. Open the raw view
          for the full file.
        </div>
      )}
    </div>
  );
}
