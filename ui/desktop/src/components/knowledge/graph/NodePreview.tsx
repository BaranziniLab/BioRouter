// ui/desktop/src/components/knowledge/graph/NodePreview.tsx
import { useEffect, useMemo, useRef } from 'react';
import { FileCode2, X } from '../../icons/app-icons';
import type { GraphNode } from '../../../api/types.gen';
import type { GraphMode } from '../../../styles/graphPalette';
import MarkdownContent from '../../MarkdownContent';
import { Button } from '../../ui/button';
import { usePagePreview } from '../hooks/usePagePreview';
import { GraphShapeGlyph } from './GraphShapeGlyph';
import { fillFor, shapeFor } from './nodeMark';
import { prettyLabel } from './labelText';

interface Props {
  kbId: string;
  node: GraphNode;
  /**
   * The resolved light/dark mode, passed in rather than read here so the
   * inspector and the canvas it describes resolve the palette from the same
   * value on the same commit — the facet strip and the legend take it the same
   * way, from `KnowledgeGraphPanel`.
   */
  mode: GraphMode;
  previewSha?: string | null;
  onClose: () => void;
}

/**
 * The node's display identity (ui-spec §5.8).
 *
 * `identifier` first, `label` second — the same order `buildGraphModel` uses for
 * the canvas label. The two used to disagree: the inspector's title showed the
 * slug (`metformin`) while the frontmatter block one line below it showed the
 * `identifier` (`Metformin`), so the panel contradicted itself inside 40px.
 */
export function nodeTitle(node: GraphNode): string {
  return prettyLabel(node.identifier || node.label, node.kind);
}

/**
 * The line under the title.
 *
 * ⚠ **It reads `node_type`, and only falls back to `kind` when there is none.**
 * The deriver writes `kind: 'hub'` onto every typed page, so a subtitle written
 * against `kind` said "hub" for every node in an OKF base — beside a canvas that
 * was correctly drawing the same node as a `Drug`. An untyped page is labelled
 * `Untyped` and then by its legacy `kind`, because DR-28 makes absence a real
 * state that has to be *shown*, not filled in.
 */
export function nodeSubtitle(node: GraphNode, previewSha?: string | null): string {
  const parts: string[] = [node.node_type ?? `Untyped · ${node.kind}`];
  if (node.subtype) parts.push(node.subtype);
  if (node.status) parts.push(node.status);
  if (node.credibility_tier) parts.push(node.credibility_tier.replace(/_/g, ' '));
  if (node.retracted) parts.push('retracted');
  if (previewSha) parts.push(previewSha.slice(0, 7));
  return parts.join(' · ');
}

function splitFrontmatter(content: string): { frontmatter: string | null; body: string } {
  if (!content.startsWith('---\n')) {
    return { frontmatter: null, body: content };
  }

  const end = content.indexOf('\n---\n', 4);
  if (end === -1) {
    return { frontmatter: null, body: content };
  }

  return {
    frontmatter: content.slice(4, end).trim(),
    body: content.slice(end + 5).trimStart(),
  };
}

export function NodePreview({ kbId, node, mode, previewSha, onClose }: Props) {
  const { content, loading, error } = usePagePreview(kbId, node.path, previewSha);
  const parsed = useMemo(() => splitFrontmatter(content ?? ''), [content]);
  const panelRef = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);

  useEffect(() => {
    onCloseRef.current = onClose;
  }, [onClose]);

  useEffect(() => {
    const handlePointerDown = (event: globalThis.PointerEvent) => {
      if (!panelRef.current?.contains(event.target as Node)) onCloseRef.current();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCloseRef.current();
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, []);

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-label={`Preview ${nodeTitle(node)}`}
      className="absolute top-12 right-4 z-[var(--z-dropdown)] flex max-h-[calc(100%-5rem)] w-[min(360px,calc(100%-2rem))] flex-col overflow-hidden rounded-container border border-border-subtle bg-background-default shadow-popover"
    >
      <div className="flex items-center justify-between border-b border-border-subtle bg-background-muted px-4 py-3">
        <div className="flex items-center gap-2 min-w-0">
          {/* The SAME mark the canvas paints, from the same function — fill by
              `node_type` through `GRAPH_PALETTE`, silhouette by family. A swatch
              that disagrees with the mark it describes is worse than none. */}
          <GraphShapeGlyph
            shape={shapeFor(node, mode)}
            fill={fillFor(node, mode)}
            size={14}
            className="br-swatch-ring flex-shrink-0"
          />
          <div className="flex flex-col min-w-0">
            <div className="text-label truncate">{nodeTitle(node)}</div>
            <div className="text-supporting text-text-muted truncate">
              {nodeSubtitle(node, previewSha)}
            </div>
          </div>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onClose}
          className="flex-shrink-0"
          aria-label="Close preview"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 text-body text-text-default">
        {loading && <span className="text-text-muted">Loading…</span>}
        {error && <span className="text-text-danger">{error}</span>}
        {!loading && !error && content && (
          <div className="space-y-4">
            {parsed.frontmatter && (
              <div className="overflow-hidden rounded-element bg-background-muted">
                <div className="flex items-center gap-2 px-3 py-1.5 text-caps text-text-muted">
                  <FileCode2 className="h-3 w-3" />
                  Overview
                </div>
                <pre className="overflow-x-auto px-3 py-2.5 font-mono text-code text-text-muted whitespace-pre-wrap">
                  {parsed.frontmatter}
                </pre>
              </div>
            )}
            {parsed.body ? (
              <MarkdownContent content={parsed.body} className="text-body" />
            ) : (
              <span className="text-text-muted">No body content.</span>
            )}
          </div>
        )}
        {!loading && !error && !content && <span className="text-text-muted">No content.</span>}
      </div>
      <div className="border-t border-border-subtle bg-background-muted px-4 py-2 text-supporting text-text-muted break-all">
        {node.path}
      </div>
    </div>
  );
}
