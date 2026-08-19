import { useEffect, useState } from 'react';
import { Check, FolderPlus, Settings } from '../../icons/app-icons';
import { Badge } from '../../ui/badge';
import { PrivacyBadge } from '../../ui/PrivacyBadge';
import { Button } from '../../ui/button';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '../../ui/command';
import { useKnowledge } from '../KnowledgeContext';
import { KbDot } from '../KbDot';
import { kbFormatLabel, LEGACY_FORMAT_TITLE } from '../kbFormat';

interface Props {
  /** Switch to the manager dialog. */
  onManage: () => void;
  /** Start a create flow (the manager owns the form). */
  onCreate: () => void;
  /** Picking a base closes the picker. */
  onClose: () => void;
}

/**
 * The KB picker: an anchored, searchable list of the bases this chat can see.
 *
 * This is the half of the old 493-line `KBSelectorPalette` that answers "which
 * base am I pointed at" (ui-spec §4.1). The other half — creating, renaming,
 * exporting, deleting and changing which bases are in the chat at all — is
 * `KBManagerDialog`, because a 760px modal is not what clicking a selector
 * should open.
 *
 * ⌘K in the Knowledge view opens THIS, not the manager: ⌘K has always meant
 * "switch base".
 *
 * ⚠ The list is `visibleBases`, not `bases`. A base hidden from this chat is
 * not a thing the chat can be pointed at, so offering it here would present a
 * choice that silently also un-hides. `Manage bases…` is one row away.
 */
export function KBSelectorMenu({ onManage, onCreate, onClose }: Props) {
  const {
    visibleBases,
    primaryKbId,
    defaultPrimaryKb,
    canFollowDefaultPrimary,
    followDefaultPrimary,
    refreshDefaultPrimary,
    refresh,
    setPrimaryKbId,
  } = useKnowledge();
  const [query, setQuery] = useState('');

  useEffect(() => {
    // Re-read on open: a base created in chat, and a machine-wide default moved
    // in another chat, are both invisible here otherwise until a full reload.
    void refresh();
    void refreshDefaultPrimary();
  }, [refresh, refreshDefaultPrimary]);

  const needle = query.trim().toLowerCase();
  const filtered = needle
    ? visibleBases.filter(
        (base) => base.name.toLowerCase().includes(needle) || base.id.toLowerCase().includes(needle)
      )
    : visibleBases;

  return (
    <Command
      label="Knowledge bases"
      query={query}
      onQueryChange={setQuery}
      className="max-h-[min(420px,60vh)]"
    >
      <CommandInput
        data-testid="knowledge-kb-search"
        placeholder="Search knowledge bases"
        aria-label="Search knowledge bases"
        autoFocus
      />

      {/*
        The way back to the default. Deliberately NOT a third state on every row
        — a row is "in this chat" plus "make it primary", and a third control
        there would make the invariant look violable. One notice about the whole
        chat, shown only while the chat holds a primary of its own: a chat
        already following the default has nothing to inherit (DR-12).
      */}
      {canFollowDefaultPrimary && defaultPrimaryKb && (
        <div className="flex-none border-b border-border-subtle px-2 py-2">
          <p className="text-supporting text-text-muted">
            {primaryKbId
              ? `This chat uses its own primary, so it no longer follows your default knowledge base, ${defaultPrimaryKb.name}.`
              : `This chat has no primary knowledge base, even though your default is ${defaultPrimaryKb.name}. Deleting the base a chat was using leaves it this way.`}
          </p>
          <Button
            data-testid="knowledge-kb-follow-default"
            type="button"
            variant="secondary"
            size="sm"
            className="mt-2 w-full"
            onClick={followDefaultPrimary}
            title={`This chat will use ${defaultPrimaryKb.name}, and will keep following your default if it changes.`}
          >
            Follow the default ({defaultPrimaryKb.name})
          </Button>
        </div>
      )}

      <CommandList aria-label="Knowledge bases">
        {filtered.length === 0 ? (
          <CommandEmpty>
            <p className="text-body text-text-muted">No knowledge bases match</p>
            <p className="mt-1 text-supporting text-text-muted">Try a different name or id.</p>
          </CommandEmpty>
        ) : (
          <CommandGroup>
            {filtered.map((base) => {
              const isPrimary = primaryKbId === base.id;
              return (
                <CommandItem
                  key={base.id}
                  selected={isPrimary}
                  onSelect={() => {
                    setPrimaryKbId(base.id);
                    onClose();
                  }}
                >
                  <KbDot color={base.color} />
                  <span data-row-name className="min-w-0 flex-1 truncate">{base.name}</span>
                  <Badge
                    uppercase
                    title={kbFormatLabel(base) === 'Legacy' ? LEGACY_FORMAT_TITLE : undefined}
                  >
                    {kbFormatLabel(base)}
                  </Badge>
                  {base.tier !== 'public' && <PrivacyBadge tier={base.tier} dense />}
                  {isPrimary && (
                    <Check
                      className="h-icon-row w-icon-row shrink-0 text-text-default"
                      aria-hidden="true"
                    />
                  )}
                </CommandItem>
              );
            })}
          </CommandGroup>
        )}

        <CommandSeparator />
        <CommandGroup>
          <CommandItem onSelect={onManage} data-testid="knowledge-kb-open-manager">
            <Settings className="h-icon-row w-icon-row shrink-0 text-text-muted" aria-hidden />
            <span data-row-name className="min-w-0 flex-1 truncate">Manage bases…</span>
          </CommandItem>
          <CommandItem onSelect={onCreate} data-testid="knowledge-kb-open-create">
            <FolderPlus className="h-icon-row w-icon-row shrink-0 text-text-muted" aria-hidden />
            <span data-row-name className="min-w-0 flex-1 truncate">Create knowledge base…</span>
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  );
}
