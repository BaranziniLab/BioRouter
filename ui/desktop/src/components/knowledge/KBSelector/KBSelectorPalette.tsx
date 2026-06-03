import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Download,
  FolderInput,
  FolderPlus,
  Pencil,
  Search,
  Trash2,
} from 'lucide-react';
import type { Manifest } from '../../../api/types.gen';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { useKnowledge } from '../KnowledgeContext';
import { useKnowledgeBases } from '../hooks/useKnowledgeBases';

interface Props {
  onClose: () => void;
}

type DraftMode =
  | { kind: 'create' }
  | { kind: 'rename'; base: Manifest }
  | null;

export function KBSelectorPalette({ onClose }: Props) {
  const { bases, activeKbId, refresh, setActiveKbId } = useKnowledge();
  const { create, exportArchive, importArchive, remove, rename } = useKnowledgeBases();
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState('');
  const [draftMode, setDraftMode] = useState<DraftMode>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const importRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  const filtered = useMemo(() => {
    const value = query.trim().toLowerCase();
    if (!value) {
      return bases;
    }
    return bases.filter(
      (base) =>
        base.name.toLowerCase().includes(value) || base.id.toLowerCase().includes(value)
    );
  }, [bases, query]);

  function slugify(input: string): string {
    return input
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .substring(0, 64);
  }

  function makeUniqueId(name: string): string {
    const baseSlug = slugify(name);
    if (!baseSlug) {
      return '';
    }
    if (!bases.some((base) => base.id === baseSlug)) {
      return baseSlug;
    }

    let suffix = 2;
    let candidate = `${baseSlug}-${suffix}`;
    while (bases.some((base) => base.id === candidate) && suffix < 1000) {
      suffix += 1;
      candidate = `${baseSlug}-${suffix}`;
    }
    return candidate;
  }

  function startCreate() {
    setError(null);
    setDraftMode({ kind: 'create' });
    setDraft(query.trim());
  }

  function startRename(base: Manifest) {
    setError(null);
    setDraftMode({ kind: 'rename', base });
    setDraft(base.name);
  }

  function resetDraft() {
    setDraft('');
    setDraftMode(null);
    setError(null);
  }

  async function submitDraft() {
    const trimmed = draft.trim();
    if (!trimmed) {
      setError('Please enter a name.');
      return;
    }

    try {
      if (draftMode?.kind === 'create') {
        const id = makeUniqueId(trimmed);
        if (!id) {
          setError('Please choose a name with letters or numbers.');
          return;
        }
        setBusyId('__create');
        const manifest = await create(id, trimmed);
        if (manifest?.id) {
          setActiveKbId(manifest.id);
        }
      } else if (draftMode?.kind === 'rename') {
        setBusyId(draftMode.base.id);
        const manifest = await rename(draftMode.base.id, trimmed);
        if (activeKbId === draftMode.base.id) {
          setActiveKbId(manifest.id);
        }
      }
      resetDraft();
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  async function handleRemove(base: Manifest) {
    if (!window.confirm(`Delete knowledge base "${base.name}"? This cannot be undone.`)) {
      return;
    }
    setError(null);
    setBusyId(base.id);
    try {
      await remove(base.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  async function handleExport(base: Manifest) {
    setError(null);
    setBusyId(base.id);
    try {
      await exportArchive(base.id, base.name);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  async function handleImport(file: File | null) {
    if (!file) {
      return;
    }
    setError(null);
    setBusyId('__import');
    try {
      await importArchive(file);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-[760px] p-0 overflow-hidden">
        <DialogHeader className="px-6 pt-6 pb-0">
          <DialogTitle>Knowledge Bases</DialogTitle>
          <DialogDescription>
            Switch, create, import, export, rename, and remove your knowledge bases.
          </DialogDescription>
        </DialogHeader>

        <div className="px-6 py-4 border-b border-border-subtle">
          <div className="flex items-center gap-2 rounded-xl border border-border-subtle bg-background-default px-3 py-2">
            <Search className="h-4 w-4 text-text-muted" />
            <input
              data-testid="knowledge-kb-search"
              ref={searchRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search knowledge bases"
              className="flex-1 bg-transparent text-sm outline-none"
            />
          </div>

          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              data-testid="knowledge-kb-create"
              type="button"
              variant="default"
              size="sm"
              onClick={startCreate}
            >
              <FolderPlus className="mr-1.5 h-4 w-4" />
              Create Knowledge Base
            </Button>
            <Button
              data-testid="knowledge-kb-import"
              type="button"
              variant="outline"
              size="sm"
              onClick={() => importRef.current?.click()}
              disabled={busyId === '__import'}
            >
              <FolderInput className="mr-1.5 h-4 w-4" />
              {busyId === '__import' ? 'Importing…' : 'Import from .brkb'}
            </Button>
            <input
              ref={importRef}
              type="file"
              accept=".brkb"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0] ?? null;
                void handleImport(file);
                e.target.value = '';
              }}
            />
          </div>

          {draftMode && (
            <div className="mt-4 rounded-xl border border-border-subtle bg-background-surface p-4">
              <div className="mb-2 text-sm font-medium">
                {draftMode.kind === 'create'
                  ? 'Name your new knowledge base'
                  : `Rename "${draftMode.base.name}"`}
              </div>
              <div className="flex flex-col gap-2 sm:flex-row">
                <input
                  data-testid="knowledge-kb-name-input"
                  type="text"
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      e.preventDefault();
                      void submitDraft();
                    }
                  }}
                  placeholder="Knowledge base name"
                  className="flex-1 rounded-lg border border-border-subtle bg-background-default px-3 py-2 text-sm outline-none transition-colors focus:border-border-default"
                />
                <div className="flex gap-2">
                  <Button type="button" variant="outline" size="sm" onClick={resetDraft}>
                    Cancel
                  </Button>
                  <Button
                    data-testid="knowledge-kb-submit"
                    type="button"
                    size="sm"
                    onClick={() => void submitDraft()}
                  >
                    {draftMode.kind === 'create' ? 'Create' : 'Save'}
                  </Button>
                </div>
              </div>
            </div>
          )}

          {error && <div className="mt-3 text-sm text-red-500">{error}</div>}
        </div>

        <div className="max-h-[420px] overflow-y-auto px-4 py-4">
          {filtered.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border-subtle px-4 py-10 text-center text-sm text-text-muted">
              No knowledge bases match this search.
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {filtered.map((base) => {
                const isActive = activeKbId === base.id;
                const isBusy = busyId === base.id;

                return (
                  <div
                    key={base.id}
                    className={`flex items-center gap-3 rounded-xl border px-3 py-3 transition-all ${
                      isActive
                        ? 'border-border-default bg-background-muted'
                        : 'border-border-subtle bg-background-surface hover:border-border-default hover:bg-background-muted/50'
                    }`}
                  >
                    <button
                      type="button"
                      onClick={() => {
                        setActiveKbId(base.id);
                        onClose();
                      }}
                      className="flex min-w-0 flex-1 items-center gap-3 text-left"
                    >
                      <span
                        className="h-3 w-3 rounded-full border border-black/10"
                        style={{ background: base.color }}
                      />
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-sm font-medium text-text-default">
                          {base.name}
                        </div>
                        <div className="truncate text-[11px] font-mono text-text-muted">
                          {base.id}
                        </div>
                      </div>
                      {isActive && (
                        <span className="rounded-full bg-background-default px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-text-muted">
                          Active
                        </span>
                      )}
                    </button>

                    <div className="flex items-center gap-1">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-8 px-2"
                        onClick={() => void handleExport(base)}
                        disabled={isBusy}
                        title="Export as .brkb"
                      >
                        <Download className="h-4 w-4" />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-8 px-2"
                        onClick={() => startRename(base)}
                        disabled={isBusy}
                        title="Rename knowledge base"
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="h-8 px-2 text-red-500 hover:text-red-600"
                        onClick={() => void handleRemove(base)}
                        disabled={isBusy}
                        title="Delete knowledge base"
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <DialogFooter className="border-t border-border-subtle px-6 py-4 sm:justify-between">
          <div className="text-xs text-text-muted">
            Tip: select a knowledge base here, then ingest content from the left panel.
          </div>
          <Button type="button" variant="outline" size="sm" onClick={onClose}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
