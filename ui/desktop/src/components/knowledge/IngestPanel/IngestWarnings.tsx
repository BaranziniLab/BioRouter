import { useState } from 'react';
import { AlertCircle, ChevronDown, ChevronRight, Info, X } from '../../icons/app-icons';
import { Button } from '../../ui/button';
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
    // The nesting ladder runs DOWNWARD: a 12px container holding 8px cards.
    // Before this the panel was a `.biorouter-list-row` — a list ROW used as a
    // card — with 12px cards inside it, so the ladder ran upward.
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="overflow-hidden rounded-container border border-border-subtle"
    >
      <div className="flex items-center justify-between gap-3 px-3 py-2.5">
        <CollapsibleTrigger className="flex min-w-0 flex-1 items-center gap-2 text-left">
          {open ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-text-muted" />
          )}
          <div className="min-w-0">
            <div className="text-label">Curation warnings</div>
            <p className="text-supporting text-text-muted">
              {warnings.length} {warnings.length === 1 ? 'item' : 'items'} tucked away until you
              need them.
            </p>
          </div>
        </CollapsibleTrigger>
        <Button type="button" variant="ghost" size="sm" className="shrink-0" onClick={onClear}>
          Clear
        </Button>
      </div>

      <CollapsibleContent className="px-3 pb-3">
        <p className="mb-3 text-supporting text-text-muted">
          Keep readable, high-signal sources. Leave out installers, giant raw exports, and files
          that mostly contain IDs or machine noise.
        </p>

        <div className="flex flex-col gap-2">
          {warnings.map((warning) => {
            const isError = warning.level === 'error';
            return (
              <div
                key={warning.id}
                className={`rounded-element border px-3 py-2 ${
                  isError
                    ? 'border-border-danger/40 bg-background-danger/10 text-text-danger'
                    : 'border-border-warning/40 bg-background-warning/10 text-text-warning'
                }`}
              >
                <div className="flex items-start gap-2">
                  {isError ? (
                    <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                  ) : (
                    <Info className="mt-0.5 h-4 w-4 shrink-0" />
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="text-label">{warning.title}</div>
                    {/* `text-text-muted`, not `opacity-90`: an alpha on ink is a
                        colour nobody can name, and it fades the message out of
                        the token system entirely. */}
                    <p className="mt-1 break-words text-supporting text-text-muted">
                      {warning.message}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    shape="round"
                    size="xs"
                    onClick={() => onDismiss(warning.id)}
                    aria-label={`Dismiss "${warning.title}"`}
                  >
                    <X aria-hidden="true" />
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
