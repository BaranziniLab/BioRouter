import { useEffect, useMemo, useRef, useState } from 'react';
import {
  Download,
  EyeOff,
  FolderInput,
  FolderPlus,
  Pencil,
  Search,
  Trash2,
} from '../../icons/app-icons';
import type { Manifest } from '../../../api/types.gen';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import { Input } from '../../ui/input';
import { Switch } from '../../ui/switch';
import { ConfirmationModal } from '../../ui/ConfirmationModal';
import BuiltInBadge from '../../ui/BuiltInBadge';
import { BUILTIN_RECREATED_TITLE, isBuiltinKnowledgeBase } from '../../../utils/builtins';
import { useKnowledge } from '../KnowledgeContext';
import { useKnowledgeBases } from '../hooks/useKnowledgeBases';

interface Props {
  onClose: () => void;
}

type DraftMode = { kind: 'create' } | { kind: 'rename'; base: Manifest } | null;

export function KBSelectorPalette({ onClose }: Props) {
  const {
    bases,
    primaryKbId,
    hiddenKbIds,
    defaultPrimaryKb,
    canFollowDefaultPrimary,
    followDefaultPrimary,
    refreshDefaultPrimary,
    refresh,
    setPrimaryKbId,
    toggleKbHidden,
  } = useKnowledge();
  const { create, exportArchive, importArchive, remove, rename } = useKnowledgeBases();
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState('');
  const [draftMode, setDraftMode] = useState<DraftMode>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [baseToDelete, setBaseToDelete] = useState<Manifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const importRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchRef.current?.focus();
    // Re-fetch the base list every time the palette opens so knowledge bases
    // created elsewhere (e.g. via chat / the knowledge MCP tools) appear here
    // without requiring a full app reload.
    void refresh();
    // Same reasoning for the machine-wide default: it is not part of the base
    // list and another chat may have moved it since this one last looked.
    void refreshDefaultPrimary();
  }, [refresh, refreshDefaultPrimary]);

  const filtered = useMemo(() => {
    const value = query.trim().toLowerCase();
    if (!value) {
      return bases;
    }
    return bases.filter(
      (base) => base.name.toLowerCase().includes(value) || base.id.toLowerCase().includes(value)
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
          setPrimaryKbId(manifest.id);
        }
      } else if (draftMode?.kind === 'rename') {
        setBusyId(draftMode.base.id);
        const manifest = await rename(draftMode.base.id, trimmed);
        if (primaryKbId === draftMode.base.id) {
          setPrimaryKbId(manifest.id);
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

  async function handleRemove() {
    if (!baseToDelete || busyId !== null) return;
    const base = baseToDelete;
    setError(null);
    setBusyId(base.id);
    try {
      await remove(base.id);
      setBaseToDelete(null);
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
    <>
      <Dialog open onOpenChange={(open) => !open && busyId === null && onClose()}>
        <DialogContent
          dismissible={busyId === null}
          className="sm:max-w-[760px] p-0 overflow-hidden"
        >
          <DialogHeader className="px-6 pt-6 pb-0">
            <DialogTitle>Knowledge Bases</DialogTitle>
            <DialogDescription>
              Choose which knowledge bases this chat uses, and which one of them is the primary —
              the base a knowledge write lands in when none is named.
            </DialogDescription>
          </DialogHeader>

          <div className="px-6 py-4 border-b border-border-subtle">
            <div className="flex h-control-md items-center gap-2 rounded-element border border-border-input bg-background-default px-3 transition-colors hover:border-border-strong focus-within:border-border-strong">
              <Search className="h-4 w-4 text-text-muted" />
              <input
                data-testid="knowledge-kb-search"
                ref={searchRef}
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search knowledge bases"
                className="flex-1 bg-transparent text-body outline-none placeholder:text-text-muted"
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
              <div className="biorouter-modal-panel mt-4 rounded-container p-4">
                <div className="mb-2 text-label">
                  {draftMode.kind === 'create'
                    ? 'Name your new knowledge base'
                    : `Rename "${draftMode.base.name}"`}
                </div>
                <div className="flex flex-col gap-2 sm:flex-row">
                  <Input
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
                    className="flex-1"
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

            {error && <div className="mt-3 text-body text-text-danger">{error}</div>}
          </div>

          {/*
            The way back to the default. It is deliberately not a third state on
            every row — the row is "in this chat" plus "make it primary", and a
            third control there would make the invariant look violable. This is
            one notice about the whole chat, and it exists only while the chat is
            holding a primary of its own: a chat already following the default
            has nothing to inherit, and saying so would be noise on every open.
          */}
          {canFollowDefaultPrimary && defaultPrimaryKb && (
            <div className="border-b border-border-subtle px-6 py-3">
              <div className="biorouter-modal-panel flex flex-col gap-3 rounded-container p-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0 text-supporting text-text-muted">
                  {primaryKbId
                    ? `This chat uses its own primary, so it no longer follows your default knowledge base, ${defaultPrimaryKb.name}.`
                    : `This chat has no primary knowledge base, even though your default is ${defaultPrimaryKb.name}. Deleting the base a chat was using leaves it this way.`}
                </div>
                <Button
                  data-testid="knowledge-kb-follow-default"
                  type="button"
                  variant="outline"
                  size="sm"
                  className="shrink-0"
                  onClick={followDefaultPrimary}
                  title={`This chat will use ${defaultPrimaryKb.name}, and will keep following your default if it changes.`}
                >
                  Follow the default ({defaultPrimaryKb.name})
                </Button>
              </div>
            </div>
          )}

          <div className="max-h-[420px] overflow-y-auto px-4 py-4">
            {filtered.length === 0 ? (
              <div className="biorouter-modal-panel rounded-container px-4 py-10 text-center text-body text-text-muted">
                No knowledge bases match this search.
              </div>
            ) : (
              <div className="biorouter-list-shell">
                {filtered.map((base) => {
                  const isPrimary = primaryKbId === base.id;
                  const isBusy = busyId === base.id;
                  const hidden = hiddenKbIds.includes(base.id);

                  return (
                    <div
                      key={base.id}
                      className={`biorouter-list-row flex items-center gap-3 px-3 py-2 transition-colors ${isPrimary ? 'bg-background-medium' : ''}`}
                    >
                      <button
                        type="button"
                        // Making a base primary is not a navigation: the palette
                        // is where the whole selection is managed, so it stays open.
                        onClick={() => setPrimaryKbId(base.id)}
                        className="flex min-w-0 flex-1 items-center gap-3 text-left"
                      >
                        <span
                          className="h-3 w-3 rounded-full border border-border-subtle"
                          style={{ background: base.color }}
                        />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <div className="truncate text-label text-text-default">{base.name}</div>
                            {isBuiltinKnowledgeBase(base.id) && (
                              <BuiltInBadge title={BUILTIN_RECREATED_TITLE} />
                            )}
                            {hidden && <Badge uppercase>Not in this chat</Badge>}
                          </div>
                          <div className="truncate text-supporting font-mono text-text-muted">
                            {base.id}
                          </div>
                        </div>
                        {isPrimary && (
                          <Badge uppercase tone="accent">
                            Primary
                          </Badge>
                        )}
                      </button>

                      <div className="flex items-center gap-2">
                        <div className="flex items-center gap-2 rounded-element border border-border-subtle bg-background-default px-2 py-1">
                          <EyeOff className="h-3.5 w-3.5 text-text-muted" />
                          <Switch
                            checked={!hidden}
                            onCheckedChange={() => toggleKbHidden(base.id)}
                            variant="mono"
                            aria-label={`Include ${base.name} in this chat`}
                          />
                        </div>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-8 px-2"
                          onClick={() => void handleExport(base)}
                          disabled={isBusy}
                          title="Export as .brkb"
                          aria-label={`Export ${base.name} as .brkb`}
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
                          aria-label={`Rename ${base.name}`}
                        >
                          <Pencil className="h-4 w-4" />
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          className="h-8 px-2 text-text-danger hover:text-text-danger/80"
                          onClick={() => setBaseToDelete(base)}
                          disabled={isBusy}
                          title="Delete knowledge base"
                          aria-label={`Delete ${base.name}`}
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
            <div className="text-supporting text-text-muted">
              Tip: the switch decides whether a base is in this chat; clicking its name makes it the
              primary. Bases left out are still reachable by naming them explicitly.
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={onClose}
              disabled={busyId !== null}
            >
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ConfirmationModal
        isOpen={baseToDelete !== null}
        title={`Delete "${baseToDelete?.name ?? ''}"?`}
        message="This permanently removes the knowledge base and cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={baseToDelete !== null && busyId === baseToDelete.id}
        onConfirm={() => void handleRemove()}
        onCancel={() => setBaseToDelete(null)}
      />
    </>
  );
}
