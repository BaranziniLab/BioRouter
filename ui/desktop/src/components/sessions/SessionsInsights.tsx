import { useEffect, useRef, useState } from 'react';
import { Greeting } from '../common/Greeting';
import { useNavigate } from 'react-router-dom';
import { Button } from '../ui/button';
import { ChatSmart } from '../icons/';
import { Skeleton } from '../ui/skeleton';
import { ActivityWindow, Session } from '../../api';
import { resumeSession } from '../../sessions';
import { useNavigation } from '../../hooks/useNavigation';
import { ReadableContent } from '../Layout/ReadableContent';
import { UsageHeatmap, UsageHeatmapLoading } from './UsageHeatmap';
import { getCachedSessionList, refreshSessionList } from '../../utils/sessionListCache';
import {
  cacheHomeRecentSessions,
  getCachedHomeActivity,
  getCachedRecentSessions,
  refreshHomeActivity,
} from '../../utils/homeInsightsCache';

const RECENT_LIMIT = 3;

// The Home view sheds its lower sections as it gets short, in a fixed order:
// the recent-chats list folds away FIRST, then the usage heatmap — so the
// greeting + composer are the last things standing. Thresholds are the available
// height (the scroll area above the composer), tuned on screen. Hysteresis is
// unnecessary: hiding a section only frees height (pushes further from the
// threshold) and showing one only consumes it, so the states cannot chase.
const FOLD_RECENTS_BELOW_PX = 660;
const FOLD_HEATMAP_BELOW_PX = 470;

export function SessionInsights() {
  const initialActivity = useRef(getCachedHomeActivity()).current;
  const initialSessions = useRef(
    getCachedSessionList()?.slice(0, RECENT_LIMIT) ?? getCachedRecentSessions()
  ).current;
  const [activity, setActivity] = useState<ActivityWindow | null>(initialActivity);
  const [activityFailed, setActivityFailed] = useState(false);
  const [recentSessions, setRecentSessions] = useState<Session[]>(initialSessions ?? []);
  const [isLoadingSessions, setIsLoadingSessions] = useState(initialSessions === null);
  const navigate = useNavigate();
  const setView = useNavigation();

  // Watch the height the Home view is given (its parent scroll area) and decide
  // which lower sections fit — recents fold before the heatmap.
  const rootRef = useRef<HTMLDivElement>(null);
  const [availableHeight, setAvailableHeight] = useState(Number.POSITIVE_INFINITY);
  useEffect(() => {
    const host = rootRef.current?.parentElement;
    if (!host) return;
    // clientHeight is 0 in jsdom (no layout) and briefly during mount — treat
    // that as "unknown, show everything", never as "too short, fold it all".
    const measure = () => {
      const h = host.clientHeight;
      if (h > 0) setAvailableHeight(h);
    };
    measure();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', measure);
      return () => window.removeEventListener('resize', measure);
    }
    const observer = new ResizeObserver(measure);
    observer.observe(host);
    return () => observer.disconnect();
  }, []);
  const showHeatmap = availableHeight >= FOLD_HEATMAP_BELOW_PX;
  const showRecents = availableHeight >= FOLD_RECENTS_BELOW_PX;

  useEffect(() => {
    // The heatmap carries the usage story now, so there is no separate insights
    // fetch to gate a full-page skeleton on — each section shows its own loading
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

    const loadRecentSessions = async () => {
      try {
        const sessions = await refreshSessionList();
        const refreshedRecentSessions = sessions.slice(0, RECENT_LIMIT);
        cacheHomeRecentSessions(refreshedRecentSessions);
        setRecentSessions(refreshedRecentSessions);
      } catch (error) {
        console.error('Failed to load recent sessions:', error);
      } finally {
        setIsLoadingSessions(false);
      }
    };

    loadActivity();
    loadRecentSessions();
  }, [initialActivity]);

  const handleSessionClick = async (session: Session) => {
    try {
      resumeSession(session, setView);
    } catch (error) {
      console.error('Failed to start session:', error);
      navigate('/sessions', {
        state: { selectedSessionId: session.id },
        replace: true,
      });
    }
  };

  const navigateToSessionHistory = () => {
    navigate('/sessions');
  };

  const formatDateOnly = (dateStr: string) => {
    const date = new Date(dateStr);
    return date
      .toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })
      .replace(/\//g, '/');
  };

  return (
    <div ref={rootRef} className="flex min-h-full flex-col bg-background-muted">
      {/* Hero — text directly on canvas. Aligned to the composer's column. */}
      <ReadableContent size="chat" className="biorouter-home-hero px-4 pb-6 pt-16 sm:px-6">
        <p className="text-xs font-medium text-text-muted tracking-widest mb-3">UCSF Biorouter</p>
        <Greeting />
      </ReadableContent>

      {/* Usage heatmap — the single source of the usage story. Folds away only
          after recents, when the window is too short for both. */}
      {showHeatmap && (
        <ReadableContent size="chat" className="biorouter-home-activity px-4 pb-8 sm:px-6">
          {activity ? (
            <div className="page-transition">
              <UsageHeatmap window={activity} />
            </div>
          ) : activityFailed ? null : ( // definitive failure: collapse, don't leave a void
            <UsageHeatmapLoading />
          )}
        </ReadableContent>
      )}

      {/* Recent chats — the first section to fold when the window gets short. */}
      {showRecents && (
        <ReadableContent
          size="chat"
          className="biorouter-home-recents page-transition px-4 pb-8 sm:px-6"
        >
        <div>
          <div className="flex justify-between items-center pb-2">
            <span className="text-xs font-medium text-text-muted uppercase tracking-wider">
              Recent chats
            </span>
            <Button
              variant="ghost"
              size="sm"
              className="text-xs text-text-muted !px-2 hover:bg-background-muted/60 hover:text-text-default"
              onClick={navigateToSessionHistory}
            >
              See all
            </Button>
          </div>

          <div className="biorouter-list-shell min-h-[96px] transition-all duration-300 ease-in-out">
            {isLoadingSessions ? (
              [200, 160, 220].map((w, i) => (
                <div
                  key={i}
                  className="biorouter-list-row flex items-center justify-between px-3 py-2"
                >
                  <div className="flex items-center space-x-2.5">
                    <Skeleton className="h-4 w-4 rounded-sm flex-shrink-0" />
                    <Skeleton style={{ width: w }} className="h-3.5" />
                  </div>
                  <Skeleton className="h-3.5 w-16" />
                </div>
              ))
            ) : recentSessions.length > 0 ? (
              recentSessions.map((session, index) => (
                <div
                  key={session.id}
                  className="biorouter-list-row session-item flex items-center justify-between text-sm px-3 py-2 cursor-pointer"
                  onClick={() => handleSessionClick(session)}
                  role="button"
                  tabIndex={0}
                  style={{ animationDelay: `${index * 0.1}s` }}
                  onKeyDown={async (e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      await handleSessionClick(session);
                    }
                  }}
                >
                  <div className="flex items-center space-x-2.5 min-w-0">
                    <ChatSmart className="h-4 w-4 text-text-muted flex-shrink-0" />
                    <span className="truncate max-w-[300px]">{session.name}</span>
                  </div>
                  <span className="text-text-muted text-xs flex-shrink-0">
                    {formatDateOnly(session.updated_at)}
                  </span>
                </div>
              ))
            ) : (
              <div className="text-text-muted text-sm py-3">No recent chat sessions found.</div>
            )}
          </div>
        </div>
        </ReadableContent>
      )}
    </div>
  );
}
