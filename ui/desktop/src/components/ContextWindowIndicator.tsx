import React, { useState } from 'react';
import { Activity, ScrollText } from './icons/app-icons';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';

interface ContextWindowGaugeProps {
  totalTokens: number | undefined;
  tokenLimit: number;
  isTokenLimitLoaded: boolean;
  onCompact: () => void;
}

/** Inline bar-style context gauge. Used both as a row inside the picker
 * popover (dashboard mode) and as the popover body of the standalone
 * indicator (chat-tab mode). Icon stays neutral so it matches the rest of
 * the picker icons; the bar alone goes green → yellow → orange → red as
 * usage climbs. */
export const ContextWindowGauge: React.FC<ContextWindowGaugeProps> = ({
  totalTokens,
  tokenLimit,
  isTokenLimitLoaded,
  onCompact,
}) => {
  const current = totalTokens ?? 0;
  const total = tokenLimit || 0;
  if (!isTokenLimitLoaded && !current) return null;
  const ratio = total > 0 ? Math.min(1, current / total) : 0;
  const pct = Math.round(ratio * 100);
  const barColor =
    ratio <= 0.5
      ? 'bg-green-500'
      : ratio <= 0.75
        ? 'bg-yellow-500'
        : ratio <= 0.9
          ? 'bg-orange-500'
          : 'bg-red-500';
  return (
    <div className="flex items-center gap-2 px-2 py-1.5 rounded">
      <span className="flex items-center justify-center w-4 h-4 flex-shrink-0 text-text-default/70">
        <Activity className="w-4 h-4" />
      </span>
      <span className="text-[11px] text-text-default/60 w-12 flex-shrink-0">Context</span>
      <div className="flex-1 min-w-0 flex flex-col gap-1">
        <div className="h-1 rounded-full bg-background-muted overflow-hidden">
          <div
            className={`h-full ${barColor} transition-[width]`}
            style={{ width: `${Math.max(2, pct)}%` }}
          />
        </div>
        <div className="flex items-center justify-between text-sm text-text-muted">
          <span>
            {fmt(current)} / {fmt(total)}
          </span>
          <span>{pct}%</span>
        </div>
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onCompact();
        }}
        disabled={current === 0}
        title={current === 0 ? 'Nothing to compact yet' : 'Compact conversation'}
        className={`flex items-center justify-center w-7 h-7 rounded transition-colors flex-shrink-0 ${
          current === 0
            ? 'opacity-40 cursor-not-allowed'
            : 'text-text-default/70 hover:text-text-default hover:bg-background-medium cursor-pointer'
        }`}
      >
        <ScrollText size={14} />
      </button>
    </div>
  );
};

function fmt(n: number): string {
  if (n >= 1_000_000) {
    const m = n / 1_000_000;
    return m % 1 === 0 ? `${m.toFixed(0)}M` : `${m.toFixed(1)}M`;
  }
  if (n >= 1000) {
    const k = n / 1000;
    return k % 1 === 0 ? `${k.toFixed(0)}k` : `${k.toFixed(1)}k`;
  }
  return n.toString();
}

interface ContextWindowIndicatorProps extends ContextWindowGaugeProps {
  /** Override the popover trigger button's title hover text. */
  triggerTitle?: string;
}

/** Compact-row variant: a single black vital-sign button which, on click,
 * opens a popover that renders the same bar-style gauge used in the
 * dashboard picker. For the chat tab the user sees one neutral icon and,
 * on click, gets the full real-time gauge with a Compact button — the
 * same UI the dashboard exposes. Bar color reflects usage. */
export const ContextWindowIndicator: React.FC<ContextWindowIndicatorProps> = ({
  totalTokens,
  tokenLimit,
  isTokenLimitLoaded,
  onCompact,
  triggerTitle = 'Context window usage',
}) => {
  const [open, setOpen] = useState(false);
  const current = totalTokens ?? 0;
  if (!isTokenLimitLoaded && !current) return null;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          title={triggerTitle}
          className="flex items-center justify-center w-7 h-7 rounded text-text-default/70 hover:text-text-default hover:bg-background-medium cursor-pointer transition-colors"
        >
          <Activity className="w-4 h-4" />
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-72 p-1.5">
        <ContextWindowGauge
          totalTokens={totalTokens}
          tokenLimit={tokenLimit}
          isTokenLimitLoaded={isTokenLimitLoaded}
          onCompact={() => {
            onCompact();
            setOpen(false);
          }}
        />
      </PopoverContent>
    </Popover>
  );
};
