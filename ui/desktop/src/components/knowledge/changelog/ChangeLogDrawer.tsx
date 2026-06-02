import { useMemo, useState } from 'react';
import type { ChangeKind, HistoryEntry } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '../../ui/sheet';
import { useKnowledge } from '../KnowledgeContext';
import { useHistory } from '../hooks/useHistory';
import { ChangeKindChip } from './ChangeKindChip';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPreview: (sha: string) => void;
  onRestored: () => void;
}

const ALL_KINDS: ChangeKind[] = [
  'ingest',
  'link',
  'flag',
  'query',
  'lint',
  'restore',
  'manual',
];

function relativeTime(iso: string): string {
  const t = new Date(iso).getTime();
  const diff = Date.now() - t;
  const s = Math.floor(diff / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d ago`;
  return new Date(iso).toLocaleDateString();
}

export function ChangeLogDrawer({ open, onOpenChange, onPreview, onRestored }: Props) {
  const { activeKbId, triggerGraphRefresh } = useKnowledge();
  const { history, loading, error, restore } = useHistory(activeKbId);
  const [activeKinds, setActiveKinds] = useState<Set<ChangeKind>>(new Set(ALL_KINDS));
  const [restoring, setRestoring] = useState<string | null>(null);

  const filtered = useMemo(
    () => history.filter((h) => activeKinds.has(h.kind)),
    [history, activeKinds]
  );

  function toggleKind(k: ChangeKind) {
    setActiveKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }

  async function handleRestore(entry: HistoryEntry) {
    if (
      !window.confirm(
        `Restore knowledge base to ${entry.commit_sha.slice(0, 7)}? A new revert commit will be created.`
      )
    ) {
      return;
    }
    setRestoring(entry.commit_sha);
    try {
      await restore(entry.commit_sha);
      triggerGraphRefresh();
      onRestored();
    } catch (err) {
      window.alert(`Restore failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setRestoring(null);
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[420px] sm:max-w-[420px] flex flex-col p-0">
        <SheetHeader className="px-5 py-3 border-b border-border-subtle flex-row items-center justify-between">
          <SheetTitle className="text-sm">Change log</SheetTitle>
        </SheetHeader>

        <div className="px-5 py-2 border-b border-border-subtle flex flex-wrap gap-1.5">
          {ALL_KINDS.map((k) => (
            <button
              key={k}
              onClick={() => toggleKind(k)}
              className={`text-[10px] uppercase tracking-wide rounded px-1.5 py-0.5 border ${
                activeKinds.has(k)
                  ? 'border-border-default text-text-default'
                  : 'border-border-subtle text-text-muted opacity-50'
              }`}
            >
              {k}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto">
          {loading && <div className="p-5 text-xs text-text-muted">Loading…</div>}
          {error && <div className="p-5 text-xs text-red-400">{error}</div>}
          {!loading && !error && filtered.length === 0 && (
            <div className="p-5 text-xs text-text-muted">No history entries match.</div>
          )}
          {!loading &&
            !error &&
            filtered.map((entry) => (
              <div
                key={entry.commit_sha}
                className="px-5 py-3 border-b border-border-subtle hover:bg-background-muted/40"
              >
                <div className="flex items-center gap-2 mb-1">
                  <ChangeKindChip kind={entry.kind} />
                  <span className="text-[10px] text-text-muted font-mono">
                    {entry.commit_sha.slice(0, 7)}
                  </span>
                  <span className="text-[10px] text-text-muted ml-auto">
                    {relativeTime(entry.timestamp)}
                  </span>
                </div>
                <div className="text-xs text-text-default mb-2">{entry.summary}</div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => onPreview(entry.commit_sha)}
                    className="text-[11px] h-6 px-2"
                  >
                    Preview
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleRestore(entry)}
                    disabled={restoring !== null}
                    className="text-[11px] h-6 px-2"
                  >
                    {restoring === entry.commit_sha ? 'Restoring…' : 'Restore'}
                  </Button>
                </div>
              </div>
            ))}
        </div>
      </SheetContent>
    </Sheet>
  );
}
