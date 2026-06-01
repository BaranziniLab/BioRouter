import { useEffect, useRef, useState } from 'react';
import { Search, Plus } from 'lucide-react';
import { createBase } from '../../../api';
import { useKnowledge } from '../KnowledgeContext';
import type { Manifest } from '../../../api/types.gen';

interface Props {
  onClose: () => void;
}

type CreateItem = { create: true; slug: string; name: string };
type PaletteItem = Manifest | CreateItem;

export function KBSelectorPalette({ onClose }: Props) {
  const { bases, refresh, setActiveKbId } = useKnowledge();
  const [query, setQuery] = useState('');
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  // Filter bases by name (case-insensitive substring).
  const filtered: Manifest[] = bases.filter((b) =>
    b.name.toLowerCase().includes(query.toLowerCase())
  );
  const showCreate = query.length > 0 && !filtered.some((b) => b.id === slugify(query));
  const items: PaletteItem[] = [
    ...filtered,
    ...(showCreate ? [{ create: true as const, slug: slugify(query), name: query }] : []),
  ];

  useEffect(() => { setCursor(0); }, [query]);

  function commitAt(i: number) {
    const it = items[i];
    if (!it) return;
    if ('create' in it) {
      void (async () => {
        try {
          const res = await createBase({ throwOnError: true, body: { id: it.slug, name: it.name } });
          await refresh();
          if (res.data?.id) setActiveKbId(res.data.id);
        } catch (err) {
          console.error('createBase failed', err);
        } finally {
          onClose();
        }
      })();
    } else {
      setActiveKbId(it.id);
      onClose();
    }
  }

  function onKey(e: React.KeyboardEvent) {
    if (e.key === 'Escape') onClose();
    else if (e.key === 'ArrowDown') { e.preventDefault(); setCursor((c) => Math.min(c + 1, items.length - 1)); }
    else if (e.key === 'ArrowUp')   { e.preventDefault(); setCursor((c) => Math.max(c - 1, 0)); }
    else if (e.key === 'Enter')     { e.preventDefault(); commitAt(cursor); }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-24 bg-black/30 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="w-[540px] max-w-[92vw] max-h-[70vh] bg-background-surface border border-border-subtle rounded-2xl shadow-2xl overflow-hidden flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-4 py-3 border-b border-border-subtle">
          <Search className="w-4 h-4 text-text-muted" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder="Switch knowledge base… type to search"
            className="flex-1 bg-transparent outline-none text-sm"
          />
          <kbd className="text-[10px] font-mono text-text-muted border border-border-subtle rounded px-1.5 py-0.5">
            esc
          </kbd>
        </div>
        <div className="flex-1 overflow-y-auto p-2">
          {items.length === 0 && (
            <div className="px-4 py-6 text-center text-sm text-text-muted">
              No knowledge base matches.
            </div>
          )}
          {items.map((it, i) => (
            <div
              key={'create' in it ? `__create_${it.slug}` : it.id}
              onMouseEnter={() => setCursor(i)}
              onClick={() => commitAt(i)}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer ${
                i === cursor ? 'bg-background-muted' : ''
              }`}
            >
              {'create' in it ? (
                <>
                  <Plus className="w-3 h-3 text-text-muted" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">Create &ldquo;{it.name}&rdquo;</div>
                    <div className="text-[10px] font-mono text-text-muted">new knowledge base</div>
                  </div>
                </>
              ) : (
                <>
                  <span className="w-2 h-2 rounded-full flex-shrink-0" style={{ background: it.color }} />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{it.name}</div>
                    <div className="text-[10px] font-mono text-text-muted">{it.id}</div>
                  </div>
                </>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function slugify(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .substring(0, 64);
}
