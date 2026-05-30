import { useEffect, useState } from 'react';
import { Greeting } from '../common/Greeting';
import { useNavigate } from 'react-router-dom';
import { Button } from '../ui/button';
import { ChatSmart } from '../icons/';
import { Skeleton } from '../ui/skeleton';
import {
  getSessionInsights,
  listSessions,
  Session,
  SessionInsights as ApiSessionInsights,
} from '../../api';
import { resumeSession } from '../../sessions';
import { useNavigation } from '../../hooks/useNavigation';

export function SessionInsights() {
  const [insights, setInsights] = useState<ApiSessionInsights | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recentSessions, setRecentSessions] = useState<Session[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingSessions, setIsLoadingSessions] = useState(true);
  const navigate = useNavigate();
  const setView = useNavigation();

  useEffect(() => {
    let loadingTimeout: ReturnType<typeof setTimeout>;

    const loadInsights = async () => {
      try {
        const response = await getSessionInsights({ throwOnError: true });
        setInsights(response.data);
        setError(null);
      } catch (error) {
        console.error('Failed to load insights:', error);
        setError(error instanceof Error ? error.message : 'Failed to load insights');
        setInsights({
          totalSessions: 0,
          totalTokens: 0,
          sessionsLast7Days: 0,
          sessionsLast30Days: 0,
          tokensLast7Days: 0,
          tokensLast30Days: 0,
        });
      } finally {
        setIsLoading(false);
      }
    };

    const loadRecentSessions = async () => {
      try {
        const response = await listSessions<true>({ throwOnError: true });
        setRecentSessions(response.data.sessions.slice(0, 3));
      } finally {
        setIsLoadingSessions(false);
      }
    };

    loadingTimeout = setTimeout(() => {
      setInsights((currentInsights) => {
        if (!currentInsights) {
          console.warn('Loading timeout reached, showing fallback content');
          setError('Failed to load insights');
          setIsLoading(false);
          return {
            totalSessions: 0,
            totalTokens: 0,
            sessionsLast7Days: 0,
            sessionsLast30Days: 0,
            tokensLast7Days: 0,
            tokensLast30Days: 0,
          };
        }
        setIsLoading(false);
        return currentInsights;
      });
    }, 10000);

    loadInsights();
    loadRecentSessions();

    return () => {
      if (loadingTimeout) {
        window.clearTimeout(loadingTimeout);
      }
    };
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

  const formatTokens = (tokens: number | undefined): string => {
    return new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 2 }).format(
      tokens || 0
    );
  };

  const formatCount = (count: number | undefined): string => {
    return Math.max(count ?? 0, 0).toLocaleString();
  };

  const renderSkeleton = () => (
    <div className="bg-background-muted flex flex-col h-full">
      <div className="px-8 pt-16 pb-4">
        <Greeting />
      </div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-x-12 gap-y-8 px-8 pb-8">
        {(['Sessions', 'Tokens'] as const).map((group) => (
          <div key={group}>
            <p className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-3">
              {group}
            </p>
            <div className="flex items-end gap-8">
              {[0, 1, 2].map((i) => (
                <div key={i}>
                  <Skeleton className="h-6 w-14 mb-1.5" />
                  <Skeleton className="h-3 w-16" />
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="px-8 pb-8">
        <div className="flex justify-between items-center pb-3 border-b border-border-subtle">
          <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">Recent chats</span>
        </div>
        <div className="space-y-0 min-h-[96px]">
          {[200, 160, 220].map((w, i) => (
            <div key={i} className="flex items-center justify-between py-3 border-b border-border-subtle last:border-0">
              <div className="flex items-center space-x-2.5">
                <Skeleton className="h-4 w-4 rounded-sm flex-shrink-0" />
                <Skeleton style={{ width: w }} className="h-3.5" />
              </div>
              <Skeleton className="h-3.5 w-16" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );

  if (isLoading) {
    return renderSkeleton();
  }

  return (
    <div className="bg-background-muted flex flex-col h-full">
      {/* Hero — text directly on canvas */}
      <div className="px-8 pt-16 pb-6">
        <p className="text-xs font-medium text-text-muted tracking-widest mb-3">UCSF Biorouter</p>
        <Greeting />
      </div>

      {/* Grouped stats — Sessions / Tokens, each with Total · Past 30d · Past 7d */}
      <div className="px-8 pb-8">
        {error ? (
          <div className="flex items-center gap-2 text-xs text-orange-600 dark:text-orange-400">
            <div className="w-2 h-2 bg-orange-400 rounded-full flex-shrink-0" />
            Failed to load insights
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-x-12 gap-y-8 page-transition">
            {([
              {
                heading: 'Sessions',
                items: [
                  { value: formatCount(insights?.totalSessions), label: 'Total' },
                  {
                    value: formatCount(insights?.sessionsLast30Days),
                    label: 'Past 30 days',
                  },
                  {
                    value: formatCount(insights?.sessionsLast7Days),
                    label: 'Past 7 days',
                  },
                ],
              },
              {
                heading: 'Tokens',
                items: [
                  { value: formatTokens(insights?.totalTokens), label: 'Total' },
                  {
                    value: formatTokens(insights?.tokensLast30Days),
                    label: 'Past 30 days',
                  },
                  {
                    value: formatTokens(insights?.tokensLast7Days),
                    label: 'Past 7 days',
                  },
                ],
              },
            ] as const).map((group) => (
              <div key={group.heading}>
                <p className="text-[11px] font-medium text-text-muted uppercase tracking-wider mb-3">
                  {group.heading}
                </p>
                <div className="flex items-end gap-8">
                  {group.items.map((item, idx) => (
                    <div key={item.label}>
                      <p
                        className={
                          idx === 0
                            ? 'text-2xl font-medium leading-none mb-1.5 tabular-nums'
                            : 'text-lg font-medium leading-none mb-1.5 text-text-muted tabular-nums'
                        }
                      >
                        {item.value}
                      </p>
                      <span className="text-[11px] text-text-muted uppercase tracking-wider whitespace-nowrap">
                        {item.label}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Recent chats — flat list, no card wrapper */}
      <div className="px-8 pb-8 page-transition">
        <div className="flex justify-between items-center pb-3 border-b border-border-subtle">
          <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
            Recent chats
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="text-xs text-text-muted !px-0 hover:bg-transparent hover:underline hover:text-text-default"
            onClick={navigateToSessionHistory}
          >
            See all
          </Button>
        </div>

        <div className="min-h-[96px] transition-all duration-300 ease-in-out">
          {isLoadingSessions ? (
            [200, 160, 220].map((w, i) => (
              <div key={i} className="flex items-center justify-between py-3 border-b border-border-subtle last:border-0">
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
                className="flex items-center justify-between text-sm py-3 px-2 -mx-2 rounded-lg hover:bg-background-medium cursor-pointer transition-colors session-item border-b border-border-subtle last:border-0"
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
                <div className="flex items-center space-x-2.5">
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
    </div>
  );
}
