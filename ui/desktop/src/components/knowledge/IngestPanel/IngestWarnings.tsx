import { useState } from 'react';
import { AlertCircle, ChevronDown, ChevronRight, Info, X } from '../../icons/app-icons';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../../ui/collapsible';
import type { FileDropWarning } from './fileValidation';

interface Props {
  warnings: FileDropWarning[];
  onDismiss: (id: string) => void;
  onClear: () => void;
}

export function IngestWarnings({ warnings, onDismiss, onClear }: Props) {
  const [open, setOpen] = useState(false);

  if (warnings.length === 0) {
    return null;
  }

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="biorouter-list-row overflow-hidden">
      <div className="flex items-center justify-between gap-3 px-3 py-2.5">
        <CollapsibleTrigger className="flex min-w-0 flex-1 items-center gap-2 text-left">
          {open ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.5} />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-text-muted" strokeWidth={1.5} />
          )}
          <div className="min-w-0">
            <div className="text-sm font-medium">Curation warnings</div>
            <p className="text-xs text-text-muted">
              {warnings.length} {warnings.length === 1 ? 'item' : 'items'} tucked away until you
              need them.
            </p>
          </div>
        </CollapsibleTrigger>
        <button
          type="button"
          onClick={onClear}
          className="shrink-0 text-[11px] text-text-muted transition-colors hover:text-text-default"
        >
          Clear
        </button>
      </div>

      <CollapsibleContent className="px-3 pb-3">
        <p className="mb-3 text-xs leading-5 text-text-muted">
          Keep readable, high-signal sources. Leave out installers, giant raw exports, and files
          that mostly contain IDs or machine noise.
        </p>

        <div className="flex flex-col gap-2">
          {warnings.map((warning) => {
            const isError = warning.level === 'error';
            return (
              <div
                key={warning.id}
                className={`rounded-lg border px-3 py-2 ${
                  isError
                    ? 'border-border-danger/40 bg-background-danger/10 text-text-danger'
                    : 'border-border-warning/40 bg-background-warning/10 text-text-warning'
                }`}
              >
                <div className="flex items-start gap-2">
                  {isError ? (
                    <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.5} />
                  ) : (
                    <Info className="mt-0.5 h-4 w-4 shrink-0" strokeWidth={1.5} />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="text-xs font-medium">{warning.title}</div>
                    <p className="mt-1 break-words text-[11px] leading-5 opacity-90">
                      {warning.message}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={() => onDismiss(warning.id)}
                    className="shrink-0 opacity-70 transition-opacity hover:opacity-100"
                    aria-label="Dismiss warning"
                  >
                    <X className="h-3.5 w-3.5" strokeWidth={1.5} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
