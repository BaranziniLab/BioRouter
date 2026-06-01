import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { useKnowledge } from '../KnowledgeContext';
import { KBSelectorPalette } from './KBSelectorPalette';

export function KBSelectorTrigger() {
  const { activeKb } = useKnowledge();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="w-full inline-flex items-center gap-2 px-3 py-2 rounded-lg border border-border-subtle bg-background-surface hover:border-border-default transition-colors"
      >
        <span
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ background: activeKb?.color ?? 'var(--text-muted)' }}
        />
        <span className="flex-1 text-left min-w-0">
          <span className="block text-sm font-medium truncate">
            {activeKb?.name ?? 'Select a knowledge base'}
          </span>
        </span>
        <ChevronDown className="w-3 h-3 text-text-muted flex-shrink-0" />
      </button>
      {open && <KBSelectorPalette onClose={() => setOpen(false)} />}
    </>
  );
}
