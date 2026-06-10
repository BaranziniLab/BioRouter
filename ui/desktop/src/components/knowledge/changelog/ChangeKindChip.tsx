import type { ChangeKind } from '../../../api/types.gen';

const styleByKind: Record<ChangeKind, string> = {
  ingest: 'bg-background-info/15 text-text-info',
  link: 'bg-background-info/15 text-text-info',
  flag: 'bg-background-danger/15 text-text-danger',
  query: 'bg-background-success/15 text-text-success',
  lint: 'bg-background-warning/15 text-text-warning',
  restore: 'bg-background-warning/15 text-text-warning',
  manual: 'bg-background-medium text-text-muted',
};

export function ChangeKindChip({ kind }: { kind: ChangeKind }) {
  return (
    <span
      className={`inline-block text-[11px] uppercase tracking-wider rounded px-1.5 py-0.5 ${styleByKind[kind]}`}
    >
      {kind}
    </span>
  );
}
