import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { useKnowledge } from '../KnowledgeContext';
import { KBSelectorPalette } from './KBSelectorPalette';

interface Props {
  /** Controlled open state. When provided, the trigger acts as a controlled component. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

export function KBSelectorTrigger({ open: openProp, onOpenChange }: Props) {
  const { activeKb } = useKnowledge();
  const [openInternal, setOpenInternal] = useState(false);

  // Support both controlled (open+onOpenChange) and uncontrolled usage.
  const isControlled = openProp !== undefined;
  const open = isControlled ? openProp : openInternal;
  const setOpen = isControlled ? (v: boolean) => onOpenChange?.(v) : setOpenInternal;

  return (
    <>
      <button
        data-testid="knowledge-kb-selector-trigger"
        onClick={() => setOpen(true)}
        className="group inline-flex w-full items-center gap-3 rounded-xl border border-border-subtle bg-background-default px-3 py-3 transition-colors duration-150 hover:bg-background-medium/82 focus:outline-none focus:ring-1 focus:ring-ring"
      >
        <span
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ background: activeKb?.color ?? 'var(--text-muted)' }}
        />
        <span className="flex-1 text-left min-w-0">
          <span className="block text-sm font-semibold truncate">
            {activeKb?.name ?? 'Focus a knowledge base'}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-1 rounded-md bg-background-muted px-2 py-1 text-[10px] font-medium uppercase tracking-wider text-text-muted transition-colors group-hover:bg-background-default/72 group-hover:text-text-default">
          KB
          <ChevronDown className="w-3 h-3" />
        </span>
      </button>
      {open && <KBSelectorPalette onClose={() => setOpen(false)} />}
    </>
  );
}
