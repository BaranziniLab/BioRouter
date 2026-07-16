import { useCallback, useEffect, useMemo, useRef, type UIEvent } from 'react';
import type { SessionSummary } from '../../api';
import { Clock, Folder, History } from '../icons/app-icons';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';

const LOAD_MORE_THRESHOLD_PX = 64;

export interface RecentChatGroup {
  label: string;
  sessions: SessionSummary[];
}

function sessionActivityTime(session: SessionSummary): number {
  const updatedAt = Date.parse(session.updated_at);
  if (!Number.isNaN(updatedAt)) return updatedAt;

  const createdAt = Date.parse(session.created_at);
  return Number.isNaN(createdAt) ? 0 : createdAt;
}

export function sortRecentChats(sessions: SessionSummary[]): SessionSummary[] {
  return [...sessions].sort((left, right) => {
    const activityDifference = sessionActivityTime(right) - sessionActivityTime(left);
    return activityDifference || left.id.localeCompare(right.id);
  });
}

function localCalendarDay(timestamp: number): number {
  const date = new Date(timestamp);
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
}

export function formatSessionDateLabel(updatedAt: string, now = Date.now()): string {
  const timestamp = Date.parse(updatedAt);
  if (Number.isNaN(timestamp)) return 'Unknown date';

  const dayDifference = Math.round(
    (localCalendarDay(now) - localCalendarDay(timestamp)) / 86_400_000
  );
  if (dayDifference === 0) return 'Today';
  if (dayDifference === 1) return 'Yesterday';

  const date = new Date(timestamp);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    ...(sameYear ? {} : { year: 'numeric' }),
  }).format(date);
}

export function groupRecentChatsByDate(
  sessions: SessionSummary[],
  now = Date.now()
): RecentChatGroup[] {
  const groups: RecentChatGroup[] = [];

  for (const session of sortRecentChats(sessions)) {
    const label = formatSessionDateLabel(session.updated_at, now);
    const existingGroup = groups[groups.length - 1];
    if (existingGroup?.label === label) {
      existingGroup.sessions.push(session);
    } else {
      groups.push({ label, sessions: [session] });
    }
  }

  return groups;
}

