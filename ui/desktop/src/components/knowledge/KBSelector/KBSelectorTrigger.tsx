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
        className="inline-flex w-full items-center gap-2 rounded-xl bg-background-default/60 px-3 py-2.5 shadow-[0_1px_2px_rgba(32,25,15,0.025)] transition-colors hover:bg-background-default focus:outline-none focus:ring-1 focus:ring-ring"
      >
        <span
          className="w-2 h-2 rounded-full flex-shrink-0"
          style={{ background: activeKb?.color ?? 'var(--text-muted)' }}
        />
        <span className="flex-1 text-left min-w-0">
          <span className="block text-sm font-medium truncate">
            {activeKb?.name ?? 'Focus a knowledge base'}
          </span>
        </span>
        <ChevronDown className="w-3 h-3 text-text-muted flex-shrink-0" />
      </button>
      {open && <KBSelectorPalette onClose={() => setOpen(false)} />}
    </>
  );
}
