import { useSyncExternalStore } from 'react';
import { useState } from 'react';
import { Check, Gauge } from '../icons/app-icons';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';
import { Button } from '../ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
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
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              className="flex h-7 items-center gap-0.5 rounded-md px-0.5 cursor-pointer text-text-default/70 hover:bg-background-medium hover:text-text-default text-xs"
              aria-label={`Reasoning effort: ${REASONING_EFFORT_LABELS[effort]}`}
            >
              <Gauge className="size-[18px]" strokeWidth={1.75} />
              {/* The default is the quiet state — only a deliberate quick/deep
                  choice is worth spending composer width on. */}
              {!isDefault && <span>{REASONING_EFFORT_LABELS[effort]}</span>}
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">
          Reasoning effort: {REASONING_EFFORT_LABELS[effort]}
        </TooltipContent>
      </Tooltip>
      <PopoverContent side="top" align="center" className="w-64 p-0 font-sans">
        <div className="border-b border-border-subtle px-3 py-2.5">
          <div className="text-sm font-medium text-text-default">Reasoning effort</div>
          <div className="mt-0.5 text-[11px] leading-4 text-text-muted">
            How hard to think on the next message. Also settable with /effort.
          </div>
        </div>
        <div className="p-1.5" role="menu" aria-label="Reasoning effort">
          {REASONING_EFFORTS.map((level) => {
            const selected = level === effort;
            return (
              <Button
                key={level}
                type="button"
                variant="ghost"
                size="sm"
                shape="pill"
                role="menuitemradio"
                aria-checked={selected}
                onClick={() => select(level)}
                className="h-auto w-full items-start justify-start gap-2 rounded-md px-2 py-1.5 text-left whitespace-normal hover:bg-background-medium/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-medium text-text-default">
                    {REASONING_EFFORT_LABELS[level]}
                  </div>
                  <div className="text-[10px] leading-3.5 text-text-muted">
                    {REASONING_EFFORT_DESCRIPTIONS[level]}
                  </div>
                </div>
                <span aria-hidden="true" className="mt-0.5 size-3.5 shrink-0">
                  {selected && <Check className="size-3.5 text-text-default" />}
                </span>
              </Button>
            );
          })}
        </div>
      </PopoverContent>
    </Popover>
  );
}
