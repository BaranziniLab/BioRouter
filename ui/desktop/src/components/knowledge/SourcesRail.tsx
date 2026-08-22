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
 * the `h-row` header strip; the ingest panel owns the scroller and, beside it
 * rather than inside it, the action footer.
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
      {/* ⚠ **THE RAIL NO LONGER OWNS ONE SCROLL CONTAINER** (R-06). It did, and
          the ingest panel pinned its footer `sticky bottom-0` inside it — so the
          footer painted OVER the body by DOM order and occluded 109–149px of the
          scroll region. That already bit the paste box, which mounted at y=790
          in an 887px window underneath the pinned strip and read as a dead
          button, and was patched with a runtime-measured `scroll-margin-bottom`.
          The footer is now a flex SIBLING of the scroller: it cannot occlude
          anything, and the workaround is deleted rather than maintained. */}
      <div className="flex min-h-0 flex-1 flex-col">
        {kb && (
          <div className="flex-none px-4 pt-4">
            <KbTierPanel kb={kb} />
          </div>
        )}
        <IngestPanel />
      </div>
    </section>
  );
}
