import { UIResourceRenderer } from '@mcp-ui/client';
import { type CSSProperties, type PointerEvent, useEffect, useRef, useState } from 'react';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { oneLight } from 'react-syntax-highlighter/dist/esm/styles/prism';
import { useTheme } from '../../contexts/ThemeContext';
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
import type { ArtifactFilePreview, ArtifactSource, PreparedArtifactHtml } from './artifactTypes';
import { basenameFromPath, languageFromPath } from './artifactUtils';

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

const previewCodeTheme = {
  ...oneLight,
  'pre[class*="language-"]': {
    ...oneLight['pre[class*="language-"]'],
    margin: 0,
    background: 'transparent',
  },
  'code[class*="language-"]': {
    ...oneLight['code[class*="language-"]'],
    background: 'transparent',
    fontFamily: 'var(--font-sans)',
    fontSize: '13px',
    lineHeight: '1.55',
  },
};

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
          const prepared = (await window.electron.prepareArtifactHtml({
            html: response.text,
          })) as PreparedArtifactHtml;
          if (!cancelled) setPreview({ kind: 'html', html: prepared.html });
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
          ? data.payload.message
          : 'This visualization could not be rendered.';
      const detail = typeof data.payload?.detail === 'string' ? data.payload.detail : undefined;
      const href = typeof data.payload?.href === 'string' ? data.payload.href : undefined;
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
      await window.electron.openArtifactWindow({
        html: preview.html,
        title: artifact.title,
        width: artifact.kind === 'html' ? artifact.preferredWidth : undefined,
        height: artifact.kind === 'html' ? artifact.preferredHeight : undefined,
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
        willChange: 'width, flex-basis, transform, opacity',
      }}
      className={cn(
        'no-drag relative isolate flex h-full min-h-0 w-full flex-col overflow-hidden border-l border-border-subtle bg-background-muted/95 backdrop-blur',
        isResizing
          ? 'transition-none'
          : 'transition-[width,flex-basis,opacity,transform] duration-200 ease-out',
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

      <div className="no-drag relative z-50 flex h-12 flex-shrink-0 items-center gap-2 border-b border-border-subtle/35 bg-background-muted/95 px-4">
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
            className="no-drag relative z-50 inline-flex h-7 w-7 items-center justify-center rounded-md text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
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
        <button
          type="button"
          onClick={onClose}
          className="no-drag relative z-50 inline-flex h-7 w-7 items-center justify-center rounded-md text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
          aria-label="Close artifact viewer"
          title="Close"
        >
          <X className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </div>

      <div className="relative z-0 flex min-h-0 flex-1 flex-col p-3">
        <div className="min-h-0 flex-1 overflow-hidden rounded-lg border border-border-subtle bg-background-default shadow-[0_18px_46px_rgba(15,23,42,0.10)]">
          <ArtifactPreviewBody
            preview={preview}
            artifact={artifact}
            onOpenArtifact={onOpenArtifact}
            resolvedTheme={resolvedTheme}
            isResizing={isResizing}
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
}: {
  preview: PreviewState;
  artifact: ArtifactSource;
  onOpenArtifact: (artifact: ArtifactSource) => void;
  resolvedTheme: 'light' | 'dark';
  isResizing: boolean;
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
    return (
      <iframe
        title={artifact.title}
        srcDoc={preview.html}
        sandbox="allow-scripts allow-forms allow-popups allow-modals"
        className={cn('h-full w-full bg-white', isResizing && 'pointer-events-none')}
      />
    );
  }

  if (preview.kind === 'externalUrl') {
    return (
      <iframe
        title={artifact.title}
        src={preview.url}
        sandbox="allow-scripts allow-forms allow-popups allow-modals"
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
          className="max-h-full max-w-full rounded-md object-contain shadow-sm"
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
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 p-5 text-center text-sm text-text-muted">
        <File className="h-8 w-8" aria-hidden="true" />
        <div>{basenameFromPath(file.path)}</div>
        <div>
          {file.mimeType} · {formatBytes(file.size)}
        </div>
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto">
      <SyntaxHighlighter
        style={previewCodeTheme}
        language={languageFromPath(file.path, file.mimeType)}
        PreTag="div"
        customStyle={{
          margin: 0,
          padding: '14px 16px',
          minHeight: '100%',
          background: 'transparent',
        }}
        codeTagProps={{
          style: {
            fontFamily: 'var(--font-sans)',
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
            overflowWrap: 'anywhere',
          },
        }}
      >
        {file.text}
      </SyntaxHighlighter>
    </div>
  );
}
