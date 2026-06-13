import { useMemo, useState } from 'react';
import { BookOpen } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';
import { Switch } from '../ui/switch';
import BuiltInBadge from '../ui/BuiltInBadge';
import { BUILTIN_RECREATED_TITLE, isBuiltinKnowledgeBase } from '../../utils/builtins';
import { useKnowledge } from '../knowledge/KnowledgeContext';

function labelForVisibleCount(count: number): string {
  if (count === 0) {
    return 'No KBs visible';
  }
  if (count === 1) {
    return '1 KB visible';
  }
  return `${count} KBs visible`;
}

export function BottomMenuKnowledgeSelection() {
  const {
    bases,
    visibleBases,
    hiddenKbIds,
    toggleKbHidden,
    hideAllKnowledgeBases,
    showAllKnowledgeBases,
    activeKb,
  } = useKnowledge();
  const [open, setOpen] = useState(false);

  const allHidden = useMemo(
    () => bases.length > 0 && hiddenKbIds.length === bases.length,
    [bases.length, hiddenKbIds.length]
  );

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className="flex items-center cursor-pointer text-text-default/70 hover:text-text-default text-xs min-w-0"
          title="Knowledge bases visible to chat discovery"
        >
          <BookOpen className="mr-1 h-4 w-4 flex-shrink-0" />
          <span className="max-w-[160px] truncate">
            {labelForVisibleCount(visibleBases.length)}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="center" className="w-80 p-0">
        <div className="border-b border-border-subtle px-4 py-3">
          <div className="text-sm font-medium text-text-default">Chat knowledge discovery</div>
          <div className="mt-1 text-xs text-text-muted">
            Hidden knowledge bases stay editable in the Knowledge tab, but chats will not search
            them by default.
          </div>
        </div>
        {bases.length === 0 ? (
          <div className="p-4 text-xs text-text-default/70">
            No knowledge bases yet. Create one in the Knowledge view.
          </div>
        ) : (
          <>
            <div className="flex items-center justify-between gap-2 border-b border-border-subtle px-4 py-3">
              <div className="text-xs text-text-muted">
                {labelForVisibleCount(visibleBases.length)}
              </div>
              <button
                type="button"
                className="text-xs text-text-default/70 hover:text-text-default"
                onClick={() => {
                  if (allHidden) {
                    showAllKnowledgeBases();
                  } else {
                    hideAllKnowledgeBases();
                  }
                }}
              >
                {allHidden ? 'Show all' : 'Hide all'}
              </button>
            </div>
            <div className="max-h-80 overflow-y-auto p-2">
              {bases.map((base) => {
                const hidden = hiddenKbIds.includes(base.id);
                return (
                  <div
                    key={base.id}
                    className="flex items-center gap-3 rounded-lg px-2 py-2 hover:bg-background-medium/40"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5 min-w-0">
                        <div className="truncate text-sm text-text-default">{base.name}</div>
                        {isBuiltinKnowledgeBase(base.id) && (
                          <BuiltInBadge title={BUILTIN_RECREATED_TITLE} />
                        )}
                      </div>
                      <div className="truncate text-[11px] text-text-muted">
                        {hidden
                          ? 'Hidden from chat discovery'
                          : activeKb?.id === base.id
                            ? 'Visible to chat discovery · focused in Knowledge'
                            : 'Visible to chat discovery'}
                      </div>
                    </div>
                    <Switch
                      checked={!hidden}
                      onCheckedChange={() => toggleKbHidden(base.id)}
                      variant="mono"
                      aria-label={`Toggle visibility for ${base.name}`}
                    />
                  </div>
                );
              })}
            </div>
          </>
        )}
      </PopoverContent>
    </Popover>
  );
}
