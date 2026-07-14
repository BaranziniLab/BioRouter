/**
 * BR-63: the "what will this actually do?" half of the tool-confirmation card.
 *
 * The backend (`conversation::tool_preview`) resolves a pending call into one of
 * four shapes — a shell command, a file diff, a new file, or a bag of arguments.
 * This renders them. Before BR-63 the card showed the tool's *name* and nothing
 * else, so "Allow?" was asked with no way to tell `ls` from `rm -rf`.
 *
 * Long previews collapse to a fixed window with a "Show all" toggle so a 200-line
 * diff cannot push the buttons off screen — the decision must always stay in reach.
 */
import { useId, useState } from 'react';
import { ChevronDown, ChevronUp, Code, FileText, Info, Terminal } from './icons/app-icons';
import type { ActionRequired, ToolPreview, ToolPreviewLine, ToolRisk } from '../api';
import { Button } from './ui/button';

type ToolConfirmationData = Extract<ActionRequired['data'], { actionType: 'toolConfirmation' }>;
export type ToolConfirmationPreview = NonNullable<ToolConfirmationData['preview']>;

/** Lines shown before the preview collapses behind a "Show all" toggle. */
const COLLAPSED_LINES = 12;

const RISK_STYLES: Record<ToolRisk, { label: string; className: string }> = {
  low: {
    label: 'Read-only',
    className: 'bg-background-success/10 text-text-success',
  },
  medium: {
    label: 'Modifies data',
    className: 'bg-background-warning/10 text-text-warning',
  },
  high: {
    label: 'Destructive',
    className: 'bg-background-danger/10 text-text-danger',
  },
  unknown: {
    label: 'Unverified',
    className: 'bg-background-muted text-text-muted',
  },
};

/**
 * The BR-18 risk grade, in words. "Destructive" is a far more useful thing to put
 * in front of someone about to click "Always Allow" than the tool's name.
 */
export function ToolRiskBadge({ risk }: { risk: ToolRisk }) {
  const style = RISK_STYLES[risk] ?? RISK_STYLES.unknown;
  return (
    <span
      data-testid="tool-risk-badge"
      className={`rounded-full px-2 py-0.5 text-xs font-medium ${style.className}`}
    >
      {style.label}
    </span>
  );
}

function TruncationNote() {
  return (
    <div className="flex items-center gap-1.5 border-t border-border-subtle px-3 py-1.5 text-xs text-text-muted">
      <Info className="h-3.5 w-3.5 shrink-0" />
      <span>Preview truncated — the full call is larger than shown.</span>
    </div>
  );
}

/** Shared chrome: a titled, bordered box that can collapse its body. */
function PreviewFrame({
  icon,
  title,
  meta,
  truncated,
  collapsible,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  meta?: React.ReactNode;
  truncated: boolean;
  collapsible: boolean;
  children: React.ReactNode;
}) {
  const [expanded, setExpanded] = useState(false);
  const bodyId = useId();
  const showToggle = collapsible && !expanded;

  return (
    <div className="overflow-hidden rounded-lg border border-border-subtle bg-background-muted">
      <div className="flex items-center gap-2 border-b border-border-subtle px-3 py-1.5 text-xs text-text-muted">
        <span className="shrink-0">{icon}</span>
        <span className="truncate font-mono" title={title}>
          {title}
        </span>
        {meta && <span className="ml-auto shrink-0">{meta}</span>}
      </div>

      <div
        id={bodyId}
        data-testid="tool-preview-body"
        // A collapsed preview is clipped, not unmounted: the toggle then only
        // changes height, and screen readers still see the whole call.
        className={`overflow-x-auto ${collapsible && !expanded ? 'max-h-52 overflow-y-hidden' : ''}`}
      >
        {children}
      </div>

      {collapsible && (
        <Button
          type="button"
          variant="ghost"
          size="xs"
          shape="pill"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
          aria-controls={bodyId}
          className="flex w-full items-center justify-center gap-1 border-t border-border-subtle py-1.5 text-xs text-text-muted transition-colors hover:text-text-default"
        >
          {showToggle ? (
            <>
              Show all <ChevronDown className="h-3.5 w-3.5" />
            </>
          ) : (
            <>
              Show less <ChevronUp className="h-3.5 w-3.5" />
            </>
          )}
        </Button>
      )}

      {truncated && <TruncationNote />}
    </div>
  );
}

const LINE_STYLES: Record<ToolPreviewLine['kind'], { marker: string; className: string }> = {
  added: { marker: '+', className: 'bg-background-success/10 text-text-success' },
  removed: { marker: '-', className: 'bg-background-danger/10 text-text-danger' },
  context: { marker: ' ', className: 'text-text-muted' },
};

function DiffLines({ lines }: { lines: ToolPreviewLine[] }) {
  return (
    <pre className="py-1 font-mono text-xs leading-relaxed">
      {lines.map((line, i) => {
        const style = LINE_STYLES[line.kind] ?? LINE_STYLES.context;
        return (
          <div key={i} data-testid={`diff-line-${line.kind}`} className={`px-3 ${style.className}`}>
            <span aria-hidden="true" className="select-none opacity-60">
              {style.marker}{' '}
            </span>
            {line.text}
          </div>
        );
      })}
    </pre>
  );
}

function CodeBlock({ text }: { text: string }) {
  return (
    <pre className="whitespace-pre px-3 py-2 font-mono text-xs leading-relaxed text-text-default">
      {text}
    </pre>
  );
}

/** Render whatever the backend resolved this call into. */
export function ToolCallPreview({ preview }: { preview: ToolPreview }) {
  switch (preview.kind) {
    case 'shell':
      return (
        <PreviewFrame
          icon={<Terminal className="h-3.5 w-3.5" />}
          title="Command"
          truncated={preview.truncated}
          collapsible={countLines(preview.command) > COLLAPSED_LINES}
        >
          <CodeBlock text={preview.command} />
        </PreviewFrame>
      );

    case 'fileEdit':
      return (
        <PreviewFrame
          icon={<FileText className="h-3.5 w-3.5" />}
          title={preview.path}
          meta={
            <span className="font-mono">
              <span className="text-text-success">+{preview.added}</span>{' '}
              <span className="text-text-danger">-{preview.removed}</span>
            </span>
          }
          truncated={preview.truncated}
          collapsible={preview.lines.length > COLLAPSED_LINES}
        >
          <DiffLines lines={preview.lines} />
        </PreviewFrame>
      );

    case 'fileWrite':
      return (
        <PreviewFrame
          icon={<FileText className="h-3.5 w-3.5" />}
          title={preview.path}
          meta={
            <span className="font-mono text-text-success">
              new file · {preview.lineCount} {preview.lineCount === 1 ? 'line' : 'lines'}
            </span>
          }
          truncated={preview.truncated}
          collapsible={preview.lineCount > COLLAPSED_LINES}
        >
          <CodeBlock text={preview.content} />
        </PreviewFrame>
      );

    case 'arguments':
      return (
        <PreviewFrame
          icon={<Code className="h-3.5 w-3.5" />}
          title="Arguments"
          truncated={preview.truncated}
          collapsible={countLines(preview.json) > COLLAPSED_LINES}
        >
          <CodeBlock text={preview.json} />
        </PreviewFrame>
      );

    default:
      // An older backend, or a preview kind this build does not know yet.
      // Showing nothing is correct — never invent a preview.
      return null;
  }
}

function countLines(text: string): number {
  return text.split('\n').length;
}
