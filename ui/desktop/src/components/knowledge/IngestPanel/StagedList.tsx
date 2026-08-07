import { ClipboardList, FileText, Globe, X } from '../../icons/app-icons';
import type { StagedSource } from '../hooks/useStagedSources';

interface Props {
  items: StagedSource[];
  onRemove: (id: string) => void;
  onClear: () => void;
}

export function StagedList({ items, onRemove, onClear }: Props) {
  if (items.length === 0) {
    return (
      <div className="text-supporting text-text-muted py-3 text-center">Nothing staged yet.</div>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between mb-1">
        <span className="text-caps uppercase text-text-muted">Staged · {items.length}</span>
        <button onClick={onClear} className="text-label text-text-muted hover:text-text-default">
          clear all
        </button>
      </div>
      {items.map((s) => {
        const Icon =
          s.kind === 'file' || s.kind === 'path'
            ? FileText
            : s.kind === 'url'
              ? Globe
              : ClipboardList;
        const label =
          s.kind === 'file'
            ? s.label || s.file.name
            : s.kind === 'path'
              ? s.label
              : s.kind === 'url'
                ? s.url
                : s.title || s.text.substring(0, 60);

        return (
          <div
            key={s.id}
            data-testid="knowledge-staged-item"
            data-label={label}
            className="biorouter-list-row flex items-center gap-2 px-3 py-2"
          >
            <Icon className="w-3.5 h-3.5 text-text-muted flex-shrink-0" />
            <div className="flex-1 min-w-0">
              <div className="text-label truncate">{label}</div>
              {s.status !== 'pending' && (
                <div className="text-supporting font-mono text-text-muted">
                  {s.status}
                  {s.error ? `: ${s.error}` : ''}
                </div>
              )}
            </div>
            <button
              onClick={() => onRemove(s.id)}
              className="text-text-muted hover:text-text-default"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
