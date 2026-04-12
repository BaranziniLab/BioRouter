import { useEffect, useState } from 'react';
import { Card, CardContent, CardDescription } from '../ui/card';
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
            mostActiveDirs: [],
            avgSessionDuration: 0,
            totalTokens: 0,
            recentActivity: [],
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

  const renderSkeleton = () => (
    <div className="bg-background-muted flex flex-col h-full">
      {/* Header */}
      <div className="bg-background-default rounded-b-2xl mb-4" style={{ boxShadow: 'var(--shadow-default)' }}>
        <div className="px-8 pb-12 pt-19">
          <Greeting />
        </div>
      </div>

      {/* Cards area */}
      <div className="flex flex-col flex-1 gap-3 px-4 pb-4">
        <div className="grid grid-cols-2 gap-3">
          <Card className="w-full py-7 px-6 border-none rounded-2xl bg-background-default" style={{ boxShadow: 'var(--shadow-default)' }}>
            <CardContent className="flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <Skeleton className="h-10 w-16 mb-2" />
                <span className="text-xs text-text-muted uppercase tracking-wider">Total sessions</span>
              </div>
            </CardContent>
          </Card>

          <Card className="w-full py-7 px-6 border-none rounded-2xl bg-background-default" style={{ boxShadow: 'var(--shadow-default)' }}>
            <CardContent className="flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <Skeleton className="h-10 w-24 mb-2" />
                <span className="text-xs text-text-muted uppercase tracking-wider">Total tokens</span>
              </div>
            </CardContent>
          </Card>
        </div>

        <Card className="w-full py-7 px-6 border-none rounded-2xl bg-background-default" style={{ boxShadow: 'var(--shadow-default)' }}>
          <CardContent className="p-0">
            <div className="flex justify-between items-center mb-5">
              <span className="text-sm font-medium text-text-default uppercase tracking-wider">Recent chats</span>
              <Button
                variant="ghost"
                size="sm"
                className="text-xs text-text-muted !px-0 hover:bg-transparent hover:underline hover:text-text-default"
                onClick={navigateToSessionHistory}
              >
                See all
              </Button>
            </div>
            <div className="space-y-0 min-h-[96px] divide-y divide-border-default">
              {[48, 40, 52].map((w, i) => (
                <div key={i} className="flex items-center justify-between py-3">
                  <div className="flex items-center space-x-2.5">
                    <Skeleton className="h-4 w-4 rounded-sm flex-shrink-0" />
                    <Skeleton className={`h-3.5 w-${w}`} />
                  </div>
                  <Skeleton className="h-3.5 w-16" />
                </div>
              ))}
            </div>
          </CardContent>
        </Card>

        <div className="rounded-2xl flex-1 bg-background-muted" />
      </div>
    </div>
  );

  if (isLoading) {
    return renderSkeleton();
  }

  return (
    <div className="bg-background-muted flex flex-col h-full">
      {/* Header — full width, rounded bottom, shadow to lift it off the page */}
      <div
        className="bg-background-default rounded-b-2xl mb-4 relative overflow-hidden"
        style={{ boxShadow: 'var(--shadow-default)' }}
      >
        {/* Coral accent line across the very top */}
        <div
          className="absolute top-0 left-0 right-0 h-[3px]"
          style={{ background: 'linear-gradient(90deg, #cf6d47 0%, #e8935f 60%, #d4784e 100%)' }}
        />
        <div className="px-8 pb-10 pt-20">
          <p className="text-xs font-medium text-text-muted uppercase tracking-widest mb-3">BioRouter</p>
          <Greeting />
        </div>
      </div>

      {/* Dashboard cards with proper breathing room */}
      <div className="flex flex-col flex-1 gap-3 px-4 pb-4">
        {error && (
          <div className="px-4 py-2 bg-orange-50 dark:bg-orange-950/20 border border-orange-200 dark:border-orange-800/30 rounded-xl">
            <div className="flex items-center space-x-2">
              <div className="w-2 h-2 bg-orange-400 rounded-full flex-shrink-0" />
              <span className="text-xs text-orange-700 dark:text-orange-300">
                Failed to load insights
              </span>
            </div>
          </div>
        )}

        {/* Stats row — two equal cards */}
        <div className="grid grid-cols-2 gap-3">
          {/* Sessions card */}
          <Card
            className="w-full py-7 px-6 border-none rounded-2xl bg-background-default"
            style={{ boxShadow: 'var(--shadow-default)' }}
          >
            <CardContent className="page-transition flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <p className="text-4xl font-mono font-light mb-1.5">
                  {Math.max(insights?.totalSessions ?? 0, 0)}
                </p>
                <span className="text-[11px] text-text-muted uppercase tracking-wider">
                  Total sessions
                </span>
              </div>
            </CardContent>
          </Card>

          {/* Tokens card */}
          <Card
            className="w-full py-7 px-6 border-none rounded-2xl bg-background-default"
            style={{ boxShadow: 'var(--shadow-default)' }}
          >
            <CardContent className="page-transition flex flex-col justify-end h-full p-0">
              <div className="flex flex-col justify-end">
                <p className="text-4xl font-mono font-light mb-1.5">
                  {formatTokens(insights?.totalTokens)}
                </p>
                <span className="text-[11px] text-text-muted uppercase tracking-wider">
                  Total tokens
                </span>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* Recent chats card */}
        <Card
          className="w-full py-7 px-6 border-none rounded-2xl bg-background-default"
          style={{ boxShadow: 'var(--shadow-default)' }}
        >
          <CardContent className="page-transition p-0">
            <div className="flex justify-between items-center mb-4">
              <CardDescription className="mb-0">
                <span className="text-[11px] font-medium text-text-muted uppercase tracking-wider">
                  Recent chats
                </span>
              </CardDescription>
              <Button
                variant="ghost"
                size="sm"
                className="text-xs text-text-muted !px-0 hover:bg-transparent hover:underline hover:text-text-default"
                onClick={navigateToSessionHistory}
              >
                See all
              </Button>
            </div>

            <div className="divide-y divide-border-default min-h-[96px] transition-all duration-300 ease-in-out">
              {isLoadingSessions ? (
                [48, 40, 52].map((w, i) => (
                  <div key={i} className="flex items-center justify-between py-3">
                    <div className="flex items-center space-x-2.5">
                      <Skeleton className="h-4 w-4 rounded-sm flex-shrink-0" />
                      <Skeleton className={`h-3.5 w-${w}`} />
                    </div>
                    <Skeleton className="h-3.5 w-16" />
                  </div>
                ))
              ) : recentSessions.length > 0 ? (
                recentSessions.map((session, index) => (
                  <div
                    key={session.id}
                    className="flex items-center justify-between text-sm py-3 px-1 rounded-md hover:bg-background-muted cursor-pointer transition-colors session-item"
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
                    <span className="text-text-muted font-mono font-light text-xs flex-shrink-0">
                      {formatDateOnly(session.updated_at)}
                    </span>
                  </div>
                ))
              ) : (
                <div className="text-text-muted text-sm py-3">No recent chat sessions found.</div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Bottom spacer — intentionally empty, blends into background */}
        <div className="rounded-2xl flex-1" />
      </div>
    </div>
  );
}
