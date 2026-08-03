import React from 'react';
import { formatDate } from '../../utils/date';
import { Session } from '../../api';
import { PrivacyBadge } from '../ui/PrivacyBadge';

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
    <div className="biorouter-list-row flex cursor-pointer items-center justify-between gap-3 px-4 py-2 hover:bg-background-medium">
      <div className="min-w-0">
        {/* Dense, not the pill: this is an enumerable row, and a chip on every
            line trains people to stop reading chips (issue #56, R10). The dot
            renders for Private only — Public and "no tier recorded" both stay
            silent, which is why the marker still means something when it does
            appear. The flex wrap is load-bearing: the dot must not be shrunk
            away by the truncating title beside it. */}
        <div className="flex min-w-0 items-center gap-1.5">
          <p className="truncate text-sm font-medium text-text-default">{session.name}</p>
          {session.privacy_tier && <PrivacyBadge tier={session.privacy_tier} dense />}
        </div>
        <p className="mt-0.5 truncate text-xs text-text-muted">
          {formatDate(session.updated_at)} • {session.message_count} messages
        </p>
        <p className="truncate font-mono text-xs text-text-subtle">{session.working_dir}</p>
      </div>
      {extraActions && <div className="flex shrink-0 items-center gap-1">{extraActions}</div>}
    </div>
  );
};

export default SessionItem;
