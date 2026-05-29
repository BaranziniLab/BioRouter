import React from 'react';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';

interface Props {
  name: string;
  cwd?: string;
  accentColor: string;
  isBusy: boolean;
  onUnfold: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onClose: () => void;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}

const iconBtnClass =
  'flex-shrink-0 p-1 rounded hover:bg-background-medium/60 transition-colors';

export const FoldedCard: React.FC<Props> = ({
  name,
  cwd,
  accentColor,
  isBusy,
  onUnfold,
  onShrink,
  onEnlarge,
  onClose,
  onPointerDownDrag,
}) => {
  // 18% alpha → "2E"; 6% → "0F"; 28% (border) → "47"; 40% (pulse ring) → "66".
  const bg = `linear-gradient(135deg, ${accentColor}2E, ${accentColor}0F)`;
  const border = `${accentColor}47`;

  const indicator = isBusy ? (
    <span
      className="relative inline-flex w-2.5 h-2.5 flex-shrink-0"
      aria-label="busy"
    >
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
      className="h-full w-full rounded-2xl overflow-hidden select-none cursor-grab active:cursor-grabbing flex flex-col"
      style={{
        background: bg,
        border: `1px solid ${border}`,
      }}
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        onPointerDownDrag(e);
      }}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('button')) return;
        onUnfold();
      }}
      title="Click to unfold"
    >
      {/* Row 1: status · title · buttons */}
      <div className="flex items-center gap-2 px-3 pt-2">
        {indicator}
        <span className="flex-1 min-w-0 truncate text-sm font-medium">{name}</span>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onUnfold(); }}
          title="Unfold"
        >
          <Minus className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onShrink(); }}
          title="Shrink to minimum size"
        >
          <Minimize2 className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onEnlarge(); }}
          title="Enlarge"
        >
          <Maximize2 className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          className={iconBtnClass}
          onClick={(e) => { e.stopPropagation(); onClose(); }}
          title="Close"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
      {/* Row 2: working directory */}
      <div className="px-3 pb-2 mt-0.5 text-[11px] font-mono text-text-muted/80 truncate">
        {cwd ?? ''}
      </div>
    </div>
  );
};
