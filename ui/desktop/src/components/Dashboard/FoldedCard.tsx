import React, { useRef, useState } from 'react';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';

interface Props {
  name: string;
  cwd?: string;
  accentColor: string;
  isBusy: boolean;
  /** Tail of the latest assistant message (or current stream). Shown in the
   * hover preview popup. */
  previewTail?: string;
  onUnfold: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onClose: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}

const iconBtnClass = 'flex-shrink-0 p-1 rounded hover:bg-background-medium/60 transition-colors';

// Pointer movement (in CSS px) below which a pointerdown→pointerup counts as
// a click rather than a drag. Matches the canvas pan threshold so the two
// gestures stay symmetric.
const CLICK_SLOP_PX = 4;

export const FoldedCard: React.FC<Props> = ({
  name,
  cwd,
  accentColor,
  isBusy,
  previewTail,
  onUnfold,
  onShrink,
  onEnlarge,
  onClose,
  onPointerDownDrag,
}) => {
  // Neutral opaque card fill. The design language forbids accent-tinted card
  // backgrounds, so the per-window accent shows only through the status dot
  // below — never the surface. The card sits on a subtle neutral border that
  // matches the chat window chrome.
  const [hovered, setHovered] = useState(false);
  const downRef = useRef<{ x: number; y: number; t: number } | null>(null);

  const indicator = isBusy ? (
    <span className="relative inline-flex w-2.5 h-2.5 flex-shrink-0" aria-label="busy">
      <span
        className="absolute inset-0 rounded-full"
        style={{ backgroundColor: accentColor, animation: 'breathe 1.4s ease-in-out infinite' }}
      />
      <span
        className="absolute inset-0 rounded-full"
        style={{
          backgroundColor: `${accentColor}66`,
          animation: 'breathe-pulse 1.4s ease-out infinite',
        }}
      />
    </span>
  ) : (
    <span
      className="inline-block w-2.5 h-2.5 rounded-full flex-shrink-0"
      style={{ border: `1.5px solid ${accentColor}`, backgroundColor: 'transparent' }}
      aria-label="idle"
    />
  );

  return (
    <div
      className="relative h-full w-full animate-[fadein_180ms_ease-out_forwards]"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <div
        className="h-full w-full rounded-2xl overflow-hidden select-none cursor-grab active:cursor-grabbing flex flex-col bg-background-default border border-border-subtle transition-transform duration-200 ease-out"
        style={{
          transform: hovered ? 'translateY(-1px)' : 'translateY(0)',
        }}
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest('button')) return;
          downRef.current = { x: e.clientX, y: e.clientY, t: Date.now() };
          onPointerDownDrag(e);
        }}
        onPointerUp={(e) => {
          if ((e.target as HTMLElement).closest('button')) return;
          const d = downRef.current;
          downRef.current = null;
          if (!d) return;
          const dx = e.clientX - d.x;
          const dy = e.clientY - d.y;
          // Treat a near-stationary press as a click → unfold. Anything past
          // CLICK_SLOP_PX is a drag and must NOT expand the card.
          if (Math.hypot(dx, dy) <= CLICK_SLOP_PX && Date.now() - d.t < 350) {
            onUnfold();
          }
        }}
        onPointerCancel={() => {
          downRef.current = null;
        }}
        title="Click to expand · drag to move · ⌘⌥Enter"
      >
        {/* Row 1: status indicator + control buttons. No title here so even
            short cards never crop the name. */}
        <div className="flex items-center gap-2 px-3 pt-2">
          {indicator}
          <span className="flex-1" />
          <button
            type="button"
            className={iconBtnClass}
            onClick={(e) => {
              e.stopPropagation();
              onUnfold();
            }}
            title="Expand (⌘⌥Enter)"
          >
            <Minus className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            className={iconBtnClass}
            onClick={(e) => {
              e.stopPropagation();
              onShrink();
            }}
            title="Shrink to minimum size"
          >
            <Minimize2 className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            className={iconBtnClass}
            onClick={(e) => {
              e.stopPropagation();
              onEnlarge();
            }}
            title="Enlarge"
          >
            <Maximize2 className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            className={iconBtnClass}
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            title="Remove from dashboard (⌘⌥⌫)"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
        {/* Row 2: title — own line, plenty of horizontal room. */}
        <div className="px-3 mt-1 text-sm font-medium leading-snug truncate">{name}</div>
        {/* Row 3: working directory. Extra bottom padding so the descenders
            and the leading don't visually clip at small heights. */}
        <div className="px-3 pb-2 mt-0.5 text-[11px] font-mono text-text-muted/80 leading-snug truncate">
          {cwd ?? ''}
        </div>
      </div>
      {/* Hover preview popup — shows the tail of the latest assistant message
          (or the in-progress stream). Sits below the card, fades + slides in.
          pointer-events:none so it never blocks drag/click on the card or
          neighboring cards. */}
      <div
        aria-hidden={!hovered}
        className="absolute left-0 right-0 top-full mt-2 z-50 pointer-events-none"
        style={{
          opacity: hovered ? 1 : 0,
          transform: hovered ? 'translateY(0)' : 'translateY(-4px)',
          transition: 'opacity 140ms ease-out, transform 140ms ease-out',
        }}
      >
        <div className="rounded-lg border border-border-subtle bg-background-default shadow-popover px-3 py-2 text-[11px] leading-snug text-text-default/90 max-h-32 overflow-hidden">
          {previewTail && previewTail.trim().length > 0 ? (
            <div className="whitespace-pre-wrap break-words">
              {isBusy ? <span className="text-text-muted/80">… </span> : null}
              {previewTail}
            </div>
          ) : (
            <div className="text-text-muted/70 italic">
              {isBusy ? 'Working…' : 'No conversation yet'}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
