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

  return (
    <div className={`inline-flex items-center gap-2 px-2 py-1 rounded-md ${className ?? ''}`}>
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
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setDraft(name);
              setEditing(false);
            }
          }}
          className="bg-transparent text-sm font-medium outline-none border-b border-border-subtle min-w-[120px]"
        />
      ) : (
        <span
          className="text-sm font-medium cursor-text"
          onDoubleClick={() => setEditing(true)}
          title="Double-click to rename"
        >
          {name}
        </span>
      )}
    </div>
  );
};
