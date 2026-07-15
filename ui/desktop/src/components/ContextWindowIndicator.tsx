import React, { useCallback, useEffect, useRef, useState } from 'react';
import { ChevronsDownUp } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from './ui/popover';
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/Tooltip';
import { readConfig, upsertConfig } from '../api';

interface ContextWindowGaugeProps {
  totalTokens: number | undefined;
  tokenLimit: number;
  isTokenLimitLoaded: boolean;
  onCompact: () => void;
}

const AUTO_COMPACT_THRESHOLD_KEY = 'BIOROUTER_AUTO_COMPACT_THRESHOLD';
const AUTO_COMPACT_MIN_PCT = 20;
const AUTO_COMPACT_MAX_PCT = 90;
const AUTO_COMPACT_DEFAULT_PCT = 80;

function clampThresholdPct(v: number): number {
  if (!Number.isFinite(v)) return AUTO_COMPACT_DEFAULT_PCT;
  return Math.max(AUTO_COMPACT_MIN_PCT, Math.min(AUTO_COMPACT_MAX_PCT, Math.round(v)));
}

/** Inline bar-style context gauge. Used both as a row inside the picker
 * popover (dashboard mode) and as the popover body of the standalone
 * indicator (chat-tab mode). Icon stays neutral so it matches the rest of
 * the picker icons; the bar alone goes green → yellow → orange → red as
 * usage climbs. The bar also carries a draggable threshold marker — the
 * point at which BioRouter auto-compacts — clamped to 20–90%. */