export function formatTimeSinceLastWorked(updatedAt: string, now = Date.now()): string {
  const timestamp = Date.parse(updatedAt);
  if (Number.isNaN(timestamp)) return 'Unknown';

  const elapsedMilliseconds = Math.max(0, now - timestamp);
  const elapsedMinutes = Math.floor(elapsedMilliseconds / 60_000);
  if (elapsedMinutes < 1) return 'Just now';
  if (elapsedMinutes < 60) return `${elapsedMinutes}m ago`;

  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours}h ago`;

  const elapsedDays = Math.floor(elapsedHours / 24);
  if (elapsedDays < 7) return `${elapsedDays}d ago`;

  const elapsedWeeks = Math.floor(elapsedDays / 7);
  if (elapsedWeeks < 5) return `${elapsedWeeks}w ago`;

  const elapsedMonths = Math.floor(elapsedDays / 30);
  if (elapsedMonths < 12) return `${elapsedMonths}mo ago`;

  return `${Math.floor(elapsedDays / 365)}y ago`;
}

function formatSessionTimestamp(updatedAt: string): string {
  const timestamp = Date.parse(updatedAt);
  if (Number.isNaN(timestamp)) return '';

  return new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  }).format(timestamp);
}

function ActiveChatIndicator({ sessionId }: { sessionId: string }) {
  return (
    <span
      data-testid={`running-chat-indicator-${sessionId}`}
      aria-hidden="true"
      className="relative flex h-4 w-4 flex-shrink-0 items-center justify-center text-text-default/80"
    >
      <span className="absolute h-4 w-4 rounded-full border border-current animate-[biorouter-working-ring_1.8s_ease-out_infinite]" />
      <span className="absolute h-2.5 w-2.5 rounded-full bg-current opacity-20 animate-[biorouter-working-glow_1.8s_ease-in-out_infinite]" />
      <span className="h-1.5 w-1.5 rounded-full bg-current opacity-70" />
    </span>
  );
}

interface RecentChatRowProps {
  session: SessionSummary;
  isActive: boolean;
  isRunning: boolean;
  onOpen: (sessionId: string) => void;
}

function RecentChatRow({ session, isActive, isRunning, onOpen }: RecentChatRowProps) {
  const title = session.name.trim() || 'Untitled chat';
  const accessibleLabel = `${isRunning ? 'Open ongoing chat' : 'Open chat'}: ${title}`;
  const messageLabel = `${session.message_count} ${session.message_count === 1 ? 'message' : 'messages'}`;
  const sessionTimestamp = formatSessionTimestamp(session.updated_at);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-testid={`recent-chat-${session.id}`}
          onClick={() => onOpen(session.id)}
          aria-label={accessibleLabel}
          aria-current={isActive ? 'page' : undefined}
          className={`relative flex h-8 w-full min-w-0 max-w-full items-center gap-2 overflow-hidden rounded-lg px-3 text-left text-sm transition-colors duration-150 before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent hover:bg-sidebar-hover ${
            isActive ? 'bg-sidebar-active font-medium before:bg-accent-bar' : ''
          }`}
        >
          <span className="min-w-0 flex-1 truncate leading-5">{title}</span>
          {isRunning && <ActiveChatIndicator sessionId={session.id} />}
        </button>
      </TooltipTrigger>
      <TooltipContent
        side="right"
        align="start"
        sideOffset={8}
        className="w-56 max-w-[min(14rem,calc(100vw-16px))] px-2 py-1.5 font-normal"
      >
        <div data-testid={`recent-chat-summary-${session.id}`}>
          <p className="line-clamp-2 font-medium text-text-inverse">{title}</p>
          <div className="mt-2 flex items-start gap-1.5 text-text-inverse/80">
            <Folder className="mt-0.5 size-3.5 shrink-0" />
            <span className="sr-only">Working folder: </span>
            <span className="min-w-0 break-all font-mono text-xs leading-4">
              {session.working_dir}
            </span>
          </div>
          <div className="mt-2 flex items-center gap-1.5 text-text-inverse/80">
            <Clock className="size-3.5 shrink-0" />
            <div>
              <p>Last worked {formatTimeSinceLastWorked(session.updated_at)}</p>
              {sessionTimestamp && <p className="mt-0.5 text-xs">{sessionTimestamp}</p>}
            </div>
          </div>
          <p className="mt-2 text-xs text-text-inverse/80">{messageLabel}</p>
        </div>
      </TooltipContent>
    </Tooltip>
  );
}

interface RecentChatsProps {
  sessions: SessionSummary[];
  activeSessionId?: string | null;
  runningSessionIds: ReadonlySet<string>;
  hasMore: boolean;
  isLoadingMore: boolean;
  onLoadMore: () => void;
  onOpen: (sessionId: string) => void;
  onViewAll: () => void;
}

export default function RecentChats({
  sessions,
  activeSessionId,
  runningSessionIds,
  hasMore,
  isLoadingMore,
  onLoadMore,
  onOpen,
  onViewAll,
}: RecentChatsProps) {
  const groups = useMemo(() => groupRecentChatsByDate(sessions), [sessions]);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const handleScroll = useCallback(
    (event: UIEvent<HTMLDivElement>) => {
      if (!hasMore || isLoadingMore) return;

      const container = event.currentTarget;
      const remainingScroll = container.scrollHeight - container.scrollTop - container.clientHeight;
      if (remainingScroll <= LOAD_MORE_THRESHOLD_PX) onLoadMore();
    },
    [hasMore, isLoadingMore, onLoadMore]
  );

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container || !hasMore || isLoadingMore || container.clientHeight === 0) return;
    if (container.scrollHeight <= container.clientHeight) onLoadMore();
  }, [hasMore, isLoadingMore, onLoadMore, sessions.length]);

  return (
    <div className="flex min-h-0 w-full min-w-0 flex-1 flex-col" data-testid="recent-chats">
      <div className="flex h-8 shrink-0 items-center px-5">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-text-subtle">
          Recents
        </span>
      </div>

      <div
        ref={scrollContainerRef}
        data-testid="recent-chat-scroll"
        className="min-h-0 w-full min-w-0 shrink overflow-y-auto overflow-x-hidden px-2 pb-1"
        onScroll={handleScroll}
      >
        {groups.length === 0 ? (
          <p className="px-3 py-3 text-xs text-text-subtle">
            {isLoadingMore ? 'Loading chats…' : 'No recent chats yet'}
          </p>
        ) : (
          groups.map((group) => (
            <section key={group.label} aria-label={group.label} className="mb-2 min-w-0 last:mb-0">
              <p className="px-3 pb-0.5 text-xs font-normal leading-4 text-text-subtle">
                {group.label}
              </p>
              <div className="min-w-0">
                {group.sessions.map((session) => (
                  <RecentChatRow
                    key={session.id}
                    session={session}
                    isActive={activeSessionId === session.id}
                    isRunning={runningSessionIds.has(session.id)}
                    onOpen={onOpen}
                  />
                ))}
              </div>
            </section>
          ))
        )}
        {isLoadingMore && groups.length > 0 && (
          <p
            role="status"
            data-testid="recent-chat-loading"
            className="px-3 py-2 text-xs text-text-subtle"
          >
            Loading more chats…
          </p>
        )}
      </div>

      <div className="shrink-0 px-2 pb-2 pt-1">
        <button
          type="button"
          data-testid="view-all-chat-history"
          onClick={onViewAll}
          className="flex h-8 w-full min-w-0 items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors duration-150 hover:bg-sidebar-hover"
        >
          <History className="h-4 w-4 shrink-0" />
          <span>View all chat history</span>
        </button>
      </div>
    </div>
  );
}
