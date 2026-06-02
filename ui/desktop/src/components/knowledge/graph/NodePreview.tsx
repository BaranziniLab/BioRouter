// ui/desktop/src/components/knowledge/graph/NodePreview.tsx
import { X } from 'lucide-react';
import type { GraphNode } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { usePagePreview } from '../hooks/usePagePreview';
import { nodeFill } from './credColors';

interface Props {
  kbId: string;
  node: GraphNode;
  onClose: () => void;
}

export function NodePreview({ kbId, node, onClose }: Props) {
  const { content, loading, error } = usePagePreview(kbId, node.path);

  return (
    <div className="absolute top-12 right-4 w-[360px] max-h-[calc(100%-5rem)] bg-background-surface border border-border-subtle rounded-lg shadow-lg flex flex-col overflow-hidden z-10">
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
            </div>
          </div>
        </div>
        <Button variant="ghost" size="sm" onClick={onClose} className="flex-shrink-0">
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3 text-xs leading-relaxed font-mono whitespace-pre-wrap text-text-default">
        {loading && <span className="text-text-muted">Loading…</span>}
        {error && <span className="text-red-400">{error}</span>}
        {!loading && !error && (content ?? <span className="text-text-muted">No content.</span>)}
      </div>
      <div className="border-t border-border-subtle px-4 py-2 text-xs text-text-muted">
        {node.path}
      </div>
    </div>
  );
}