export const ContextWindowGauge: React.FC<ContextWindowGaugeProps> = ({
  totalTokens,
  tokenLimit,
  isTokenLimitLoaded,
  onCompact,
}) => {
  const current = totalTokens ?? 0;
  const total = tokenLimit || 0;
  const [thresholdPct, setThresholdPct] = useState<number>(AUTO_COMPACT_DEFAULT_PCT);
  // Controlled tooltip so Radix's default focus-trigger doesn't pop the
  // hint open when the popover auto-focuses the slider on mount.
  const [tooltipOpen, setTooltipOpen] = useState(false);
  const barRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);
  const pendingPctRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;
    readConfig({ body: { key: AUTO_COMPACT_THRESHOLD_KEY, is_secret: false } })
      .then((res) => {
        if (cancelled) return;
        const v = res.data as unknown;
        if (typeof v === 'number' && v > 0 && v < 1) {
          setThresholdPct(clampThresholdPct(v * 100));
        }
      })
      .catch(() => {
        /* fall back to default */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Flush any in-flight pending change if the gauge unmounts mid-drag
  // (e.g. popover closes when the user releases the mouse outside of it).
  useEffect(() => {
    return () => {
      if (pendingPctRef.current !== null) {
        const v = pendingPctRef.current;
        pendingPctRef.current = null;
        upsertConfig({
          body: {
            key: AUTO_COMPACT_THRESHOLD_KEY,
            value: v / 100,
            is_secret: false,
          },
        }).catch((err) => {
          console.warn('Failed to save auto-compact threshold on unmount:', err);
        });
      }
    };
  }, []);

  const persistThreshold = (pctValue: number) => {
    pendingPctRef.current = null;
    upsertConfig({
      body: {
        key: AUTO_COMPACT_THRESHOLD_KEY,
        value: pctValue / 100,
        is_secret: false,
      },
    }).catch((err) => {
      console.warn('Failed to save auto-compact threshold:', err);
    });
  };

  // Live updates during drag don't hit the API — we only persist on release
  // so a single drag costs one POST, not dozens.
  const updateThresholdLive = (raw: number) => {
    const next = clampThresholdPct(raw);
    setThresholdPct(next);
    pendingPctRef.current = next;
  };

  const handleSliderChange = (raw: number) => {
    const next = clampThresholdPct(raw);
    setThresholdPct(next);
    persistThreshold(next);
  };

  const computePctFromClientX = useCallback((clientX: number): number => {
    const bar = barRef.current;
    if (!bar) return AUTO_COMPACT_DEFAULT_PCT;
    const rect = bar.getBoundingClientRect();
    if (rect.width <= 0) return AUTO_COMPACT_DEFAULT_PCT;
    const ratio = (clientX - rect.left) / rect.width;
    return clampThresholdPct(ratio * 100);
  }, []);

  const handleDragMove = useCallback(
    (e: MouseEvent) => {
      if (!draggingRef.current) return;
      updateThresholdLive(computePctFromClientX(e.clientX));
    },
    [computePctFromClientX]
  );

  const handleDragEnd = useCallback(() => {
    draggingRef.current = false;
    window.removeEventListener('mousemove', handleDragMove);
    window.removeEventListener('mouseup', handleDragEnd);
    if (pendingPctRef.current !== null) {
      persistThreshold(pendingPctRef.current);
    }
  }, [handleDragMove]);

  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      draggingRef.current = true;
      // Snap to where the user clicked; persist on release.
      updateThresholdLive(computePctFromClientX(e.clientX));
      window.addEventListener('mousemove', handleDragMove);
      window.addEventListener('mouseup', handleDragEnd);
    },
    [computePctFromClientX, handleDragMove, handleDragEnd]
  );

  const handleKeyAdjust = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
      e.preventDefault();
      handleSliderChange(thresholdPct - (e.shiftKey ? 10 : 1));
    } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
      e.preventDefault();
      handleSliderChange(thresholdPct + (e.shiftKey ? 10 : 1));
    } else if (e.key === 'Home') {
      e.preventDefault();
      handleSliderChange(AUTO_COMPACT_MIN_PCT);
    } else if (e.key === 'End') {
      e.preventDefault();
      handleSliderChange(AUTO_COMPACT_MAX_PCT);
    }
  };

  useEffect(() => {
    return () => {
      window.removeEventListener('mousemove', handleDragMove);
      window.removeEventListener('mouseup', handleDragEnd);
    };
  }, [handleDragMove, handleDragEnd]);

  if (!isTokenLimitLoaded && !current) return null;
  const ratio = total > 0 ? Math.min(1, current / total) : 0;
  const pct = Math.round(ratio * 100);
  const overThreshold = pct >= thresholdPct;
  const barColor = overThreshold
    ? 'bg-background-danger'
    : ratio <= 0.5
      ? 'bg-background-success'
      : ratio <= 0.75
        ? 'bg-background-warning'
        : 'bg-background-warning';
  return (
    <div className="flex items-center gap-2 px-2 py-1.5 rounded">
      <span className="flex h-4 w-4 flex-shrink-0 items-center justify-center text-text-default/70">
        <span className="h-3.5 w-3.5 rounded-full border-2 border-current" />
      </span>
      <span className="text-[11px] text-text-default/60 w-12 flex-shrink-0">Context</span>
      <div className="flex-1 min-w-0 flex flex-col gap-1">
        {/* The bar holds the usage fill *and* a draggable downward triangle
            marking the auto-compact threshold. The bar is the drag target;
            mousedown anywhere on the bar (or the triangle) jumps the
            threshold to the cursor and starts a drag. */}
        <div className="relative pt-2.5">
          <div
            ref={barRef}
            className="relative h-1 rounded-full bg-background-muted overflow-visible cursor-pointer"
            onMouseDown={handleDragStart}
          >
            <div
              className={`h-full rounded-full ${barColor} transition-[width]`}
              style={{ width: `${Math.max(2, pct)}%` }}
            />
            <Tooltip open={tooltipOpen}>
              <TooltipTrigger asChild>
                {/* Hit area is larger than the 10×6 visible triangle so the
                    hover tooltip doesn't flicker when the cursor grazes the
                    triangle edges. The visible glyph is the inner element. */}
                <div
                  role="slider"
                  aria-label="Auto-compact threshold"
                  aria-valuemin={AUTO_COMPACT_MIN_PCT}
                  aria-valuemax={AUTO_COMPACT_MAX_PCT}
                  aria-valuenow={thresholdPct}
                  tabIndex={0}
                  onKeyDown={handleKeyAdjust}
                  onMouseDown={handleDragStart}
                  onMouseEnter={() => setTooltipOpen(true)}
                  onMouseLeave={() => {
                    if (!draggingRef.current) setTooltipOpen(false);
                  }}
                  className="absolute flex items-end justify-center cursor-ew-resize"
                  style={{
                    left: `${thresholdPct}%`,
                    top: '-12px',
                    width: '22px',
                    height: '18px',
                    transform: 'translateX(-50%)',
                  }}
                >
                  <span
                    aria-hidden="true"
                    style={{
                      width: 0,
                      height: 0,
                      borderLeft: '5px solid transparent',
                      borderRight: '5px solid transparent',
                      borderTop: '6px solid currentColor',
                      color: 'var(--color-text-default, currentColor)',
                      pointerEvents: 'none',
                    }}
                  />
                </div>
              </TooltipTrigger>
              <TooltipContent side="top" className="text-[11px]">
                Drag to adjust auto-compact threshold ({thresholdPct}%)
              </TooltipContent>
            </Tooltip>
          </div>
        </div>
        <div className="flex items-center justify-between text-sm text-text-muted">
          <span>
            {fmt(current)} / {fmt(total)}
          </span>
          <span>{pct}%</span>
        </div>
      </div>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="flex flex-shrink-0">
            <button
              type="button"
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onCompact();
              }}
              disabled={current === 0}
              aria-label={current === 0 ? 'Nothing to compact yet' : 'Compact conversation'}
              className={`flex h-7 w-7 items-center justify-center rounded transition-colors ${current === 0 ? 'cursor-not-allowed text-text-default/40' : 'cursor-pointer text-text-default/70 hover:bg-background-medium hover:text-text-default'}`}
            >
              <ChevronsDownUp
                data-testid="compact-conversation-icon"
                className="size-4"
                strokeWidth={1.75}
              />
            </button>
          </span>
        </TooltipTrigger>
        <TooltipContent side="top">
          {current === 0 ? 'Nothing to compact yet' : 'Compact conversation'}
        </TooltipContent>
      </Tooltip>
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
  /** Override the popover trigger tooltip text. */
  triggerTitle?: string;
}

