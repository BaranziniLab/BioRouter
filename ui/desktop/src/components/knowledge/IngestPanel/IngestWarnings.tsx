import { AlertCircle, Info, X } from 'lucide-react';
import type { FileDropWarning } from './fileValidation';

interface Props {
  warnings: FileDropWarning[];
  onDismiss: (id: string) => void;
  onClear: () => void;
}

export function IngestWarnings({ warnings, onDismiss, onClear }: Props) {
  if (warnings.length === 0) {
    return null;
  }

  return (
    <div className="rounded-xl border border-border-subtle bg-background-surface p-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-sm font-medium">Curation guidance</div>
          <p className="mt-1 text-xs text-text-muted">
            Keep readable, high-signal sources. Leave out installers, archives, giant raw exports,
            and files that mostly contain IDs or machine noise.
          </p>
        </div>
        <button
          type="button"
          onClick={onClear}
          className="text-[11px] text-text-muted transition-colors hover:text-text-default"
        >
          Clear
        </button>
      </div>

      <div className="mt-3 flex flex-col gap-2">
        {warnings.map((warning) => {
          const isError = warning.level === 'error';
          return (
            <div
              key={warning.id}
              className={`rounded-lg border px-3 py-2 ${
                isError
                  ? 'border-red-200/70 bg-red-50/70 text-red-900 dark:border-red-900/40 dark:bg-red-950/30 dark:text-red-100'
                  : 'border-amber-200/70 bg-amber-50/70 text-amber-900 dark:border-amber-900/40 dark:bg-amber-950/30 dark:text-amber-100'
              }`}
            >
              <div className="flex items-start gap-2">
                {isError ? (
                  <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                ) : (
                  <Info className="mt-0.5 h-4 w-4 shrink-0" />
                )}
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-medium">{warning.title}</div>
                  <p className="mt-1 text-[11px] leading-5 opacity-90">{warning.message}</p>
                </div>
                <button
                  type="button"
                  onClick={() => onDismiss(warning.id)}
                  className="shrink-0 opacity-70 transition-opacity hover:opacity-100"
                  aria-label="Dismiss warning"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
