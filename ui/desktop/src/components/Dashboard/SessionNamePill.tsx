import React, { useEffect, useRef, useState } from 'react';
import { More } from '../icons';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';

interface Props {
  name: string;
  onRename: (newName: string) => void;
  onDiverge?: () => void | Promise<void>;
  canDiverge?: boolean;
  /** Optional accent color dot (dashboard windows pass theirs). */
  accentColor?: string;
  className?: string;
}

export const SessionNamePill: React.FC<Props> = ({
  name,
  onRename,
  onDiverge,
  canDiverge = false,
  accentColor,
  className,
}) => {
  const [editing, setEditing] = useState(false);
  const [diverging, setDiverging] = useState(false);
  const [draft, setDraft] = useState(name);
  const inputRef = useRef<HTMLInputElement>(null);
  const noDragStyle = { WebkitAppRegion: 'no-drag' } as React.CSSProperties;

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
      className={`inline-flex h-10 min-w-0 items-center gap-1 rounded-md no-drag ${className ?? ''}`}
      style={noDragStyle}
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
          className="h-8 w-[min(360px,70vw)] min-w-[120px] bg-transparent px-1 text-sm font-medium border-b border-border-subtle"
        />
      ) : (
        <>
          <span
            className="inline-flex h-8 max-w-full min-w-0 items-center rounded py-0 pl-1 pr-0 text-sm font-medium leading-none text-text-default"
            title={name}
          >
            <span className="truncate no-drag">{name}</span>
          </span>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label="Conversation title actions"
                title="Conversation title actions"
                className="inline-flex h-7 w-6 flex-shrink-0 items-center justify-center rounded-md text-text-muted transition-colors hover:bg-background-medium hover:text-text-default"
                style={noDragStyle}
                onPointerDown={(event) => event.stopPropagation()}
              >
                <More className="h-4 w-4" />
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
        </>
      )}
    </div>
  );
};