/** Compact-row variant: a single remaining-context ring button which, on click,
 * opens a popover that renders the same bar-style gauge used in the
 * dashboard picker. For the chat tab the user sees one compact ring and,
 * on click, gets the full real-time gauge with a Compact button — the
 * same UI the dashboard exposes. Ring color reflects remaining headroom. */
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
  const total = tokenLimit || 0;
  const ratio = total > 0 ? Math.min(1, current / total) : 0;
  const pct = Math.round(ratio * 100);
  const remainingTokens = total > 0 ? Math.max(total - current, 0) : 0;
  const remainingRatio = total > 0 ? Math.max(0, Math.min(1, remainingTokens / total)) : 0;
  const remainingPct = Math.round(remainingRatio * 100);
  const radius = 8;
  const circumference = 2 * Math.PI * radius;
  const strokeOffset = circumference * (1 - remainingRatio);
  const remainingSummary = `${fmt(remainingTokens)} of ${fmt(total)} tokens remaining`;
  const usageSummary = `${remainingPct}% remaining, ${pct}% used`;
  const tooltip = `${triggerTitle}. ${remainingSummary}. ${usageSummary}`;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <Tooltip>
        <TooltipTrigger asChild>
          <PopoverTrigger asChild>
            <button
              type="button"
              aria-label={tooltip}
              className="relative flex h-7 w-6 flex-shrink-0 items-center justify-center rounded-md text-text-default/70 hover:text-text-default hover:bg-background-medium cursor-pointer transition-colors"
            >
              <svg className="absolute size-[18px]" viewBox="0 0 24 24" aria-hidden="true">
                <circle
                  cx="12"
                  cy="12"
                  r={radius}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.75"
                  className="text-border-subtle"
                />
                <circle
                  cx="12"
                  cy="12"
                  r={radius}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.75"
                  strokeLinecap="round"
                  strokeDasharray={circumference}
                  strokeDashoffset={strokeOffset}
                  className="text-text-default/70 transition-[stroke-dashoffset] duration-200"
                  transform="rotate(-90 12 12)"
                />
              </svg>
              <span className="sr-only">{tooltip}</span>
            </button>
          </PopoverTrigger>
        </TooltipTrigger>
        <TooltipContent side="top" className="w-52 text-left text-pretty">
          <span className="block font-medium">{triggerTitle}</span>
          <span className="block">{remainingSummary}</span>
          <span className="block">{usageSummary}</span>
        </TooltipContent>
      </Tooltip>
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
