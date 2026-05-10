import React from 'react';
import { X } from '../icons/app-icons';
import { DashboardWindow } from '../../contexts/DashboardContext';

interface Props {
  win: DashboardWindow;
  preview: string[];
  onEvoke: () => void;
  onClose: () => void;
  onDragStart: (e: React.PointerEvent) => void;
}

export const TuckedCard: React.FC<Props> = ({ win, preview, onEvoke, onClose, onDragStart }) => (
  <div
    className="group relative rounded-xl bg-background-default border border-border-subtle/40 p-3 hover:bg-background-medium/60 transition-colors cursor-pointer"
    onClick={onEvoke}
    onPointerDown={onDragStart}
  >
    <div className="flex items-center gap-2 mb-1">
      <span
        className="inline-block w-2 h-2 rounded-full flex-shrink-0"
        style={{ backgroundColor: win.accentColor }}
      />
      <span className="flex-1 text-sm font-medium truncate">{win.name}</span>
      <span className="text-[10px] font-mono text-text-muted">#{win.badge}</span>
      <button
        type="button"
        className="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-background-medium transition-opacity"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        title="Remove"
      >
        <X className="w-3 h-3" />
      </button>
    </div>
    {preview.length > 0 && (
      <div className="text-[11px] leading-snug text-text-muted line-clamp-3">
        {preview.join(' · ')}
      </div>
    )}
    {win.unreadActivity && (
      <span className="absolute top-2 right-7 w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
    )}
  </div>
);
