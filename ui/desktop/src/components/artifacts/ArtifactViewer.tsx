import { UIResourceRenderer } from '@mcp-ui/client';
import { type CSSProperties, type PointerEvent, useEffect, useMemo, useRef, useState } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { useTheme, useThemeFamily } from '../../contexts/ThemeContext';
import { CODE_FONT_FAMILY, codeThemesByFamily } from '../../styles/codeTheme';
import { cn } from '../../utils';
import {
  Code,
  ExternalLink,
  File,
  FileText,
  Folder,
  Globe,
  Image,
  Maximize2,
  X,
} from '../icons/app-icons';
import MarkdownContent from '../MarkdownContent';
import type { ArtifactFilePreview, ArtifactSource, PreparedArtifactHtml } from './artifactTypes';
import {
  basenameFromPath,
  extensionFromPath,
  isDelimitedPath,
  isMarkdownPath,
  languageFromPath,
  languageLabel,
  parseDelimitedTable,
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
  const [preview, setPreview] = useState<PreviewState>({ kind: 'loading' });
  const lastRenderErrorKeyRef = useRef<string | null>(null);
  const trustedFrameRef = useRef<HTMLIFrameElement | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadPreview() {
      if (!artifact) return;
      setPreview({ kind: 'loading' });

      if (artifact.kind === 'html') {
        try {
          const prepared = (await window.electron.prepareArtifactHtml({
            html: artifact.html,
          })) as PreparedArtifactHtml;
          if (!cancelled) setPreview({ kind: 'html', html: prepared.html });
        } catch {
          if (!cancelled) setPreview({ kind: 'html', html: artifact.html });
        }
        return;
      }

      if (artifact.kind === 'externalUrl') {
        setPreview({ kind: 'externalUrl', url: artifact.url });
        return;
      }

      if (artifact.kind === 'mcpResource') {
        setPreview({ kind: 'mcpResource' });
        return;
      }

      try {
        const response = (await window.electron.readArtifactFile(
          artifact.path
        )) as ArtifactFilePreview;
        if (cancelled) return;
        if (response.kind === 'html') {
          // An HTML file the agent wrote gets a Preview/Raw toggle, like markdown:
          // keep it a file preview (so `text` remains the raw source) and attach a
          // security-prepared copy for the rendered Preview. A `ui://` figure
          // resource is `artifact.kind === 'html'` and took the branch above, so it
          // still renders figure-only with no raw source — only real files land here.
          let preparedHtml = response.text;
          try {
            const prepared = (await window.electron.prepareArtifactHtml({
              html: response.text,
            })) as PreparedArtifactHtml;
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
  }, [artifact]);

  useEffect(() => {
    if (!artifact || !onRenderError) return;

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
      const key = `${artifact.title}\n${message}\n${detail ?? ''}`;
      if (lastRenderErrorKeyRef.current === key) return;
      lastRenderErrorKeyRef.current = key;

      onRenderError({
        artifactTitle: artifact.title,
        message,
        detail,
        href,
      });
    };

    window.addEventListener('message', handleMessage);
    return () => window.removeEventListener('message', handleMessage);
  }, [artifact, onRenderError]);

  const Icon = iconForArtifact(artifact);

  if (!artifact) return null;

  const openStandalone = async () => {
    if (preview.kind === 'html') {
      // Expand opens the artifact in the user's default browser (agentic
      // artifacts fall back to a managed window in the main process). The theme
      // is passed so the browser view matches this preview.
      await window.electron.openArtifactInBrowser({
        html: preview.html,
        title: artifact.title,
        theme: resolvedTheme,
      });
      return;
    }
    if (preview.kind === 'externalUrl') {
      await window.electron.openExternal(preview.url);
    }
  };

  return (
    <aside
      data-testid="artifact-viewer"
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

      <div className="no-drag relative z-50 flex h-14 flex-shrink-0 items-center gap-2 border-b border-border-subtle bg-background-muted px-4">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-background-medium/80 text-text-muted">
            <Icon className="h-4 w-4" aria-hidden="true" />
          </div>
          <div className="min-w-0 flex-1">
            <div
              className="truncate text-sm font-medium leading-5 text-text-default"
              title={artifact.title}
            >
              {artifact.title}
            </div>
          </div>
        </div>
        {(preview.kind === 'html' || preview.kind === 'externalUrl') && (
          <button
            type="button"
            onClick={openStandalone}
            className="no-drag relative z-50 inline-flex h-7 w-7 items-center justify-center rounded-md text-text-muted transition-[background-color,color,transform,scale] duration-[var(--motion-fast)] active:scale-[0.97] hover:bg-background-medium hover:text-text-default"
            aria-label="Open artifact outside side viewer"
            title="Open outside side viewer"
          >
            {preview.kind === 'externalUrl' ? (
              <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
            ) : (
              <Maximize2 className="h-3.5 w-3.5" aria-hidden="true" />
            )}
          </button>
        )}
        {/* A file preview (text / binary / directory) is not HTML, so "expand"
            hands it to the OS default app for that type rather than a standalone
            window — the user shouldn't have to hunt for it in Files. */}
        {preview.kind === 'file' && (
          <button
            type="button"
            onClick={() => window.electron.openDirectoryInExplorer(preview.preview.path)}
            className="no-drag relative z-50 inline-flex h-7 w-7 items-center justify-center rounded-md text-text-muted transition-[background-color,color,transform,scale] duration-[var(--motion-fast)] active:scale-[0.97] hover:bg-background-medium hover:text-text-default"
            aria-label="Open file in the default app"
            title="Open in default app"
          >
            <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        )}
        <button
          type="button"
          onClick={onClose}
          className="no-drag relative z-50 inline-flex h-7 w-7 items-center justify-center rounded-md text-text-muted transition-[background-color,color,transform,scale] duration-[var(--motion-fast)] active:scale-[0.97] hover:bg-background-medium hover:text-text-default"
          aria-label="Close artifact viewer"
          title="Close"
        >
          <X className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </div>

      <div className="relative z-0 flex min-h-0 flex-1 flex-col p-3">
        <div className="min-h-0 flex-1 overflow-hidden rounded-lg border border-border-subtle bg-background-default shadow-popover">
          {isResizing && (
            <div
              data-testid="artifact-resize-shield"
              aria-hidden="true"
              className="absolute inset-3 z-50 cursor-col-resize"
            />
          )}
          <ArtifactPreviewBody
            preview={preview}
            artifact={artifact}
            onOpenArtifact={onOpenArtifact}
            resolvedTheme={resolvedTheme}
            isResizing={isResizing}
            trustedFrameRef={trustedFrameRef}
          />
        </div>
      </div>
    </aside>
  );
}

function ArtifactPreviewBody({
  preview,
  artifact,
  onOpenArtifact,
  resolvedTheme,
  isResizing,
  trustedFrameRef,
}: {
  preview: PreviewState;
  artifact: ArtifactSource;
  onOpenArtifact: (artifact: ArtifactSource) => void;
  resolvedTheme: 'light' | 'dark';
  isResizing: boolean;
  trustedFrameRef: React.RefObject<HTMLIFrameElement | null>;
}) {
  if (preview.kind === 'loading') {
    return (
      <div className="flex h-full items-center justify-center text-sm text-text-muted">Loading</div>
    );
  }

  if (preview.kind === 'error') {
    return <div className="p-4 text-sm text-text-muted">{preview.message}</div>;
  }

  if (preview.kind === 'html') {
    // `allow-popups` is withheld: with it, figure HTML can window.open() a
    // `data:` URL, which the main window's open handler turns into a real
    // BrowserWindow that inherits the preload IPC bridge.
    return (
      <iframe
        ref={trustedFrameRef}
        aria-label={artifact.title}
        // Inject the app theme so this preview matches the expanded/opened view,
        // which loads with an explicit `?theme=`; a srcdoc iframe has no query.
        srcDoc={withHostTheme(preview.html, resolvedTheme)}
        sandbox="allow-scripts allow-forms allow-modals"
        className={cn('h-full w-full bg-white', isResizing && 'pointer-events-none')}
      />
    );
  }

  if (preview.kind === 'externalUrl') {
    return (
      <iframe
        aria-label={artifact.title}
        src={preview.url}
        sandbox="allow-scripts allow-forms allow-modals"
        className={cn('h-full w-full bg-white', isResizing && 'pointer-events-none')}
      />
    );
  }

  if (preview.kind === 'mcpResource' && artifact.kind === 'mcpResource') {
    return (
      <UIResourceRenderer
        resource={artifact.resource}
        supportedContentTypes={['rawHtml', 'externalUrl']}
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
    return <div className="p-4 text-sm text-text-muted">{file.error}</div>;
  }

  if (file.kind === 'image') {
    return (
      <div className="flex h-full items-center justify-center overflow-auto bg-background-medium p-4">
        <img
          src={file.dataUrl}
          alt={file.title}
          className="max-h-full max-w-full rounded-md object-contain"
        />
      </div>
    );
  }

  if (file.kind === 'directory') {
    return (
      <div className="h-full overflow-auto p-2">
        {file.entries.map((entry) => {
          const EntryIcon = entry.isDirectory ? Folder : FileText;
          return (
            <button
              type="button"
              key={entry.path}
              className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors hover:bg-background-medium"
              onClick={() =>
                onOpenArtifact({
                  kind: 'file',
                  title: entry.name,
                  path: entry.path,
                })
              }
            >
              <EntryIcon className="h-4 w-4 shrink-0 text-text-muted" aria-hidden="true" />
              <span className="min-w-0 flex-1 truncate text-text-default">{entry.name}</span>
              {!entry.isDirectory && (
                <span className="shrink-0 text-xs text-text-muted">{formatBytes(entry.size)}</span>
              )}
            </button>
          );
        })}
      </div>
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
  return <TextFilePreview key={file.path} file={file} resolvedTheme={resolvedTheme} />;
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
      className="rounded px-2 py-0.5 text-xs text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
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
}: {
  file: Extract<ArtifactFilePreview, { kind: 'text' | 'html' }>;
  resolvedTheme: 'light' | 'dark';
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

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-shrink-0 items-center gap-2 border-b border-border-subtle px-2.5 py-1.5">
        <span className="text-[10.5px] font-medium uppercase tracking-wider text-text-muted">
          {languageLabel(file.path, file.mimeType)}
        </span>
        {showingCode && (
          <span className="text-[11px] tabular-nums text-text-muted/70">
            {lineCount.toLocaleString()} line{lineCount === 1 ? '' : 's'}
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <CopyButton text={file.text} />
          {renderable && (
            <div className="inline-flex rounded-md border border-border-subtle p-0.5 text-xs">
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
                    'rounded px-2 py-0.5 transition-colors',
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
      <div className="min-h-0 flex-1 overflow-auto">
        {showingCode ? (
          code
        ) : markdown ? (
          <div className="px-4 py-3">
            <MarkdownContent content={file.text} />
          </div>
        ) : html ? (
          // Same sandbox + theme injection as the figure preview above. `allow-popups`
          // is withheld so the framed HTML can't window.open() into a real BrowserWindow
          // that would inherit the preload IPC bridge.
          <iframe
            aria-label={file.title}
            srcDoc={withHostTheme(file.preparedHtml ?? file.text, resolvedTheme)}
            sandbox="allow-scripts allow-forms allow-modals"
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
