import { useEffect, useRef, useState } from 'react';
import { Greeting } from '../common/Greeting';
import { ActivityWindow } from '../../api';
import { ReadableContent } from '../Layout/ReadableContent';
import { UsageHeatmap, UsageHeatmapLoading } from './UsageHeatmap';
import { getCachedHomeActivity, refreshHomeActivity } from '../../utils/homeInsightsCache';

// The Home view no longer folds sections away or lists recent chats (the
// sidebar's Recents already owns that job). The hero compacts via container
// queries (main.css) and the heatmap scales its cells to whatever box is left
// (see UsageHeatmap), so the usage story survives even the smallest window.

export function SessionInsights() {
  const initialActivity = useRef(getCachedHomeActivity()).current;
  const [activity, setActivity] = useState<ActivityWindow | null>(initialActivity);
  const [activityFailed, setActivityFailed] = useState(false);

  useEffect(() => {
    // The heatmap carries the usage story now, so there is no separate insights
    // fetch to gate a full-page skeleton on — the section shows its own loading
    // state and the greeting renders instantly.
    const loadActivity = async () => {
      try {
        const refreshedActivity = await refreshHomeActivity();
        setActivity(refreshedActivity);
        setActivityFailed(false);
      } catch (error) {
        // A missing/old backend (404) or any failure must not leave a permanent
        // blank where the heatmap should be — collapse the section instead.
        console.error('Failed to load activity:', error);
        if (!initialActivity) setActivityFailed(true);
      }
    };

    loadActivity();
  }, [initialActivity]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-background-canvas">
      {/* Hero — text directly on canvas. Aligned to the composer's column. */}
      <ReadableContent
        size="chat"
        className="biorouter-home-hero shrink-0 px-4 pb-6 pt-16 sm:px-6"
      >
        <p className="text-xs font-medium text-text-muted tracking-widest mb-3">UCSF Biorouter</p>
        <Greeting />
      </ReadableContent>

      {/* Usage heatmap — the single source of the usage story. It is never
          folded away: the flex-1 box hands it the height the window can spare
          (min-h floor keeps it legible) and UsageHeatmap sizes its cells to
          fit. If a pathological squeeze leaves less than the floor, the Hub
          scrolls rather than letting anything overlap. */}
      <ReadableContent
        size="chat"
        className="biorouter-home-activity min-h-[190px] min-w-0 flex-1 px-4 pb-6 sm:px-6"
      >
        {activity ? (
          <div className="page-transition h-full">
            <UsageHeatmap window={activity} />
          </div>
        ) : activityFailed ? null : ( // definitive failure: collapse, don't leave a void
          <UsageHeatmapLoading />
        )}
      </ReadableContent>
    </div>
  );
}
