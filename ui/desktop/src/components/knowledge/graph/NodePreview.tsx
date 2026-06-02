// ui/desktop/src/components/knowledge/graph/NodePreview.tsx
import { useMemo } from 'react';
import { FileCode2, X } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import MarkdownContent from '../../MarkdownContent';
import { Button } from '../../ui/button';
import { usePagePreview } from '../hooks/usePagePreview';
import { nodeFill } from './credColors';

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
  const parsed = useMemo(
    () => splitFrontmatter(content ?? ''),
    [content]
  );

  return (
    <div className="absolute top-12 right-4 z-10 flex max-h-[calc(100%-5rem)] w-[360px] flex-col overflow-hidden rounded-2xl border border-border-subtle bg-background-default/90 shadow-2xl backdrop-blur-md">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border-subtle">
        <div className="flex items-center gap-2 min-w-0">
          <span
            aria-hidden
            className="w-2.5 h-2.5 rounded-full flex-shrink-0"
            style={{ background: nodeFill(node) }}
          />
          <div className="flex flex-col min-w-0">
            <div className="text-sm font-medium truncate">{node.label}</div>
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
        {error && <span className="text-red-400">{error}</span>}
        {!loading && !error && content && (
          <div className="space-y-4">
            {parsed.frontmatter && (
              <div className="overflow-hidden rounded-2xl border border-amber-200/50 bg-amber-50/70 shadow-sm">
                <div className="flex items-center gap-2 border-b border-amber-200/60 px-3 py-2 text-[10px] font-medium uppercase tracking-[0.18em] text-amber-900/80">
                  <FileCode2 className="h-3.5 w-3.5" />
                  Overview
                </div>
                <pre className="overflow-x-auto px-3 py-3 font-mono text-[11px] leading-5 text-amber-950/90 whitespace-pre-wrap">
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
      <div className="border-t border-border-subtle px-4 py-2 text-xs text-text-muted">
        {node.path}
      </div>
    </div>
  );
}
