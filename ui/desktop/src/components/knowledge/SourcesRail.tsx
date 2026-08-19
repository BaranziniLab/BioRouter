import type { KbTier } from '../../api';
import { KbTierPanel } from './KbTierControl';
import { IngestPanel } from './IngestPanel/IngestPanel';

interface Props {
  /** The base the rail acts on, or `null` when nothing is primary. */
  kb: { id: string; name: string; tier: KbTier } | null;
  className?: string;
}

/**
 * The Sources rail — the left column of the Knowledge workspace (ui-spec §4.4).
 *
 * A flat pane: `--radius-container`, one hairline, `box-shadow: none`. It owns
 * the `h-row` header strip and the single scroll container; the ingest panel
 * owns its own pinned footer inside it.
 *
 * Body order is the specified one — tier control, then dropzone, paste, warnings,
 * staged list, digest progress. The **tier control comes first** and lives here
 * rather than in a settings page, because it is a decision about the base you
 * are looking at (issue #56 DR-18): a user reading a private base would never
 * meet it anywhere else.
 */
export function SourcesRail({ kb, className = '' }: Props) {
  return (
    <section
      aria-label="Sources"
      className={`flex min-h-0 flex-col overflow-hidden rounded-container border border-border-subtle bg-background-default ${className}`.trim()}
    >
      <div className="flex h-row flex-none items-center justify-between gap-2 border-b border-border-subtle px-3">
        <h2 className="text-caps text-text-muted">Sources</h2>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {kb && (
          <div className="px-4 pt-4">
            <KbTierPanel kb={kb} />
          </div>
        )}
        <IngestPanel />
      </div>
    </section>
  );
}
