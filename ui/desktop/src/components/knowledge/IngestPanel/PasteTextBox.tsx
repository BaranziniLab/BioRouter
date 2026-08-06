import { useMemo, useState } from 'react';
import { Button } from '../../ui/button';

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
    <div className="overflow-hidden rounded-container border border-border-subtle bg-background-default transition-colors focus-within:border-border-strong">
      <div className="overflow-hidden">
        <input
          type="text"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Optional title…"
          className="w-full bg-transparent px-3 py-2 text-body text-text-default placeholder:text-text-muted"
        />
        <div className="h-px bg-border-subtle" />
        <textarea
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Paste knowledge, snippets, or a chunk of prose. URLs will be extracted and offered for ingestion."
          className="w-full min-h-[100px] resize-y bg-transparent px-3 py-2 text-body text-text-default placeholder:text-text-muted"
        />
      </div>
      {detectedUrls.length > 0 && (
        <div className="mx-3 mt-2 flex flex-wrap gap-1.5 rounded-element bg-background-muted px-3 py-2">
          <span className="mr-1 self-center text-supporting text-text-muted">Will fetch:</span>
          {detectedUrls.map((u) => {
            const on = includeUrls[u] !== false;
            return (
              <button
                key={u}
                onClick={() => toggleUrl(u)}
                className={`rounded-inner px-2 py-0.5 font-mono text-supporting transition-colors ${on ? 'bg-overlay-selected text-text-default' : 'text-text-muted line-through hover:text-text-default'}`}
              >
                {u.length > 36 ? u.substring(0, 33) + '…' : u}
              </button>
            );
          })}
        </div>
      )}
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-supporting text-text-muted">{text.length} chars</span>
        <div className="flex gap-1.5">
          <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!text.trim()}
            onClick={() => onStage(text.trim(), title.trim() || 'Pasted knowledge', urlsToFetch)}
          >
            Stage
          </Button>
        </div>
      </div>
    </div>
  );
}
