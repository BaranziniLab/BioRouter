import React, { useEffect, useRef, useState } from 'react';

interface Props {
  name: string;
  onRename: (newName: string) => void;
  /** Optional accent color dot (dashboard windows pass theirs). */
  accentColor?: string;
  className?: string;
}

export const SessionNamePill: React.FC<Props> = ({ name, onRename, accentColor, className }) => {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const noDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties;

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

  const startEditing = (event: React.SyntheticEvent) => {
    event.stopPropagation();
    setEditing(true);
  };

  return (
    <div
      className={`inline-flex h-10 min-w-0 items-center gap-2 rounded-md no-drag ${className ?? ''}`}
      style={noDragStyle}
      onPointerDown={(event) => {
        if (!editing) startEditing(event);
      }}
    >
      {accentColor && (
        <span
          className="inline-block w-2 h-2 rounded-full flex-shrink-0"
          style={{ backgroundColor: accentColor }}
        />
      )}
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
          className="h-8 w-[min(360px,70vw)] min-w-[120px] bg-transparent px-1 text-sm font-medium outline-none border-b border-border-subtle"
        />
      ) : (
        <button
          ref={buttonRef}
          type="button"
          className="inline-flex h-8 max-w-full min-w-0 cursor-text items-center rounded px-1 text-left text-sm font-medium leading-none text-text-default transition-colors hover:bg-background-medium/40"
          onPointerDown={startEditing}
          onClick={startEditing}
          style={noDragStyle}
          title="Click to rename"
        >
          <span className="pointer-events-none truncate no-drag">{name}</span>
        </button>
      )}
    </div>
  );
};
