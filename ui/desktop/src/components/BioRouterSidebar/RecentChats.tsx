import { useMemo } from 'react';
import type { Session } from '../../api';
import { Clock, Folder, History } from '../icons/app-icons';
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/Tooltip';

export const RECENT_CHAT_LIMIT = 10;

export interface RecentChatGroup {
  label: string;
  sessions: Session[];
}

function sessionActivityTime(session: Session): number {
  const updatedAt = Date.parse(session.updated_at);
  if (!Number.isNaN(updatedAt)) return updatedAt;

  const createdAt = Date.parse(session.created_at);
  return Number.isNaN(createdAt) ? 0 : createdAt;
}

export function getRecentChats(sessions: Session[], limit = RECENT_CHAT_LIMIT): Session[] {
  return [...sessions]
    .sort((left, right) => {
      const activityDifference = sessionActivityTime(right) - sessionActivityTime(left);
      return activityDifference || left.id.localeCompare(right.id);
    })
    .slice(0, limit);
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

export function groupRecentChatsByDate(sessions: Session[], now = Date.now()): RecentChatGroup[] {
  const groups: RecentChatGroup[] = [];

  for (const session of getRecentChats(sessions)) {
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
  session: Session;
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
          className={`relative flex h-8 w-full min-w-0 items-center gap-2 rounded-md px-3 text-left transition-colors duration-[var(--motion-base)] ease-[var(--ease-out)] before:absolute before:inset-y-2 before:left-0 before:w-0.5 before:rounded-full before:bg-transparent hover:bg-sidebar-hover ${
            isActive
              ? 'bg-sidebar-active before:bg-accent-bar'
              : 'text-text-muted hover:text-text-default'
          }`}
        >
          <span className="min-w-0 flex-1 truncate text-[13px] font-medium leading-[18px] text-text-default">
            {title}
          </span>
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
  sessions: Session[];
  activeSessionId?: string | null;
  runningSessionIds: ReadonlySet<string>;
  onOpen: (sessionId: string) => void;
  onViewAll: () => void;
}

export default function RecentChats({
  sessions,
  activeSessionId,
  runningSessionIds,
  onOpen,
  onViewAll,
}: RecentChatsProps) {
  const groups = useMemo(() => groupRecentChatsByDate(sessions), [sessions]);

  return (
    <div className="flex min-h-0 flex-1 flex-col" data-testid="recent-chats">
      <div className="flex h-9 shrink-0 items-center px-4 pt-1">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-text-subtle">
          Recents
        </span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-1">
        {groups.length === 0 ? (
          <p className="px-2 py-3 text-xs text-text-subtle">No recent chats yet</p>
        ) : (
          groups.map((group) => (
            <section key={group.label} aria-label={group.label} className="mb-1.5 last:mb-0">
              <p className="px-3 pb-0.5 text-[11px] font-medium leading-4 text-text-subtle">
                {group.label}
              </p>
              <div>
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
