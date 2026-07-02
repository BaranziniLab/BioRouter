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
    <div className="biorouter-list-row overflow-hidden px-3 pt-3">
      <div className="overflow-hidden rounded-xl bg-background-default/72 shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--border-subtle)_42%,transparent),0_8px_18px_-22px_rgba(32,25,15,0.32)] transition-shadow focus-within:shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--block-teal)_28%,transparent),0_10px_22px_-22px_rgba(32,25,15,0.34)]">
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Optional title…"
          className="w-full bg-background-muted/38 px-3 py-2 text-xs text-text-default outline-none placeholder:text-textPlaceholder"
        />
        <div className="h-px bg-[color-mix(in_srgb,var(--border-subtle)_38%,transparent)]" />
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Paste knowledge, snippets, or a chunk of prose. URLs will be extracted and offered for ingestion."
          className="w-full min-h-[100px] resize-y bg-background-muted/22 px-3 py-2 text-xs text-text-default outline-none placeholder:text-textPlaceholder"
        />
      </div>
      {detectedUrls.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5 rounded-xl bg-background-default/60 px-3 py-2">
          <span className="text-[11px] text-text-muted self-center mr-1">Will fetch:</span>
          {detectedUrls.map((u) => {
            const on = includeUrls[u] !== false;
            return (
              <button
                key={u}
                onClick={() => toggleUrl(u)}
                className={`text-[11px] font-mono px-2 py-0.5 rounded-md border ${
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
      <div className="py-2 flex justify-between items-center">
        <span className="text-[11px] text-text-muted">{text.length} chars</span>
        <div className="flex gap-1.5">
          <button
            onClick={onCancel}
            className="text-xs px-2.5 py-1 rounded text-text-muted hover:text-text-default"
          >
            Cancel
          </button>
          <button
            disabled={!text.trim()}
            onClick={() => onStage(text.trim(), title.trim() || 'Pasted knowledge', urlsToFetch)}
            className="text-xs px-2.5 py-1 rounded bg-text-default text-white font-medium disabled:opacity-50 disabled:text-white"
          >
            Stage
          </button>
        </div>
      </div>
    </div>
  );
}
