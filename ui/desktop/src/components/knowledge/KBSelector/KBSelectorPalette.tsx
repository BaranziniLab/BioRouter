import { useEffect, useRef, useState } from 'react';
import { Search, Plus, FolderPlus } from 'lucide-react';
import { createBase } from '../../../api';
import { useKnowledge } from '../KnowledgeContext';
import type { Manifest } from '../../../api/types.gen';

interface Props {
  onClose: () => void;
}

type CreateItem = { create: true; slug: string; name: string };
type CreatePromptItem = { createPrompt: true };
type PaletteItem = Manifest | CreateItem | CreatePromptItem;

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
  const slug = slugify(query);
  const showCreate = query.length > 0 && slug.length > 0 && !filtered.some((b) => b.id === slug);
  const items: PaletteItem[] = [
    ...filtered,
    ...(showCreate ? [{ create: true as const, slug, name: query }] : []),
    // Always-visible "Create new knowledge base…" fallback. Clicking this
    // opens a window.prompt so users can name the KB without first typing
    // into the search field.
    { createPrompt: true as const },
  ];

  useEffect(() => { setCursor(0); }, [query]);

  async function performCreate(rawName: string): Promise<void> {
    const name = rawName.trim();
    if (!name) return;
    const id = slugify(name);
    if (!id) {
      window.alert('Please choose a name with at least one letter or number.');
      return;
    }
    try {
      const res = await createBase({ throwOnError: true, body: { id, name } });
      await refresh();
      if (res.data?.id) setActiveKbId(res.data.id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error('createBase failed', err);
      // Surface the error to the user rather than closing silently.
      window.alert(`Failed to create knowledge base: ${msg}`);
    }
  }

  function commitAt(i: number) {
    const it = items[i];
    if (!it) return;
    if ('createPrompt' in it) {
      const name = window.prompt('Name for the new knowledge base:');
      if (name === null) return; // user cancelled — keep palette open
      void (async () => {
        await performCreate(name);
        onClose();
      })();
    } else if ('create' in it) {
      // Guard against empty slugs (can happen if the user types only special characters).
      if (!it.slug) {
        console.warn('KBSelectorPalette: slugify produced an empty string for query:', query);
        return;
      }
      void (async () => {
        await performCreate(it.name);
        onClose();
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
          {filtered.length === 0 && query.length > 0 && !showCreate && (
            <div className="px-4 py-6 text-center text-sm text-text-muted">
              No knowledge base matches.
            </div>
          )}
          {items.map((it, i) => (
            <div
              key={
                'createPrompt' in it
                  ? '__createPrompt'
                  : 'create' in it
                    ? `__create_${it.slug}`
                    : it.id
              }
              onMouseEnter={() => setCursor(i)}
              onClick={() => commitAt(i)}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer ${
                i === cursor ? 'bg-background-muted' : ''
              }`}
            >
              {'createPrompt' in it ? (
                <>
                  <FolderPlus className="w-4 h-4 text-text-muted flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">
                      Create new knowledge base…
                    </div>
                    <div className="text-[10px] font-mono text-text-muted">
                      name it yourself
                    </div>
                  </div>
                </>
              ) : 'create' in it ? (
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
