import { Brain, ChevronDown } from 'lucide-react';
import type { ModelRef } from '../../../api/types.gen';

interface Props {
  value: ModelRef;
  // Plan-4-simple: read-only display. Plan-6 polish adds a real chooser.
  onChange: (v: ModelRef) => void;
}

export function IngestModelPicker({ value, onChange: _ }: Props) {
  return (
    <div className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border border-border-subtle bg-background-surface text-xs">
      <Brain className="w-3 h-3 text-text-muted" />
      <span className="truncate max-w-[200px]">
        {value.provider} / {value.model}
      </span>
      <ChevronDown className="w-3 h-3 text-text-muted" />
    </div>
  );
}
