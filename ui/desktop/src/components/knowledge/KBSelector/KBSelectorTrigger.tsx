import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { Badge } from '../../ui/badge';
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
        className="group flex h-9 w-full items-center gap-2.5 rounded-md border border-border-input bg-background-default px-3 text-sm transition-colors duration-[var(--motion-fast)] hover:border-border-strong"
      >
        <span
          className="h-2 w-2 flex-shrink-0 rounded-full"
          style={{ background: activeKb?.color ?? 'var(--text-muted)' }}
        />
        <span className="min-w-0 flex-1 truncate text-left font-semibold">
          {activeKb?.name ?? 'Focus a knowledge base'}
        </span>
        <Badge uppercase className="text-[10px]">
          KB
        </Badge>
        <ChevronDown
          className={`h-4 w-4 shrink-0 text-text-muted transition-transform duration-[var(--motion-fast)] ${open ? 'rotate-180' : ''}`}
          strokeWidth={1.5}
        />
      </button>
      {open && <KBSelectorPalette onClose={() => setOpen(false)} />}
    </>
  );
}
