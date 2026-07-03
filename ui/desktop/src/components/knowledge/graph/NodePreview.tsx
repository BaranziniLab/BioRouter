// ui/desktop/src/components/knowledge/graph/NodePreview.tsx
import { useMemo } from 'react';
import { FileCode2, X } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import MarkdownContent from '../../MarkdownContent';
import { Button } from '../../ui/button';
import { usePagePreview } from '../hooks/usePagePreview';
import { nodeFill } from './credColors';
import { prettyLabel } from './labelText';

interface Props {
  kbId: string;
  node: GraphNode;
  previewSha?: string | null;
  onClose: () => void;
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

export function NodePreview({ kbId, node, previewSha, onClose }: Props) {
  const { content, loading, error } = usePagePreview(kbId, node.path, previewSha);
  const parsed = useMemo(() => splitFrontmatter(content ?? ''), [content]);

  return (
    <div className="biorouter-popover-surface absolute top-12 right-4 z-10 flex max-h-[calc(100%-5rem)] w-[360px] flex-col overflow-hidden rounded-2xl bg-background-default/90 backdrop-blur-md">
      <div className="flex items-center justify-between border-b border-border-default bg-background-muted/55 px-4 py-3">
        <div className="flex items-center gap-2 min-w-0">
          <span
            aria-hidden
            className="w-2.5 h-2.5 rounded-full flex-shrink-0"
            style={{ background: nodeFill(node) }}
          />
          <div className="flex flex-col min-w-0">
            <div className="text-sm font-medium truncate">{prettyLabel(node.label, node.kind)}</div>
            <div className="text-xs text-text-muted truncate">
              {node.kind}
              {node.credibility_tier ? ` · ${node.credibility_tier.replace('_', ' ')}` : ''}
              {previewSha ? ` · ${previewSha.slice(0, 7)}` : ''}
            </div>
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose} className="flex-shrink-0">
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 text-xs leading-relaxed text-text-default">
        {loading && <span className="text-text-muted">Loading…</span>}
        {error && <span className="text-text-danger">{error}</span>}
        {!loading && !error && content && (
          <div className="space-y-4">
            {parsed.frontmatter && (
              <div className="overflow-hidden rounded-xl bg-background-muted/55">
                <div className="flex items-center gap-2 px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">
                  <FileCode2 className="h-3 w-3" />
                  Overview
                </div>
                <pre className="overflow-x-auto px-3 py-2.5 font-mono text-[11px] leading-5 text-text-default/80 whitespace-pre-wrap">
                  {parsed.frontmatter}
                </pre>
              </div>
            )}
            {parsed.body ? (
              <MarkdownContent content={parsed.body} className="text-sm" />
            ) : (
              <span className="text-text-muted">No body content.</span>
            )}
          </div>
        )}
        {!loading && !error && !content && <span className="text-text-muted">No content.</span>}
      </div>
      <div className="bg-background-muted/45 px-4 py-2 text-xs text-text-muted">{node.path}</div>
    </div>
  );
}
