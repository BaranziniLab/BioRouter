import React, { useState, useRef, useEffect } from 'react';
import { More } from '../icons';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

interface Props {
  name: string;
  accentColor: string;
  onRename: (name: string) => void;
  onClose: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onFold: () => void;
  onDiverge?: () => void | Promise<void>;
  canDiverge?: boolean;
  onPointerDownDrag: (e: React.PointerEvent<HTMLDivElement>) => void;
}

export const WindowTitleBar: React.FC<Props> = ({
  name,
  accentColor,
  onRename,
  onClose,
  onShrink,
  onEnlarge,
  onFold,
  onDiverge,
  canDiverge = false,
  onPointerDownDrag,
}) => {
  const [editing, setEditing] = useState(false);
  const [diverging, setDiverging] = useState(false);
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setDraft(name);
  }, [name]);
  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== name) onRename(trimmed);
    setEditing(false);
  };

  const iconBtnClass = 'flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors';
  const noDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties;
  const startEditing = (event?: React.SyntheticEvent | Event) => {
    event?.stopPropagation();
    setEditing(true);
  };
  const handleDiverge = (event?: React.SyntheticEvent | Event) => {
    event?.stopPropagation();
    if (!onDiverge || !canDiverge || diverging) return;
    setDiverging(true);
    void Promise.resolve(onDiverge()).finally(() => setDiverging(false));
  };

  return (
    <div
      className="flex items-center gap-1.5 px-3 h-9 select-none cursor-grab active:cursor-grabbing border-b border-border-subtle/30 bg-background-default/80 backdrop-blur-sm rounded-t-2xl"
      onPointerDown={(e) => {
        if ((e.target as HTMLElement).closest('button, input')) return;
        onPointerDownDrag(e);
      }}
    >
      <span
        className="inline-block w-2.5 h-2.5 rounded-full flex-shrink-0"
        style={{ backgroundColor: accentColor }}
      />
      {editing ? (
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onPointerDown={(event) => event.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setDraft(name);
              setEditing(false);
            }
          }}
          style={noDragStyle}
          className="h-8 w-[min(360px,calc(100vw-180px))] min-w-[120px] bg-transparent px-1 text-sm font-medium outline-none border-b border-border-subtle"
        />
      ) : (
        <span className="inline-flex h-8 min-w-0 max-w-[min(420px,calc(100vw-220px))] items-center rounded py-0 pl-1 pr-0 text-sm font-medium leading-none text-text-default">
          <span className="truncate" title={name}>
            {name}
          </span>
        </span>
      )}
      {!editing && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Conversation title actions"
              title="Conversation title actions"
              className={iconBtnClass}
              style={noDragStyle}
              onPointerDown={(event) => event.stopPropagation()}
            >
              <More className="h-3.5 w-3.5" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" side="bottom" className="w-36">
            <DropdownMenuItem onSelect={startEditing}>Rename</DropdownMenuItem>
            {onDiverge && (
              <DropdownMenuItem disabled={!canDiverge || diverging} onSelect={handleDiverge}>
                {diverging ? 'Diverging...' : 'Diverge'}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <span className="min-w-0 flex-1" />
      {/* Order: Fold | Shrink | Enlarge | Close — right-aligned. */}
      <button
        type="button"
        className={iconBtnClass}
        onClick={onFold}
        title="Fold to card (⌘⌥Enter)"
      >
        <Minus className="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        className={iconBtnClass}
        onClick={onShrink}
        title="Shrink to minimum size"
      >
        <Minimize2 className="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        className={iconBtnClass}
        onClick={onEnlarge}
        title="Enlarge to default chat size"
      >
        <Maximize2 className="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        className={iconBtnClass}
        onClick={onClose}
        title="Remove from dashboard (⌘⌥⌫)"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};
