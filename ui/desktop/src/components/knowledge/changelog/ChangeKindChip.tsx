import type { ChangeKind } from '../../../api/types.gen';

const styleByKind: Record<ChangeKind, string> = {
  ingest: 'bg-blue-900/40 text-blue-200',
  link: 'bg-purple-900/40 text-purple-200',
  flag: 'bg-red-900/40 text-red-200',
  query: 'bg-green-900/40 text-green-200',
  lint: 'bg-yellow-900/40 text-yellow-200',
  restore: 'bg-orange-900/40 text-orange-200',
  manual: 'bg-zinc-800 text-zinc-300',
};

export function ChangeKindChip({ kind }: { kind: ChangeKind }) {
  return (
    <span
      className={`inline-block text-[10px] uppercase tracking-wide rounded px-1.5 py-0.5 ${styleByKind[kind]}`}
    >
      {kind}
    </span>
  );
}
