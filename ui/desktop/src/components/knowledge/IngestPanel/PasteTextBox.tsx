import { useMemo, useState } from 'react';

interface Props {
  onStage: (text: string, title: string, urls: string[]) => void;
  onCancel: () => void;
}

export function PasteTextBox({ onStage, onCancel }: Props) {
  const [text, setText] = useState('');
  const [title, setTitle] = useState('');

  const detectedUrls = useMemo(() => {
    // Redeclare regex inside useMemo to avoid sharing lastIndex state across calls
    const urlRe = /https?:\/\/[^\s<>"')]+[^\s<>"').,;:!?]/g;
    const set = new Set<string>();
    let m: RegExpExecArray | null;
    while ((m = urlRe.exec(text)) !== null) set.add(m[0]);
    return Array.from(set);
  }, [text]);

  const [includeUrls, setIncludeUrls] = useState<Record<string, boolean>>({});
  const urlsToFetch = detectedUrls.filter((u) => includeUrls[u] !== false);

  function toggleUrl(u: string) {
    setIncludeUrls((prev) => ({ ...prev, [u]: prev[u] !== false ? false : true }));
  }

  return (
    <div className="bg-background-surface border border-border-subtle rounded-xl overflow-hidden">
      <input
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Optional title…"
        className="w-full px-3 py-2 text-xs border-b border-border-subtle bg-transparent outline-none"
      />
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Paste notes, snippets, or a chunk of prose. URLs will be extracted and offered for ingestion."
        className="w-full min-h-[100px] px-3 py-2 text-xs bg-transparent outline-none resize-y"
      />
      {detectedUrls.length > 0 && (
        <div className="border-t border-border-subtle px-3 py-2 flex flex-wrap gap-1.5">
          <span className="text-[10px] text-text-muted self-center mr-1">Will fetch:</span>
          {detectedUrls.map((u) => {
            const on = includeUrls[u] !== false;
            return (
              <button
                key={u}
                onClick={() => toggleUrl(u)}
                className={`text-[10px] font-mono px-2 py-0.5 rounded-full border ${
                  on
                    ? 'border-border-default bg-background-muted'
                    : 'border-border-subtle text-text-muted line-through'
                }`}
              >
                {u.length > 36 ? u.substring(0, 33) + '…' : u}
              </button>
            );
          })}
        </div>
      )}
      <div className="border-t border-border-subtle px-3 py-2 flex justify-between items-center">
        <span className="text-[10px] text-text-muted">{text.length} chars</span>
        <div className="flex gap-1.5">
          <button
            onClick={onCancel}
            className="text-xs px-2.5 py-1 rounded text-text-muted hover:text-text-default"
          >
            Cancel
          </button>
          <button
            disabled={!text.trim()}
            onClick={() => onStage(text.trim(), title.trim() || 'Pasted note', urlsToFetch)}
            className="text-xs px-2.5 py-1 rounded bg-text-default text-background-surface font-medium disabled:opacity-50"
          >
            Stage
          </button>
        </div>
      </div>
    </div>
  );
}
