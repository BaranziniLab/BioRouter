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
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    setDraft(name);
  }, [name]);
  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);
  useEffect(() => {
    const button = buttonRef.current;
    if (!button || editing) return;

    const startEditingFromNativeEvent = (event: Event) => {
      event.preventDefault();
      event.stopPropagation();
      setEditing(true);
    };

    const eventNames = ['pointerdown', 'mousedown', 'click', 'dblclick'];
    eventNames.forEach((eventName) => {
      button.addEventListener(eventName, startEditingFromNativeEvent, { capture: true });
    });

    return () => {
      eventNames.forEach((eventName) => {
        button.removeEventListener(eventName, startEditingFromNativeEvent, { capture: true });
      });
    };
  }, [editing]);

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed && trimmed !== name) onRename(trimmed);
    setEditing(false);
  };

  const iconBtnClass = 'flex-shrink-0 p-1 rounded hover:bg-background-medium transition-colors';
  const noDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties;
  const startEditing = (event: React.SyntheticEvent) => {
    event.stopPropagation();
    setEditing(true);
  };

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
        <button
          ref={buttonRef}
          type="button"
          className="inline-flex h-8 min-w-0 max-w-[min(420px,calc(100vw-180px))] cursor-text items-center rounded px-1 text-left text-sm font-medium leading-none text-text-default transition-colors hover:bg-background-medium/40"
          onPointerDown={startEditing}
          onClick={startEditing}
          style={noDragStyle}
          title="Click to rename"
        >
          <span className="pointer-events-none truncate no-drag">{name}</span>
        </button>
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
