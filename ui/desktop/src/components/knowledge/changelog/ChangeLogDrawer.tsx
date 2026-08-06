import { useMemo, useState } from 'react';
import type { ChangeKind, HistoryEntry } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import { Sheet, SheetContent, SheetHeader, SheetTitle } from '../../ui/sheet';
import { useKnowledge } from '../KnowledgeContext';
import { useHistory } from '../hooks/useHistory';
import { ChangeKindChip } from './ChangeKindChip';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import { toastError } from '../../../toasts';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPreview: (sha: string) => void;
  onRestored: () => void;
}

const ALL_KINDS: ChangeKind[] = ['ingest', 'link', 'flag', 'query', 'lint', 'restore', 'manual'];

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
  const { primaryKbId, triggerGraphRefresh } = useKnowledge();
  const { history, loading, error, restore } = useHistory(primaryKbId);
  const [activeKinds, setActiveKinds] = useState<Set<ChangeKind>>(new Set(ALL_KINDS));
  const [restoring, setRestoring] = useState<string | null>(null);
  const [entryToRestore, setEntryToRestore] = useState<HistoryEntry | null>(null);

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

  async function confirmRestore() {
    const entry = entryToRestore;
    if (!entry || restoring) return;
    setRestoring(entry.commit_sha);
    try {
      await restore(entry.commit_sha);
      triggerGraphRefresh();
      onRestored();
    } catch (err) {
      toastError({
        title: 'Restore failed',
        msg: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setRestoring(null);
      setEntryToRestore(null);
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[420px] sm:max-w-[420px] flex flex-col p-0">
        <SheetHeader className="px-5 py-3 border-b border-border-subtle flex-row items-center justify-between">
          <SheetTitle className="text-label">Change log</SheetTitle>
        </SheetHeader>

        <div className="px-5 py-2 border-b border-border-subtle flex flex-wrap gap-1.5">
          {ALL_KINDS.map((k) => (
            <button
              key={k}
              onClick={() => toggleKind(k)}
              className={`rounded-inner px-1.5 py-0.5 text-caps uppercase transition-colors ${
                activeKinds.has(k)
                  ? 'bg-overlay-selected text-text-default'
                  : 'text-text-muted hover:text-text-default'
              }`}
            >
              {k}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto">
          {loading && <div className="p-5 text-supporting text-text-muted">Loading…</div>}
          {error && <div className="p-5 text-supporting text-text-danger">{error}</div>}
          {!loading && !error && filtered.length === 0 && (
            <div className="p-5 text-supporting text-text-muted">No history entries match.</div>
          )}
          {!loading &&
            !error &&
            filtered.map((entry) => (
              <div
                key={entry.commit_sha}
                className="px-5 py-3 border-b border-border-subtle transition-colors hover:bg-overlay-hover"
              >
                <div className="flex items-center gap-2 mb-1">
                  <ChangeKindChip kind={entry.kind} />
                  <span className="text-supporting text-text-muted font-mono">
                    {entry.commit_sha.slice(0, 7)}
                  </span>
                  <span className="text-supporting text-text-muted ml-auto">
                    {relativeTime(entry.timestamp)}
                  </span>
                </div>
                <div className="text-body text-text-default mb-2">{entry.summary}</div>
                <div className="flex items-center gap-2">
                  <Button variant="ghost" size="xs" onClick={() => onPreview(entry.commit_sha)}>
                    Preview
                  </Button>
                  <Button
                    variant="outline"
                    size="xs"
                    onClick={() => setEntryToRestore(entry)}
                    disabled={restoring !== null}
                  >
                    {restoring === entry.commit_sha ? 'Restoring…' : 'Restore'}
                  </Button>
                </div>
              </div>
            ))}
        </div>
      </SheetContent>
      <ConfirmationModal
        isOpen={entryToRestore !== null}
        title="Restore knowledge base?"
        message={`Restore to ${entryToRestore?.commit_sha.slice(0, 7) ?? ''}? A new revert commit will be created.`}
        confirmLabel="Restore"
        cancelLabel="Cancel"
        isSubmitting={restoring !== null}
        onConfirm={() => void confirmRestore()}
        onCancel={() => setEntryToRestore(null)}
      />
    </Sheet>
  );
}
