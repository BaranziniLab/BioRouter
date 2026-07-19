import { useMemo, useState } from 'react';
import { BookOpen } from '../icons/app-icons';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '../ui/dropdown-menu';
import { Input } from '../ui/input';
import { Switch } from '../ui/switch';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';
import BuiltInBadge from '../ui/BuiltInBadge';
import { BUILTIN_RECREATED_TITLE, isBuiltinKnowledgeBase } from '../../utils/builtins';
import { useKnowledge } from '../knowledge/KnowledgeContext';

export function BottomMenuKnowledgeSelection() {
  const {
    bases,
    visibleBases,
    hiddenKbIds,
    toggleKbHidden,
    hideAllKnowledgeBases,
    showAllKnowledgeBases,
  } = useKnowledge();
  const [open, setOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const filteredBases = useMemo(
    () =>
      bases.filter((base) => base.name.toLowerCase().includes(searchQuery.trim().toLowerCase())),
    [bases, searchQuery]
  );

  const filteredVisibleCount = useMemo(
    () => filteredBases.filter((base) => !hiddenKbIds.includes(base.id)).length,
    [filteredBases, hiddenKbIds]
  );

  const handleBulkToggle = () => {
    if (filteredBases.length === 0) return;

    const targetVisible = filteredVisibleCount === 0;
    if (!searchQuery.trim()) {
      if (targetVisible) {
        showAllKnowledgeBases();
      } else {
        hideAllKnowledgeBases();
      }
      return;
    }

    filteredBases.forEach((base) => {
      const visible = !hiddenKbIds.includes(base.id);
      if (visible !== targetVisible) toggleKbHidden(base.id);
    });
  };

  return (
    <DropdownMenu
      open={open}
      onOpenChange={(nextOpen) => {
        setOpen(nextOpen);
        if (!nextOpen) setSearchQuery('');
      }}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="flex h-7 items-center rounded-md px-0.5 cursor-pointer [&_svg]:size-4 text-text-default/70 hover:bg-background-medium hover:text-text-default text-xs"
              aria-label={`Manage knowledge bases (${visibleBases.length} visible)`}
            >
              <BookOpen className="mr-0.5 h-4 w-4" />
              <span>{visibleBases.length}</span>
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">Manage knowledge bases</TooltipContent>
      </Tooltip>
      <DropdownMenuContent
        side="top"
        align="center"
        className="w-64 font-sans"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        <div className="p-2">
          <Input
            type="text"
            placeholder="search knowledge bases..."
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            className="h-8 text-sm"
            autoFocus
          />
          {filteredBases.length > 0 && (
            <button
              type="button"
              onClick={handleBulkToggle}
              className="mt-1.5 cursor-pointer text-xs text-text-default/70 underline-offset-2 hover:text-text-default hover:underline"
            >
              {filteredVisibleCount === 0
                ? `Show all (${filteredBases.length})`
                : `Hide all (${filteredVisibleCount})`}
            </button>
          )}
        </div>
        <div className="max-h-[400px] overflow-y-auto">
          {filteredBases.length === 0 ? (
            <div className="px-2 py-4 text-center text-sm text-text-default/70">
              {searchQuery ? 'no knowledge bases found' : 'no knowledge bases available'}
            </div>
          ) : (
            filteredBases.map((base) => {
              const hidden = hiddenKbIds.includes(base.id);
              return (
                <DropdownMenuCheckboxItem
                  key={base.id}
                  checked={!hidden}
                  showIndicator={false}
                  onCheckedChange={() => toggleKbHidden(base.id)}
                  onSelect={(event) => event.preventDefault()}
                  className="flex cursor-pointer items-center justify-between px-2 py-2 transition-colors duration-[var(--motion-fast)] hover:bg-background-medium"
                >
                  <div className="flex min-w-0 items-center gap-1.5 pr-2">
                    <div className="truncate text-sm font-medium text-text-default">
                      {base.name}
                    </div>
                    {isBuiltinKnowledgeBase(base.id) && (
                      <BuiltInBadge title={BUILTIN_RECREATED_TITLE} />
                    )}
                  </div>
                  <div className="pointer-events-none" aria-hidden="true">
                    <Switch checked={!hidden} variant="mono" tabIndex={-1} aria-hidden="true" />
                  </div>
                </DropdownMenuCheckboxItem>
              );
            })
          )}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
