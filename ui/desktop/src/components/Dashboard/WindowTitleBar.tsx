import React, { useState, useRef, useEffect } from 'react';
import { X, Minimize2, Maximize2, Minus } from '../icons/app-icons';

interface Props {
  name: string;
  accentColor: string;
  onRename: (name: string) => void;
  onClose: () => void;
  onShrink: () => void;
  onEnlarge: () => void;
  onFold: () => void;
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
  onPointerDownDrag,
}) => {
  const [editing, setEditing] = useState(false);
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

  const iconBtnClass =
    'flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors';

  return (
    <div
      className="flex items-center gap-2 px-3 h-9 select-none cursor-grab active:cursor-grabbing border-b border-border-subtle/30 bg-background-default/80 backdrop-blur-sm rounded-t-2xl"
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
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setDraft(name);
              setEditing(false);
            }
          }}
          className="flex-1 min-w-0 bg-transparent text-sm font-medium outline-none border-b border-border-subtle"
        />
      ) : (
        <span
          className="flex-1 min-w-0 truncate text-sm font-medium"
          onDoubleClick={() => setEditing(true)}
          title="Double-click to rename"
        >
          {name}
        </span>
      )}
      {/* Order: Fold | Shrink | Enlarge | Close — right-aligned. */}
      <button
        type="button"
        className={iconBtnClass}
        onClick={onFold}
        title="Fold to card"
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
        title="Close conversation"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
};
