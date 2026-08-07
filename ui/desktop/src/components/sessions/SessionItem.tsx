import React from 'react';
import { formatDate } from '../../utils/date';
import { Session } from '../../api';
import { ChatKindIcon } from '../chats/ChatKindIcon';

interface SessionItemProps {
  session: Session;
  extraActions?: React.ReactNode;
}

/**
 * A session is enumerable, so it is a ROW, not a card (design.md P2, §4.14).
 * It previously rendered a <Card> per session, which stacked elevated surfaces
 * down the page.
 */
const SessionItem: React.FC<SessionItemProps> = ({ session, extraActions }) => {
  return (
    // `hover:bg-background-medium` was a SECOND hover on a row that already has
    // one: `.biorouter-list-row:hover` paints the shared list wash. Two hovers
    // on one row is how the list-vs-settings 42%/38% fork started.
    <div className="biorouter-list-row flex cursor-pointer items-center justify-between gap-3 px-4 py-2">
      <div className="min-w-0">
        {/* One leading glyph carrying both what this chat is and whether it is
            private, in place of the identical-for-everything bubble plus a
            separate dense dot. `flex-none` on the icon is load-bearing: it must
            not be shrunk away by the truncating title beside it. */}
        <div className="flex min-w-0 items-center gap-1.5">
          <ChatKindIcon session={session} tier={session.privacy_tier} className="h-3.5 w-3.5" />
          <p className="truncate text-label text-text-default">{session.name}</p>
        </div>
        <p className="mt-0.5 truncate text-supporting text-text-muted">
          {formatDate(session.updated_at)} • {session.message_count} messages
        </p>
        <p className="truncate font-mono text-supporting text-text-subtle">{session.working_dir}</p>
      </div>
      {extraActions && <div className="flex shrink-0 items-center gap-1">{extraActions}</div>}
    </div>
  );
};

export default SessionItem;
