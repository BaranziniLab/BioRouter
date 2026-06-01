// ui/desktop/src/components/bottom_menu/BottomMenuKnowledgeSelection.tsx
import { useState } from 'react';
import { BookOpen } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';
import { useKnowledge } from '../knowledge/KnowledgeContext';

export function BottomMenuKnowledgeSelection() {
  const { bases, activeKb, setActiveKbId } = useKnowledge();
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          className="flex items-center cursor-pointer text-text-default/70 hover:text-text-default text-xs"
          title="Active knowledge base"
        >
          <BookOpen className="mr-1 h-4 w-4" />
          <span className="max-w-[140px] truncate">{activeKb?.name ?? 'No KB'}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="center" className="w-64 p-1">
        {bases.length === 0 ? (
          <div className="p-3 text-xs text-text-default/70">
            No knowledge bases yet. Create one in the Knowledge view.
          </div>
        ) : (
          <div className="flex flex-col">
            <button
              onClick={() => {
                setActiveKbId(null);
                setOpen(false);
              }}
              className={`px-3 py-2 text-xs text-left rounded hover:bg-background-medium ${
                !activeKb ? 'text-text-default' : 'text-text-default/70'
              }`}
            >
              No active KB
            </button>
            {bases.map((b) => (
              <button
                key={b.id}
                onClick={() => {
                  setActiveKbId(b.id);
                  setOpen(false);
                }}
                className={`px-3 py-2 text-xs text-left rounded hover:bg-background-medium truncate ${
                  activeKb?.id === b.id
                    ? 'text-text-default font-medium'
                    : 'text-text-default/70'
                }`}
                title={b.id}
              >
                {b.name}
              </button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
