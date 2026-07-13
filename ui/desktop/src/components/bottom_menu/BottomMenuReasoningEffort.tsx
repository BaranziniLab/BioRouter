import { useSyncExternalStore } from 'react';
import { useState } from 'react';
import { Check, Gauge } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';
import {
  DEFAULT_REASONING_EFFORT,
  getReasoningEffort,
  REASONING_EFFORT_DESCRIPTIONS,
  REASONING_EFFORT_LABELS,
  REASONING_EFFORTS,
  ReasoningEffort,
  setReasoningEffort,
  subscribeToReasoningEffort,
} from '../../store/reasoningEffort';

/**
 * BR-63: the composer's reasoning-effort control — the explore-vs-answer knob.
 *
 * The picked level rides on the next chat request (`reasoning_effort`), where it
 * maps to the provider's reasoning effort / thinking budget and to the agent
 * loop's exploration caps. `/effort quick|normal|deep` in the chat box does the
 * same thing for the whole session.
 */
export function BottomMenuReasoningEffort() {
  const [open, setOpen] = useState(false);
  const effort = useSyncExternalStore(subscribeToReasoningEffort, getReasoningEffort);
  const isDefault = effort === DEFAULT_REASONING_EFFORT;

  const select = (next: ReasoningEffort) => {
    setReasoningEffort(next);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className="flex h-7 items-center rounded-md px-0.5 cursor-pointer [&_svg]:size-4 text-text-default/70 hover:bg-background-medium hover:text-text-default text-xs"
          title={`Reasoning effort: ${REASONING_EFFORT_LABELS[effort]}`}
          aria-label={`Reasoning effort: ${REASONING_EFFORT_LABELS[effort]}`}
        >
          <Gauge className="mr-0.5 h-4 w-4" strokeWidth={1.5} />
          {/* The default is the quiet state — only a deliberate quick/deep
              choice is worth spending composer width on. */}
          {!isDefault && <span>{REASONING_EFFORT_LABELS[effort]}</span>}
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="center" className="w-80 p-0">
        <div className="border-b border-border-subtle px-4 py-3">
          <div className="text-sm font-medium text-text-default">Reasoning effort</div>
          <div className="mt-1 text-xs text-text-muted">
            How hard to think on the next message. Also settable with /effort.
          </div>
        </div>
        <div className="p-2">
          {REASONING_EFFORTS.map((level) => {
            const selected = level === effort;
            return (
              <button
                key={level}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                onClick={() => select(level)}
                className="flex w-full items-start gap-3 rounded-lg px-2 py-2 text-left hover:bg-background-medium/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="text-sm text-text-default">{REASONING_EFFORT_LABELS[level]}</div>
                  <div className="text-[11px] text-text-muted">
                    {REASONING_EFFORT_DESCRIPTIONS[level]}
                  </div>
                </div>
                {selected && <Check className="mt-0.5 h-4 w-4 text-text-default" />}
              </button>
            );
          })}
        </div>
      </PopoverContent>
    </Popover>
  );
}
