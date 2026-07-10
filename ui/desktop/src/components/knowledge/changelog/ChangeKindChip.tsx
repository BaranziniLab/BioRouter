import type { ChangeKind } from '../../../api/types.gen';
import { Badge, type BadgeTone } from '../../ui/badge';

const toneByKind: Record<ChangeKind, BadgeTone> = {
  ingest: 'info',
  link: 'info',
  flag: 'danger',
  query: 'success',
  lint: 'warning',
  restore: 'warning',
  manual: 'neutral',
};

export function ChangeKindChip({ kind }: { kind: ChangeKind }) {
  return (
    <Badge tone={toneByKind[kind]} uppercase>
      {kind}
    </Badge>
  );
}
