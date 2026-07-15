import { useEffect, useState } from 'react';
import { Greeting } from '../common/Greeting';
import { useNavigate } from 'react-router-dom';
import { Button } from '../ui/button';
import { ChatSmart } from '../icons/';
import { Skeleton } from '../ui/skeleton';
import { getSessionActivity, listSessions, ActivityWindow, Session } from '../../api';
import { resumeSession } from '../../sessions';
import { useNavigation } from '../../hooks/useNavigation';
import { ReadableContent } from '../Layout/ReadableContent';
import { UsageHeatmap, UsageHeatmapLoading } from './UsageHeatmap';

/** ~5 months of weeks; 22 columns fit the 760px chat column at 24px cells. */
const ACTIVITY_DAYS = 155;

const RECENT_LIMIT = 3;

export function SessionInsights() {
  const [activity, setActivity] = useState<ActivityWindow | null>(null);
  const [activityFailed, setActivityFailed] = useState(false);
  const [recentSessions, setRecentSessions] = useState<Session[]>([]);
  const [isLoadingSessions, setIsLoadingSessions] = useState(true);
  const navigate = useNavigate();
  const setView = useNavigation();

  useEffect(() => {
    // The heatmap carries the usage story now, so there is no separate insights
    // fetch to gate a full-page skeleton on — each section shows its own loading
    // state and the greeting renders instantly.
    const loadActivity = async () => {
      try {
        const response = await getSessionActivity<true>({
          query: { days: ACTIVITY_DAYS },
          throwOnError: true,
        });
        setActivity(response.data);
        setActivityFailed(false);
      } catch (error) {
        // A missing/old backend (404) or any failure must not leave a permanent
        // blank where the heatmap should be — collapse the section instead.
        console.error('Failed to load activity:', error);
        setActivityFailed(true);
      }
    };

    const loadRecentSessions = async () => {
      try {
        const response = await listSessions<true>({ throwOnError: true });
        setRecentSessions(response.data.sessions.slice(0, RECENT_LIMIT));
      } finally {
        setIsLoadingSessions(false);
      }
    };

    loadActivity();
    loadRecentSessions();
  }, []);

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
    <div className="flex min-h-full flex-col bg-background-muted">
      {/* Hero — text directly on canvas. Aligned to the composer's column. */}
      <ReadableContent size="chat" className="biorouter-home-hero px-4 pb-6 pt-16 sm:px-6">
        <p className="text-xs font-medium text-text-muted tracking-widest mb-3">UCSF Biorouter</p>
        <Greeting />
      </ReadableContent>

      {/* Usage heatmap — the single source of the usage story. */}
      <ReadableContent size="chat" className="biorouter-home-activity px-4 pb-8 sm:px-6">
        {activity ? (
          <div className="page-transition">
            <UsageHeatmap window={activity} />
          </div>
        ) : activityFailed ? null : ( // definitive failure: collapse, don't leave a void
          <UsageHeatmapLoading />
        )}
      </ReadableContent>

      {/* Recent chats */}
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
    </div>
  );
}
